//! Gating coverage for the desktop application boundary (issue #34).
//!
//! These assert the rules the UI depends on being enforced in Rust, so a
//! frontend cannot reach a state the core would refuse.

mod common;

use common::{TARGET_ID, core_with, fake};
use rom_manager::{ExecutionOutcome, OutcomeKind, OutcomeView, PlanView, Snapshot, WizardStep};

fn a_plan() -> rom_manager::SyncPlan {
    let mut core = core_with(fake());
    core.refresh().unwrap();
    core.build_plan().unwrap()
}

#[test]
fn a_plan_view_carries_everything_the_workflow_must_display() {
    let plan = a_plan();
    let view = PlanView::of(&plan, true);

    // Identity the user needs to know *which* device and rules are in play.
    assert_eq!(view.target_id, TARGET_ID);
    assert_eq!(view.binding_locator, plan.binding_locator);
    assert_eq!(view.profile_id, plan.profile_id);
    assert_eq!(view.profile_revision, plan.profile_revision);
    assert_eq!(view.rom_pack_revision, plan.rom_pack_revision);

    // Safety-bearing figures.
    assert_eq!(view.peak_capacity_required, plan.required_capacity);
    assert_eq!(view.safety_margin, plan.safety_margin);
    assert_eq!(view.permanent_removal_count, plan.removal_count());
    assert!(view.inventory_fresh);
    assert!(view.executable);
}

#[test]
fn a_stale_plan_is_not_executable_even_when_nothing_about_it_is_wrong() {
    let plan = a_plan();
    let fresh = PlanView::of(&plan, true);
    let stale = PlanView::of(&plan, false);

    assert!(fresh.executable);
    assert!(
        !stale.executable,
        "freshness is part of executability: the device changed underneath the user"
    );
}

#[test]
fn a_removal_acknowledgement_must_match_what_was_displayed() {
    let plan = a_plan();
    let view = PlanView::of(&plan, true);
    let shown = view.permanent_removal_count;

    assert!(view.removal_acknowledgement_matches(shown));
    assert!(
        !view.removal_acknowledgement_matches(shown + 1),
        "an acknowledgement must never cover more destruction than was on screen"
    );
}

#[test]
fn transport_limitations_are_disclosed_in_the_plan_view() {
    let plan = a_plan();
    let view = PlanView::of(&plan, true);

    assert!(
        view.transport_limitations
            .iter()
            .any(|limit| limit.contains("atomically")),
        "a binding that cannot publish atomically must say so, got {:?}",
        view.transport_limitations
    );
}

#[test]
fn an_indeterminate_outcome_is_its_own_kind_and_demands_a_refresh() {
    let outcome = ExecutionOutcome::Indeterminate {
        reason: "target disconnected during write".into(),
        report: Default::default(),
    };
    let view = OutcomeView::of(&outcome);

    assert_eq!(view.kind, OutcomeKind::Indeterminate);
    assert!(
        view.refresh_required,
        "the application cannot claim anything about the device until it looks again"
    );
}

#[test]
fn only_a_clean_completion_clears_the_refresh_requirement() {
    for (outcome, expected) in [
        (
            ExecutionOutcome::Completed {
                report: Default::default(),
            },
            false,
        ),
        (
            ExecutionOutcome::Cancelled {
                report: Default::default(),
            },
            true,
        ),
        (
            ExecutionOutcome::Incomplete {
                reason: "write failed".into(),
                report: Default::default(),
            },
            true,
        ),
    ] {
        assert_eq!(
            OutcomeView::of(&outcome).refresh_required,
            expected,
            "unexpected refresh requirement for {outcome:?}"
        );
    }
}

#[test]
fn a_snapshot_round_trips_across_the_webview_boundary() {
    // The UI's authority is this payload, so it must survive serialization
    // exactly — a field lost in transit would be a field the UI silently
    // stops showing.
    let plan = a_plan();
    let snapshot = Snapshot {
        step: WizardStep::ReviewPlan,
        rom_pack: None,
        media_target: None,
        plan: Some(PlanView::of(&plan, true)),
        progress: None,
        outcome: None,
        recovery_disclosure: vec!["1 permanent removal was not performed".into()],
    };

    let json = serde_json::to_string(&snapshot).unwrap();
    let restored: Snapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, snapshot);

    // Field names reach the WebView in the casing the bindings declare.
    assert!(json.contains("permanentRemovalCount"));
    assert!(json.contains("recoveryDisclosure"));
    assert!(json.contains("inventoryFresh"));
}
