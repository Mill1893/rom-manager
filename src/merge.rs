//! Three-way merge of owned metadata fields (issue #69).
//!
//! # Why three values and not two
//!
//! Comparing only "what the device has" against "what the Library wants" cannot
//! tell the difference between *the user edited this* and *we wrote this last
//! time and the Library has since changed*. Those need opposite responses:
//! overwrite the second, never overwrite the first.
//!
//! The third value — the **ledger**, what was last exported — is what makes
//! them distinguishable. Per field:
//!
//! | device vs ledger | desired vs device | outcome |
//! | --- | --- | --- |
//! | unchanged | differs | update: ours to change |
//! | unchanged | equal | refresh evidence only |
//! | **changed** | anything | **conflict**: the user changed it |
//! | no ledger entry | equal | adoption offer |
//! | no ledger entry | differs | conflict: pre-existing and not ours |
//!
//! # Conflicts are never resolved automatically
//!
//! A device-side change to a field we own is never silently overwritten *or*
//! silently imported. Both directions destroy an intention: overwriting loses
//! the user's edit, importing turns an edit on one device into a Library-wide
//! fact they never asked for. So a conflict stops and asks.

use std::collections::BTreeMap;

/// What the ledger recorded at last export, for one entry.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LedgerEntry {
    /// Field values as ROM Manager last wrote them.
    pub exported: BTreeMap<String, String>,
    /// The document fingerprint observed when that export was verified.
    pub document_fingerprint: String,
}

/// What should happen to one field.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FieldOutcome {
    /// Not present on the device, and we have a value for it.
    Add { value: String },
    /// Ours, unchanged on the device, and the Library now wants something else.
    Update { from: String, to: String },
    /// Device and desired already agree; only the ledger needs refreshing.
    RefreshEvidence,
    /// Present, equal to what we want, but not yet ours. Requires explicit
    /// adoption — approval grants authority over this exact value only.
    OfferAdoption { value: String },
    /// The device value changed under us, or a pre-existing value differs.
    /// Never resolved automatically.
    Conflict {
        ledger: Option<String>,
        device: String,
        desired: String,
    },
}

impl FieldOutcome {
    /// Whether applying this requires the user to decide first.
    pub fn needs_user_decision(&self) -> bool {
        matches!(self, Self::OfferAdoption { .. } | Self::Conflict { .. })
    }

    /// Whether this outcome writes to the document without asking.
    pub fn writes_silently(&self) -> bool {
        matches!(self, Self::Add { .. } | Self::Update { .. })
    }
}

/// Merges one field.
pub fn merge_field(
    ledger: Option<&String>,
    device: Option<&String>,
    desired: Option<&String>,
) -> Option<FieldOutcome> {
    match (ledger, device, desired) {
        // Nothing wanted and nothing there.
        (_, None, None) => None,

        // Absent on the device, and we have a value: ours to add.
        (_, None, Some(desired)) => Some(FieldOutcome::Add {
            value: desired.clone(),
        }),

        // Present on the device but the Library no longer wants it. Retirement
        // is issue #70's; here it is simply not an update.
        (_, Some(_), None) => None,

        (ledger, Some(device), Some(desired)) => {
            let ours_unchanged = ledger.is_some_and(|recorded| recorded == device);

            if ours_unchanged {
                return Some(if device == desired {
                    FieldOutcome::RefreshEvidence
                } else {
                    FieldOutcome::Update {
                        from: device.clone(),
                        to: desired.clone(),
                    }
                });
            }

            // Not ours, or ours and changed underneath us.
            if ledger.is_none() && device == desired {
                // Pre-existing and already correct: an adoption offer, not a
                // silent claim.
                return Some(FieldOutcome::OfferAdoption {
                    value: device.clone(),
                });
            }

            Some(FieldOutcome::Conflict {
                ledger: ledger.cloned(),
                device: device.clone(),
                desired: desired.clone(),
            })
        }
    }
}

/// Merges every field of one entry.
pub fn merge_entry(
    ledger: Option<&LedgerEntry>,
    device: &BTreeMap<String, String>,
    desired: &BTreeMap<String, String>,
) -> BTreeMap<String, FieldOutcome> {
    let empty = BTreeMap::new();
    let exported = ledger.map(|entry| &entry.exported).unwrap_or(&empty);

    let mut fields: Vec<&String> = device.keys().chain(desired.keys()).collect();
    fields.sort_unstable();
    fields.dedup();

    fields
        .into_iter()
        .filter_map(|field| {
            merge_field(exported.get(field), device.get(field), desired.get(field))
                .map(|outcome| (field.clone(), outcome))
        })
        .collect()
}

/// Whether any field in a merge needs the user before anything can be written.
pub fn requires_user_decision(outcomes: &BTreeMap<String, FieldOutcome>) -> bool {
    outcomes.values().any(FieldOutcome::needs_user_decision)
}

/// The conflicting fields, for presentation.
pub fn conflicts(outcomes: &BTreeMap<String, FieldOutcome>) -> Vec<&String> {
    outcomes
        .iter()
        .filter(|(_, outcome)| matches!(outcome, FieldOutcome::Conflict { .. }))
        .map(|(field, _)| field)
        .collect()
}
