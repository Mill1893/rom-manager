//! Scale and privacy evidence for the milestone gate (issue #39).
//!
//! These measure what this host can measure. The reference-host threshold in
//! the milestone report is a *packaged Windows* figure; what follows is a Linux
//! development-host baseline, and is reported as such rather than substituted
//! for it.

mod common;

use std::time::Instant;

use common::{TARGET_ID, fake, manifest_naming};
use rom_manager::{
    DeviceProfile, FakeTransport, ManagedEvidence, ManagementOrigin, RelativePath, SyncCore,
    TargetArtifact,
};

const SCALE: usize = 10_000;

fn artifact(index: usize) -> TargetArtifact {
    // Distinct content per artifact, so hashing is not accidentally trivial.
    let bytes = format!("rom-{index:05}").into_bytes();
    TargetArtifact::new(
        format!("rom-set-{index:05}"),
        RelativePath::new(format!("ROMs/nes/Game{index:05}.nes")).unwrap(),
        bytes,
    )
}

#[test]
fn planning_ten_thousand_target_artifacts_is_measured() {
    let desired: Vec<_> = (0..SCALE).map(artifact).collect();
    let total_bytes: u64 = desired.iter().map(TargetArtifact::size).sum();

    let mut core = SyncCore::new(
        FakeTransport::new("wpd://odin/storage", 1 << 30),
        TARGET_ID,
        DeviceProfile::generic_nes(),
        desired,
        1,
    );
    core.initialize_target(true).unwrap();

    let observed = Instant::now();
    core.refresh().unwrap();
    let refresh = observed.elapsed();

    let planned = Instant::now();
    let plan = core.build_plan().unwrap();
    let build = planned.elapsed();

    assert_eq!(plan.actions.len(), SCALE);
    assert!(
        plan.is_executable(),
        "a clean plan at scale must be runnable"
    );

    // Reported rather than asserted against a wall-clock bound: a shared CI
    // runner's timing is not a threshold anyone should gate on. The milestone
    // report carries these as a development-host baseline.
    println!("scale: {SCALE} artifacts, {total_bytes} bytes");
    println!("scale: refresh {refresh:?}, build_plan {build:?}");

    // A generous ceiling that only catches accidental quadratic behaviour,
    // which is the failure this test actually exists to prevent.
    assert!(
        build.as_secs() < 60,
        "planning {SCALE} artifacts took {build:?}, which suggests non-linear growth"
    );
}

#[test]
fn repeated_planning_does_not_grow_the_plan() {
    // Resource growth across repeated work: the same input must produce the
    // same plan, not accumulate.
    let desired: Vec<_> = (0..500).map(artifact).collect();
    let mut core = SyncCore::new(
        FakeTransport::new("wpd://odin/storage", 1 << 30),
        TARGET_ID,
        DeviceProfile::generic_nes(),
        desired,
        1,
    );
    core.initialize_target(true).unwrap();

    core.refresh().unwrap();
    let first = core.build_plan().unwrap();
    core.refresh().unwrap();
    let second = core.build_plan().unwrap();

    assert_eq!(
        first.digest, second.digest,
        "repeated planning must be stable"
    );
    assert_eq!(first.actions.len(), second.actions.len());
}

#[test]
fn a_large_managed_set_reconciles_to_retain_without_rework() {
    // The steady state: everything already present and managed. This is the
    // common case after the first sync, and it must not re-add anything.
    let mut transport = fake();
    let mut manifest = manifest_naming(b"seed");
    manifest.artifacts.clear();

    let desired: Vec<_> = (0..1_000).map(artifact).collect();
    for artifact in &desired {
        transport = transport.with_artifact(artifact.path.clone(), artifact.bytes().to_vec());
        manifest.artifacts.insert(
            artifact.path.clone(),
            ManagedEvidence {
                rom_set_id: artifact.rom_set_id.clone(),
                size: artifact.size(),
                sha256: artifact.sha256().to_owned(),
                origin: ManagementOrigin::Placed,
            },
        );
    }

    let mut core = SyncCore::new(
        transport,
        TARGET_ID,
        DeviceProfile::generic_nes(),
        desired,
        1,
    );
    core.initialize_target(true).unwrap();
    core.transport_mut().set_manifest(Some(manifest.clone()));
    core.replace_local_manifest(Some(manifest));
    core.refresh().unwrap();

    let plan = core.build_plan().unwrap();
    assert_eq!(
        plan.actions
            .iter()
            .filter(|action| action.action == rom_manager::Action::Retain)
            .count(),
        1_000,
        "already-managed content must retain rather than be re-added"
    );
    assert_eq!(plan.required_capacity, 0, "retaining consumes no capacity");
}
