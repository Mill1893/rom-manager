//! Coverage for Import Folders and changed-origin reconciliation (issue #62).
//!
//! The invariant throughout: a scan reconciles **provenance** and never mutates
//! Library content.

mod common;

use std::{fs, path::PathBuf};

#[cfg(unix)]
use rom_manager::Skipped;
use rom_manager::{Library, Store};

struct Fixture {
    _directory: tempfile::TempDir,
    library: Library,
    store: Store,
    folder: PathBuf,
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().join("roms");
    fs::create_dir_all(&folder).unwrap();
    Fixture {
        library: Library::open(directory.path().join("library")).unwrap(),
        store: Store::open(&directory.path().join("library.sqlite3")).unwrap(),
        folder,
        _directory: directory,
    }
}

#[test]
fn remembering_a_folder_does_not_scan_it() {
    // The application never walks the user's disks on its own schedule.
    let fixture = fixture();
    fs::write(fixture.folder.join("Tracers.nes"), b"rom").unwrap();

    let id = fixture
        .store
        .remember_import_folder(&fixture.folder.to_string_lossy(), Some("NES"))
        .unwrap();

    assert_eq!(fixture.store.import_folders().unwrap().len(), 1);
    assert_eq!(
        fixture.store.owned_object_count().unwrap(),
        0,
        "remembering is not importing"
    );
    let _ = id;
}

#[test]
fn a_scan_finds_new_candidates_without_importing_them() {
    let fixture = fixture();
    fs::write(fixture.folder.join("Alpha.nes"), b"alpha").unwrap();
    fs::create_dir_all(fixture.folder.join("nested")).unwrap();
    fs::write(fixture.folder.join("nested/Beta.nes"), b"beta").unwrap();

    let report = fixture
        .library
        .scan_folder(&fixture.store, &fixture.folder, 100)
        .unwrap();

    assert_eq!(report.new_candidates.len(), 2, "recurses into subfolders");
    assert_eq!(
        fixture.store.owned_object_count().unwrap(),
        0,
        "a scan discovers; it does not import"
    );
}

/// Unix-only: creating a symbolic link on Windows needs elevation or Developer
/// Mode, which CI runners do not reliably have. The rule is enforced on both
/// platforms; only this way of *planting* the indirection is Unix-specific.
#[cfg(unix)]
#[test]
fn a_scan_never_follows_an_indirection() {
    // A scan must not be steerable outside the folder the user pointed at.
    let fixture = fixture();
    let outside = fixture._directory.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("Secret.nes"), b"not yours").unwrap();
    std::os::unix::fs::symlink(&outside, fixture.folder.join("escape")).unwrap();
    fs::write(fixture.folder.join("Real.nes"), b"real").unwrap();

    let report = fixture
        .library
        .scan_folder(&fixture.store, &fixture.folder, 100)
        .unwrap();

    assert_eq!(report.new_candidates.len(), 1, "only the real file");
    assert!(
        report
            .skipped
            .iter()
            .any(|skip| matches!(skip, Skipped::Indirection(path) if path.ends_with("escape"))),
        "the indirection is reported, not silently ignored"
    );
}

#[test]
fn a_moved_input_is_matched_by_content_not_path() {
    let fixture = fixture();
    let original = fixture.folder.join("Tracers.nes");
    fs::write(&original, b"rom bytes").unwrap();
    let imported = fixture
        .library
        .import_file(&fixture.store, &original, 100)
        .unwrap();

    // The user reorganises: same bytes, different place.
    fs::create_dir_all(fixture.folder.join("NES")).unwrap();
    let moved = fixture.folder.join("NES/Tracers.nes");
    fs::rename(&original, &moved).unwrap();

    let report = fixture
        .library
        .scan_folder(&fixture.store, &fixture.folder, 200)
        .unwrap();

    assert!(
        report.new_candidates.is_empty(),
        "already-owned content is recognised wherever it moved to"
    );
    assert!(report.unchanged.contains(&moved));
    assert!(
        fixture
            .store
            .origin_observations(&imported.content_digest)
            .unwrap()
            .iter()
            // Separator-agnostic: an Origin Observation records a *host*
            // path, so it carries backslashes on Windows. That is correct —
            // provenance points at the user's filesystem, not at a target.
            .any(|path| {
                let normalized = path.replace('\\', "/");
                normalized.ends_with("NES/Tracers.nes")
            }),
        "the new location is remembered"
    );
    assert_eq!(fixture.store.owned_object_count().unwrap(), 1);
}

#[test]
fn changed_bytes_at_a_known_path_become_a_new_candidate() {
    // The existing object is immutable, so this is never an update to it.
    let fixture = fixture();
    let path = fixture.folder.join("Tracers.nes");
    fs::write(&path, b"original bytes").unwrap();
    let imported = fixture
        .library
        .import_file(&fixture.store, &path, 100)
        .unwrap();

    fs::write(&path, b"different bytes entirely").unwrap();
    let report = fixture
        .library
        .scan_folder(&fixture.store, &fixture.folder, 200)
        .unwrap();

    assert!(report.changed.contains(&path));
    assert_eq!(
        fixture
            .library
            .read_object(&imported.content_digest)
            .unwrap(),
        b"original bytes",
        "Library content is untouched by what happened at the origin"
    );
    assert!(
        fixture
            .store
            .available_observations(&imported.content_digest)
            .unwrap()
            .is_empty(),
        "the old observation is no longer findable there"
    );
}

#[test]
fn a_vanished_input_becomes_unavailable_without_losing_content() {
    let fixture = fixture();
    let path = fixture.folder.join("Tracers.nes");
    fs::write(&path, b"rom bytes").unwrap();
    let imported = fixture
        .library
        .import_file(&fixture.store, &path, 100)
        .unwrap();

    fs::remove_file(&path).unwrap();
    let report = fixture
        .library
        .scan_folder(&fixture.store, &fixture.folder, 200)
        .unwrap();

    assert_eq!(report.now_unavailable.len(), 1);
    assert!(
        fixture
            .store
            .available_observations(&imported.content_digest)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        fixture
            .library
            .read_object(&imported.content_digest)
            .unwrap(),
        b"rom bytes",
        "losing the origin never costs the content"
    );
}

#[test]
fn relinking_a_folder_affects_only_future_discovery() {
    let fixture = fixture();
    let path = fixture.folder.join("Tracers.nes");
    fs::write(&path, b"rom bytes").unwrap();
    let imported = fixture
        .library
        .import_file(&fixture.store, &path, 100)
        .unwrap();

    let id = fixture
        .store
        .remember_import_folder(&fixture.folder.to_string_lossy(), None)
        .unwrap();
    let elsewhere = fixture._directory.path().join("moved-library");
    fs::create_dir_all(&elsewhere).unwrap();
    fixture
        .store
        .relink_import_folder(id, &elsewhere.to_string_lossy())
        .unwrap();

    assert_eq!(
        fixture
            .library
            .read_object(&imported.content_digest)
            .unwrap(),
        b"rom bytes",
        "content came from the bytes, not from the folder"
    );
    assert_eq!(fixture.store.owned_object_count().unwrap(), 1);
}

#[test]
fn forgetting_a_folder_never_affects_library_content() {
    let fixture = fixture();
    let path = fixture.folder.join("Tracers.nes");
    fs::write(&path, b"rom bytes").unwrap();
    let imported = fixture
        .library
        .import_file(&fixture.store, &path, 100)
        .unwrap();
    let id = fixture
        .store
        .remember_import_folder(&fixture.folder.to_string_lossy(), None)
        .unwrap();

    fixture.store.forget_import_folder(id).unwrap();

    assert!(fixture.store.import_folders().unwrap().is_empty());
    assert_eq!(
        fixture
            .library
            .read_object(&imported.content_digest)
            .unwrap(),
        b"rom bytes"
    );
}
