//! Coverage for Library removal (issue #64).
//!
//! Three separate acts that are never conflated: forgetting provenance,
//! removing managed bytes, and deleting an identity. Nothing silently cascades.

mod common;

use std::{fs, path::PathBuf};

use rom_manager::{DeletionBlocked, Library, Store};

struct Fixture {
    _directory: tempfile::TempDir,
    library: Library,
    store: Store,
    incoming: PathBuf,
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let incoming = directory.path().join("incoming");
    fs::create_dir_all(&incoming).unwrap();
    Fixture {
        library: Library::open(directory.path().join("library")).unwrap(),
        store: Store::open(&directory.path().join("library.sqlite3")).unwrap(),
        incoming,
        _directory: directory,
    }
}

fn import(fixture: &Fixture, name: &str, bytes: &[u8]) -> String {
    let path = fixture.incoming.join(name);
    fs::write(&path, bytes).unwrap();
    fixture
        .library
        .import_file(&fixture.store, &path, 100)
        .unwrap()
        .content_digest
}

/// An imported ROM wired to a ROM Set identity.
fn imported_set(fixture: &Fixture) -> String {
    let digest = import(fixture, "Tracers.nes", b"rom bytes");
    fixture
        .store
        .upsert_rom_set(
            ("game-a", "NES", "Tracers"),
            ("release-a", "USA"),
            ("rom-set-a", &digest, "Tracers.nes", 9),
        )
        .unwrap();
    digest
}

#[test]
fn forgetting_provenance_never_touches_content() {
    let fixture = fixture();
    let digest = import(&fixture, "Tracers.nes", b"rom bytes");
    let folder = fixture
        .store
        .remember_import_folder(&fixture.incoming.to_string_lossy(), None)
        .unwrap();

    fixture.store.forget_import_folder(folder).unwrap();

    assert_eq!(
        fixture.library.read_object(&digest).unwrap(),
        b"rom bytes",
        "forgetting where content came from is not removing it"
    );
}

#[test]
fn an_impact_preview_names_what_would_become_unavailable() {
    let fixture = fixture();
    let digest = imported_set(&fixture);
    fixture
        .store
        .record_pack_selection("pack-1", 1, &[("rom-set-a", &digest)])
        .unwrap();

    let impact = fixture
        .library
        .removal_impact(&fixture.store, &digest)
        .unwrap();

    assert_eq!(impact.rom_sets_becoming_unavailable, vec!["rom-set-a"]);
    assert_eq!(impact.rom_packs_becoming_unsyncable, vec!["pack-1"]);
    assert!(impact.requires_confirmation());
}

#[test]
fn removal_is_refused_without_confirmation() {
    let fixture = fixture();
    let digest = imported_set(&fixture);

    assert!(matches!(
        fixture
            .library
            .remove_managed_object(&fixture.store, &digest, false),
        Err(DeletionBlocked::NotConfirmed)
    ));
    assert_eq!(
        fixture.library.read_object(&digest).unwrap(),
        b"rom bytes",
        "a refused removal removes nothing"
    );
}

#[test]
fn a_confirmed_removal_retains_the_identity() {
    // The ROM Set still exists; it is unavailable, not deleted. Reimporting the
    // same content makes it whole again without rebuilding anything.
    let fixture = fixture();
    let digest = imported_set(&fixture);

    fixture
        .library
        .remove_managed_object(&fixture.store, &digest, true)
        .unwrap();

    assert!(
        fixture.library.read_object(&digest).is_err(),
        "bytes are gone"
    );
    assert!(
        fixture.store.rom_set_exists("rom-set-a").unwrap(),
        "the identity is retained"
    );
    assert!(
        !fixture
            .library
            .rom_is_available(&fixture.store, &digest)
            .unwrap(),
        "and is now unavailable"
    );
}

#[test]
fn removing_a_redundant_copy_needs_no_confirmation() {
    // The same ROM is also inside an archive, so the loose copy is not
    // load-bearing and removing it costs nothing.
    use std::io::Write;

    let fixture = fixture();
    let rom = b"rom bytes".to_vec();
    let digest = import(&fixture, "Tracers.nes", &rom);

    let archive = fixture.incoming.join("Pack.zip");
    {
        let file = fs::File::create(&archive).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        zip.start_file("Tracers.nes", options).unwrap();
        zip.write_all(&rom).unwrap();
        zip.finish().unwrap();
    }
    fixture
        .library
        .import_archive(&fixture.store, &archive, 101)
        .unwrap();

    let impact = fixture
        .library
        .removal_impact(&fixture.store, &digest)
        .unwrap();
    assert!(impact.still_reproducible_elsewhere);
    assert!(
        !impact.requires_confirmation(),
        "removing a copy that is reproducible elsewhere costs nothing"
    );
}

#[test]
fn deleting_an_identity_is_blocked_while_a_rom_pack_selects_it() {
    // A selection is a promise the user made; only the user withdraws it.
    let fixture = fixture();
    let digest = imported_set(&fixture);
    fixture
        .store
        .record_pack_selection("pack-1", 1, &[("rom-set-a", &digest)])
        .unwrap();

    assert!(
        fixture.store.delete_rom_set("rom-set-a").is_err(),
        "deletion must not cascade through a live selection"
    );
    assert!(fixture.store.rom_set_exists("rom-set-a").unwrap());
}

#[test]
fn an_unselected_identity_can_be_deleted() {
    let fixture = fixture();
    imported_set(&fixture);

    fixture.store.delete_rom_set("rom-set-a").unwrap();
    assert!(!fixture.store.rom_set_exists("rom-set-a").unwrap());
}

#[test]
fn clearing_the_cache_is_not_a_removal() {
    // Cache cleanup is a separate, nondestructive action.
    use rom_manager::{MaterializationCache, sha256};

    let fixture = fixture();
    let digest = import(&fixture, "Tracers.nes", b"rom bytes");
    let cache =
        MaterializationCache::open(fixture._directory.path().join("cache"), 1 << 20).unwrap();
    let bytes = b"rom bytes".to_vec();
    cache.put(&sha256(&bytes), &bytes).unwrap();

    cache.clear().unwrap();

    assert_eq!(
        fixture.library.read_object(&digest).unwrap(),
        b"rom bytes",
        "clearing derived data never removes owned content"
    );
    assert!(
        fixture
            .library
            .rom_is_available(&fixture.store, &digest)
            .unwrap()
    );
}
