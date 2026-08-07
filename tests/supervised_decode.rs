//! The supervised decoder worker protocol (issue #19, under #17).
//!
//! Deadlines are checked against a supplied instant rather than by sleeping, so
//! the half-hour and one-minute ceilings from the manifest are exercised
//! exactly rather than approximated by a test that waits.

use std::time::{Duration, Instant};

use rom_manager::{
    Outcome, ReasonCode,
    worker::{Budget, PROTOCOL_VERSION, Progress, Supervisor, WorkerFault, attribute},
};

fn small_budget() -> Budget {
    Budget {
        max_output_bytes: 1024,
        max_memory_bytes: 4096,
        total_deadline: Duration::from_secs(30),
        no_progress_deadline: Duration::from_secs(5),
    }
}

// ── Attribution: whose fault is it ──────────────────────────────────────────

#[test]
fn only_a_decoder_rejection_blames_the_users_file() {
    // #17: "a worker fault is never reported as malformed user input."
    assert_eq!(
        WorkerFault::Rejected("bad map".into()).outcome(),
        Outcome::Invalid
    );

    for ours in [
        WorkerFault::Crashed("segfault".into()),
        WorkerFault::TimedOut,
        WorkerFault::Stalled,
        WorkerFault::MemoryExhausted,
        WorkerFault::VersionMismatch {
            expected: 1,
            found: 2,
        },
    ] {
        assert_eq!(
            ours.outcome(),
            Outcome::ParserFailure,
            "{ours:?} is our failure, not a defect in the file"
        );
        assert!(
            !ours.outcome().blames_the_input(),
            "{ours:?} must not be phrased as bad user input"
        );
    }
}

#[test]
fn a_crash_and_a_truncated_file_are_told_apart() {
    // They present identically from the outside — no output — which is exactly
    // why the supervisor has to make the distinction.
    let crashed = WorkerFault::Crashed("killed".into());
    let rejected = WorkerFault::Rejected("truncated".into());

    assert_eq!(crashed.reason(), ReasonCode::WorkerFailed);
    assert_eq!(rejected.reason(), ReasonCode::MalformedStructure);
    assert!(
        crashed
            .diagnostic()
            .remediation()
            .contains("not in your file"),
        "our bug must not send the user to re-dump a disc"
    );
}

#[test]
fn cancellation_is_neither_partys_fault() {
    let fault = WorkerFault::Cancelled;
    assert_eq!(fault.outcome(), Outcome::Cancelled);
    assert!(!fault.outcome().blames_the_input());
    assert!(!fault.outcome().is_rom_pack_eligible());
}

#[test]
fn a_limit_fault_reports_both_sides_of_the_ceiling() {
    let fault = WorkerFault::OutputTooLarge {
        limit: 1024,
        observed: 99_999,
    };
    let measurement = fault.diagnostic().measurement.expect("measured");
    assert_eq!(measurement.limit, 1024);
    assert_eq!(measurement.observed, 99_999);
}

#[test]
fn a_worker_speaking_another_protocol_version_is_refused() {
    let refusal = attribute::<()>(Ok(()), PROTOCOL_VERSION + 1)
        .expect_err("a mismatched worker must be refused, not misread");
    assert_eq!(refusal.outcome, Outcome::ParserFailure);
}

#[test]
fn a_matching_version_passes_the_result_through() {
    assert!(attribute(Ok(42), PROTOCOL_VERSION).is_ok());
}

// ── Supervision ─────────────────────────────────────────────────────────────

#[test]
fn an_ordinary_decode_runs_to_completion() {
    let progress = Progress::new();
    let mut supervisor = Supervisor::new(small_budget(), progress.clone());

    for _ in 0..8 {
        progress.advance(32, 64);
        supervisor
            .check()
            .expect("an ordinary decode is not stopped");
    }
    assert_eq!(progress.decoded_written(), 512);
}

#[test]
fn output_past_the_ceiling_stops_the_decode() {
    let progress = Progress::new();
    let mut supervisor = Supervisor::new(small_budget(), progress.clone());
    progress.advance(64, 2048);

    assert!(matches!(
        supervisor.check(),
        Err(WorkerFault::OutputTooLarge { .. })
    ));
}

#[test]
fn a_decoder_reaching_its_memory_ceiling_is_stopped() {
    let progress = Progress::new();
    let mut supervisor = Supervisor::new(small_budget(), progress.clone());
    progress.record_memory(8192);

    assert_eq!(supervisor.check(), Err(WorkerFault::MemoryExhausted));
}

#[test]
fn cancellation_is_reported_before_any_other_verdict() {
    // A user who asked to stop should not be told their file is too large on
    // the way out.
    let progress = Progress::new();
    let mut supervisor = Supervisor::new(small_budget(), progress.clone());
    progress.advance(1, 999_999);
    progress.cancel();

    assert_eq!(supervisor.check(), Err(WorkerFault::Cancelled));
}

#[test]
fn the_total_deadline_ends_a_long_running_decode() {
    let progress = Progress::new();
    let mut supervisor = Supervisor::new(small_budget(), progress.clone());
    let start = Instant::now();

    // Producing output the whole time, so only the total deadline can fire.
    progress.advance(1, 1);
    supervisor.check_at(start).unwrap();
    progress.advance(1, 1);

    assert_eq!(
        supervisor.check_at(start + Duration::from_secs(31)),
        Err(WorkerFault::TimedOut)
    );
}

#[test]
fn a_decoder_producing_nothing_is_stopped_long_before_the_total_deadline() {
    // The common shape: still running, still burning CPU, no output. Only the
    // total deadline would let that continue for half an hour.
    let progress = Progress::new();
    let mut supervisor = Supervisor::new(small_budget(), progress.clone());
    let start = Instant::now();

    progress.advance(64, 64);
    supervisor.check_at(start).unwrap();

    // Reading input, producing nothing.
    progress.advance(64, 0);
    assert_eq!(
        supervisor.check_at(start + Duration::from_secs(6)),
        Err(WorkerFault::Stalled)
    );
}

#[test]
fn the_no_progress_clock_restarts_whenever_output_appears() {
    let progress = Progress::new();
    let mut supervisor = Supervisor::new(small_budget(), progress.clone());
    let start = Instant::now();

    // Four seconds of silence, then a byte, then four more. Neither gap alone
    // crosses the five-second deadline, and the total is eight.
    progress.advance(1, 1);
    supervisor.check_at(start).unwrap();
    supervisor
        .check_at(start + Duration::from_secs(4))
        .expect("four seconds is within the deadline");

    progress.advance(1, 1);
    supervisor
        .check_at(start + Duration::from_secs(5))
        .expect("output restarts the clock");
    supervisor
        .check_at(start + Duration::from_secs(9))
        .expect("the second gap is measured from the new output, not the start");
}

#[test]
fn a_decompression_bomb_is_caught_by_ratio_not_by_size_alone() {
    // The output ceiling alone would not catch this until 32 GiB had been
    // written. The ratio catches it after the first megabyte.
    let budget = Budget {
        max_output_bytes: u64::MAX,
        ..small_budget()
    };
    let progress = Progress::new();
    let mut supervisor = Supervisor::new(budget, progress.clone());

    let compressed = 2 * 1024 * 1024;
    progress.advance(compressed, compressed * 20_000);

    assert!(matches!(
        supervisor.check(),
        Err(WorkerFault::RatioExceeded { .. })
    ));
}

#[test]
fn a_small_highly_compressible_file_is_not_mistaken_for_a_bomb() {
    // Below the grace threshold the ratio arithmetic is meaningless: a tiny
    // file that expands a thousandfold is ordinary.
    let progress = Progress::new();
    let mut supervisor = Supervisor::new(
        Budget {
            max_output_bytes: u64::MAX,
            ..small_budget()
        },
        progress.clone(),
    );
    progress.advance(512, 512 * 5_000);

    assert!(supervisor.check().is_ok());
}

#[test]
fn the_decoding_side_can_report_but_never_waive_its_own_ceiling() {
    // Progress is the only handle a decoder holds, and it exposes no way to
    // change the budget. A decoder that could waive its ceiling would not be
    // supervised at all.
    let progress = Progress::new();
    let mut supervisor = Supervisor::new(small_budget(), progress.clone());

    progress.advance(1, 4096);
    progress.record_memory(0);

    assert!(
        supervisor.check().is_err(),
        "reporting more output cannot raise the ceiling it is checked against"
    );
}

#[test]
fn the_default_budget_is_the_manifests_ceilings() {
    let budget = Budget::default();
    assert_eq!(budget.max_output_bytes, 32 * 1024 * 1024 * 1024);
    assert_eq!(budget.max_memory_bytes, 1024 * 1024 * 1024);
    assert_eq!(budget.total_deadline, Duration::from_secs(1800));
    assert_eq!(budget.no_progress_deadline, Duration::from_secs(60));
}
