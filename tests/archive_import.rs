//! Coverage for importing archives as Source Containers (issue #58).

mod common;

use std::{fs, io::Write, path::PathBuf};

use rom_manager::{ImportError, Library, Store, sha256};

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

/// Writes a ZIP holding `members`, optionally with a stored comment so two
/// archives with identical members still differ byte-for-byte.
fn write_zip(path: &PathBuf, members: &[(&str, &[u8])], comment: &str) {
    let file = fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in members {
        zip.start_file(*name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.set_comment(comment);
    zip.finish().unwrap();
}

#[test]
fn an_archive_is_preserved_byte_for_byte() {
    let fixture = fixture();
    let archive = fixture.incoming.join("Tracers.zip");
    write_zip(&archive, &[("Tracers.nes", ROM)], "");
    let original = fs::read(&archive).unwrap();

    let container = fixture
        .library
        .import_archive(&fixture.store, &archive, 100)
        .unwrap();

    assert_eq!(
        fixture
            .library
            .read_object(&container.content_digest)
            .unwrap(),
        original,
        "the archive itself is what was imported, unchanged"
    );
}

#[test]
fn members_are_enumerated_without_being_stored() {
    let fixture = fixture();
    let archive = fixture.incoming.join("Tracers.zip");
    write_zip(
        &archive,
        &[("Tracers.nes", ROM), ("readme.txt", b"hello")],
        "",
    );

    let container = fixture
        .library
        .import_archive(&fixture.store, &archive, 100)
        .unwrap();

    assert_eq!(container.members.len(), 2);
    assert_eq!(
        fixture
            .store
            .container_members(&container.content_digest)
            .unwrap()
            .len(),
        2
    );
    // Only the archive is owned content. Member bytes are derived.
    assert_eq!(
        fixture.store.owned_object_count().unwrap(),
        1,
        "extracted members must not become separate stored objects"
    );
}

#[test]
fn a_member_materializes_and_is_verified() {
    let fixture = fixture();
    let archive = fixture.incoming.join("Tracers.zip");
    write_zip(&archive, &[("Tracers.nes", ROM)], "");

    let container = fixture
        .library
        .import_archive(&fixture.store, &archive, 100)
        .unwrap();

    let bytes = fixture
        .library
        .materialize_member(&container.content_digest, "Tracers.nes", &sha256(ROM))
        .unwrap();
    assert_eq!(bytes, ROM);

    // A materialization that does not reproduce the recorded identity is not
    // the ROM that was imported.
    assert!(matches!(
        fixture.library.materialize_member(
            &container.content_digest,
            "Tracers.nes",
            &"0".repeat(64)
        ),
        Err(ImportError::MaterializationMismatch)
    ));
}

#[test]
fn differently_packaged_sources_stay_distinct_containers() {
    // Same ROM, two archives. What was imported is the archive, so these are
    // two Source Containers even though their ROM content is identical.
    let fixture = fixture();
    let first = fixture.incoming.join("Tracers (A).zip");
    let second = fixture.incoming.join("Tracers (B).zip");
    write_zip(&first, &[("Tracers.nes", ROM)], "packaged by A");
    write_zip(&second, &[("Tracers.nes", ROM)], "packaged by B");

    let one = fixture
        .library
        .import_archive(&fixture.store, &first, 100)
        .unwrap();
    let two = fixture
        .library
        .import_archive(&fixture.store, &second, 101)
        .unwrap();

    assert_ne!(
        one.content_digest, two.content_digest,
        "different packaging is different content"
    );
    assert_eq!(fixture.store.owned_object_count().unwrap(), 2);

    // But the ROM inside is recognisably the same, and both containers are
    // known to hold it.
    let rom_digest = sha256(ROM);
    assert_eq!(
        fixture.store.containers_holding(&rom_digest).unwrap().len(),
        2,
        "content identity spans containers even though the containers differ"
    );
}

#[test]
fn byte_identical_archives_still_deduplicate() {
    let fixture = fixture();
    let first = fixture.incoming.join("Tracers.zip");
    let second = fixture.incoming.join("Tracers (copy).zip");
    write_zip(&first, &[("Tracers.nes", ROM)], "same");
    fs::copy(&first, &second).unwrap();

    let one = fixture
        .library
        .import_archive(&fixture.store, &first, 100)
        .unwrap();
    let two = fixture
        .library
        .import_archive(&fixture.store, &second, 101)
        .unwrap();

    assert_eq!(one.content_digest, two.content_digest);
    assert_eq!(fixture.store.owned_object_count().unwrap(), 1);
}

#[test]
fn a_malformed_archive_is_reported_not_imported() {
    // An archive this application cannot read is not a ROM it can claim to
    // hold. It must not become opaque complete content.
    let fixture = fixture();
    let broken = fixture.incoming.join("Broken.zip");
    fs::write(&broken, b"PK\x03\x04 and then nonsense").unwrap();

    assert!(matches!(
        fixture.library.import_archive(&fixture.store, &broken, 100),
        Err(ImportError::Archive(_))
    ));
    assert_eq!(
        fixture.store.owned_object_count().unwrap(),
        0,
        "a container that could not be read must leave nothing behind"
    );
}

#[test]
fn a_member_escaping_the_archive_root_is_refused() {
    let fixture = fixture();
    let hostile = fixture.incoming.join("Hostile.zip");
    write_zip(&hostile, &[("../escape.nes", ROM)], "");

    assert!(
        matches!(
            fixture
                .library
                .import_archive(&fixture.store, &hostile, 100),
            Err(ImportError::UnsafeMember(_))
        ),
        "an escaping member name is refused, never sanitized"
    );
    assert_eq!(fixture.store.owned_object_count().unwrap(), 0);
}
