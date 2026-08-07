//! Taking an Import Folder in as ROM Sets and a ROM Pack (issue #22).
//!
//! Fixtures are the project's own generated NES ROM and a few bytes standing in
//! for other formats. Nothing here is a commercial dump: what is being proved
//! is classification and identity, and neither needs real game content.

use std::{fs, path::Path};

use rom_manager::{Library, Outcome, ReasonCode, Session, Store, take_in};

const ROM_BYTES: &[u8] = include_bytes!("../fixtures/nes/tracers.nes");

struct Fixture {
    _directory: tempfile::TempDir,
    library: Library,
    store: Store,
    incoming: std::path::PathBuf,
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let incoming = directory.path().join("incoming");
    fs::create_dir_all(&incoming).unwrap();
    Fixture {
        library: Library::open(directory.path().join("library")).unwrap(),
        store: Store::open(&directory.path().join("state.sqlite3")).unwrap(),
        incoming,
        _directory: directory,
    }
}

impl Fixture {
    fn place(&self, name: &str, bytes: &[u8]) {
        fs::write(self.incoming.join(name), bytes).unwrap();
    }

    fn take_in(&self) -> rom_manager::IntakeReport {
        take_in(&self.library, &self.store, &self.incoming, 1).expect("the folder is readable")
    }
}

#[test]
fn a_recognized_rom_becomes_a_rom_set_in_a_pack() {
    // The gap this closes: before, a folder could be remembered and read and
    // still produce nothing the user could put on a device.
    let fixture = fixture();
    fixture.place("Tracers.nes", ROM_BYTES);

    let report = fixture.take_in();

    assert_eq!(report.rom_sets.len(), 1);
    assert_eq!(report.rom_sets[0].title, "Tracers");
    assert_eq!(report.rom_sets[0].platform, "Nintendo Entertainment System");
    assert!(report.rom_sets[0].newly_stored);
    assert!(report.declined.is_empty());

    let (pack_id, revision) = report.pack.expect("a pack was gathered");
    let selection = fixture.store.pack_selection(&pack_id, revision).unwrap();
    assert_eq!(selection.len(), 1);
}

#[test]
fn the_pack_is_named_for_the_folder() {
    let fixture = fixture();
    fixture.place("Tracers.nes", ROM_BYTES);
    let (pack_id, revision) = fixture.take_in().pack.unwrap();

    let titled = fixture
        .store
        .rom_packs()
        .unwrap()
        .into_iter()
        .find(|pack| pack.rom_pack_id == pack_id && pack.revision == revision)
        .expect("the pack is listed");
    assert_eq!(titled.title.as_deref(), Some("incoming"));
    assert_eq!(titled.rom_set_count, 1);
}

#[test]
fn underscores_become_spaces_but_nothing_else_is_rewritten() {
    // Stripping region and revision tags is a judgement about *which copy* this
    // is, and that belongs to metadata work rather than to reading a directory.
    let fixture = fixture();
    fixture.place("Super_Tracers_Bros (USA) (Rev 1).nes", ROM_BYTES);

    let report = fixture.take_in();
    assert_eq!(report.rom_sets[0].title, "Super Tracers Bros (USA) (Rev 1)");
}

#[test]
fn scanning_the_same_folder_twice_does_not_duplicate_anything() {
    // Identity comes from content, so re-reading a folder resolves to the same
    // ROM Set rather than a second one that happens to look identical.
    let fixture = fixture();
    fixture.place("Tracers.nes", ROM_BYTES);

    let first = fixture.take_in();
    let second = fixture.take_in();

    assert_eq!(first.pack, second.pack, "no revision that says nothing");
    assert!(
        second.rom_sets.is_empty(),
        "unchanged content is not a new candidate"
    );

    let (pack_id, revision) = first.pack.unwrap();
    assert_eq!(
        fixture
            .store
            .pack_selection(&pack_id, revision)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn the_same_bytes_under_a_different_name_are_the_same_rom_set() {
    // A path-derived identifier would make a renamed file into a new game.
    let fixture = fixture();
    fixture.place("Tracers.nes", ROM_BYTES);
    let first = fixture.take_in();

    fixture.place("Tracers (copy).nes", ROM_BYTES);
    let second = fixture.take_in();

    if let Some(added) = second.rom_sets.first() {
        assert_eq!(
            added.rom_set_id, first.rom_sets[0].rom_set_id,
            "identical bytes must resolve to one ROM Set"
        );
        assert!(!added.newly_stored, "the content was already owned");
    }
}

#[test]
fn adding_a_second_game_mints_a_new_pack_revision() {
    let fixture = fixture();
    fixture.place("Tracers.nes", ROM_BYTES);
    let (pack_id, first) = fixture.take_in().pack.unwrap();

    let mut other = ROM_BYTES.to_vec();
    other.extend_from_slice(b"a different game entirely");
    fixture.place("Another.nes", &other);
    let (same_pack, second) = fixture.take_in().pack.unwrap();

    assert_eq!(pack_id, same_pack);
    assert!(second > first, "a changed selection is a new revision");
    assert_eq!(
        fixture
            .store
            .pack_selection(&pack_id, second)
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn an_extension_several_platforms_share_needs_a_platform_rather_than_a_guess() {
    // .iso is PlayStation 2 and PSP. Picking the first match would file a PSP
    // game under PlayStation 2, and the user would find out when their handheld
    // refused to launch it.
    let fixture = fixture();
    fixture.place("Something.iso", b"not really an iso");

    let report = fixture.take_in();

    assert!(report.rom_sets.is_empty());
    assert_eq!(report.declined.len(), 1);
    assert_eq!(report.declined[0].outcome, Outcome::NeedsPlatform);
    assert_eq!(report.declined[0].reason, ReasonCode::PlatformUndetermined);
    assert!(report.pack.is_none(), "nothing to gather");
}

#[test]
fn an_unrecognized_extension_is_unsupported_not_undetermined() {
    // Different failures: one file is fine and ambiguous, the other is simply
    // not something this release handles.
    let fixture = fixture();
    fixture.place("notes.docx", b"a document");

    let report = fixture.take_in();
    assert_eq!(report.declined.len(), 1);
    assert_eq!(report.declined[0].outcome, Outcome::Unsupported);
    assert_eq!(report.declined[0].reason, ReasonCode::UnknownExtension);
}

#[test]
fn a_bare_bin_is_never_taken_in_as_a_game() {
    // It is a track, and only a descriptor may claim it.
    let fixture = fixture();
    fixture.place("track01.bin", b"raw track bytes");

    let report = fixture.take_in();
    assert!(report.rom_sets.is_empty());
    assert_eq!(report.declined[0].outcome, Outcome::Ambiguous);
}

#[test]
fn a_declined_file_names_itself_so_the_user_can_find_it() {
    let fixture = fixture();
    fixture.place("Something.iso", b"ambiguous");

    let report = fixture.take_in();
    let source = report.declined[0]
        .location
        .source
        .as_deref()
        .expect("the diagnostic names the file");
    assert!(source.ends_with("Something.iso"), "{source}");
}

#[test]
fn recognized_and_declined_files_coexist_without_blocking_each_other() {
    // One unreadable oddity in a folder of a thousand games must not stop the
    // other nine hundred and ninety-nine being taken in.
    let fixture = fixture();
    fixture.place("Tracers.nes", ROM_BYTES);
    fixture.place("readme.txt", b"about this collection");
    fixture.place("Something.iso", b"ambiguous");

    let report = fixture.take_in();

    assert_eq!(report.rom_sets.len(), 1);
    assert_eq!(report.declined.len(), 2);
    assert!(report.pack.is_some());
}

#[test]
fn an_empty_folder_gathers_no_pack() {
    // An empty ROM Pack would be a thing the user could select and sync,
    // producing a plan that removes everything. Better to have none.
    let fixture = fixture();
    let report = fixture.take_in();

    assert!(report.rom_sets.is_empty());
    assert!(report.pack.is_none());
}

#[test]
fn the_original_file_is_left_exactly_where_it_was() {
    // Import copies into Library storage. A tool that moved the user's files
    // out from under them would be unforgivable on a first run.
    let fixture = fixture();
    fixture.place("Tracers.nes", ROM_BYTES);
    fixture.take_in();

    let original = fixture.incoming.join("Tracers.nes");
    assert!(original.exists(), "the source file was moved or deleted");
    assert_eq!(fs::read(&original).unwrap(), ROM_BYTES);
}

// ── Through the session ─────────────────────────────────────────────────────

#[test]
fn scanning_through_the_session_makes_the_pack_choosable() {
    // The whole journey: remember a folder, scan it, and the wizard has
    // something to offer.
    let directory = tempfile::tempdir().unwrap();
    let incoming = directory.path().join("incoming");
    fs::create_dir_all(&incoming).unwrap();
    fs::write(incoming.join("Tracers.nes"), ROM_BYTES).unwrap();

    let store = Store::open(&directory.path().join("state.sqlite3")).unwrap();
    let library = Library::open(directory.path().join("library")).unwrap();
    let mut session: Session<rom_manager::FakeTransport> = Session::new(
        store,
        Box::new(|_| Ok(rom_manager::FakeTransport::new("fake://", 1 << 20))),
    );
    session.set_library(library);

    assert!(session.available_packs().is_empty());

    let folder_id = session
        .nominate_import_folder(&incoming.to_string_lossy())
        .unwrap();
    assert!(
        session.available_packs().is_empty(),
        "remembering a folder must not read it — the application never walks \
         the user's disks on its own schedule"
    );

    let report = session.scan_import_folder(folder_id).unwrap();
    assert_eq!(report.rom_sets.len(), 1);

    let packs = session.available_packs();
    assert_eq!(packs.len(), 1);
    assert_eq!(packs[0].title, "incoming");
    assert_eq!(packs[0].rom_set_count, 1);
}

#[test]
fn scanning_a_folder_that_was_never_remembered_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(&directory.path().join("state.sqlite3")).unwrap();
    let mut session: Session<rom_manager::FakeTransport> = Session::new(
        store,
        Box::new(|_| Ok(rom_manager::FakeTransport::new("fake://", 1 << 20))),
    );
    session.set_library(Library::open(directory.path().join("library")).unwrap());

    assert!(session.scan_import_folder(999).is_err());
}

#[test]
fn scanning_without_a_library_says_so_rather_than_panicking() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(&directory.path().join("state.sqlite3")).unwrap();
    let mut session: Session<rom_manager::FakeTransport> = Session::new(
        store,
        Box::new(|_| Ok(rom_manager::FakeTransport::new("fake://", 1 << 20))),
    );
    let folder_id = session
        .nominate_import_folder(&directory.path().to_string_lossy())
        .unwrap();

    assert!(session.scan_import_folder(folder_id).is_err());
}

#[test]
fn nothing_in_the_library_root_escapes_it() {
    // A guard on the storage layout: every object the intake wrote must live
    // under the Library root, never beside it.
    let fixture = fixture();
    fixture.place("Tracers.nes", ROM_BYTES);
    fixture.take_in();

    let owned = fixture.store.owned_objects().unwrap();
    assert_eq!(owned.len(), 1);
    for digest in owned {
        assert!(
            fixture.library.read_object(&digest).is_ok(),
            "the object is not readable from the Library root"
        );
    }
}

#[test]
fn provenance_records_where_the_file_was_found() {
    let fixture = fixture();
    fixture.place("Tracers.nes", ROM_BYTES);
    let report = fixture.take_in();

    let observations = fixture
        .store
        .origin_observations(&report.rom_sets[0].content_digest)
        .unwrap();
    assert_eq!(observations.len(), 1);
    assert!(
        observations[0].ends_with("Tracers.nes"),
        "{:?}",
        observations
    );
}

#[test]
fn nothing_taken_in_is_game_content_from_this_repository() {
    // The fixture is project-generated. A guard against a real dump creeping in
    // beside it later.
    assert!(Path::new("fixtures/nes/generate.mjs").exists());
    assert!(ROM_BYTES.len() < 64 * 1024);
}

#[test]
fn a_pack_describes_the_whole_folder_not_just_what_changed() {
    // The bug this guards is destructive, not cosmetic. If the pack were built
    // from only the files a scan newly saw, then adding one game to a folder
    // would produce a pack containing exactly that game — and syncing it would
    // plan the removal of every other game already on the device.
    let fixture = fixture();
    fixture.place("First.nes", ROM_BYTES);
    fixture.take_in();

    let mut second_bytes = ROM_BYTES.to_vec();
    second_bytes.extend_from_slice(b"second game");
    fixture.place("Second.nes", &second_bytes);

    let mut third_bytes = ROM_BYTES.to_vec();
    third_bytes.extend_from_slice(b"third game");
    fixture.place("Third.nes", &third_bytes);

    let report = fixture.take_in();
    assert_eq!(report.rom_sets.len(), 2, "two games are new this scan");

    let (pack_id, revision) = report.pack.expect("a pack");
    let selection = fixture.store.pack_selection(&pack_id, revision).unwrap();
    assert_eq!(
        selection.len(),
        3,
        "the pack must hold all three games, not only the two just added"
    );
}

#[test]
fn removing_a_file_from_the_folder_removes_it_from_the_next_pack_revision() {
    // The other direction, and the one that must stay deliberate: a game the
    // user deleted should leave the pack, so the next sync can offer to remove
    // it — with the acknowledgement every permanent removal requires.
    let fixture = fixture();
    fixture.place("Keeper.nes", ROM_BYTES);
    let mut other = ROM_BYTES.to_vec();
    other.extend_from_slice(b"goes away");
    fixture.place("Goes-Away.nes", &other);

    let (pack_id, first) = fixture.take_in().pack.unwrap();
    assert_eq!(
        fixture.store.pack_selection(&pack_id, first).unwrap().len(),
        2
    );

    fs::remove_file(fixture.incoming.join("Goes-Away.nes")).unwrap();
    let (_, second) = fixture.take_in().pack.unwrap();

    assert!(second > first);
    assert_eq!(
        fixture
            .store
            .pack_selection(&pack_id, second)
            .unwrap()
            .len(),
        1,
        "the deleted game leaves the pack"
    );
}

#[test]
fn one_missing_folder_does_not_stop_the_others_being_scanned() {
    // An unplugged drive is an ordinary Tuesday. It must not mean the folders
    // still present go unread.
    let directory = tempfile::tempdir().unwrap();
    let present = directory.path().join("present");
    fs::create_dir_all(&present).unwrap();
    fs::write(present.join("Tracers.nes"), ROM_BYTES).unwrap();

    let store = Store::open(&directory.path().join("state.sqlite3")).unwrap();
    let mut session: Session<rom_manager::FakeTransport> = Session::new(
        store,
        Box::new(|_| Ok(rom_manager::FakeTransport::new("fake://", 1 << 20))),
    );
    session.set_library(Library::open(directory.path().join("library")).unwrap());

    session
        .nominate_import_folder("/a/drive/that/is/not/plugged/in")
        .unwrap();
    session
        .nominate_import_folder(&present.to_string_lossy())
        .unwrap();

    let reports = session
        .scan_all_import_folders()
        .expect("a missing folder does not stop the run");

    assert_eq!(reports.len(), 2, "both folders were attempted");
    assert_eq!(
        session.available_packs().len(),
        1,
        "only the reachable folder produced a pack"
    );

    // The unreachable one is reported rather than passed off as an empty scan,
    // which would tell the user their games had vanished.
    let unreadable: Vec<_> = reports
        .iter()
        .flat_map(|report| &report.declined)
        .filter(|diagnostic| diagnostic.outcome == Outcome::IoFailure)
        .collect();
    assert_eq!(unreadable.len(), 1);
    assert!(
        unreadable[0]
            .location
            .source
            .as_deref()
            .unwrap_or_default()
            .contains("not/plugged/in"),
        "the diagnostic names the folder that could not be read"
    );
}

#[test]
fn scanning_everything_with_no_library_is_refused_rather_than_silently_doing_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(&directory.path().join("state.sqlite3")).unwrap();
    let mut session: Session<rom_manager::FakeTransport> = Session::new(
        store,
        Box::new(|_| Ok(rom_manager::FakeTransport::new("fake://", 1 << 20))),
    );
    session
        .nominate_import_folder(&directory.path().to_string_lossy())
        .unwrap();

    assert!(session.scan_all_import_folders().is_err());
}

#[test]
fn the_snapshot_carries_what_the_scan_refused_and_why() {
    // A scan that reported only its successes would leave a user whose
    // collection is missing a game with no way to find out which one.
    let directory = tempfile::tempdir().unwrap();
    let incoming = directory.path().join("incoming");
    fs::create_dir_all(&incoming).unwrap();
    fs::write(incoming.join("Tracers.nes"), ROM_BYTES).unwrap();
    fs::write(incoming.join("Ambiguous.iso"), b"could be two platforms").unwrap();
    fs::write(incoming.join("notes.docx"), b"a document").unwrap();

    let store = Store::open(&directory.path().join("state.sqlite3")).unwrap();
    let mut session: Session<rom_manager::FakeTransport> = Session::new(
        store,
        Box::new(|_| Ok(rom_manager::FakeTransport::new("fake://", 1 << 20))),
    );
    session.set_library(Library::open(directory.path().join("library")).unwrap());
    let folder_id = session
        .nominate_import_folder(&incoming.to_string_lossy())
        .unwrap();

    assert!(
        session.snapshot().last_scan.is_none(),
        "nothing is claimed before a scan happens"
    );

    session.scan_import_folder(folder_id).unwrap();
    session.scan_all_import_folders().unwrap();

    let summary = session.snapshot().last_scan.expect("a scan was recorded");
    assert_eq!(
        summary.rom_sets_added, 0,
        "the first scan already took it in"
    );
    assert_eq!(summary.declined.len(), 2);

    for declined in &summary.declined {
        assert!(!declined.path.is_empty(), "every refusal names its file");
        assert!(
            declined.remediation.len() > 20,
            "a refusal with no remedy leaves the user stuck: {}",
            declined.code
        );
    }
    let codes: Vec<&str> = summary.declined.iter().map(|d| d.code.as_str()).collect();
    assert!(codes.contains(&"platform_undetermined"), "{codes:?}");
    assert!(codes.contains(&"unknown_extension"), "{codes:?}");
}
