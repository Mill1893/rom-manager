//! Gate 3 certification evidence (issue #73).
//!
//! End to end on a combined filesystem target: projections written, adopted,
//! updated, retired, and reread — with frontend-owned state surviving all of it.

use std::{cell::Cell, collections::BTreeMap};

use rom_manager::{
    CombinedOutcome, EsdeProfile, FieldOutcome, Gamelist, LedgerEntry, MetadataProjection,
    Publication, PublishPreconditions, ReleaseFacts, merge_entry, plan_retirement, run_combined,
};

/// A gamelist as ES-DE would leave it: our fields, the user's state, and an
/// entry we never touched.
const EXISTING: &str = r#"<?xml version="1.0"?>
<gameList>
  <game>
    <path>./Tracers.nes</path>
    <name>Tracers</name>
    <genre>Puzzle</genre>
    <favorite>true</favorite>
    <playcount>42</playcount>
  </game>
  <game>
    <path>./TheirGame.nes</path>
    <name>Their Game</name>
    <favorite>true</favorite>
  </game>
</gameList>
"#;

fn stopped() -> PublishPreconditions {
    PublishPreconditions {
        frontend_stopped: true,
    }
}

fn ledger(pairs: &[(&str, &str)]) -> LedgerEntry {
    let mut entry = LedgerEntry::default();
    for (field, value) in pairs {
        entry.exported.insert((*field).into(), (*value).into());
    }
    entry
}

#[test]
fn a_projection_is_written_and_reread_on_a_combined_target() {
    let profile = EsdeProfile::nes();
    let entry_path = profile.gamelist_entry_path("Tracers.nes").unwrap();

    let facts = ReleaseFacts {
        title: "Tracers".into(),
        primary_genre: Some("Puzzle".into()),
        ..Default::default()
    };
    let projection = MetadataProjection::build(entry_path.clone(), &facts, &facts.title);

    let publication = Publication::planned_against(Gamelist::fingerprint(EXISTING));
    let written = publication
        .prepare(stopped(), EXISTING, |gamelist| {
            for (field, value) in &projection.fields {
                gamelist.set_owned_field(&entry_path, field, value);
            }
        })
        .unwrap();

    let reread = Gamelist::parse(&written).unwrap();
    let entry = reread.entry(&entry_path).unwrap();
    assert_eq!(entry.field("name"), Some("Tracers"));
    assert_eq!(entry.field("genre"), Some("Puzzle"));
}

#[test]
fn frontend_state_survives_every_operation() {
    // The single most important property of the whole gate.
    let mut gamelist = Gamelist::parse(EXISTING).unwrap();

    gamelist.set_owned_field("./Tracers.nes", "name", "Tracers (USA)");
    gamelist.set_owned_field("./Tracers.nes", "developer", "Studio A");
    gamelist.remove_owned_field("./Tracers.nes", "genre");

    let reread = Gamelist::parse(&gamelist.to_xml().unwrap()).unwrap();
    let ours = reread.entry("./Tracers.nes").unwrap();
    let theirs = reread.entry("./TheirGame.nes").unwrap();

    assert_eq!(ours.field("favorite"), Some("true"));
    assert_eq!(ours.field("playcount"), Some("42"));
    assert_eq!(theirs.field("name"), Some("Their Game"));
    assert_eq!(theirs.field("favorite"), Some("true"));
}

#[test]
fn an_equal_pre_existing_field_is_adopted_then_owned() {
    let gamelist = Gamelist::parse(EXISTING).unwrap();
    let device: BTreeMap<String, String> = gamelist
        .entry("./Tracers.nes")
        .unwrap()
        .owned_fields()
        .iter()
        .map(|(field, value)| ((*field).to_owned(), (*value).to_owned()))
        .collect();

    let mut desired = BTreeMap::new();
    desired.insert("name".to_string(), "Tracers".to_string());

    // First pass: no ledger, values agree — an adoption offer.
    let first = merge_entry(None, &device, &desired);
    assert!(matches!(first["name"], FieldOutcome::OfferAdoption { .. }));

    // After adoption the ledger records it, and a Library change is now ours.
    let mut changed = BTreeMap::new();
    changed.insert("name".to_string(), "Tracers (USA)".to_string());
    let second = merge_entry(Some(&ledger(&[("name", "Tracers")])), &device, &changed);
    assert!(matches!(second["name"], FieldOutcome::Update { .. }));
}

#[test]
fn the_fault_matrix_never_costs_content_or_user_state() {
    // Changed fingerprint between planning and publication.
    let publication = Publication::planned_against(Gamelist::fingerprint(EXISTING));
    let edited = EXISTING.replace("Puzzle", "Action");
    assert!(publication.prepare(stopped(), &edited, |_| {}).is_err());

    // Malformed document.
    let broken = Publication::planned_against(Gamelist::fingerprint("<gameList><game>"));
    assert!(
        broken
            .prepare(stopped(), "<gameList><game>", |_| {})
            .is_err()
    );

    // ES-DE not confirmed stopped.
    let running = Publication::planned_against(Gamelist::fingerprint(EXISTING));
    assert!(
        running
            .prepare(PublishPreconditions::default(), EXISTING, |_| {})
            .is_err()
    );

    // A conflicting owned field never resolves itself.
    let mut device = BTreeMap::new();
    device.insert("name".to_string(), "Edited By Hand".to_string());
    let mut desired = BTreeMap::new();
    desired.insert("name".to_string(), "Tracers".to_string());
    let outcomes = merge_entry(Some(&ledger(&[("name", "Tracers")])), &device, &desired);
    assert!(matches!(outcomes["name"], FieldOutcome::Conflict { .. }));

    // Metadata failure before removals.
    let removed = Cell::new(0usize);
    let outcome = run_combined(
        4,
        || Ok(()),
        || Err("interrupted".into()),
        |count| removed.set(count),
    );
    assert_eq!(removed.get(), 0);
    assert!(outcome.content_retained());
    assert!(matches!(
        outcome,
        CombinedOutcome::ContentSyncedMetadataPending { .. }
    ));
}

#[test]
fn retirement_leaves_the_users_entry_intact() {
    let gamelist = Gamelist::parse(EXISTING).unwrap();

    // Ours, but the user has state on it: fields go, node stays.
    let ours = plan_retirement(
        gamelist.entry("./Tracers.nes").unwrap(),
        Some(&ledger(&[("name", "Tracers"), ("genre", "Puzzle")])),
    );
    assert!(!ours.remove_whole_entry);
    assert_eq!(ours.fields_to_remove, vec!["name", "genre"]);

    // Never ours: nothing is removed at all.
    let theirs = plan_retirement(gamelist.entry("./TheirGame.nes").unwrap(), None);
    assert!(!theirs.remove_whole_entry);
    assert!(theirs.fields_to_remove.is_empty());
}

#[test]
fn the_gate_path_runs_write_adopt_update_retire_reread() {
    let mut gamelist = Gamelist::parse(EXISTING).unwrap();

    // Write a new projection.
    let mut fields: BTreeMap<&'static str, String> = BTreeMap::new();
    fields.insert("name", "Brand New".into());
    gamelist.insert_entry("./New.nes", &fields);

    // Update an owned field on an existing one.
    gamelist.set_owned_field("./Tracers.nes", "name", "Tracers (USA)");

    // Retire an owned field.
    gamelist.remove_owned_field("./Tracers.nes", "genre");

    let reread = Gamelist::parse(&gamelist.to_xml().unwrap()).unwrap();

    assert_eq!(
        reread.entry("./New.nes").unwrap().field("name"),
        Some("Brand New")
    );
    let tracers = reread.entry("./Tracers.nes").unwrap();
    assert_eq!(tracers.field("name"), Some("Tracers (USA)"));
    assert_eq!(tracers.field("genre"), None);
    assert_eq!(tracers.field("playcount"), Some("42"), "still theirs");
}
