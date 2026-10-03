use async_trait::async_trait;
use axum::{
    body::Body,
    http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header},
};
use gpsart_api::{
    Readiness, app_with_auth,
    auth::{
        AuthError, InMemorySessionStore, PrivateResourceKind, SessionPolicy, authorize_resource,
    },
};
use http_body_util::BodyExt;
use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};
use tower::ServiceExt;
use uuid::Uuid;

#[derive(Clone)]
struct Healthy;

#[async_trait]
impl Readiness for Healthy {
    async fn check(&self) -> Result<(), &'static str> {
        Ok(())
    }
}

fn policy() -> SessionPolicy {
    SessionPolicy::new(
        "https://app.gpsart.test",
        true,
        Duration::from_secs(86_400),
        Duration::from_secs(43_200),
    )
    .unwrap()
}

async fn issue(store: Arc<InMemorySessionStore>) -> (String, String) {
    let response = app_with_auth(Healthy, store, policy())
        .oneshot(
            Request::post("/api/v1/guest-sessions")
                .header(header::ORIGIN, "https://app.gpsart.test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let cookies = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let session = cookies
        .iter()
        .find(|v| v.starts_with("gpsart_guest="))
        .unwrap();
    assert!(
        session.contains("HttpOnly")
            && session.contains("Secure")
            && session.contains("SameSite=Lax")
            && session.contains("Path=/")
    );
    assert!(!session.contains("latitude") && !session.contains("longitude"));
    let csrf_cookie = cookies
        .iter()
        .find(|v| v.starts_with("gpsart_csrf="))
        .unwrap();
    let csrf = csrf_cookie
        .split(';')
        .next()
        .unwrap()
        .split_once('=')
        .unwrap()
        .1
        .to_owned();
    (session.split(';').next().unwrap().to_owned(), csrf)
}

#[tokio::test]
async fn issues_opaque_private_cookies_and_persists_only_digest() {
    let store = Arc::new(InMemorySessionStore::default());
    let (cookie, _) = issue(store.clone()).await;
    let raw_token = cookie.split_once('=').unwrap().1;
    assert!(raw_token.len() >= 43);
    assert_eq!(store.session_count().await, 1);
    assert!(!store.contains_plaintext(raw_token.as_bytes()).await);
    assert!(store.all_digests_are_sha256().await);
}

#[tokio::test]
async fn state_changes_require_exact_origin_and_double_submit_csrf() {
    let store = Arc::new(InMemorySessionStore::default());
    let (cookie, csrf) = issue(store.clone()).await;
    for (origin, supplied_csrf) in [
        (None, Some(csrf.as_str())),
        (Some("https://evil.test"), Some(csrf.as_str())),
        (Some("https://app.gpsart.test"), None),
    ] {
        let mut request = Request::delete("/api/v1/guest-sessions/current")
            .header(header::COOKIE, format!("{cookie}; gpsart_csrf={csrf}"));
        if let Some(value) = origin {
            request = request.header(header::ORIGIN, value);
        }
        if let Some(value) = supplied_csrf {
            request = request.header("x-csrf-token", value);
        }
        let response = app_with_auth(Healthy, store.clone(), policy())
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
    let response = app_with_auth(Healthy, store, policy())
        .oneshot(
            Request::delete("/api/v1/guest-sessions/current")
                .header(header::COOKIE, format!("{cookie}; gpsart_csrf={csrf}"))
                .header(header::ORIGIN, "https://app.gpsart.test")
                .header("x-csrf-token", csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn foreign_and_unknown_resources_receive_the_same_denial() {
    let store = Arc::new(InMemorySessionStore::default());
    let (owner_cookie, _owner_csrf) = issue(store.clone()).await;
    let (other_cookie, other_csrf) = issue(store.clone()).await;
    let job = Uuid::new_v4();
    let route = Uuid::new_v4();
    let resolved_start = Uuid::new_v4();
    store
        .assign(PrivateResourceKind::ResolvedStart, resolved_start, &owner_cookie)
        .await
        .unwrap();
    store
        .assign(PrivateResourceKind::Job, job, &owner_cookie)
        .await
        .unwrap();
    store
        .assign(PrivateResourceKind::Route, route, &owner_cookie)
        .await
        .unwrap();

    for (kind, id, method, cookie, csrf) in [
        (
            PrivateResourceKind::ResolvedStart,
            resolved_start,
            Method::GET,
            &other_cookie,
            &other_csrf,
        ),
        (
            PrivateResourceKind::Job,
            job,
            Method::GET,
            &other_cookie,
            &other_csrf,
        ),
        (
            PrivateResourceKind::Job,
            job,
            Method::DELETE,
            &other_cookie,
            &other_csrf,
        ),
        (
            PrivateResourceKind::Route,
            route,
            Method::GET,
            &other_cookie,
            &other_csrf,
        ),
    ] {
        let headers = auth_headers(cookie, csrf);
        assert_eq!(
            authorize_resource(
                store.as_ref(),
                &policy(),
                &method,
                &headers,
                kind,
                id,
                SystemTime::now()
            )
            .await,
            Err(AuthError::NotFound)
        );
        assert_eq!(
            authorize_resource(
                store.as_ref(),
                &policy(),
                &method,
                &headers,
                kind,
                Uuid::new_v4(),
                SystemTime::now()
            )
            .await,
            Err(AuthError::NotFound)
        );
    }
}

#[tokio::test]
async fn expired_sessions_cannot_authorize_work_and_cleanup_exposes_only_counts() {
    let store = Arc::new(InMemorySessionStore::default());
    let cookie = store.insert_expired_for_test().await;
    let headers = auth_headers(&cookie, "test-csrf");
    assert_eq!(
        authorize_resource(
            store.as_ref(),
            &policy(),
            &Method::GET,
            &headers,
            PrivateResourceKind::Job,
            Uuid::new_v4(),
            SystemTime::now()
        )
        .await,
        Err(AuthError::Unauthenticated)
    );
    let report = store.cleanup(SystemTime::now()).await.unwrap();
    assert_eq!(report.deleted_sessions, 1);
    assert_eq!(
        format!("{report:?}"),
        "CleanupReport { deleted_sessions: 1 }"
    );
}

fn auth_headers(cookie: &str, csrf: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::COOKIE,
        HeaderValue::from_str(&format!("{cookie}; gpsart_csrf={csrf}")).unwrap(),
    );
    headers.insert(
        header::ORIGIN,
        HeaderValue::from_static("https://app.gpsart.test"),
    );
    headers.insert("x-csrf-token", HeaderValue::from_str(csrf).unwrap());
    headers
}

#[tokio::test]
async fn session_response_never_contains_cookie_token() {
    let store = Arc::new(InMemorySessionStore::default());
    let response = app_with_auth(Healthy, store, policy())
        .oneshot(
            Request::post("/api/v1/guest-sessions")
                .header(header::ORIGIN, "https://app.gpsart.test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let raw = set_cookie
        .split(';')
        .next()
        .unwrap()
        .split_once('=')
        .unwrap()
        .1
        .to_owned();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(!String::from_utf8_lossy(&body).contains(&raw));
}

#[tokio::test]
async fn rotation_replaces_and_revokes_the_previous_credential() {
    let store = Arc::new(InMemorySessionStore::default());
    let (cookie, csrf) = issue(store.clone()).await;
    let early = app_with_auth(Healthy, store.clone(), policy())
        .oneshot(
            Request::post("/api/v1/guest-sessions/current/rotate")
                .header(header::COOKIE, format!("{cookie}; gpsart_csrf={csrf}"))
                .header(header::ORIGIN, "https://app.gpsart.test")
                .header("x-csrf-token", csrf.as_str())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(early.status(), StatusCode::CONFLICT);
    store
        .age_session_for_test(&cookie, Duration::from_secs(43_200))
        .await
        .unwrap();
    let response = app_with_auth(Healthy, store.clone(), policy())
        .oneshot(
            Request::post("/api/v1/guest-sessions/current/rotate")
                .header(header::COOKIE, format!("{cookie}; gpsart_csrf={csrf}"))
                .header(header::ORIGIN, "https://app.gpsart.test")
                .header("x-csrf-token", csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut old_headers = HeaderMap::new();
    old_headers.insert(header::COOKIE, HeaderValue::from_str(&cookie).unwrap());
    assert_eq!(
        gpsart_api::auth::authenticate(store.as_ref(), &old_headers, SystemTime::now()).await,
        Err(AuthError::Unauthenticated)
    );
    assert_eq!(
        store.session_count().await,
        2,
        "rotation preserves only digests for audit/cleanup; the old record is revoked"
    );
}
