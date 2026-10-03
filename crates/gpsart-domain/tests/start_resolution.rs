use gpsart_domain::starts::{
    ConfiguredNetworkPoint, ConfiguredNetworkProvider, Coverage, NetworkPoint, Point,
    ProviderError, ResolutionContext, ResolveError, StartResolver, StartResolverProvider,
};
use gpsart_domain::{ActivityProfile, RouteRequestProfile};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

struct FixtureProvider(Mutex<Result<Option<NetworkPoint>, ProviderError>>);

impl StartResolverProvider for FixtureProvider {
    fn nearest_routable(
        &self,
        _requested: Point,
        _profile: RouteRequestProfile,
    ) -> Result<Option<NetworkPoint>, ProviderError> {
        *self.0.lock().unwrap()
    }
}

fn context(session: &str) -> ResolutionContext {
    ResolutionContext {
        guest_session_id: session.into(),
        profile: RouteRequestProfile::new(ActivityProfile::Walking),
        coverage_policy_version: "coverage-v1".into(),
        data_version: "map-v1".into(),
    }
}

fn resolver(result: Result<Option<NetworkPoint>, ProviderError>) -> StartResolver<FixtureProvider> {
    StartResolver::new(
        Coverage::rectangle("coverage-v1", 77.0, 28.0, 78.0, 29.0).unwrap(),
        FixtureProvider(Mutex::new(result)),
        Duration::from_secs(300),
    )
}

#[test]
fn distinguishes_invalid_outside_no_access_and_provider_failure() {
    let inside = Point::new(77.2, 28.6).unwrap();
    assert_eq!(
        Point::new(181.0, 28.6).unwrap_err(),
        ResolveError::InvalidCoordinates
    );
    assert_eq!(
        resolver(Ok(None))
            .resolve(
                Point::new(79.0, 28.6).unwrap(),
                context("guest-a"),
                SystemTime::now()
            )
            .unwrap_err(),
        ResolveError::UnsupportedCoverage
    );
    assert_eq!(
        resolver(Ok(None))
            .resolve(inside, context("guest-a"), SystemTime::now())
            .unwrap_err(),
        ResolveError::NoRoutableAccess
    );
    assert_eq!(
        resolver(Err(ProviderError))
            .resolve(inside, context("guest-a"), SystemTime::now())
            .unwrap_err(),
        ResolveError::ProviderFailure
    );
}

#[test]
fn confirmation_requires_short_snap_same_private_context_and_unexpired_record() {
    let requested = Point::new(77.2, 28.6).unwrap();
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);
    let short = resolver(Ok(Some(NetworkPoint {
        point: Point::new(77.2001, 28.6001).unwrap(),
        snap_distance_metres: 30,
    })));
    let resolution = short.resolve(requested, context("guest-a"), now).unwrap();
    assert_eq!(resolution.requested_point, requested);
    assert_eq!(resolution.snap_distance_metres, 30);
    assert!(resolution.confirmation_required);
    assert!(
        short
            .confirm(resolution.id, &context("guest-a"), now)
            .is_ok()
    );
    assert_eq!(
        short.confirm(resolution.id, &context("guest-b"), now),
        Err(ResolveError::WrongSession)
    );

    let long = resolver(Ok(Some(NetworkPoint {
        point: Point::new(77.21, 28.61).unwrap(),
        snap_distance_metres: 31,
    })));
    let long_resolution = long.resolve(requested, context("guest-a"), now).unwrap();
    assert_eq!(
        long.confirm(long_resolution.id, &context("guest-a"), now),
        Err(ResolveError::SnapTooFar)
    );
    assert_eq!(
        short.confirm(
            resolution.id,
            &context("guest-a"),
            now + Duration::from_secs(301)
        ),
        Err(ResolveError::ExpiredResolution)
    );
}

#[test]
fn confirmation_rejects_changed_profile_policy_or_data_version() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);
    let service = resolver(Ok(Some(NetworkPoint {
        point: Point::new(77.2, 28.6).unwrap(),
        snap_distance_metres: 5,
    })));
    let resolution = service
        .resolve(Point::new(77.2, 28.6).unwrap(), context("guest-a"), now)
        .unwrap();

    let mut changed = context("guest-a");
    changed.profile = RouteRequestProfile::new(ActivityProfile::Cycling);
    assert_eq!(
        service.confirm(resolution.id, &changed, now),
        Err(ResolveError::ProfileChanged)
    );
    changed = context("guest-a");
    changed.coverage_policy_version = "coverage-v2".into();
    assert_eq!(
        service.confirm(resolution.id, &changed, now),
        Err(ResolveError::CoveragePolicyChanged)
    );
    changed = context("guest-a");
    changed.data_version = "map-v2".into();
    assert_eq!(
        service.confirm(resolution.id, &changed, now),
        Err(ResolveError::DataVersionChanged)
    );
}

#[test]
fn configured_network_snapshot_uses_only_explicit_profiled_points() {
    let provider = ConfiguredNetworkProvider::new(vec![ConfiguredNetworkPoint {
        point: Point::new(77.2001, 28.6001).unwrap(),
        profile_versions: vec![gpsart_domain::ProfileVersion::WalkingV1],
    }])
    .unwrap();
    assert!(
        provider
            .nearest_routable(
                Point::new(77.2, 28.6).unwrap(),
                RouteRequestProfile::new(ActivityProfile::Walking),
            )
            .unwrap()
            .is_some()
    );
    assert_eq!(
        provider
            .nearest_routable(
                Point::new(77.2, 28.6).unwrap(),
                RouteRequestProfile::new(ActivityProfile::Cycling),
            )
            .unwrap(),
        None
    );
}
