//! Gating coverage for the ephemeral Sync Plan approval (issue #44, as amended
//! by issue #47 to bind the inventory digest rather than the generation).

use rom_manager::{
    Approval, CancellationToken, DeviceProfile, ExecutionOutcome, FakeTransport, RelativePath,
    SyncCore, SyncError, TargetArtifact,
};

const TARGET_ID: &str = "target-fixture-001";
const ROM_BYTES: &[u8] = include_bytes!("../fixtures/nes/tracers.nes");

fn path(value: &str) -> RelativePath {
    RelativePath::new(value).unwrap()
}

fn expected() -> TargetArtifact {
    TargetArtifact::new(
        "rom-set-tracer",
        path("ROMs/nes/Tracers.nes"),
        ROM_BYTES.to_vec(),
    )
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
    core.refresh().unwrap();
    core
}

#[test]
fn a_matching_approval_authorizes_execution() {
    let mut core = core();
    let plan = core.build_plan().unwrap();
    let approval = Approval::grant(&plan, plan.removal_count());

    assert_eq!(
        core.execute(&plan, approval, &CancellationToken::default())
            .unwrap(),
        ExecutionOutcome::Completed
    );
}

#[test]
fn an_approval_for_a_different_plan_is_rejected() {
    let mut core = core();
    let plan = core.build_plan().unwrap();

    let mut forged = Approval::grant(&plan, plan.removal_count());
    forged.plan_digest = "0".repeat(64);

    // The rejection names the specific mismatch rather than failing opaquely.
    assert!(matches!(
        core.execute(&plan, forged, &CancellationToken::default()),
        Err(SyncError::ApprovalInvalid("plan digest"))
    ));
}

#[test]
fn an_approval_bound_to_another_binding_is_rejected() {
    let mut core = core();
    let plan = core.build_plan().unwrap();

    let mut elsewhere = Approval::grant(&plan, plan.removal_count());
    elsewhere.binding_locator = "wpd://someone-elses-device/storage".into();

    assert!(matches!(
        core.execute(&plan, elsewhere, &CancellationToken::default()),
        Err(SyncError::ApprovalInvalid("Transport Binding locator"))
    ));
}

#[test]
fn an_approval_bound_to_stale_evidence_is_rejected() {
    let mut core = core();
    let plan = core.build_plan().unwrap();

    let mut stale = Approval::grant(&plan, plan.removal_count());
    stale.inventory_digest = "0".repeat(64);

    assert!(matches!(
        core.execute(&plan, stale, &CancellationToken::default()),
        Err(SyncError::ApprovalInvalid("inventory evidence"))
    ));
}

#[test]
fn an_understated_removal_acknowledgement_is_rejected() {
    // An approval that acknowledged fewer removals than the plan performs can
    // never authorize it — approving a smaller plan is not approving a larger.
    let mut core = core();
    let plan = core.build_plan().unwrap();
    let understated = Approval::grant(&plan, plan.removal_count() + 1);

    assert!(matches!(
        core.execute(&plan, understated, &CancellationToken::default()),
        Err(SyncError::RemovalAcknowledgement)
    ));
}

#[test]
fn re_observing_an_unchanged_target_keeps_an_approval_valid() {
    // The freshness identity is a digest over observed evidence, not an
    // observation counter. A refresh that sees no change must not invalidate
    // work the user already approved.
    let mut core = core();
    let plan = core.build_plan().unwrap();
    let approval = Approval::grant(&plan, plan.removal_count());

    core.refresh().unwrap();
    let replanned = core.build_plan().unwrap();

    assert_eq!(
        replanned.inventory_digest, plan.inventory_digest,
        "an unchanged target must reproduce its digest"
    );
    assert_eq!(
        core.execute(&plan, approval, &CancellationToken::default())
            .unwrap(),
        ExecutionOutcome::Completed
    );
}

#[test]
fn target_mutation_between_planning_and_execution_invalidates_the_approval() {
    let mut core = core();
    let plan = core.build_plan().unwrap();
    let approval = Approval::grant(&plan, plan.removal_count());

    core.transport_mut().mutate(
        path("ROMs/nes/Interloper.nes"),
        b"placed by another tool".to_vec(),
    );

    let outcome = core.execute(&plan, approval, &CancellationToken::default());
    assert!(
        matches!(
            outcome,
            Err(SyncError::PlanChanged) | Err(SyncError::ApprovalInvalid(_))
        ),
        "a target that changed underneath the user must not be written to"
    );
}

#[test]
fn a_blocked_plan_is_never_executed_even_with_an_approval() {
    let mut core = SyncCore::new(
        // Too small to hold the fixture, so the plan carries a capacity block.
        FakeTransport::new("wpd://odin/storage", 1024),
        TARGET_ID,
        DeviceProfile::generic_nes(),
        vec![expected()],
        1,
    );
    core.initialize_target(true).unwrap();
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    assert!(!plan.is_executable());

    let approval = Approval::grant(&plan, plan.removal_count());
    assert!(matches!(
        core.execute(&plan, approval, &CancellationToken::default()),
        Err(SyncError::Blocked)
    ));
}
