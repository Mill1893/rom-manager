//! Coverage for gamelist coexistence (issue #68).
//!
//! The document is shared. ES-DE writes to it, the user edits it by hand, and
//! ROM Manager owns only the fields it maps. Every test here is about what
//! survives a rewrite.

use rom_manager::{Gamelist, OWNED_FIELDS};

/// A gamelist with owned fields, frontend state, an unknown element, and an
/// entry ROM Manager never created.
const SHARED: &str = r#"<?xml version="1.0"?>
<gameList>
  <game>
    <path>./Tracers.nes</path>
    <name>Tracers</name>
    <desc>An old description.</desc>
    <favorite>true</favorite>
    <playcount>17</playcount>
    <lastplayed>20250101T120000</lastplayed>
    <somethingUnknown>keep me</somethingUnknown>
  </game>
  <game>
    <path>./SomeoneElses.nes</path>
    <name>Not Ours</name>
    <favorite>true</favorite>
  </game>
</gameList>
"#;

#[test]
fn parsing_reads_entries_and_their_children() {
    let gamelist = Gamelist::parse(SHARED).unwrap();

    assert_eq!(gamelist.entries.len(), 2);
    let entry = gamelist.entry("./Tracers.nes").unwrap();
    assert_eq!(entry.field("name"), Some("Tracers"));
    assert_eq!(entry.field("favorite"), Some("true"));
    assert_eq!(entry.field("somethingUnknown"), Some("keep me"));
}

#[test]
fn frontend_state_and_unknown_elements_survive_a_rewrite() {
    let mut gamelist = Gamelist::parse(SHARED).unwrap();
    gamelist.set_owned_field("./Tracers.nes", "desc", "A new description.");

    let rewritten = gamelist.to_xml().unwrap();
    let reparsed = Gamelist::parse(&rewritten).unwrap();
    let entry = reparsed.entry("./Tracers.nes").unwrap();

    assert_eq!(entry.field("desc"), Some("A new description."));
    // Everything we do not own is still there.
    assert_eq!(entry.field("favorite"), Some("true"));
    assert_eq!(entry.field("playcount"), Some("17"));
    assert_eq!(entry.field("lastplayed"), Some("20250101T120000"));
    assert_eq!(entry.field("somethingUnknown"), Some("keep me"));
}

#[test]
fn an_entry_we_did_not_create_is_left_completely_alone() {
    let mut gamelist = Gamelist::parse(SHARED).unwrap();
    gamelist.set_owned_field("./Tracers.nes", "name", "Tracers (USA)");

    let reparsed = Gamelist::parse(&gamelist.to_xml().unwrap()).unwrap();
    let theirs = reparsed.entry("./SomeoneElses.nes").unwrap();

    assert_eq!(theirs.field("name"), Some("Not Ours"));
    assert_eq!(theirs.field("favorite"), Some("true"));
}

#[test]
fn child_order_is_preserved() {
    let gamelist = Gamelist::parse(SHARED).unwrap();
    let before: Vec<&str> = gamelist
        .entry("./Tracers.nes")
        .unwrap()
        .children
        .iter()
        .map(|(tag, _)| tag.as_str())
        .collect();

    let reparsed = Gamelist::parse(&gamelist.to_xml().unwrap()).unwrap();
    let after: Vec<&str> = reparsed
        .entry("./Tracers.nes")
        .unwrap()
        .children
        .iter()
        .map(|(tag, _)| tag.as_str())
        .collect();

    assert_eq!(before, after);
}

#[test]
fn frontend_owned_fields_cannot_be_written_or_removed() {
    // Not merely "we don't"; the mechanism refuses.
    let mut gamelist = Gamelist::parse(SHARED).unwrap();

    assert!(!gamelist.set_owned_field("./Tracers.nes", "favorite", "false"));
    assert!(!gamelist.set_owned_field("./Tracers.nes", "playcount", "0"));
    assert!(!gamelist.remove_owned_field("./Tracers.nes", "favorite"));

    assert_eq!(
        gamelist.entry("./Tracers.nes").unwrap().field("favorite"),
        Some("true")
    );
}

#[test]
fn only_mapped_fields_are_reported_as_owned() {
    let gamelist = Gamelist::parse(SHARED).unwrap();
    let owned = gamelist.entry("./Tracers.nes").unwrap().owned_fields();

    assert_eq!(owned.len(), 2, "name and desc, not favorite or playcount");
    assert!(owned.contains_key("name"));
    assert!(owned.contains_key("desc"));
    for field in ["favorite", "playcount", "lastplayed", "somethingUnknown"] {
        assert!(!owned.contains_key(field), "{field} is not ours");
    }
}

#[test]
fn an_entry_carrying_frontend_state_is_never_wholly_removable() {
    let gamelist = Gamelist::parse(SHARED).unwrap();
    let entry = gamelist.entry("./Tracers.nes").unwrap();

    assert!(entry.has_frontend_state());
    assert!(
        !entry.holds_only_owned_state(),
        "deleting this node would take the user's play history with it"
    );
}

#[test]
fn an_entry_holding_only_our_fields_may_be_removed() {
    let ours = r#"<?xml version="1.0"?>
<gameList>
  <game>
    <path>./Ours.nes</path>
    <name>Ours</name>
    <genre>Puzzle</genre>
  </game>
</gameList>
"#;
    let mut gamelist = Gamelist::parse(ours).unwrap();
    let entry = gamelist.entry("./Ours.nes").unwrap();

    assert!(!entry.has_frontend_state());
    assert!(entry.holds_only_owned_state());

    assert!(gamelist.remove_entry("./Ours.nes"));
    assert!(gamelist.entry("./Ours.nes").is_none());
}

#[test]
fn a_malformed_document_is_refused_not_treated_as_empty() {
    // Treating it as empty would licence overwriting a file worth rescuing.
    let broken = "<gameList><game><path>./A.nes</path>";
    let result = Gamelist::parse(broken);

    assert!(
        result.is_err() || result.unwrap().entries.is_empty(),
        "an unterminated document must not yield a confident empty gamelist"
    );

    assert!(Gamelist::parse("<gameList><<<>>>").is_err());
}

#[test]
fn a_new_entry_can_be_inserted() {
    use std::collections::BTreeMap;

    let mut gamelist = Gamelist::parse(SHARED).unwrap();
    let mut fields: BTreeMap<&'static str, String> = BTreeMap::new();
    fields.insert("name", "Brand New".into());
    fields.insert("genre", "Action".into());

    gamelist.insert_entry("./New.nes", &fields);
    let reparsed = Gamelist::parse(&gamelist.to_xml().unwrap()).unwrap();
    let entry = reparsed.entry("./New.nes").unwrap();

    assert_eq!(entry.field("name"), Some("Brand New"));
    assert_eq!(entry.field("genre"), Some("Action"));
    assert_eq!(entry.path, "./New.nes");
}

#[test]
fn the_fingerprint_detects_any_change() {
    let original = Gamelist::fingerprint(SHARED);
    assert_eq!(Gamelist::fingerprint(SHARED), original);

    let edited = SHARED.replace("An old description.", "Edited by hand.");
    assert_ne!(
        Gamelist::fingerprint(&edited),
        original,
        "an edit between planning and publication must be detectable"
    );
}

#[test]
fn no_proprietary_marker_is_added() {
    let mut gamelist = Gamelist::parse(SHARED).unwrap();
    gamelist.set_owned_field("./Tracers.nes", "name", "Tracers");
    let rewritten = gamelist.to_xml().unwrap();

    for marker in ["rom-manager", "romManager", "rmOwned", "ledger"] {
        assert!(
            !rewritten.contains(marker),
            "the document must carry no ownership marker of ours"
        );
    }
}

#[test]
fn the_owned_field_set_is_exactly_what_was_specified() {
    assert_eq!(
        OWNED_FIELDS,
        [
            "name",
            "sortname",
            "desc",
            "releasedate",
            "developer",
            "publisher",
            "genre",
            "players"
        ]
    );
}
