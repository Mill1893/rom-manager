//! Coverage for retirement and ineligible-field removal (issue #70).
//!
//! Removing a field has to be correct *about the past*: only what we put there,
//! and only while it is still what we put there.

use rom_manager::{
    EligibilityAction, Gamelist, Ineligibility, LedgerEntry, ProjectionMove, plan_retirement,
    withdraw_ineligible_field,
};

fn ledger(pairs: &[(&str, &str)]) -> LedgerEntry {
    let mut entry = LedgerEntry::default();
    for (field, value) in pairs {
        entry.exported.insert((*field).into(), (*value).into());
    }
    entry
}

fn parse(xml: &str) -> Gamelist {
    Gamelist::parse(xml).unwrap()
}

const OURS_ONLY: &str = r#"<?xml version="1.0"?>
<gameList>
  <game>
    <path>./Tracers.nes</path>
    <name>Tracers</name>
    <genre>Puzzle</genre>
  </game>
</gameList>
"#;

const WITH_FRONTEND_STATE: &str = r#"<?xml version="1.0"?>
<gameList>
  <game>
    <path>./Tracers.nes</path>
    <name>Tracers</name>
    <favorite>true</favorite>
    <playcount>42</playcount>
  </game>
</gameList>
"#;

#[test]
fn an_entry_we_created_holding_only_our_fields_is_removed_whole() {
    let gamelist = parse(OURS_ONLY);
    let entry = gamelist.entry("./Tracers.nes").unwrap();
    let recorded = ledger(&[("name", "Tracers"), ("genre", "Puzzle")]);

    let retirement = plan_retirement(entry, Some(&recorded));

    assert!(!retirement.is_blocked());
    assert!(retirement.remove_whole_entry);
    assert_eq!(retirement.fields_to_remove, vec!["name", "genre"]);
}

#[test]
fn frontend_state_keeps_the_node_alive() {
    // Deleting it would take the user's play count with it.
    let gamelist = parse(WITH_FRONTEND_STATE);
    let entry = gamelist.entry("./Tracers.nes").unwrap();
    let recorded = ledger(&[("name", "Tracers")]);

    let retirement = plan_retirement(entry, Some(&recorded));

    assert!(!retirement.remove_whole_entry);
    assert_eq!(
        retirement.retained_because,
        Some("the entry carries frontend-owned state")
    );
    // Our field still goes; theirs stays.
    assert_eq!(retirement.fields_to_remove, vec!["name"]);
}

#[test]
fn a_changed_owned_field_blocks_retirement_entirely() {
    let gamelist = parse(OURS_ONLY);
    let entry = gamelist.entry("./Tracers.nes").unwrap();
    // We wrote a different genre; the user changed it.
    let recorded = ledger(&[("name", "Tracers"), ("genre", "Action")]);

    let retirement = plan_retirement(entry, Some(&recorded));

    assert!(retirement.is_blocked());
    assert_eq!(retirement.changed_fields, vec!["genre"]);
    assert!(!retirement.remove_whole_entry);
    assert_eq!(
        retirement.retained_because,
        Some("an owned field was changed on the device")
    );
}

#[test]
fn an_entry_we_never_created_is_never_removed_whole() {
    let gamelist = parse(OURS_ONLY);
    let entry = gamelist.entry("./Tracers.nes").unwrap();

    let retirement = plan_retirement(entry, None);

    assert!(!retirement.remove_whole_entry);
    assert_eq!(
        retirement.retained_because,
        Some("the ledger does not show this entry as ours")
    );
    assert!(
        retirement.fields_to_remove.is_empty(),
        "with no ledger, nothing is provably ours to remove"
    );
}

#[test]
fn unknown_state_keeps_the_node_alive() {
    let with_unknown = r#"<?xml version="1.0"?>
<gameList>
  <game>
    <path>./Tracers.nes</path>
    <name>Tracers</name>
    <somethingElse>who knows</somethingElse>
  </game>
</gameList>
"#;
    let gamelist = parse(with_unknown);
    let entry = gamelist.entry("./Tracers.nes").unwrap();
    let retirement = plan_retirement(entry, Some(&ledger(&[("name", "Tracers")])));

    assert!(!retirement.remove_whole_entry);
    assert_eq!(
        retirement.retained_because,
        Some("the entry carries unknown state")
    );
}

#[test]
fn an_ineligible_field_matching_the_ledger_is_removed_explicitly() {
    let action = withdraw_ineligible_field(
        "desc",
        Ineligibility::ProviderTerms,
        Some(&"A description.".to_string()),
        Some(&"A description.".to_string()),
    );

    assert_eq!(
        action,
        EligibilityAction::Remove {
            field: "desc".into(),
            reason: Ineligibility::ProviderTerms
        }
    );
}

#[test]
fn an_ineligible_field_the_user_edited_goes_through_the_conflict_flow() {
    // Removing it would discard the edit.
    let action = withdraw_ineligible_field(
        "desc",
        Ineligibility::ProviderTerms,
        Some(&"What we wrote.".to_string()),
        Some(&"What they wrote.".to_string()),
    );

    assert!(matches!(action, EligibilityAction::Conflict { .. }));
}

#[test]
fn an_ineligible_field_absent_from_the_device_needs_no_action() {
    let action = withdraw_ineligible_field(
        "desc",
        Ineligibility::UnrepresentableAttribution,
        Some(&"Gone already.".to_string()),
        None,
    );

    assert_eq!(action, EligibilityAction::NothingToDo);
}

#[test]
fn a_move_is_retirement_plus_creation_never_a_rename() {
    // A rename would carry the old entry's frontend state onto a different
    // file — the user's play count for one game landing on another.
    let moved = ProjectionMove::new("./Old.nes", "./New.nes");

    assert!(!moved.is_rename());
    assert_eq!(moved.retire, "./Old.nes");
    assert_eq!(moved.create, "./New.nes");
}
