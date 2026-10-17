use gpsart_persistence::jobs::{
    Completion, InMemoryJobCoordinator, JobError, JobInput, JobOutcomeCode, JobState,
    StartOwnership,
};
use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};
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
    jobs.register_start(StartOwnership {
        id: request.resolved_start_id,
        owner,
        expires_at: now + Duration::from_secs(60),
    });

    let first = jobs.submit(owner, request.clone(), now).unwrap();
    assert_eq!(jobs.submit(owner, request.clone(), now).unwrap(), first);
    let mut changed = request.clone();
    changed.input_hash = [2; 32];
    assert_eq!(
        jobs.submit(owner, changed, now),
        Err(JobError::IdempotencyConflict)
    );
    assert_eq!(
        jobs.submit(foreign, request.clone(), now),
        Err(JobError::StartNotFound)
    );
    assert_eq!(
        jobs.submit(owner, request, now + Duration::from_secs(61)),
        Err(JobError::StartNotFound)
    );
}

#[test]
fn leases_are_exclusive_recoverable_and_fenced() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(2_000);
    let owner = Uuid::new_v4();
    let jobs = Arc::new(InMemoryJobCoordinator::default());
    let request = input("lease", 3);
    jobs.register_start(StartOwnership {
        id: request.resolved_start_id,
        owner,
        expires_at: now + Duration::from_secs(60),
    });
    let job = jobs.submit(owner, request, now).unwrap();
    let worker_one = Uuid::new_v4();
    let worker_two = Uuid::new_v4();
    let first = jobs
        .claim(worker_one, now, Duration::from_secs(10))
        .unwrap();
    assert_eq!(first.job_id, job);
    assert!(
        jobs.claim(worker_two, now, Duration::from_secs(10))
            .is_none()
    );
    let recovered = jobs
        .claim(
            worker_two,
            now + Duration::from_secs(11),
            Duration::from_secs(10),
        )
        .unwrap();
    assert_eq!(recovered.attempt, 2);
    assert_eq!(
        jobs.complete(
            first,
            Completion::Succeeded {
                candidate_ids: vec![Uuid::new_v4()],
                budget_exhausted: false
            },
            now + Duration::from_secs(12)
        ),
        Err(JobError::LeaseLost)
    );
    jobs.complete(
        recovered,
        Completion::Succeeded {
            candidate_ids: vec![Uuid::new_v4()],
            budget_exhausted: false,
        },
        now + Duration::from_secs(12),
    )
    .unwrap();
    assert_eq!(jobs.state(job), Some(JobState::Succeeded));
}

#[test]
fn cancellation_and_deadline_prevent_new_provider_work() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(3_000);
    let owner = Uuid::new_v4();
    let jobs = InMemoryJobCoordinator::default();
    let mut request = input("cancel", 4);
    request.deadline_after = Duration::from_secs(20);
    jobs.register_start(StartOwnership {
        id: request.resolved_start_id,
        owner,
        expires_at: now + Duration::from_secs(60),
    });
    let job = jobs.submit(owner, request, now).unwrap();
    let lease = jobs
        .claim(Uuid::new_v4(), now, Duration::from_secs(10))
        .unwrap();
    assert!(jobs.may_schedule_provider(&lease, now, Duration::from_secs(5)));
    assert_eq!(jobs.cancel(owner, job, now).unwrap(), JobState::Cancelled);
    assert_eq!(jobs.cancel(owner, job, now).unwrap(), JobState::Cancelled);
    assert!(!jobs.may_schedule_provider(&lease, now, Duration::from_secs(5)));
    assert_eq!(
        jobs.complete(lease, Completion::TimedOut, now),
        Err(JobError::Terminal)
    );
}

#[test]
fn deadline_publishes_only_validated_candidates_otherwise_times_out() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(4_000);
    let owner = Uuid::new_v4();
    let jobs = InMemoryJobCoordinator::default();
    let mut request = input("deadline", 5);
    request.deadline_after = Duration::from_secs(10);
    jobs.register_start(StartOwnership {
        id: request.resolved_start_id,
        owner,
        expires_at: now + Duration::from_secs(60),
    });
    let job = jobs.submit(owner, request, now).unwrap();
    let lease = jobs
        .claim(Uuid::new_v4(), now, Duration::from_secs(20))
        .unwrap();
    assert!(!jobs.may_schedule_provider(
        &lease,
        now + Duration::from_secs(8),
        Duration::from_secs(3)
    ));
    jobs.complete(
        lease,
        Completion::NoMatch {
            code: JobOutcomeCode::InsufficientVisualMatch,
        },
        now + Duration::from_secs(10),
    )
    .unwrap();
    assert_eq!(jobs.state(job), Some(JobState::TimedOut));
}

#[test]
fn workers_cannot_report_timeout_before_the_job_deadline() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(4_500);
    let owner = Uuid::new_v4();
    let jobs = InMemoryJobCoordinator::default();
    let request = input("early-timeout", 7);
    jobs.register_start(StartOwnership {
        id: request.resolved_start_id,
        owner,
        expires_at: now + Duration::from_secs(60),
    });
    let job = jobs.submit(owner, request, now).unwrap();
    let lease = jobs
        .claim(Uuid::new_v4(), now, Duration::from_secs(10))
        .unwrap();

    assert_eq!(
        jobs.complete(lease, Completion::TimedOut, now + Duration::from_secs(1)),
        Err(JobError::InvalidInput)
    );
    assert_eq!(jobs.state(job), Some(JobState::Running));
}

#[test]
fn exhausted_attempts_become_terminal_instead_of_staying_stuck() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(5_000);
    let owner = Uuid::new_v4();
    let jobs = InMemoryJobCoordinator::default();
    let mut request = input("bounded", 6);
    request.max_attempts = 1;
    jobs.register_start(StartOwnership {
        id: request.resolved_start_id,
        owner,
        expires_at: now + Duration::from_secs(60),
    });
    let job = jobs.submit(owner, request, now).unwrap();
    jobs.claim(Uuid::new_v4(), now, Duration::from_secs(1))
        .unwrap();
    assert!(
        jobs.claim(
            Uuid::new_v4(),
            now + Duration::from_secs(2),
            Duration::from_secs(1)
        )
        .is_none()
    );
    assert_eq!(jobs.state(job), Some(JobState::Failed));
}

#[test]
fn classified_worker_outcomes_must_match_their_terminal_state() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(6_000);
    let owner = Uuid::new_v4();
    let jobs = InMemoryJobCoordinator::default();

    let no_match_request = input("classified-no-match", 8);
    jobs.register_start(StartOwnership {
        id: no_match_request.resolved_start_id,
        owner,
        expires_at: now + Duration::from_secs(60),
    });
    let no_match_job = jobs.submit(owner, no_match_request, now).unwrap();
    let no_match_lease = jobs
        .claim(Uuid::new_v4(), now, Duration::from_secs(10))
        .unwrap();
    jobs.complete(
        no_match_lease,
        Completion::NoMatch {
            code: JobOutcomeCode::InsufficientVisualMatch,
        },
        now,
    )
    .unwrap();
    assert_eq!(jobs.state(no_match_job), Some(JobState::NoMatch));

    let failed_request = input("classified-failure", 9);
    jobs.register_start(StartOwnership {
        id: failed_request.resolved_start_id,
        owner,
        expires_at: now + Duration::from_secs(60),
    });
    let failed_job = jobs.submit(owner, failed_request, now).unwrap();
    let failed_lease = jobs
        .claim(Uuid::new_v4(), now, Duration::from_secs(10))
        .unwrap();
    assert_eq!(
        jobs.complete(
            failed_lease.clone(),
            Completion::Failed {
                code: JobOutcomeCode::LegacyUnknown,
            },
            now,
        ),
        Err(JobError::InvalidInput)
    );
    assert_eq!(
        jobs.complete(
            failed_lease.clone(),
            Completion::Failed {
                code: JobOutcomeCode::OutsideDistance,
            },
            now,
        ),
        Err(JobError::InvalidInput)
    );
    jobs.complete(
        failed_lease,
        Completion::Failed {
            code: JobOutcomeCode::ProviderUnavailable,
        },
        now,
    )
    .unwrap();
    assert_eq!(jobs.state(failed_job), Some(JobState::Failed));
}
