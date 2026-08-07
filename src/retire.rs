//! Retiring projections and removing ineligible fields (issue #70).
//!
//! # Withdrawing is harder than writing
//!
//! Adding a field only has to be correct. Removing one has to be correct
//! *about the past*: it must remove only what ROM Manager put there and only
//! while it is still what ROM Manager put there. A removal that guesses can
//! delete something the user typed.
//!
//! So retirement is gated on the ledger twice — the field must be recorded as
//! ours, and its current value must still match what was recorded. Anything
//! else is a conflict, handled by the normal flow rather than by deleting.
//!
//! # A move is not a rename
//!
//! When a ROM's canonical path or system changes, the old projection is retired
//! and a new one created. Treating it as a rename would carry the old entry's
//! state across, including frontend-owned state that was attached to a
//! *different* file — the user's play count for one game landing on another.

use std::collections::BTreeMap;

use crate::{FieldOutcome, GameEntry, LedgerEntry, OWNED_FIELDS};

/// What retiring a projection would do.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Retirement {
    /// Owned fields still matching the ledger, safe to remove.
    pub fields_to_remove: Vec<String>,
    /// Owned fields whose value changed under us. These block retirement.
    pub changed_fields: Vec<String>,
    /// Whether the whole `<game>` node may be deleted.
    pub remove_whole_entry: bool,
    /// Why the node cannot be deleted, when it cannot.
    pub retained_because: Option<&'static str>,
}

impl Retirement {
    /// Retirement is blocked while any owned field has changed — removing it
    /// would discard an edit the user made.
    pub fn is_blocked(&self) -> bool {
        !self.changed_fields.is_empty()
    }
}

/// Plans the retirement of one projection.
pub fn plan_retirement(entry: &GameEntry, ledger: Option<&LedgerEntry>) -> Retirement {
    let mut retirement = Retirement::default();
    let empty = BTreeMap::new();
    let exported = ledger.map(|entry| &entry.exported).unwrap_or(&empty);

    for (tag, current) in &entry.children {
        if tag == "path" || !OWNED_FIELDS.contains(&tag.as_str()) {
            continue;
        }
        match exported.get(tag) {
            // Ours and untouched: safe to remove.
            Some(recorded) if recorded == current => retirement.fields_to_remove.push(tag.clone()),
            // Ours, but changed underneath us.
            Some(_) => retirement.changed_fields.push(tag.clone()),
            // Never ours. Leaving it is the only safe action.
            None => {}
        }
    }

    if retirement.is_blocked() {
        retirement.retained_because = Some("an owned field was changed on the device");
        return retirement;
    }

    // The whole node goes only when the ledger proves we created it and it
    // holds nothing but its path and our unchanged fields.
    let created_by_us = ledger.is_some();
    if !created_by_us {
        retirement.retained_because = Some("the ledger does not show this entry as ours");
    } else if entry.has_frontend_state() {
        retirement.retained_because = Some("the entry carries frontend-owned state");
    } else if !entry.holds_only_owned_state() {
        retirement.retained_because = Some("the entry carries unknown state");
    } else {
        retirement.remove_whole_entry = true;
    }

    retirement
}

/// Why a field can no longer be exported.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ineligibility {
    /// Provider terms no longer permit exporting this value.
    ProviderTerms,
    /// The value needs attribution ES-DE output cannot carry.
    UnrepresentableAttribution,
    /// Adapter policy excludes the field.
    AdapterPolicy,
}

/// What to do about a field that has become ineligible.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EligibilityAction {
    /// The value still matches the ledger, so removing it is safe and is shown
    /// explicitly rather than happening quietly.
    Remove {
        field: String,
        reason: Ineligibility,
    },
    /// The device value diverged. Removing it would discard a user edit, so
    /// this goes through the ordinary conflict flow instead.
    Conflict {
        field: String,
        outcome: FieldOutcome,
    },
    /// Nothing on the device to remove.
    NothingToDo,
}

/// Decides how to withdraw an ineligible field.
///
/// Never invents a substitute: if a value cannot be exported with the
/// attribution it requires, it is omitted field by field rather than replaced
/// with another provider's value or a nonstandard attribution artifact.
pub fn withdraw_ineligible_field(
    field: &str,
    reason: Ineligibility,
    ledger: Option<&String>,
    device: Option<&String>,
) -> EligibilityAction {
    match (ledger, device) {
        (_, None) => EligibilityAction::NothingToDo,
        (Some(recorded), Some(current)) if recorded == current => EligibilityAction::Remove {
            field: field.to_owned(),
            reason,
        },
        (ledger, Some(current)) => EligibilityAction::Conflict {
            field: field.to_owned(),
            outcome: FieldOutcome::Conflict {
                ledger: ledger.cloned(),
                device: current.clone(),
                // The Library wants it gone, which is not a value.
                desired: String::new(),
            },
        },
    }
}

/// A projection whose path or system changed.
///
/// Deliberately modelled as two operations rather than a rename: carrying the
/// old entry across would bring frontend-owned state attached to a *different*
/// file with it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectionMove {
    pub retire: String,
    pub create: String,
}

impl ProjectionMove {
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            retire: from.into(),
            create: to.into(),
        }
    }

    /// Always false. A move is retirement plus creation; there is no rename
    /// path, and this exists so the intent is checkable rather than implied.
    pub fn is_rename(&self) -> bool {
        false
    }
}
