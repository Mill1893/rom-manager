use rom_manager::{
    Action, Approval, BlockReason, CancellationToken, DeviceProfile, ExecutionOutcome, FakeFault,
    FakeTransport, FilesystemTransport, ManagedArtifactManifest, RelativePath, SyncCore, SyncError,
    TargetArtifact, Transport,
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

fn initialized_fake(capacity: u64) -> SyncCore<FakeTransport> {
    let mut core = SyncCore::new(
        FakeTransport::new("wpd://odin/storage", capacity),
        TARGET_ID,
        DeviceProfile::generic_nes(),
        vec![expected()],
        1,
    );
    core.initialize_target(true).unwrap();
    core
}

#[test]
fn generic_profile_confines_nes_to_its_managed_root() {
    let profile = DeviceProfile::generic_nes();
    assert_eq!(
        profile.target_path("Tracers.nes").unwrap().as_str(),
        "ROMs/nes/Tracers.nes"
    );
    assert!(profile.target_path("../escape.nes").is_err());
    assert!(profile.target_path("wrong.zip").is_err());
    assert!(profile.target_path("stream:.nes").is_err());
    assert!(RelativePath::new("C:\\escape.nes").is_err());
    assert!(RelativePath::new("/escape.nes").is_err());
    assert!(serde_json::from_str::<RelativePath>(r#""ROMs/nes/../../../escape.nes""#).is_err());
}

#[test]
fn packaged_nes_fixture_has_the_frozen_identity() {
    assert_eq!(
        rom_manager::sha256(ROM_BYTES),
        "ac46556f3c6a5e3a0ed4ce7a4a09dd05ae8b01d012f473d29201b1ec2a200946"
    );
}

#[test]
fn marker_initialization_requires_explicit_confirmation() {
    let mut core = SyncCore::new(
        FakeTransport::new("fake://target", 1_000_000),
        TARGET_ID,
        DeviceProfile::generic_nes(),
        vec![expected()],
        1,
    );
    assert!(matches!(
        core.initialize_target(false),
        Err(SyncError::ConfirmationRequired)
    ));
    assert_eq!(core.transport_mut().marker().unwrap(), None);
    core.initialize_target(true).unwrap();
}

#[test]
fn unsupported_marker_schema_blocks_refresh() {
    let mut core = initialized_fake(1_000_000);
    core.transport_mut()
        .set_marker(Some(rom_manager::TargetMarker {
            schema_version: 2,
            target_id: TARGET_ID.into(),
        }));
    assert!(matches!(core.refresh(), Err(SyncError::MarkerConflict)));
}

#[test]
fn empty_target_plans_add_then_retain_after_verified_execution() {
    let mut core = initialized_fake(1_000_000);
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    assert!(plan.is_executable());
    assert_eq!(plan.actions.len(), 1);
    assert_eq!(plan.actions[0].action, Action::Add);

    let outcome = core
        .execute(
            &plan,
            Approval::grant(&plan, 0),
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(matches!(outcome, ExecutionOutcome::Completed { .. }));
    assert_eq!(
        core.transport_mut().read(&expected().path).unwrap(),
        ROM_BYTES
    );

    core.refresh().unwrap();
    let retained = core.build_plan().unwrap();
    assert_eq!(retained.actions[0].action, Action::Retain);
}

#[test]
fn equal_unrecognized_content_requires_explicit_adoption() {
    let transport = FakeTransport::new("wpd://odin/storage", 1_000_000)
        .with_artifact(expected().path, ROM_BYTES.to_vec());
    let mut core = SyncCore::new(
        transport,
        TARGET_ID,
        DeviceProfile::generic_nes(),
        vec![expected()],
        1,
    );
    core.initialize_target(true).unwrap();
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    assert_eq!(plan.actions[0].action, Action::Adopt);
    assert!(matches!(
        core.execute(
            &plan,
            Approval::grant(&plan, 0),
            &CancellationToken::default()
        )
        .unwrap(),
        ExecutionOutcome::Completed { .. }
    ));
}

#[test]
fn mismatching_canonical_content_blocks_without_overwrite() {
    let transport = FakeTransport::new("fake://target", 1_000_000)
        .with_artifact(expected().path, b"somebody else's bytes".to_vec());
    let mut core = SyncCore::new(
        transport,
        TARGET_ID,
        DeviceProfile::generic_nes(),
        vec![expected()],
        1,
    );
    core.initialize_target(true).unwrap();
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    assert_eq!(
        plan.blocked,
        vec![BlockReason::PathConflict {
            path: expected().path
        }]
    );
    assert!(matches!(
        core.execute(
            &plan,
            Approval::grant(&plan, 0),
            &CancellationToken::default()
        ),
        Err(SyncError::Blocked)
    ));
}

#[test]
fn desired_content_outside_the_profile_root_is_blocked() {
    let outside = TargetArtifact::new(
        "rom-set-tracer",
        path("Other/Tracers.nes"),
        ROM_BYTES.to_vec(),
    );
    let mut core = SyncCore::new(
        FakeTransport::new("fake://target", 1_000_000),
        TARGET_ID,
        DeviceProfile::generic_nes(),
        vec![outside.clone()],
        1,
    );
    core.initialize_target(true).unwrap();
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    assert!(
        plan.blocked
            .contains(&BlockReason::OutsideManagedRoot { path: outside.path })
    );
}

#[test]
fn effective_case_collisions_are_blocked() {
    let first = TargetArtifact::new("set-a", path("ROMs/nes/Game.nes"), b"a".to_vec());
    let second = TargetArtifact::new("set-b", path("ROMs/nes/game.nes"), b"b".to_vec());
    let mut core = SyncCore::new(
        FakeTransport::new("fake://target", 1_000_000),
        TARGET_ID,
        DeviceProfile::generic_nes(),
        vec![first, second.clone()],
        1,
    );
    core.initialize_target(true).unwrap();
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    assert!(plan.blocked.contains(&BlockReason::EffectiveCaseCollision {
        path: second.path,
        existing: None,
    }));
}

#[test]
fn existing_effective_case_collision_blocks_a_desired_path() {
    let colliding = path("ROMs/nes/tracers.nes");
    let transport = FakeTransport::new("fake://target", 1_000_000)
        .with_artifact(colliding, b"existing bytes".to_vec());
    let mut core = SyncCore::new(
        transport,
        TARGET_ID,
        DeviceProfile::generic_nes(),
        vec![expected()],
        1,
    );
    core.initialize_target(true).unwrap();
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    assert!(plan.blocked.contains(&BlockReason::EffectiveCaseCollision {
        path: expected().path,
        existing: Some(path("ROMs/nes/tracers.nes")),
    }));
}

#[test]
fn unknown_noncanonical_content_is_preserved_and_disclosed() {
    let unknown = path("ROMs/nes/Personal.nes");
    let transport = FakeTransport::new("fake://target", 1_000_000)
        .with_artifact(unknown.clone(), b"personal bytes".to_vec());
    let mut core = SyncCore::new(
        transport,
        TARGET_ID,
        DeviceProfile::generic_nes(),
        vec![expected()],
        1,
    );
    core.initialize_target(true).unwrap();
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    assert_eq!(plan.preserved_unknowns, vec![unknown.clone()]);

    assert!(matches!(
        core.execute(
            &plan,
            Approval::grant(&plan, 0),
            &CancellationToken::default()
        )
        .unwrap(),
        ExecutionOutcome::Completed { .. }
    ));
    assert_eq!(
        core.transport_mut().read(&unknown).unwrap(),
        b"personal bytes"
    );
}

#[test]
fn equal_noncanonical_content_is_disclosed_as_a_preserved_duplicate() {
    let duplicate = path("ROMs/nes/Copy.nes");
    let transport = FakeTransport::new("fake://target", 1_000_000)
        .with_artifact(duplicate.clone(), ROM_BYTES.to_vec());
    let mut core = SyncCore::new(
        transport,
        TARGET_ID,
        DeviceProfile::generic_nes(),
        vec![expected()],
        1,
    );
    core.initialize_target(true).unwrap();
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    assert_eq!(plan.preserved_duplicates, vec![duplicate]);
    assert!(plan.preserved_unknowns.is_empty());
}

#[test]
fn capacity_uses_a_high_water_mark_without_crediting_removals() {
    let mut core = initialized_fake(ROM_BYTES.len() as u64);
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    assert!(matches!(
        plan.blocked.as_slice(),
        [BlockReason::InsufficientCapacity { .. }]
    ));
}

#[test]
fn read_back_mismatch_never_authorizes_removal() {
    let old_path = path("ROMs/nes/Old.nes");
    let old_bytes = b"old managed ROM".to_vec();
    let transport = FakeTransport::new("fake://target", 1_000_000)
        .with_artifact(old_path.clone(), old_bytes.clone());
    let mut core = SyncCore::new(
        transport,
        TARGET_ID,
        DeviceProfile::generic_nes(),
        vec![expected()],
        1,
    );
    core.initialize_target(true).unwrap();
    let mut manifest = ManagedArtifactManifest::empty(TARGET_ID, &DeviceProfile::generic_nes());
    manifest.generation = 1;
    manifest.artifacts.insert(
        old_path.clone(),
        rom_manager::ManagedEvidence {
            rom_set_id: "old-set".into(),
            size: old_bytes.len() as u64,
            sha256: rom_manager::sha256(&old_bytes),
            origin: rom_manager::ManagementOrigin::Placed,
        },
    );
    core.transport_mut().set_manifest(Some(manifest.clone()));
    core.replace_local_manifest(Some(manifest));
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    assert_eq!(plan.removal_count(), 1);
    core.transport_mut()
        .set_fault(Some(FakeFault::CorruptReadBack(expected().path)));

    let outcome = core
        .execute(
            &plan,
            Approval::grant(&plan, 1),
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(matches!(outcome, ExecutionOutcome::Incomplete { .. }));
    assert_eq!(core.transport_mut().read(&old_path).unwrap(), old_bytes);
}

#[test]
fn managed_removal_is_permanent_only_after_explicit_count_acknowledgement() {
    let old_path = path("ROMs/nes/Old.nes");
    let old_bytes = b"old managed ROM".to_vec();
    let transport = FakeTransport::new("fake://target", 1_000_000)
        .with_artifact(old_path.clone(), old_bytes.clone());
    let mut core = SyncCore::new(
        transport,
        TARGET_ID,
        DeviceProfile::generic_nes(),
        vec![],
        1,
    );
    core.initialize_target(true).unwrap();
    let mut manifest = ManagedArtifactManifest::empty(TARGET_ID, &DeviceProfile::generic_nes());
    manifest.generation = 1;
    manifest.artifacts.insert(
        old_path.clone(),
        rom_manager::ManagedEvidence {
            rom_set_id: "old-set".into(),
            size: old_bytes.len() as u64,
            sha256: rom_manager::sha256(&old_bytes),
            origin: rom_manager::ManagementOrigin::Placed,
        },
    );
    core.transport_mut().set_manifest(Some(manifest.clone()));
    core.replace_local_manifest(Some(manifest));
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();

    assert!(matches!(
        core.execute(
            &plan,
            Approval::grant(&plan, 0),
            &CancellationToken::default()
        ),
        Err(SyncError::RemovalAcknowledgement)
    ));
    assert!(matches!(
        core.execute(
            &plan,
            Approval::grant(&plan, 1),
            &CancellationToken::default()
        )
        .unwrap(),
        ExecutionOutcome::Completed { .. }
    ));
    assert!(core.transport_mut().read(&old_path).is_err());
}

#[test]
fn cancellation_during_leaf_deletion_cannot_report_success() {
    let old_path = path("ROMs/nes/Old.nes");
    let old_bytes = b"old managed ROM".to_vec();
    let transport = FakeTransport::new("fake://target", 1_000_000)
        .with_artifact(old_path.clone(), old_bytes.clone());
    let mut core = SyncCore::new(
        transport,
        TARGET_ID,
        DeviceProfile::generic_nes(),
        vec![],
        1,
    );
    core.initialize_target(true).unwrap();
    let mut manifest = ManagedArtifactManifest::empty(TARGET_ID, &DeviceProfile::generic_nes());
    manifest.artifacts.insert(
        old_path,
        rom_manager::ManagedEvidence {
            rom_set_id: "old-set".into(),
            size: old_bytes.len() as u64,
            sha256: rom_manager::sha256(&old_bytes),
            origin: rom_manager::ManagementOrigin::Placed,
        },
    );
    core.transport_mut().set_manifest(Some(manifest.clone()));
    core.replace_local_manifest(Some(manifest));
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    core.transport_mut()
        .set_fault(Some(FakeFault::DelayDelete(30)));
    let cancellation = CancellationToken::default();
    let cancellation_request = cancellation.clone();
    let request = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(5));
        cancellation_request.cancel();
    });

    let outcome = core
        .execute(&plan, Approval::grant(&plan, 1), &cancellation)
        .unwrap();
    request.join().unwrap();
    assert!(matches!(outcome, ExecutionOutcome::Cancelled { .. }));
}

#[test]
fn disconnect_during_add_is_indeterminate_and_never_success() {
    let mut core = initialized_fake(1_000_000);
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    core.transport_mut()
        .set_fault(Some(FakeFault::DisconnectOnWrite));

    assert!(matches!(
        core.execute(
            &plan,
            Approval::grant(&plan, 0),
            &CancellationToken::default()
        )
        .unwrap(),
        ExecutionOutcome::Indeterminate { .. }
    ));
    assert!(matches!(core.build_plan(), Err(SyncError::RefreshRequired)));
}

#[test]
fn lost_manifest_read_back_is_indeterminate() {
    let mut core = initialized_fake(1_000_000);
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    core.transport_mut()
        .set_fault(Some(FakeFault::DisconnectAfterManifestWrite));

    assert!(matches!(
        core.execute(
            &plan,
            Approval::grant(&plan, 0),
            &CancellationToken::default()
        )
        .unwrap(),
        ExecutionOutcome::Indeterminate { .. }
    ));
}

#[test]
fn approval_cannot_execute_a_tampered_plan() {
    let mut core = initialized_fake(1_000_000);
    core.refresh().unwrap();
    let mut plan = core.build_plan().unwrap();
    plan.actions[0].path = path("ROMs/nes/Changed.nes");
    plan.digest.clear();
    plan.digest = rom_manager::sha256(&serde_json::to_vec(&plan).unwrap());

    assert!(matches!(
        core.execute(
            &plan,
            Approval::grant(&plan, 0),
            &CancellationToken::default()
        ),
        Err(SyncError::PlanChanged)
    ));
}

#[test]
fn cancellation_after_a_write_starts_no_removals_and_requires_refresh() {
    let old_path = path("ROMs/nes/Old.nes");
    let old_bytes = b"old managed ROM".to_vec();
    let transport = FakeTransport::new("fake://target", 1_000_000)
        .with_artifact(old_path.clone(), old_bytes.clone());
    let mut core = SyncCore::new(
        transport,
        TARGET_ID,
        DeviceProfile::generic_nes(),
        vec![expected()],
        1,
    );
    core.initialize_target(true).unwrap();
    let mut manifest = ManagedArtifactManifest::empty(TARGET_ID, &DeviceProfile::generic_nes());
    manifest.generation = 1;
    manifest.artifacts.insert(
        old_path.clone(),
        rom_manager::ManagedEvidence {
            rom_set_id: "old-set".into(),
            size: old_bytes.len() as u64,
            sha256: rom_manager::sha256(&old_bytes),
            origin: rom_manager::ManagementOrigin::Placed,
        },
    );
    core.transport_mut().set_manifest(Some(manifest.clone()));
    core.replace_local_manifest(Some(manifest));
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    core.transport_mut()
        .set_fault(Some(FakeFault::CancelAfterWrite));

    let cancellation = CancellationToken::default();
    let outcome = core
        .execute(&plan, Approval::grant(&plan, 1), &cancellation)
        .unwrap();
    assert!(matches!(outcome, ExecutionOutcome::Cancelled { .. }));
    core.transport_mut().set_fault(None);
    assert_eq!(core.transport_mut().read(&old_path).unwrap(), old_bytes);
    assert!(matches!(core.build_plan(), Err(SyncError::RefreshRequired)));
}

#[test]
fn post_plan_target_mutation_invalidates_approval() {
    let mut core = initialized_fake(1_000_000);
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    core.transport_mut()
        .mutate(path("unrecognized.txt"), b"mutation".to_vec());
    assert!(matches!(
        core.execute(
            &plan,
            Approval::grant(&plan, 0),
            &CancellationToken::default()
        ),
        Err(SyncError::PlanChanged)
    ));
}

#[test]
fn locator_change_preserves_target_identity_but_invalidates_existing_plan() {
    let mut core = initialized_fake(1_000_000);
    core.refresh().unwrap();
    let old_plan = core.build_plan().unwrap();
    core.transport_mut().set_locator("wpd://odin/new-session");
    assert!(matches!(
        core.execute(
            &old_plan,
            Approval::grant(&old_plan, 0),
            &CancellationToken::default()
        ),
        Err(SyncError::PlanChanged)
    ));
    core.refresh().unwrap();
    assert!(core.build_plan().unwrap().is_executable());
}

#[test]
fn manifest_disagreement_blocks_destructive_authority() {
    let mut core = initialized_fake(1_000_000);
    let mut target = ManagedArtifactManifest::empty(TARGET_ID, &DeviceProfile::generic_nes());
    target.generation = 2;
    core.transport_mut().set_manifest(Some(target));
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    assert!(plan.blocked.contains(&BlockReason::ManifestDisagreement));
}

#[test]
fn matching_foreign_manifest_copies_cannot_authorize_removal() {
    let foreign_path = path("ROMs/nes/Foreign.nes");
    let foreign_bytes = b"foreign managed bytes".to_vec();
    let transport = FakeTransport::new("fake://target", 1_000_000)
        .with_artifact(foreign_path.clone(), foreign_bytes.clone());
    let mut core = SyncCore::new(
        transport,
        TARGET_ID,
        DeviceProfile::generic_nes(),
        vec![],
        1,
    );
    core.initialize_target(true).unwrap();
    let mut foreign =
        ManagedArtifactManifest::empty("another-target", &DeviceProfile::generic_nes());
    foreign.artifacts.insert(
        foreign_path.clone(),
        rom_manager::ManagedEvidence {
            rom_set_id: "foreign-set".into(),
            size: foreign_bytes.len() as u64,
            sha256: rom_manager::sha256(&foreign_bytes),
            origin: rom_manager::ManagementOrigin::Placed,
        },
    );
    core.transport_mut().set_manifest(Some(foreign.clone()));
    core.replace_local_manifest(Some(foreign));
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    assert!(plan.blocked.contains(&BlockReason::ManifestDisagreement));
    assert_eq!(plan.removal_count(), 0);
}

#[test]
fn filesystem_transport_executes_the_same_verified_contract() {
    let directory = tempfile::tempdir().unwrap();
    let transport = FilesystemTransport::new(directory.path()).unwrap();
    let mut core = SyncCore::new(
        transport,
        TARGET_ID,
        DeviceProfile::generic_nes(),
        vec![expected()],
        1,
    );
    core.initialize_target(true).unwrap();
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    assert!(!plan.atomic_publication);
    assert!(matches!(
        core.execute(
            &plan,
            Approval::grant(&plan, 0),
            &CancellationToken::default()
        )
        .unwrap(),
        ExecutionOutcome::Completed { .. }
    ));
    let fixture = std::fs::read(directory.path().join("ROMs/nes/Tracers.nes")).unwrap();
    assert_eq!(fixture, ROM_BYTES);
}

#[test]
fn filesystem_target_mutation_after_planning_invalidates_approval() {
    let directory = tempfile::tempdir().unwrap();
    let transport = FilesystemTransport::new(directory.path()).unwrap();
    let mut core = SyncCore::new(
        transport,
        TARGET_ID,
        DeviceProfile::generic_nes(),
        vec![expected()],
        1,
    );
    core.initialize_target(true).unwrap();
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();
    std::fs::write(
        directory.path().join("unexpected.txt"),
        b"changed after approval",
    )
    .unwrap();

    assert!(matches!(
        core.execute(
            &plan,
            Approval::grant(&plan, 0),
            &CancellationToken::default()
        ),
        Err(SyncError::PlanChanged)
    ));
}

#[cfg(unix)]
#[test]
fn filesystem_transport_rejects_symlink_indirection() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(directory.path().join("ROMs")).unwrap();
    symlink(outside.path(), directory.path().join("ROMs/nes")).unwrap();
    let mut transport = FilesystemTransport::new(directory.path()).unwrap();

    assert!(matches!(
        transport.inventory(),
        Err(rom_manager::TransportError::Unsupported(_))
    ));
}
