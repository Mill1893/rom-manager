//! Gating coverage for occupied-path classification (issue #49).
//!
//! One test per row of the classification table. The invariants under test are
//! that every desired path resolves to exactly one classification, and that no
//! classification overwrites, relocates, or deletes content the application did
//! not both place and re-verify.

use rom_manager::{
    Action, BlockReason, DeviceProfile, FakeTransport, ManagedArtifactManifest, ManagedEvidence,
    ManagementOrigin, RelativePath, SyncCore, SyncPlan, TargetArtifact,
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

/// A manifest claiming `DESIRED` holds `bytes`.
fn manifest_naming(bytes: &[u8]) -> ManagedArtifactManifest {
    let mut manifest = ManagedArtifactManifest::empty(TARGET_ID, &DeviceProfile::generic_nes());
    manifest.generation = 1;
    manifest.artifacts.insert(
        path(DESIRED),
        ManagedEvidence {
            rom_set_id: "rom-set-tracer".into(),
            size: bytes.len() as u64,
            sha256: rom_manager::sha256(bytes),
            origin: ManagementOrigin::Placed,
        },
    );
    manifest
}

fn plan_with(transport: FakeTransport, manifest: Option<ManagedArtifactManifest>) -> SyncPlan {
    let mut core = SyncCore::new(
        transport,
        TARGET_ID,
        DeviceProfile::generic_nes(),
        vec![expected()],
        1,
    );
    core.initialize_target(true).unwrap();
    if let Some(manifest) = manifest {
        core.transport_mut().set_manifest(Some(manifest.clone()));
        core.replace_local_manifest(Some(manifest));
    }
    core.refresh().unwrap();
    core.build_plan().unwrap()
}

fn fake() -> FakeTransport {
    FakeTransport::new("wpd://odin/storage", 8 * 1024 * 1024)
}

fn actions_for(plan: &SyncPlan, wanted: Action) -> usize {
    plan.actions
        .iter()
        .filter(|action| action.action == wanted)
        .count()
}

#[test]
fn row_1_a_free_path_is_added() {
    let plan = plan_with(fake(), None);
    assert_eq!(actions_for(&plan, Action::Add), 1);
    assert!(plan.is_executable());
}

#[test]
fn row_2_managed_and_current_content_is_retained() {
    let transport = fake().with_artifact(path(DESIRED), ROM_BYTES.to_vec());
    let plan = plan_with(transport, Some(manifest_naming(ROM_BYTES)));

    assert_eq!(actions_for(&plan, Action::Retain), 1);
    assert!(plan.is_executable());
}

#[test]
fn row_4_externally_changed_managed_content_blocks() {
    // The manifest says this path holds the fixture, but it now holds something
    // else. The recorded evidence no longer describes reality, so the plan must
    // block rather than overwrite the user's change.
    let transport = fake().with_artifact(path(DESIRED), b"edited by another tool".to_vec());
    let plan = plan_with(transport, Some(manifest_naming(ROM_BYTES)));

    assert!(
        plan.blocked.iter().any(|reason| matches!(
            reason,
            BlockReason::ManagedContentChanged { path } if path.as_str() == DESIRED
        )),
        "expected ManagedContentChanged, got {:?}",
        plan.blocked
    );
    assert_eq!(actions_for(&plan, Action::Add), 0);
}

#[test]
fn row_5_an_exact_match_is_offered_as_adoption_never_taken_silently() {
    // Content identity is strong evidence, but not management authority. The
    // path becomes an Adopt action the approval authorizes — it is never
    // adopted as a side effect of planning.
    let transport = fake().with_artifact(path(DESIRED), ROM_BYTES.to_vec());
    let plan = plan_with(transport, None);

    assert_eq!(actions_for(&plan, Action::Adopt), 1);
    assert_eq!(actions_for(&plan, Action::Add), 0);
}

#[test]
fn row_6_unknown_content_blocks_and_is_preserved() {
    let transport = fake().with_artifact(path(DESIRED), b"someone else's file".to_vec());
    let plan = plan_with(transport, None);

    assert!(
        plan.blocked.iter().any(|reason| matches!(
            reason,
            BlockReason::PathConflict { path } if path.as_str() == DESIRED
        )),
        "expected PathConflict, got {:?}",
        plan.blocked
    );
    assert!(!plan.is_executable());
    assert_eq!(actions_for(&plan, Action::Add), 0);
}

#[test]
fn row_7_a_directory_at_a_desired_path_blocks_and_is_never_cleared() {
    let transport = fake().with_directory(path(DESIRED));
    let plan = plan_with(transport, None);

    assert!(
        plan.blocked.iter().any(|reason| matches!(
            reason,
            BlockReason::PathOccupiedByDirectory { path } if path.as_str() == DESIRED
        )),
        "expected PathOccupiedByDirectory, got {:?}",
        plan.blocked
    );
    // Nothing in the plan removes or relocates the directory.
    assert_eq!(actions_for(&plan, Action::Remove), 0);
    assert_eq!(actions_for(&plan, Action::Add), 0);
}

#[test]
fn row_8_a_differently_spelled_key_equal_entry_blocks() {
    let transport = fake().with_artifact(path("ROMs/nes/TRACERS.NES"), ROM_BYTES.to_vec());
    let plan = plan_with(transport, None);

    assert!(
        plan.blocked
            .iter()
            .any(|reason| matches!(reason, BlockReason::EffectiveCaseCollision { .. })),
        "expected EffectiveCaseCollision, got {:?}",
        plan.blocked
    );
    assert_eq!(actions_for(&plan, Action::Add), 0);
}

#[test]
fn row_9_two_key_equal_entries_make_the_namespace_ambiguous() {
    // Reachable without malice — an unprivileged process can flip a directory
    // case-sensitive, after which both spellings coexist.
    let transport = fake()
        .with_artifact(path("ROMs/nes/Other.nes"), b"one".to_vec())
        .with_artifact(path("ROMs/nes/OTHER.nes"), b"two".to_vec());
    let plan = plan_with(transport, None);

    assert!(
        plan.blocked
            .iter()
            .any(|reason| matches!(reason, BlockReason::EffectiveCaseCollision { .. })),
        "two key-equal entries must make the namespace ambiguous, got {:?}",
        plan.blocked
    );
}

#[test]
fn row_10_missing_managed_content_is_disclosed_not_acted_on() {
    // The manifest names content at a second path that the target no longer
    // holds. That is disclosed, and never converted into a removal elsewhere.
    let mut manifest = manifest_naming(ROM_BYTES);
    manifest.artifacts.insert(
        path("ROMs/nes/Vanished.nes"),
        ManagedEvidence {
            rom_set_id: "rom-set-vanished".into(),
            size: 4,
            sha256: rom_manager::sha256(b"gone"),
            origin: ManagementOrigin::Placed,
        },
    );
    let transport = fake().with_artifact(path(DESIRED), ROM_BYTES.to_vec());
    let plan = plan_with(transport, Some(manifest));

    assert!(
        plan.missing_managed
            .iter()
            .any(|path| path.as_str() == "ROMs/nes/Vanished.nes"),
        "absent managed content must be disclosed"
    );
    assert_eq!(
        actions_for(&plan, Action::Remove),
        0,
        "absence is never licence to remove anything else"
    );
}

#[test]
fn directories_are_not_reported_as_unknown_or_duplicate_content() {
    // Structural directories exist in the inventory so row 7 can see them, but
    // they are not user content and must not be listed as preserved.
    let transport = fake().with_directory(path("ROMs/nes/Subfolder"));
    let plan = plan_with(transport, None);

    assert!(
        !plan
            .preserved_unknowns
            .iter()
            .any(|path| path.as_str() == "ROMs/nes/Subfolder")
    );
    assert!(
        !plan
            .preserved_duplicates
            .iter()
            .any(|path| path.as_str() == "ROMs/nes/Subfolder")
    );
}

#[test]
fn an_unrepresentable_observed_name_is_preserved_not_an_error() {
    // Regression: tightening RelativePath made the filesystem transport fail
    // inventory() on any name the namespace cannot represent, so one stray NFD
    // or trailing-dot file rendered the whole Media Target unusable. Observed
    // names are reported verbatim and preserved; only contention blocks.
    let transport = fake().with_unrepresentable("ROMs/nes/cafe\u{301}.nes");
    let plan = plan_with(transport, None);

    assert!(
        plan.preserved_unrepresentable
            .iter()
            .any(|name| name == "ROMs/nes/cafe\u{301}.nes"),
        "an unrepresentable name must be preserved and disclosed"
    );
    // It contends with nothing desired, so the plan still runs.
    assert!(plan.is_executable());
}

#[test]
fn an_unrepresentable_name_contending_with_a_desired_path_blocks() {
    // A trailing-dot spelling of the desired name: Win32 path parsing would
    // resolve it to the same file, so planning cannot tell which object a
    // planned spelling would select.
    let transport = fake().with_unrepresentable(format!("{DESIRED}."));
    let plan = plan_with(transport, None);

    assert!(!plan.is_executable());
    assert!(
        plan.blocked
            .iter()
            .any(|reason| matches!(reason, BlockReason::InvalidTargetPath { .. })),
        "expected InvalidTargetPath, got {:?}",
        plan.blocked
    );
}

#[test]
fn a_profile_revision_bump_discloses_without_stranding_managed_content() {
    // The manifest was written under revision 1; the active profile is now
    // revision 2. Per #46 a revision mismatch "is not by itself a safety
    // failure" and content named by the manifest "remains managed" — discarding
    // the manifest would reclassify every managed artifact as unknown content
    // and strand it.
    let mut manifest = manifest_naming(ROM_BYTES);
    manifest.profile_revision = 1;

    let mut active = DeviceProfile::generic_nes();
    active.revision = 2;

    let mut core = SyncCore::new(
        fake().with_artifact(path(DESIRED), ROM_BYTES.to_vec()),
        TARGET_ID,
        active,
        vec![expected()],
        1,
    );
    core.initialize_target(true).unwrap();
    core.transport_mut().set_manifest(Some(manifest.clone()));
    core.replace_local_manifest(Some(manifest));
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();

    assert!(
        plan.blocked.iter().any(|reason| matches!(
            reason,
            BlockReason::ProfileRevisionChanged {
                recorded: 1,
                active: 2
            }
        )),
        "the revision change must be disclosed, got {:?}",
        plan.blocked
    );
    // Still recognized as managed and current — not reclassified as unknown.
    assert_eq!(actions_for(&plan, Action::Retain), 1);
    assert_eq!(actions_for(&plan, Action::Adopt), 0);
    assert!(
        plan.preserved_unknowns.is_empty(),
        "managed content must not be stranded as unknown by a revision bump"
    );
}
