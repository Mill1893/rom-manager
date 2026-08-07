//! Coverage for the three-way merge (issue #69).
//!
//! The distinction the ledger exists to make: *the user edited this* versus
//! *we wrote this and the Library has since changed*. Those need opposite
//! responses, and two values cannot tell them apart.

use std::collections::BTreeMap;

use rom_manager::{FieldOutcome, LedgerEntry, conflicts, merge_entry, merge_field};

fn value(text: &str) -> String {
    text.to_owned()
}

#[test]
fn an_unchanged_owned_field_is_ours_to_update() {
    let outcome = merge_field(
        Some(&value("Old Name")), // we wrote this
        Some(&value("Old Name")), // it is still what we wrote
        Some(&value("New Name")), // the Library moved on
    );

    assert_eq!(
        outcome,
        Some(FieldOutcome::Update {
            from: value("Old Name"),
            to: value("New Name")
        })
    );
    assert!(outcome.unwrap().writes_silently());
}

#[test]
fn agreement_only_refreshes_evidence() {
    let outcome = merge_field(
        Some(&value("Tracers")),
        Some(&value("Tracers")),
        Some(&value("Tracers")),
    );

    assert_eq!(outcome, Some(FieldOutcome::RefreshEvidence));
}

#[test]
fn a_user_edit_to_our_field_is_a_conflict_not_an_overwrite() {
    // The decisive case. Overwriting loses the user's edit; importing turns an
    // edit on one device into a Library-wide fact they never asked for.
    let outcome = merge_field(
        Some(&value("Tracers")),        // we wrote this
        Some(&value("Tracers (hack)")), // the user changed it
        Some(&value("Tracers")),        // the Library still says this
    );

    assert_eq!(
        outcome,
        Some(FieldOutcome::Conflict {
            ledger: Some(value("Tracers")),
            device: value("Tracers (hack)"),
            desired: value("Tracers"),
        })
    );
    assert!(outcome.as_ref().unwrap().needs_user_decision());
    assert!(!outcome.unwrap().writes_silently());
}

#[test]
fn an_equal_pre_existing_field_is_an_adoption_offer() {
    // Correct already, but not ours. Claiming it silently would take ownership
    // of something the user or ES-DE wrote.
    let outcome = merge_field(None, Some(&value("Tracers")), Some(&value("Tracers")));

    assert_eq!(
        outcome,
        Some(FieldOutcome::OfferAdoption {
            value: value("Tracers")
        })
    );
    assert!(outcome.unwrap().needs_user_decision());
}

#[test]
fn a_differing_pre_existing_field_is_a_conflict() {
    let outcome = merge_field(None, Some(&value("Their Name")), Some(&value("Our Name")));

    assert!(matches!(
        outcome,
        Some(FieldOutcome::Conflict { ledger: None, .. })
    ));
}

#[test]
fn an_absent_field_is_simply_added() {
    let outcome = merge_field(None, None, Some(&value("Tracers")));

    assert_eq!(
        outcome,
        Some(FieldOutcome::Add {
            value: value("Tracers")
        })
    );
    assert!(outcome.unwrap().writes_silently());
}

#[test]
fn a_field_the_library_no_longer_wants_is_not_an_update() {
    // Retirement is its own decision (#70), not something an update sneaks in.
    assert_eq!(
        merge_field(Some(&value("Puzzle")), Some(&value("Puzzle")), None),
        None
    );
}

#[test]
fn a_whole_entry_merges_field_by_field() {
    let mut ledger = LedgerEntry::default();
    ledger.exported.insert("name".into(), "Tracers".into());
    ledger.exported.insert("genre".into(), "Puzzle".into());

    let mut device = BTreeMap::new();
    device.insert("name".to_string(), "Tracers".to_string()); // still ours
    device.insert("genre".to_string(), "Action".to_string()); // user changed it
    device.insert("developer".to_string(), "Studio A".to_string()); // pre-existing

    let mut desired = BTreeMap::new();
    desired.insert("name".to_string(), "Tracers (USA)".to_string());
    desired.insert("genre".to_string(), "Puzzle".to_string());
    desired.insert("developer".to_string(), "Studio A".to_string());
    desired.insert("players".to_string(), "2".to_string());

    let outcomes = merge_entry(Some(&ledger), &device, &desired);

    assert!(matches!(outcomes["name"], FieldOutcome::Update { .. }));
    assert!(matches!(outcomes["genre"], FieldOutcome::Conflict { .. }));
    assert!(matches!(
        outcomes["developer"],
        FieldOutcome::OfferAdoption { .. }
    ));
    assert!(matches!(outcomes["players"], FieldOutcome::Add { .. }));

    assert_eq!(conflicts(&outcomes), vec!["genre"]);
    assert!(rom_manager::requires_user_decision(&outcomes));
}

#[test]
fn a_clean_entry_needs_no_user_decision() {
    let mut ledger = LedgerEntry::default();
    ledger.exported.insert("name".into(), "Tracers".into());

    let mut device = BTreeMap::new();
    device.insert("name".to_string(), "Tracers".to_string());

    let mut desired = BTreeMap::new();
    desired.insert("name".to_string(), "Tracers (USA)".to_string());

    let outcomes = merge_entry(Some(&ledger), &device, &desired);

    assert!(!rom_manager::requires_user_decision(&outcomes));
    assert!(conflicts(&outcomes).is_empty());
}

#[test]
fn first_management_of_a_differing_field_uses_the_conflict_flow() {
    // No ledger at all: everything on the device is pre-existing, and anything
    // that disagrees goes through the same flow as a later user edit.
    let mut device = BTreeMap::new();
    device.insert("name".to_string(), "Whatever ES-DE Scraped".to_string());

    let mut desired = BTreeMap::new();
    desired.insert("name".to_string(), "Tracers".to_string());

    let outcomes = merge_entry(None, &device, &desired);

    assert!(matches!(outcomes["name"], FieldOutcome::Conflict { .. }));
    assert!(rom_manager::requires_user_decision(&outcomes));
}
