//! Gating coverage for filesystem confinement under mutation (issue #43).
//!
//! These plant a *real* indirection on a real filesystem and assert that reads,
//! writes, and deletions fail closed rather than escaping the managed root.
//!
//! Scoped to benign concurrent mutation. Nothing here claims resistance to a
//! hostile same-privilege process, and hard-link aliasing is a separate concern
//! that reparse/symlink rejection does not address.

#![cfg(unix)]

use rom_manager::{ConfinedRoot, RelativePath};

fn path(value: &str) -> RelativePath {
    RelativePath::new(value).unwrap()
}

/// A managed root with a sibling directory holding a secret, plus whatever
/// indirection the caller plants.
fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("target");
    std::fs::create_dir_all(root.join("ROMs/nes")).unwrap();
    std::fs::create_dir_all(temporary.path().join("outside")).unwrap();
    std::fs::write(temporary.path().join("outside/secret.nes"), b"not yours").unwrap();
    (temporary, root)
}

#[test]
fn a_symlinked_leaf_cannot_be_read_through() {
    let (temporary, root) = fixture();
    std::os::unix::fs::symlink(
        temporary.path().join("outside/secret.nes"),
        root.join("ROMs/nes/Tracers.nes"),
    )
    .unwrap();

    let confined = ConfinedRoot::open(&root).unwrap();
    assert!(
        confined.read(&path("ROMs/nes/Tracers.nes")).is_err(),
        "a symlinked leaf must not be read through"
    );
}

#[test]
fn a_symlinked_intermediate_directory_cannot_be_traversed() {
    // The escape is mid-path, not at the leaf — the case a final-component-only
    // no-follow check would miss entirely.
    let (temporary, root) = fixture();
    std::fs::remove_dir_all(root.join("ROMs/nes")).unwrap();
    std::os::unix::fs::symlink(temporary.path().join("outside"), root.join("ROMs/nes")).unwrap();

    let confined = ConfinedRoot::open(&root).unwrap();
    assert!(
        confined.read(&path("ROMs/nes/secret.nes")).is_err(),
        "an indirection at any path position must fail closed"
    );
}

#[test]
fn a_write_never_follows_a_symlink_out_of_the_root() {
    let (temporary, root) = fixture();
    let outside = temporary.path().join("outside/secret.nes");
    std::os::unix::fs::symlink(&outside, root.join("ROMs/nes/Tracers.nes")).unwrap();

    let confined = ConfinedRoot::open(&root).unwrap();
    assert!(
        confined
            .write_new(&path("ROMs/nes/Tracers.nes"), b"overwritten")
            .is_err(),
        "a write must not follow a symlink"
    );
    assert_eq!(
        std::fs::read(&outside).unwrap(),
        b"not yours",
        "content outside the managed root must be untouched"
    );
}

#[test]
fn a_deletion_never_follows_a_symlink_out_of_the_root() {
    let (temporary, root) = fixture();
    let outside = temporary.path().join("outside/secret.nes");
    std::os::unix::fs::symlink(&outside, root.join("ROMs/nes/Tracers.nes")).unwrap();

    let confined = ConfinedRoot::open(&root).unwrap();
    // Unlinking removes the link itself, never its target.
    let _ = confined.delete_leaf(&path("ROMs/nes/Tracers.nes"));
    assert!(
        outside.exists(),
        "content outside the managed root must survive a deletion inside it"
    );
}

#[test]
fn creation_is_atomic_create_if_absent() {
    let (_temporary, root) = fixture();
    let confined = ConfinedRoot::open(&root).unwrap();
    let target = path("ROMs/nes/Tracers.nes");

    confined.write_new(&target, b"first").unwrap();
    assert!(
        confined.write_new(&target, b"second").is_err(),
        "an existing object must fail the create, never be overwritten"
    );
    assert_eq!(confined.read(&target).unwrap(), b"first");
}

#[test]
fn ordinary_content_round_trips_and_creates_missing_directories() {
    let (_temporary, root) = fixture();
    let confined = ConfinedRoot::open(&root).unwrap();
    let target = path("ROMs/snes/Deep/Nested.nes");

    confined.write_new(&target, b"rom bytes").unwrap();
    assert_eq!(confined.read(&target).unwrap(), b"rom bytes");

    confined.delete_leaf(&target).unwrap();
    assert!(confined.read(&target).is_err());
}

#[test]
fn a_multi_link_file_is_visible_as_such() {
    // Reparse/symlink rejection does not address hard links: the bytes are
    // reachable from a name this application cannot see, so writing through
    // this one would modify content outside the managed root.
    let (temporary, root) = fixture();
    let inside = root.join("ROMs/nes/Tracers.nes");
    std::fs::write(&inside, b"rom bytes").unwrap();
    std::fs::hard_link(&inside, temporary.path().join("outside/alias.nes")).unwrap();

    let confined = ConfinedRoot::open(&root).unwrap();
    assert_eq!(
        confined.link_count(&path("ROMs/nes/Tracers.nes")).unwrap(),
        2,
        "a second name must be detectable before any mutation"
    );
}

#[test]
fn a_root_that_is_itself_an_indirection_is_refused() {
    let (temporary, root) = fixture();
    let linked_root = temporary.path().join("linked-target");
    std::os::unix::fs::symlink(&root, &linked_root).unwrap();

    assert!(
        ConfinedRoot::open(&linked_root).is_err(),
        "a root that is itself an indirection must be refused"
    );
}
