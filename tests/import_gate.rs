//! Gate 2 certification evidence (issue #65).
//!
//! The end-to-end path — import, deduplicate, materialize, select exactly, sync
//! — plus the guarantees a ROM Pack selection makes over time.

mod common;

use std::{fs, path::PathBuf};

use common::{TARGET_ID, expected, fake};
use rom_manager::{
    Approval, CancellationToken, DeviceProfile, ExecutionOutcome, Library, MaterializationCache,
    Store, SyncCore, sha256,
};

const ROM: &[u8] = include_bytes!("../fixtures/nes/tracers.nes");

struct Gate {
    _directory: tempfile::TempDir,
    library: Library,
    store: Store,
    incoming: PathBuf,
    cache: MaterializationCache,
}

fn gate() -> Gate {
    let directory = tempfile::tempdir().unwrap();
    let incoming = directory.path().join("incoming");
    fs::create_dir_all(&incoming).unwrap();
    Gate {
        library: Library::open(directory.path().join("library")).unwrap(),
        store: Store::open(&directory.path().join("library.sqlite3")).unwrap(),
        cache: MaterializationCache::open(directory.path().join("cache"), 1 << 20).unwrap(),
        incoming,
        _directory: directory,
    }
}

#[test]
fn the_whole_path_runs_import_to_successful_sync() {
    let gate = gate();

    // Import.
    let source = gate.incoming.join("Tracers.nes");
    fs::write(&source, ROM).unwrap();
    let imported = gate.library.import_file(&gate.store, &source, 100).unwrap();

    // The source can now disappear entirely.
    fs::remove_file(&source).unwrap();

    // Library identity and an exact ROM Pack selection.
    gate.store
        .upsert_rom_set(
            ("game-tracers", "NES", "Tracers"),
            ("release-usa", "USA"),
            (
                "rom-set-tracer",
                &imported.content_digest,
                "Tracers.nes",
                imported.size,
            ),
        )
        .unwrap();
    gate.store
        .record_pack_selection("pack-1", 1, &[("rom-set-tracer", &imported.content_digest)])
        .unwrap();

    // Materialize through the cache.
    let bytes = gate.library.read_object(&imported.content_digest).unwrap();
    gate.cache.put(&imported.content_digest, &bytes).unwrap();
    assert_eq!(
        gate.cache.get(&imported.content_digest).unwrap(),
        Some(ROM.to_vec())
    );

    // Sync it.
    let mut core = SyncCore::new(
        fake(),
        TARGET_ID,
        DeviceProfile::generic_nes(),
        vec![expected()],
        1,
    );
    core.initialize_target(true).unwrap();
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
fn an_exact_selection_survives_availability_changes() {
    // Content becoming unavailable must never rewrite, substitute, or deselect
    // what a ROM Pack chose. The selection is the user's decision; availability
    // is a fact about the world.
    let gate = gate();
    let source = gate.incoming.join("Tracers.nes");
    fs::write(&source, ROM).unwrap();
    let imported = gate.library.import_file(&gate.store, &source, 100).unwrap();

    gate.store
        .upsert_rom_set(
            ("game-tracers", "NES", "Tracers"),
            ("release-usa", "USA"),
            (
                "rom-set-tracer",
                &imported.content_digest,
                "Tracers.nes",
                imported.size,
            ),
        )
        .unwrap();
    gate.store
        .record_pack_selection("pack-1", 1, &[("rom-set-tracer", &imported.content_digest)])
        .unwrap();
    let before = gate.store.pack_selection("pack-1", 1).unwrap();

    // The content is quarantined — now unavailable.
    gate.store
        .quarantine_object(&imported.content_digest)
        .unwrap();
    assert!(
        !gate
            .library
            .rom_is_available(&gate.store, &imported.content_digest)
            .unwrap()
    );

    assert_eq!(
        gate.store.pack_selection("pack-1", 1).unwrap(),
        before,
        "an exact selection is untouched by its content becoming unavailable"
    );
}

#[test]
fn cache_eviction_has_no_effect_on_a_selection() {
    let gate = gate();
    let source = gate.incoming.join("Tracers.nes");
    fs::write(&source, ROM).unwrap();
    let imported = gate.library.import_file(&gate.store, &source, 100).unwrap();

    gate.store
        .upsert_rom_set(
            ("game-tracers", "NES", "Tracers"),
            ("release-usa", "USA"),
            (
                "rom-set-tracer",
                &imported.content_digest,
                "Tracers.nes",
                imported.size,
            ),
        )
        .unwrap();
    gate.store
        .record_pack_selection("pack-1", 1, &[("rom-set-tracer", &imported.content_digest)])
        .unwrap();
    let before = gate.store.pack_selection("pack-1", 1).unwrap();

    gate.cache.put(&imported.content_digest, ROM).unwrap();
    gate.cache.clear().unwrap();

    assert_eq!(gate.store.pack_selection("pack-1", 1).unwrap(), before);
    assert!(
        gate.library
            .rom_is_available(&gate.store, &imported.content_digest)
            .unwrap(),
        "clearing derived data leaves availability untouched"
    );
}

#[test]
fn the_import_fault_matrix_never_produces_a_false_success() {
    let gate = gate();

    // Unreadable source.
    let missing = gate.incoming.join("Gone.nes");
    assert!(
        gate.library
            .import_file(&gate.store, &missing, 100)
            .is_err()
    );

    // Corrupt managed content.
    let source = gate.incoming.join("Tracers.nes");
    fs::write(&source, ROM).unwrap();
    let imported = gate.library.import_file(&gate.store, &source, 100).unwrap();
    let stored = gate
        ._directory
        .path()
        .join("library/objects")
        .join(&imported.content_digest[..2])
        .join(&imported.content_digest[2..]);
    fs::write(&stored, b"tampered").unwrap();
    assert!(
        !gate
            .library
            .verify_object(&gate.store, &imported.content_digest, 200)
            .unwrap(),
        "corruption is detected, never reported as healthy"
    );

    // Vanished origin: content survives, provenance does not.
    fs::remove_file(&source).unwrap();
    let report = gate
        .library
        .scan_folder(&gate.store, &gate.incoming, 300)
        .unwrap();
    assert!(report.new_candidates.is_empty());

    // A malformed archive leaves nothing behind.
    let broken = gate.incoming.join("Broken.zip");
    fs::write(&broken, b"PK\x03\x04 nonsense").unwrap();
    let before = gate.store.owned_object_count().unwrap();
    assert!(
        gate.library
            .import_archive(&gate.store, &broken, 400)
            .is_err()
    );
    assert_eq!(gate.store.owned_object_count().unwrap(), before);
}

#[test]
fn deduplicated_content_is_reproducible_from_either_origin() {
    let gate = gate();
    let first = gate.incoming.join("A.nes");
    let second = gate.incoming.join("B.nes");
    fs::write(&first, ROM).unwrap();
    fs::write(&second, ROM).unwrap();

    let one = gate.library.import_file(&gate.store, &first, 100).unwrap();
    gate.library.import_file(&gate.store, &second, 101).unwrap();

    assert_eq!(gate.store.owned_object_count().unwrap(), 1);
    assert_eq!(
        gate.store
            .origin_observations(&one.content_digest)
            .unwrap()
            .len(),
        2
    );

    // Removing one origin leaves the content perfectly readable.
    fs::remove_file(&first).unwrap();
    assert_eq!(gate.library.read_object(&one.content_digest).unwrap(), ROM);
    assert_eq!(sha256(ROM), one.content_digest);
}
