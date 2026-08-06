//! Gating coverage for failure outcomes and residue handling (issue #50).
//!
//! The rule with teeth here is that cleanup deletes only residue the application
//! can prove it wrote. Anything it cannot verify is left in place and recorded,
//! becoming ordinary unknown content on the next pass.

use rom_manager::{
    Action, Approval, CancellationToken, DeviceProfile, ExecutionOutcome, FakeFault, FakeTransport,
    RelativePath, SyncCore, TargetArtifact, Transport,
};

const TARGET_ID: &str = "target-fixture-001";
const ROM_BYTES: &[u8] = include_bytes!("../fixtures/nes/tracers.nes");
const DESIRED: &str = "ROMs/nes/Tracers.nes";

fn path(value: &str) -> RelativePath {
    RelativePath::new(value).unwrap()
}

fn expected() -> TargetArtifact {
    TargetArtifact::new("rom-set-tracer", path(DESIRED), ROM_BYTES.to_vec())
}

fn core() -> SyncCore<FakeTransport> {
    let mut core = SyncCore::new(
        FakeTransport::new("wpd://odin/storage", 8 * 1024 * 1024),
        TARGET_ID,
        DeviceProfile::generic_nes(),
        vec![expected()],
        1,
    );
    core.initialize_target(true).unwrap();
    core
}

#[test]
fn unverifiable_content_is_left_in_place_and_recorded() {
    // The write returns, but read-back shows bytes that are not what was
    // written. The application cannot prove it created what is at that path —
    // it may be another tool's file at a colliding name — so it must NOT delete
    // it. The previous implementation deleted unconditionally.
    let mut core = core();
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    let approval = Approval::grant(&plan, plan.removal_count());

    core.transport_mut()
        .set_fault(Some(FakeFault::CorruptReadBack(path(DESIRED))));

    let outcome = core
        .execute(&plan, approval, &CancellationToken::default())
        .unwrap();

    let residue = match &outcome {
        ExecutionOutcome::Incomplete { report, .. } => &report.residue,
        other => panic!("expected Incomplete with recorded residue, got {other:?}"),
    };
    assert!(
        residue.iter().any(|entry| entry.path.as_str() == DESIRED),
        "unverifiable residue must be recorded for the user"
    );

    // And it must still be there — not deleted on the strength of a belief
    // about what the operation did.
    core.transport_mut().set_fault(None);
    let inventory = core.transport_mut().inventory().unwrap();
    assert!(
        inventory.artifacts.contains_key(&path(DESIRED)),
        "content the application could not verify as its own must never be deleted"
    );
}

#[test]
fn unverifiable_residue_becomes_ordinary_unknown_content() {
    // The residue record informs the user; it grants no authority. On the next
    // planning pass the path is simply content the manifest does not name.
    let mut core = core();
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    let approval = Approval::grant(&plan, plan.removal_count());

    core.transport_mut()
        .set_fault(Some(FakeFault::CorruptReadBack(path(DESIRED))));
    let _ = core.execute(&plan, approval, &CancellationToken::default());
    core.transport_mut().set_fault(None);

    core.refresh().unwrap();
    let replanned = core.build_plan().unwrap();

    // The manifest was never published, so this path is content the manifest
    // does not name. It is classified like any other unknown content — here it
    // happens to match what was planned, so it is offered as an *adoption*
    // requiring approval, and is never re-added over the top or treated as
    // already-managed on the strength of the residue record.
    let action = replanned
        .actions
        .iter()
        .find(|action| action.path.as_str() == DESIRED)
        .expect("the residue path is classified, not ignored");

    assert_eq!(
        action.action,
        Action::Adopt,
        "residue grants no authority: the path must be re-earned through adoption"
    );
    assert!(
        replanned.missing_managed.is_empty(),
        "nothing was ever recorded as managed, so nothing can be missing"
    );
}

#[test]
fn a_disconnect_mid_write_is_indeterminate_not_incomplete() {
    // "We do not know what is on your device" must not be presented as an
    // ordinary failure.
    let mut core = core();
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    let approval = Approval::grant(&plan, plan.removal_count());

    core.transport_mut()
        .set_fault(Some(FakeFault::DisconnectOnWrite));

    assert!(
        matches!(
            core.execute(&plan, approval, &CancellationToken::default())
                .unwrap(),
            ExecutionOutcome::Indeterminate { .. }
        ),
        "a disconnect during a write leaves target state unestablished"
    );
}

#[test]
fn a_successful_operation_reports_no_residue() {
    let mut core = core();
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    let approval = Approval::grant(&plan, plan.removal_count());

    assert!(matches!(
        core.execute(&plan, approval, &CancellationToken::default())
            .unwrap(),
        ExecutionOutcome::Completed { .. }
    ));
}

#[test]
fn cancellation_before_any_write_reports_no_residue() {
    let mut core = core();
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    let approval = Approval::grant(&plan, plan.removal_count());

    let cancellation = CancellationToken::default();
    cancellation.cancel();

    assert!(matches!(
        core.execute(&plan, approval, &cancellation).unwrap(),
        ExecutionOutcome::Cancelled { .. }
    ));
}

#[test]
fn a_completed_operation_reports_every_action_as_performed() {
    let mut core = core();
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    let approval = Approval::grant(&plan, plan.removal_count());

    let outcome = core
        .execute(&plan, approval, &CancellationToken::default())
        .unwrap();
    let report = outcome.report();

    assert_eq!(report.performed.len(), plan.actions.len());
    assert!(report.not_attempted.is_empty());
    assert!(report.uncertain.is_empty());
    assert!(report.residue.is_empty());
    assert!(report.recovery_disclosure().is_empty());
}

#[test]
fn a_disconnect_marks_the_action_uncertain_not_failed() {
    // "We cannot say either way" is distinct from "it did not happen".
    // Collapsing them would let uncertainty read as a clean no-op.
    let mut core = core();
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    let approval = Approval::grant(&plan, plan.removal_count());

    core.transport_mut()
        .set_fault(Some(FakeFault::DisconnectOnWrite));

    let outcome = core
        .execute(&plan, approval, &CancellationToken::default())
        .unwrap();
    let report = outcome.report();

    assert!(matches!(outcome, ExecutionOutcome::Indeterminate { .. }));
    assert_eq!(
        report.uncertain.len(),
        1,
        "the in-flight write is uncertain"
    );
    assert!(
        report.performed.is_empty(),
        "an uncertain action was never performed"
    );
    assert!(
        report
            .recovery_disclosure()
            .iter()
            .any(|line| line.contains("uncertain")),
        "recovery must disclose the uncertainty, got {:?}",
        report.recovery_disclosure()
    );
}

#[test]
fn a_cancelled_operation_reports_what_it_did_not_attempt() {
    let mut core = core();
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    let approval = Approval::grant(&plan, plan.removal_count());

    let cancellation = CancellationToken::default();
    cancellation.cancel();

    let outcome = core.execute(&plan, approval, &cancellation).unwrap();
    let report = outcome.report();

    assert!(matches!(outcome, ExecutionOutcome::Cancelled { .. }));
    assert!(report.performed.is_empty());
    assert_eq!(
        report.not_attempted.len(),
        plan.actions.len(),
        "every planned action must be accounted for as not attempted"
    );
}
