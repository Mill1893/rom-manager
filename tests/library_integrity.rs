//! Coverage for integrity, quarantine, and recovery (issue #61).
//!
//! The rule with teeth: a byte mismatch is **corruption, never an update**. The
//! recorded digest is what an object *is*, so bytes that disagree are the thing
//! that is wrong — the record is never rewritten to match the disk.

mod common;

use std::{fs, path::PathBuf};

use rom_manager::{ImportError, Library, Store, sha256};

const ROM: &[u8] = include_bytes!("../fixtures/nes/tracers.nes");

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

/// The object's file inside app-owned storage.
fn stored_path(fixture: &Fixture, digest: &str) -> PathBuf {
    fixture
        .root
        .join("objects")
        .join(&digest[..2])
        .join(&digest[2..])
}

#[test]
fn a_healthy_object_verifies() {
    let fixture = fixture();
    let digest = import(&fixture, "Tracers.nes", ROM);

    assert!(
        fixture
            .library
            .verify_object(&fixture.store, &digest, 200)
            .unwrap()
    );
    assert!(fixture.store.object_is_healthy(&digest).unwrap());
}

#[test]
fn changed_bytes_are_corruption_not_an_update() {
    let fixture = fixture();
    let digest = import(&fixture, "Tracers.nes", ROM);

    fs::write(stored_path(&fixture, &digest), b"something else entirely").unwrap();

    assert!(
        !fixture
            .library
            .verify_object(&fixture.store, &digest, 200)
            .unwrap()
    );
    assert!(
        !fixture.store.object_is_healthy(&digest).unwrap(),
        "the object is unhealthy"
    );

    // The record still says what the object is. It was NOT rewritten to match
    // whatever is now on disk.
    let (_, _, health) = fixture.store.source_object(&digest).unwrap().unwrap();
    assert_eq!(health, "quarantined");
}

#[test]
fn unexpected_bytes_are_moved_aside_not_deleted() {
    // They are evidence, and they may be the user's only copy of something.
    let fixture = fixture();
    let digest = import(&fixture, "Tracers.nes", ROM);
    fs::write(stored_path(&fixture, &digest), b"unexpected").unwrap();

    fixture
        .library
        .verify_object(&fixture.store, &digest, 200)
        .unwrap();

    let quarantined = fixture.root.join("quarantine").join(&digest);
    assert!(quarantined.exists(), "the unexpected bytes are kept");
    assert_eq!(fs::read(quarantined).unwrap(), b"unexpected");
}

#[test]
fn a_full_check_reports_healthy_corrupt_and_missing_separately() {
    let fixture = fixture();
    let good = import(&fixture, "Good.nes", b"good bytes");
    let corrupt = import(&fixture, "Corrupt.nes", b"corrupt bytes");
    let gone = import(&fixture, "Gone.nes", b"gone bytes");

    fs::write(stored_path(&fixture, &corrupt), b"tampered").unwrap();
    fs::remove_file(stored_path(&fixture, &gone)).unwrap();

    let report = fixture.library.verify_all(&fixture.store, 200).unwrap();

    assert!(!report.is_clean());
    assert_eq!(report.verified, vec![good]);
    assert_eq!(report.quarantined, vec![corrupt]);
    assert_eq!(
        report.missing,
        vec![gone],
        "content that vanished is a different problem from content that changed"
    );
}

#[test]
fn recovery_requires_an_exact_match() {
    let fixture = fixture();
    let digest = import(&fixture, "Tracers.nes", ROM);
    fs::write(stored_path(&fixture, &digest), b"tampered").unwrap();
    fixture
        .library
        .verify_object(&fixture.store, &digest, 200)
        .unwrap();

    // Something merely similar is refused — accepting it would be
    // indistinguishable from accepting the corruption.
    let wrong = fixture.incoming.join("Nearly.nes");
    let mut nearly = ROM.to_vec();
    nearly[0] ^= 0xff;
    fs::write(&wrong, &nearly).unwrap();
    assert!(matches!(
        fixture
            .library
            .recover_object(&fixture.store, &digest, &wrong, 300),
        Err(ImportError::RecoveryMismatch)
    ));
    assert!(!fixture.store.object_is_healthy(&digest).unwrap());

    // The exact bytes restore it.
    let right = fixture.incoming.join("Exact.nes");
    fs::write(&right, ROM).unwrap();
    fixture
        .library
        .recover_object(&fixture.store, &digest, &right, 300)
        .unwrap();

    assert!(fixture.store.object_is_healthy(&digest).unwrap());
    assert_eq!(fixture.library.read_object(&digest).unwrap(), ROM);
    assert_eq!(
        sha256(&fixture.library.read_object(&digest).unwrap()),
        digest
    );
}

#[test]
fn a_clean_library_reports_clean() {
    let fixture = fixture();
    import(&fixture, "One.nes", b"one");
    import(&fixture, "Two.nes", b"two");

    let report = fixture.library.verify_all(&fixture.store, 200).unwrap();
    assert!(report.is_clean());
    assert_eq!(report.verified.len(), 2);
}
