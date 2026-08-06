//! Coverage for importing into app-owned Library storage (issue #57).

mod common;

use std::{fs, path::PathBuf};

use rom_manager::{ImportError, Library, Store};

const ROM: &[u8] = include_bytes!("../fixtures/nes/tracers.nes");

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

fn place(fixture: &Fixture, name: &str, bytes: &[u8]) -> PathBuf {
    let path = fixture.incoming.join(name);
    fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn an_imported_rom_outlives_its_source() {
    // The whole point of app-owned storage. Import, then destroy every trace of
    // the original, and the content must still be readable.
    let fixture = fixture();
    let source = place(&fixture, "Tracers.nes", ROM);

    let imported = fixture
        .library
        .import_file(&fixture.store, &source, 100)
        .unwrap();
    assert!(imported.stored_new_object);

    fs::remove_file(&source).unwrap();
    fs::remove_dir_all(&fixture.incoming).unwrap();

    assert_eq!(
        fixture
            .library
            .read_object(&imported.content_digest)
            .unwrap(),
        ROM,
        "content must survive the disappearance of everything it came from"
    );
}

#[test]
fn the_source_is_left_untouched() {
    let fixture = fixture();
    let source = place(&fixture, "Tracers.nes", ROM);

    fixture
        .library
        .import_file(&fixture.store, &source, 100)
        .unwrap();

    assert_eq!(
        fs::read(&source).unwrap(),
        ROM,
        "import copies; it never moves or rewrites the user's file"
    );
}

#[test]
fn the_external_path_is_recorded_as_provenance_only() {
    let fixture = fixture();
    let source = place(&fixture, "Tracers.nes", ROM);

    let imported = fixture
        .library
        .import_file(&fixture.store, &source, 100)
        .unwrap();

    let observations = fixture
        .store
        .origin_observations(&imported.content_digest)
        .unwrap();
    assert_eq!(observations.len(), 1);
    assert!(observations[0].ends_with("Tracers.nes"));

    // The object is recorded by content, and its storage path is inside the
    // Library — never the path it came from.
    let (size, stored, health) = fixture
        .store
        .source_object(&imported.content_digest)
        .unwrap()
        .expect("the object is owned");
    assert_eq!(size, ROM.len() as u64);
    assert!(stored.starts_with("objects/"));
    assert_eq!(health, "healthy");
}

#[test]
fn identical_bytes_from_a_second_path_add_provenance_not_content() {
    let fixture = fixture();
    let first = place(&fixture, "Tracers.nes", ROM);
    let second = place(&fixture, "Tracers (copy).nes", ROM);

    let one = fixture
        .library
        .import_file(&fixture.store, &first, 100)
        .unwrap();
    let two = fixture
        .library
        .import_file(&fixture.store, &second, 101)
        .unwrap();

    assert_eq!(one.content_digest, two.content_digest);
    assert!(one.stored_new_object);
    assert!(
        !two.stored_new_object,
        "the second import must not store the same bytes twice"
    );
    assert_eq!(fixture.store.owned_object_count().unwrap(), 1);
    assert_eq!(
        fixture
            .store
            .origin_observations(&one.content_digest)
            .unwrap()
            .len(),
        2,
        "both places the bytes were seen are remembered"
    );
}

#[test]
fn re_importing_the_same_path_is_idempotent() {
    let fixture = fixture();
    let source = place(&fixture, "Tracers.nes", ROM);

    let first = fixture
        .library
        .import_file(&fixture.store, &source, 100)
        .unwrap();
    fixture
        .library
        .import_file(&fixture.store, &source, 200)
        .unwrap();

    assert_eq!(fixture.store.owned_object_count().unwrap(), 1);
    assert_eq!(
        fixture
            .store
            .origin_observations(&first.content_digest)
            .unwrap()
            .len(),
        1,
        "the same bytes at the same path is one observation, not two"
    );
}

#[test]
fn one_failed_candidate_does_not_roll_back_the_others() {
    // The normal outcome of scanning a folder where one file is unreadable.
    let fixture = fixture();
    let good_one = place(&fixture, "Alpha.nes", b"alpha bytes");
    let missing = fixture.incoming.join("Vanished.nes");
    let good_two = place(&fixture, "Beta.nes", b"beta bytes");

    let results = fixture
        .library
        .import_all(&fixture.store, &[good_one, missing, good_two], 100);

    assert!(results[0].is_ok());
    assert!(
        matches!(results[1], Err(ImportError::Source(_))),
        "the unreadable candidate fails on its own"
    );
    assert!(results[2].is_ok());
    assert_eq!(
        fixture.store.owned_object_count().unwrap(),
        2,
        "everything that worked is kept"
    );
}

#[test]
fn distinct_content_yields_distinct_objects() {
    let fixture = fixture();
    let alpha = place(&fixture, "Alpha.nes", b"alpha bytes");
    let beta = place(&fixture, "Beta.nes", b"beta bytes");

    let one = fixture
        .library
        .import_file(&fixture.store, &alpha, 100)
        .unwrap();
    let two = fixture
        .library
        .import_file(&fixture.store, &beta, 100)
        .unwrap();

    assert_ne!(one.content_digest, two.content_digest);
    assert_eq!(fixture.store.owned_object_count().unwrap(), 2);
    assert_eq!(
        fixture.library.read_object(&one.content_digest).unwrap(),
        b"alpha bytes"
    );
    assert_eq!(
        fixture.library.read_object(&two.content_digest).unwrap(),
        b"beta bytes"
    );
}

#[test]
fn nothing_is_left_in_staging_after_an_import() {
    // Staging is scratch space. A completed import must leave no debris for a
    // later integrity check to have to reason about.
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("library");
    let library = Library::open(&root).unwrap();
    let store = Store::open(&directory.path().join("library.sqlite3")).unwrap();

    let source = directory.path().join("Tracers.nes");
    fs::write(&source, ROM).unwrap();
    library.import_file(&store, &source, 100).unwrap();

    let leftovers: Vec<_> = fs::read_dir(root.join("staging")).unwrap().collect();
    assert!(leftovers.is_empty(), "staging must be empty after a commit");
}

#[test]
fn a_shared_object_stays_while_any_identity_needs_it() {
    // Deduplication collapses two imports into one object. That must never mean
    // removing one identity takes the other's content with it.
    let fixture = fixture();
    let first = place(&fixture, "Tracers.nes", ROM);
    let second = place(&fixture, "Tracers (copy).nes", ROM);

    let imported = fixture
        .library
        .import_file(&fixture.store, &first, 100)
        .unwrap();
    fixture
        .library
        .import_file(&fixture.store, &second, 101)
        .unwrap();

    assert!(
        !fixture
            .store
            .object_is_still_needed(&imported.content_digest)
            .unwrap(),
        "nothing references it yet"
    );

    // Two Library identities now share the same bytes.
    for (game, release, rom_set) in [
        ("game-a", "release-a", "rom-set-a"),
        ("game-b", "release-b", "rom-set-b"),
    ] {
        fixture
            .store
            .upsert_rom_set(
                (game, "NES", "Tracers"),
                (release, "USA"),
                (
                    rom_set,
                    &imported.content_digest,
                    "Tracers.nes",
                    imported.size,
                ),
            )
            .unwrap();
    }

    assert_eq!(
        fixture
            .store
            .object_reference_count(&imported.content_digest)
            .unwrap(),
        2,
        "both identities are counted"
    );
    assert!(
        fixture
            .store
            .object_is_still_needed(&imported.content_digest)
            .unwrap()
    );
}

#[test]
fn deduplication_never_changes_what_a_rom_pack_selected() {
    // A pack selects content by digest. Importing the same bytes again from
    // somewhere else adds provenance and must leave the selection identical.
    let fixture = fixture();
    let first = place(&fixture, "Tracers.nes", ROM);

    let imported = fixture
        .library
        .import_file(&fixture.store, &first, 100)
        .unwrap();
    fixture
        .store
        .upsert_rom_set(
            ("game-a", "NES", "Tracers"),
            ("release-a", "USA"),
            (
                "rom-set-a",
                &imported.content_digest,
                "Tracers.nes",
                imported.size,
            ),
        )
        .unwrap();
    fixture
        .store
        .record_pack_selection("pack-1", 1, &[("rom-set-a", &imported.content_digest)])
        .unwrap();
    let before = fixture.store.pack_selection("pack-1", 1).unwrap();

    let second = place(&fixture, "Elsewhere.nes", ROM);
    fixture
        .library
        .import_file(&fixture.store, &second, 200)
        .unwrap();

    assert_eq!(
        fixture.store.pack_selection("pack-1", 1).unwrap(),
        before,
        "an exact selection must survive a duplicate import untouched"
    );
}
