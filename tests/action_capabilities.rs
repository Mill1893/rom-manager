//! Gating coverage for per-action capability requirements (issue #48).
//!
//! The rule under test is that a capability gates a plan **only when the plan
//! contains the action that needs it**, and that `atomic_publish` never blocks.

use rom_manager::{
    BlockReason, DeviceProfile, FakeTransport, RelativePath, SyncCore, TargetArtifact,
    TransportCapabilities,
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

fn full() -> TransportCapabilities {
    TransportCapabilities {
        read_back: true,
        leaf_delete: true,
        reports_capacity: true,
        atomic_publish: false,
    }
}

/// A core whose plan contains one `Add`, against a binding with `capabilities`.
fn core_planning_an_add(capabilities: TransportCapabilities) -> SyncCore<FakeTransport> {
    let mut core = SyncCore::new(
        FakeTransport::new("wpd://odin/storage", 8 * 1024 * 1024).with_capabilities(capabilities),
        TARGET_ID,
        DeviceProfile::generic_nes(),
        vec![expected()],
        1,
    );
    core.initialize_target(true).unwrap();
    core.refresh().unwrap();
    core
}

/// A core with nothing desired, so the plan contains no actions at all.
fn core_planning_nothing(capabilities: TransportCapabilities) -> SyncCore<FakeTransport> {
    let mut core = SyncCore::new(
        FakeTransport::new("wpd://odin/storage", 8 * 1024 * 1024).with_capabilities(capabilities),
        TARGET_ID,
        DeviceProfile::generic_nes(),
        Vec::new(),
        1,
    );
    core.initialize_target(true).unwrap();
    core.refresh().unwrap();
    core
}

fn blocked_capabilities(core: &mut SyncCore<FakeTransport>) -> Vec<String> {
    core.build_plan()
        .unwrap()
        .blocked
        .into_iter()
        .filter_map(|reason| match reason {
            BlockReason::UnsupportedCapability { capability } => Some(capability),
            _ => None,
        })
        .collect()
}

#[test]
fn read_back_gates_a_plan_that_places_content() {
    let mut core = core_planning_an_add(TransportCapabilities {
        read_back: false,
        ..full()
    });
    assert!(
        blocked_capabilities(&mut core).contains(&"read-back verification".to_string()),
        "an Add must not plan against a binding that cannot verify what it wrote"
    );
}

#[test]
fn capacity_reporting_gates_a_plan_that_places_content() {
    let mut core = core_planning_an_add(TransportCapabilities {
        reports_capacity: false,
        ..full()
    });
    assert!(
        blocked_capabilities(&mut core).contains(&"capacity reporting".to_string()),
        "capacity safety is a pre-flight guarantee, never a guess"
    );
}

#[test]
fn missing_capabilities_do_not_block_a_plan_without_the_action() {
    // No desired artifacts means no Add and no Adopt, so neither read-back nor
    // capacity reporting is required — the previous implementation blocked on
    // read-back unconditionally.
    let mut core = core_planning_nothing(TransportCapabilities {
        read_back: false,
        reports_capacity: false,
        leaf_delete: false,
        atomic_publish: false,
    });
    assert_eq!(
        blocked_capabilities(&mut core),
        Vec::<String>::new(),
        "a plan with no actions requires no capabilities"
    );
}

#[test]
fn leaf_delete_does_not_gate_a_plan_without_removals() {
    let mut core = core_planning_an_add(TransportCapabilities {
        leaf_delete: false,
        ..full()
    });
    assert!(
        !blocked_capabilities(&mut core).contains(&"leaf deletion".to_string()),
        "a plan containing no Remove must not be blocked by a binding that cannot delete"
    );
}

#[test]
fn atomic_publication_never_blocks_and_is_disclosed() {
    let mut core = core_planning_an_add(full());
    let plan = core.build_plan().unwrap();

    assert!(
        !plan.atomic_publication,
        "this binding reports no atomic publication"
    );
    assert_eq!(
        blocked_capabilities(&mut core),
        Vec::<String>::new(),
        "absent atomic publication is disclosed on the plan, never a block"
    );
    assert!(plan.is_executable());
}

#[test]
fn every_missing_capability_is_reported_at_once() {
    // The user sees every reason in one pass rather than discovering them
    // serially across repeated planning attempts.
    let mut core = core_planning_an_add(TransportCapabilities {
        read_back: false,
        reports_capacity: false,
        leaf_delete: true,
        atomic_publish: false,
    });
    let reported = blocked_capabilities(&mut core);

    assert!(reported.contains(&"read-back verification".to_string()));
    assert!(reported.contains(&"capacity reporting".to_string()));
}
