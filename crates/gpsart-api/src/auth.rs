use async_trait::async_trait;
use axum::http::{HeaderMap, Method, header};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use gpsart_persistence::{CleanupReport, GuestSessionRecord, GuestSessionStore, SessionStoreError};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::RwLock,
    time::{Duration, SystemTime},
};
use subtle::ConstantTimeEq;
use thiserror::Error;
use uuid::Uuid;

pub use gpsart_persistence::PrivateResourceKind;

pub const SESSION_COOKIE: &str = "gpsart_guest";
pub const CSRF_COOKIE: &str = "gpsart_csrf";

#[derive(Clone, Debug)]
pub struct SessionPolicy {
    pub allowed_origin: String,
    pub secure: bool,
    pub lifetime: Duration,
    pub rotate_after: Duration,
}

impl SessionPolicy {
    pub fn new(
        origin: impl Into<String>,
        secure: bool,
        lifetime: Duration,
        rotate_after: Duration,
    ) -> Result<Self, AuthError> {
        let allowed_origin = origin.into();
        if allowed_origin.is_empty()
            || rotate_after.is_zero()
            || rotate_after >= lifetime
            || (secure && !allowed_origin.starts_with("https://"))
        {
            return Err(AuthError::InvalidPolicy);
        }
        Ok(Self {
            allowed_origin,
            secure,
            lifetime,
            rotate_after,
        })
    }

    pub fn rotation_due(&self, session: &GuestSessionRecord, now: SystemTime) -> bool {
        now.duration_since(session.created_at)
            .is_ok_and(|age| age >= self.rotate_after)
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum AuthError {
    #[error("authentication required")]
    Unauthenticated,
    #[error("request origin or CSRF token is invalid")]
    Forbidden,
    #[error("resource was not found")]
    NotFound,
    #[error("authentication service unavailable")]
    Unavailable,
    #[error("credential rotation is not due")]
    RotationNotDue,
    #[error("session policy is invalid")]
    InvalidPolicy,
}

#[derive(Clone, Debug)]
pub struct IssuedSession {
    pub record: GuestSessionRecord,
    pub token: String,
    pub csrf_token: String,
}

pub async fn issue_session(
    store: &dyn GuestSessionStore,
    policy: &SessionPolicy,
    now: SystemTime,
) -> Result<IssuedSession, AuthError> {
    let token = random_token()?;
    let csrf_token = random_token()?;
    let record = GuestSessionRecord {
        id: Uuid::new_v4(),
        token_digest: digest(token.as_bytes()),
        created_at: now,
        expires_at: now + policy.lifetime,
        cleanup_after: now + policy.lifetime + Duration::from_secs(86_400),
        revoked: false,
    };
    store
        .insert_session(record.clone())
        .await
        .map_err(|_| AuthError::Unavailable)?;
    Ok(IssuedSession {
        record,
        token,
        csrf_token,
    })
}

pub async fn rotate_session(
    store: &dyn GuestSessionStore,
    previous_id: Uuid,
    policy: &SessionPolicy,
    now: SystemTime,
) -> Result<IssuedSession, AuthError> {
    let token = random_token()?;
    let csrf_token = random_token()?;
    let record = GuestSessionRecord {
        id: Uuid::new_v4(),
        token_digest: digest(token.as_bytes()),
        created_at: now,
        expires_at: now + policy.lifetime,
        cleanup_after: now + policy.lifetime + Duration::from_secs(86_400),
        revoked: false,
    };
    store
        .rotate_session(previous_id, record.clone())
        .await
        .map_err(|_| AuthError::Unavailable)?;
    Ok(IssuedSession {
        record,
        token,
        csrf_token,
    })
}

pub async fn authenticate(
    store: &dyn GuestSessionStore,
    headers: &HeaderMap,
    now: SystemTime,
) -> Result<GuestSessionRecord, AuthError> {
    let token = cookie(headers, SESSION_COOKIE).ok_or(AuthError::Unauthenticated)?;
    let presented = digest(token.as_bytes());
    let stored = store
        .find_session(presented)
        .await
        .map_err(|_| AuthError::Unavailable)?
        .ok_or(AuthError::Unauthenticated)?;
    if !bool::from(stored.token_digest.ct_eq(&presented))
        || stored.revoked
        || stored.expires_at <= now
    {
        return Err(AuthError::Unauthenticated);
    }
    Ok(stored)
}

pub fn enforce_request_protection(
    policy: &SessionPolicy,
    method: &Method,
    headers: &HeaderMap,
) -> Result<(), AuthError> {
    if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return Ok(());
    }
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok());
    if origin != Some(policy.allowed_origin.as_str()) {
        return Err(AuthError::Forbidden);
    }
    let cookie_token = cookie(headers, CSRF_COOKIE).ok_or(AuthError::Forbidden)?;
    let header_token = headers
        .get("x-csrf-token")
        .and_then(|value| value.to_str().ok())
        .ok_or(AuthError::Forbidden)?;
    if cookie_token.len() != header_token.len()
        || !bool::from(cookie_token.as_bytes().ct_eq(header_token.as_bytes()))
    {
        return Err(AuthError::Forbidden);
    }
    Ok(())
}

pub async fn authorize_resource(
    store: &dyn GuestSessionStore,
    policy: &SessionPolicy,
    method: &Method,
    headers: &HeaderMap,
    kind: PrivateResourceKind,
    resource_id: Uuid,
    now: SystemTime,
) -> Result<GuestSessionRecord, AuthError> {
    let session = authenticate(store, headers, now).await?;
    enforce_request_protection(policy, method, headers)?;
    if !store
        .owns_resource(session.id, kind, resource_id)
        .await
        .map_err(|_| AuthError::Unavailable)?
    {
        return Err(AuthError::NotFound);
    }
    Ok(session)
}

pub fn session_cookie(value: &str, policy: &SessionPolicy) -> String {
    format!(
        "{SESSION_COOKIE}={value}; Path=/; Max-Age={}; HttpOnly; SameSite=Lax{}",
        policy.lifetime.as_secs(),
        if policy.secure { "; Secure" } else { "" }
    )
}

pub fn csrf_cookie(value: &str, policy: &SessionPolicy) -> String {
    format!(
        "{CSRF_COOKIE}={value}; Path=/; Max-Age={}; SameSite=Strict{}",
        policy.lifetime.as_secs(),
        if policy.secure { "; Secure" } else { "" }
    )
}

pub fn expired_cookies(policy: &SessionPolicy) -> [String; 2] {
    [
        format!(
            "{SESSION_COOKIE}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax{}",
            if policy.secure { "; Secure" } else { "" }
        ),
        format!(
            "{CSRF_COOKIE}=; Path=/; Max-Age=0; SameSite=Strict{}",
            if policy.secure { "; Secure" } else { "" }
        ),
    ]
}

fn random_token() -> Result<String, AuthError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| AuthError::Unavailable)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn digest(value: &[u8]) -> [u8; 32] {
    Sha256::digest(value).into()
}

fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .find_map(|pair| {
            let (key, value) = pair.trim().split_once('=')?;
            (key == name && !value.is_empty()).then_some(value)
        })
}

#[derive(Default)]
pub struct InMemorySessionStore {
    sessions: RwLock<HashMap<[u8; 32], GuestSessionRecord>>,
    ownership: RwLock<HashMap<(PrivateResourceKind, Uuid), Uuid>>,
}

impl InMemorySessionStore {
    pub async fn session_count(&self) -> usize {
        self.sessions.read().expect("session lock").len()
    }
    pub async fn contains_plaintext(&self, plaintext: &[u8]) -> bool {
        self.sessions
            .read()
            .expect("session lock")
            .keys()
            .any(|key| key.as_slice() == plaintext)
    }
    pub async fn all_digests_are_sha256(&self) -> bool {
        self.sessions
            .read()
            .expect("session lock")
            .keys()
            .all(|key| key.len() == 32)
    }
    pub async fn assign(
        &self,
        kind: PrivateResourceKind,
        id: Uuid,
        cookie_header: &str,
    ) -> Result<(), AuthError> {
        let raw = cookie_header
            .split_once('=')
            .map(|(_, value)| value)
            .ok_or(AuthError::Unauthenticated)?;
        let session = self
            .find_session(digest(raw.as_bytes()))
            .await
            .map_err(|_| AuthError::Unavailable)?
            .ok_or(AuthError::Unauthenticated)?;
        self.ownership
            .write()
            .expect("ownership lock")
            .insert((kind, id), session.id);
        Ok(())
    }
    pub async fn insert_expired_for_test(&self) -> String {
        let token = random_token().expect("OS random source");
        let now = SystemTime::now();
        let record = GuestSessionRecord {
            id: Uuid::new_v4(),
            token_digest: digest(token.as_bytes()),
            created_at: now - Duration::from_secs(120),
            expires_at: now - Duration::from_secs(60),
            cleanup_after: now - Duration::from_secs(1),
            revoked: false,
        };
        self.insert_session(record).await.expect("memory insert");
        format!("{SESSION_COOKIE}={token}")
    }
    pub async fn age_session_for_test(
        &self,
        cookie_header: &str,
        age: Duration,
    ) -> Result<(), AuthError> {
        let raw = cookie_header
            .split_once('=')
            .map(|(_, value)| value)
            .ok_or(AuthError::Unauthenticated)?;
        let mut sessions = self.sessions.write().expect("session lock");
        let session = sessions
            .get_mut(&digest(raw.as_bytes()))
            .ok_or(AuthError::Unauthenticated)?;
        session.created_at = SystemTime::now() - age;
        Ok(())
    }
    pub async fn cleanup(&self, cutoff: SystemTime) -> Result<CleanupReport, SessionStoreError> {
        self.cleanup_expired_sessions(cutoff).await
    }
}

#[async_trait]
impl GuestSessionStore for InMemorySessionStore {
    async fn insert_session(&self, record: GuestSessionRecord) -> Result<(), SessionStoreError> {
        self.sessions
            .write()
            .expect("session lock")
            .insert(record.token_digest, record);
        Ok(())
    }
    async fn find_session(
        &self,
        digest: [u8; 32],
    ) -> Result<Option<GuestSessionRecord>, SessionStoreError> {
        Ok(self
            .sessions
            .read()
            .expect("session lock")
            .get(&digest)
            .cloned())
    }
    async fn revoke_session(&self, id: Uuid) -> Result<(), SessionStoreError> {
        for session in self.sessions.write().expect("session lock").values_mut() {
            if session.id == id {
                session.revoked = true;
            }
        }
        Ok(())
    }
    async fn rotate_session(
        &self,
        previous_id: Uuid,
        replacement: GuestSessionRecord,
    ) -> Result<(), SessionStoreError> {
        let mut sessions = self.sessions.write().expect("session lock");
        let mut rotated = false;
        for session in sessions.values_mut() {
            if session.id == previous_id && !session.revoked {
                session.revoked = true;
                rotated = true;
            }
        }
        if !rotated {
            return Err(SessionStoreError::Unavailable);
        }
        sessions.insert(replacement.token_digest, replacement);
        Ok(())
    }
    async fn owns_resource(
        &self,
        session_id: Uuid,
        kind: PrivateResourceKind,
        resource_id: Uuid,
    ) -> Result<bool, SessionStoreError> {
        Ok(self
            .ownership
            .read()
            .expect("ownership lock")
            .get(&(kind, resource_id))
            == Some(&session_id))
    }
    async fn cleanup_expired_sessions(
        &self,
        cutoff: SystemTime,
    ) -> Result<CleanupReport, SessionStoreError> {
        let mut sessions = self.sessions.write().expect("session lock");
        let before = sessions.len();
        sessions.retain(|_, session| session.cleanup_after > cutoff);
        Ok(CleanupReport {
            deleted_sessions: (before - sessions.len()) as u64,
        })
    }
}
