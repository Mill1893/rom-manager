//! Coverage for completeness versus availability (issue #63).
//!
//! Three states, never collapsed into one "not ready": a user finds a missing
//! ROM, reconnects a drive, or syncs — different problems, different actions.

mod common;

use std::{fs, path::PathBuf};

use rom_manager::{Library, MaterializationCache, SetState, Store, sha256};

struct Fixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
    library: Library,
    store: Store,
    incoming: PathBuf,
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("library");
    let incoming = directory.path().join("incoming");
    fs::create_dir_all(&incoming).unwrap();
    Fixture {
        library: Library::open(&root).unwrap(),
        store: Store::open(&directory.path().join("library.sqlite3")).unwrap(),
        root,
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

#[test]
fn a_whole_reproducible_set_is_available() {
    let fixture = fixture();
    let a = import(&fixture, "A.nes", b"a");
    let b = import(&fixture, "B.nes", b"b");

    let availability = fixture
        .library
        .set_availability(&fixture.store, &[a.clone(), b.clone()], &[a, b], &[])
        .unwrap();

    assert_eq!(availability.state, SetState::Available);
    assert!(availability.is_syncable());
}

#[test]
fn a_set_missing_a_member_is_incomplete_not_unavailable() {
    // Structure is judged first: a set that is not whole cannot be assessed on
    // whether its parts are reproducible.
    let fixture = fixture();
    let present = import(&fixture, "A.nes", b"a");
    let never_imported = sha256(b"the missing one");

    let availability = fixture
        .library
        .set_availability(
            &fixture.store,
            &[present.clone(), never_imported.clone()],
            &[present],
            &[],
        )
        .unwrap();

    assert_eq!(availability.state, SetState::Incomplete);
    assert_eq!(availability.missing_members, vec![never_imported]);
    assert!(
        availability.unreproducible_members.is_empty(),
        "reproducibility is not the question when membership is missing"
    );
}

#[test]
fn a_whole_set_whose_content_cannot_be_produced_is_unavailable() {
    let fixture = fixture();
    let a = import(&fixture, "A.nes", b"a");
    let b = import(&fixture, "B.nes", b"b");

    // B's bytes are damaged and quarantined.
    fs::write(
        fixture.root.join("objects").join(&b[..2]).join(&b[2..]),
        b"tampered",
    )
    .unwrap();
    fixture
        .library
        .verify_object(&fixture.store, &b, 200)
        .unwrap();

    let availability = fixture
        .library
        .set_availability(
            &fixture.store,
            &[a.clone(), b.clone()],
            &[a, b.clone()],
            &[],
        )
        .unwrap();

    assert_eq!(availability.state, SetState::Unavailable);
    assert!(availability.missing_members.is_empty(), "the set is whole");
    assert_eq!(availability.unreproducible_members, vec![b]);
}

#[test]
fn an_unavailable_dependency_makes_the_whole_closure_unavailable() {
    let fixture = fixture();
    let member = import(&fixture, "Game.nes", b"game");
    let dependency = sha256(b"a bios that was never imported");

    let availability = fixture
        .library
        .set_availability(
            &fixture.store,
            std::slice::from_ref(&member),
            std::slice::from_ref(&member),
            std::slice::from_ref(&dependency),
        )
        .unwrap();

    assert_eq!(availability.state, SetState::Unavailable);
    assert_eq!(availability.unreproducible_members, vec![dependency]);
}

#[test]
fn the_cache_alone_never_establishes_availability() {
    // The decisive test. If a cached copy could make content available, then
    // clearing the cache could make it unavailable — and clearing would stop
    // being safe.
    let fixture = fixture();
    let bytes = b"only in the cache".to_vec();
    let digest = sha256(&bytes);

    let cache = MaterializationCache::open(fixture.root.join("cache"), 1 << 20).unwrap();
    cache.put(&digest, &bytes).unwrap();
    assert!(cache.get(&digest).unwrap().is_some(), "it is cached");

    assert!(
        !fixture
            .library
            .rom_is_available(&fixture.store, &digest)
            .unwrap(),
        "a cached copy is evidence of nothing"
    );
}

#[test]
fn content_inside_a_healthy_container_is_available() {
    // Availability means reproducible, not stored loose.
    use std::io::Write;

    let fixture = fixture();
    let rom = b"rom in a zip".to_vec();
    let archive_path = fixture.incoming.join("Pack.zip");
    {
        let file = fs::File::create(&archive_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        zip.start_file("Game.nes", options).unwrap();
        zip.write_all(&rom).unwrap();
        zip.finish().unwrap();
    }
    fixture
        .library
        .import_archive(&fixture.store, &archive_path, 100)
        .unwrap();

    assert!(
        fixture
            .library
            .rom_is_available(&fixture.store, &sha256(&rom))
            .unwrap(),
        "a ROM reproducible from a healthy container is available"
    );
}

#[test]
fn quarantined_content_is_not_available() {
    let fixture = fixture();
    let digest = import(&fixture, "A.nes", b"a");
    assert!(
        fixture
            .library
            .rom_is_available(&fixture.store, &digest)
            .unwrap()
    );

    fixture.store.quarantine_object(&digest).unwrap();
    assert!(
        !fixture
            .library
            .rom_is_available(&fixture.store, &digest)
            .unwrap(),
        "content the application does not trust cannot be reproduced from"
    );
}
