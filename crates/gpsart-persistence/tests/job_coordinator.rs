use gpsart_persistence::jobs::{
    Completion, InMemoryJobCoordinator, JobError, JobInput, JobState, StartOwnership,
};
use std::{sync::Arc, time::{Duration, SystemTime}};
use uuid::Uuid;

fn input(key: &str, hash: u8) -> JobInput {
    JobInput {
        resolved_start_id: Uuid::new_v4(),
        revision: 1,
        idempotency_key: key.into(),
        input_hash: [hash; 32],
        deadline_after: Duration::from_secs(120),
        max_attempts: 2,
    }
}

#[test]
fn submission_is_owned_current_and_idempotent() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    let owner = Uuid::new_v4();
    let foreign = Uuid::new_v4();
    let jobs = InMemoryJobCoordinator::default();
    let request = input("same-key", 1);
    jobs.register_start(StartOwnership { id: request.resolved_start_id, owner, expires_at: now + Duration::from_secs(60) });

    let first = jobs.submit(owner, request.clone(), now).unwrap();
    assert_eq!(jobs.submit(owner, request.clone(), now).unwrap(), first);
    let mut changed = request.clone();
    changed.input_hash = [2; 32];
    assert_eq!(jobs.submit(owner, changed, now), Err(JobError::IdempotencyConflict));
    assert_eq!(jobs.submit(foreign, request.clone(), now), Err(JobError::StartNotFound));
    assert_eq!(jobs.submit(owner, request, now + Duration::from_secs(61)), Err(JobError::StartNotFound));
}

#[test]
fn leases_are_exclusive_recoverable_and_fenced() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(2_000);
    let owner = Uuid::new_v4();
    let jobs = Arc::new(InMemoryJobCoordinator::default());
    let request = input("lease", 3);
    jobs.register_start(StartOwnership { id: request.resolved_start_id, owner, expires_at: now + Duration::from_secs(60) });
    let job = jobs.submit(owner, request, now).unwrap();
    let worker_one = Uuid::new_v4();
    let worker_two = Uuid::new_v4();
    let first = jobs.claim(worker_one, now, Duration::from_secs(10)).unwrap();
    assert_eq!(first.job_id, job);
    assert!(jobs.claim(worker_two, now, Duration::from_secs(10)).is_none());
    let recovered = jobs.claim(worker_two, now + Duration::from_secs(11), Duration::from_secs(10)).unwrap();
    assert_eq!(recovered.attempt, 2);
    assert_eq!(jobs.complete(first, Completion::Succeeded { candidate_ids: vec![Uuid::new_v4()], budget_exhausted: false }, now + Duration::from_secs(12)), Err(JobError::LeaseLost));
    jobs.complete(recovered, Completion::Succeeded { candidate_ids: vec![Uuid::new_v4()], budget_exhausted: false }, now + Duration::from_secs(12)).unwrap();
    assert_eq!(jobs.state(job), Some(JobState::Succeeded));
}

#[test]
fn cancellation_and_deadline_prevent_new_provider_work() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(3_000);
    let owner = Uuid::new_v4();
    let jobs = InMemoryJobCoordinator::default();
    let mut request = input("cancel", 4);
    request.deadline_after = Duration::from_secs(20);
    jobs.register_start(StartOwnership { id: request.resolved_start_id, owner, expires_at: now + Duration::from_secs(60) });
    let job = jobs.submit(owner, request, now).unwrap();
    let lease = jobs.claim(Uuid::new_v4(), now, Duration::from_secs(10)).unwrap();
    assert!(jobs.may_schedule_provider(&lease, now, Duration::from_secs(5)));
    assert_eq!(jobs.cancel(owner, job, now).unwrap(), JobState::Cancelled);
    assert_eq!(jobs.cancel(owner, job, now).unwrap(), JobState::Cancelled);
    assert!(!jobs.may_schedule_provider(&lease, now, Duration::from_secs(5)));
    assert_eq!(jobs.complete(lease, Completion::TimedOut, now), Err(JobError::Terminal));
}

#[test]
fn deadline_publishes_only_validated_candidates_otherwise_times_out() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(4_000);
    let owner = Uuid::new_v4();
    let jobs = InMemoryJobCoordinator::default();
    let mut request = input("deadline", 5);
    request.deadline_after = Duration::from_secs(10);
    jobs.register_start(StartOwnership { id: request.resolved_start_id, owner, expires_at: now + Duration::from_secs(60) });
    let job = jobs.submit(owner, request, now).unwrap();
    let lease = jobs.claim(Uuid::new_v4(), now, Duration::from_secs(20)).unwrap();
    assert!(!jobs.may_schedule_provider(&lease, now + Duration::from_secs(8), Duration::from_secs(3)));
    jobs.complete(lease, Completion::Succeeded { candidate_ids: vec![], budget_exhausted: true }, now + Duration::from_secs(10)).unwrap();
    assert_eq!(jobs.state(job), Some(JobState::TimedOut));
}
