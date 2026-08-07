//! The desktop application boundary (issue #34).
//!
//! Everything the WebView can reach is here, and it is deliberately narrow.
//!
//! # Coarse commands, snapshot authority
//!
//! The UI never assembles state from a stream of events. Every command returns
//! a complete [`Snapshot`], and the snapshot is the authority — after startup,
//! after a missed event, after anything the frontend is unsure about, it calls
//! a command and replaces what it holds. Events exist only to make progress
//! *feel* live; losing every one of them costs responsiveness, never
//! correctness.
//!
//! # What the WebView cannot do
//!
//! There is no command here that takes a filesystem path, SQL, a shell string,
//! a URL, or anything else the frontend could use to reach past this boundary.
//! Selections are made by **identifier**, and the Rust side resolves them
//! against durable state. A frontend that wanted to sync a different target
//! could not express the request.
//!
//! # No automatic anything
//!
//! No command refreshes, re-plans, or resumes on its own. A stale plan cannot
//! execute; the user refreshes, re-plans, and re-approves. This mirrors the
//! recovery rule in issue #50 rather than reimplementing it.

use serde::{Deserialize, Serialize};

use crate::{BlockReason, PlanAction, RelativePath, SyncPlan};

/// Where the modal workflow currently is. The UI renders one of these; it never
/// infers a step from the presence or absence of other fields.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "step")]
pub enum WizardStep {
    /// Choosing which ROM Pack revision to sync.
    SelectRomPack,
    /// Choosing which Media Target to sync it to.
    SelectMediaTarget,
    /// A plan has been built and is awaiting review.
    ReviewPlan,
    /// Execution is under way.
    Executing,
    /// A terminal outcome the user has not yet dismissed.
    Result,
}

/// The complete state of the workflow. Returned by every command.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub step: WizardStep,
    pub rom_pack: Option<RomPackChoice>,
    pub media_target: Option<MediaTargetChoice>,
    pub plan: Option<PlanView>,
    pub progress: Option<Progress>,
    pub outcome: Option<OutcomeView>,
    /// Disclosure a replacement plan must carry after an interrupted run, per
    /// issue #50 §6. Empty when there is nothing to disclose.
    pub recovery_disclosure: Vec<String>,
    /// What the last scan found, until something else replaces it.
    pub last_scan: Option<ScanSummary>,
}

/// What a scan of the remembered folders produced.
///
/// The declined list is the part that matters. A scan that quietly imported
/// nine files out of ten and said "9 games added" would leave the user with a
/// collection missing a game they own and no way to find out which. Every file
/// the application refused is named, with a reason it can act on.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub folders_scanned: usize,
    pub rom_sets_added: usize,
    pub declined: Vec<DeclinedFile>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclinedFile {
    pub path: String,
    /// The stable machine-readable code, for reports and for matching.
    pub code: String,
    /// The sentence the user acts on.
    pub remediation: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RomPackChoice {
    pub rom_pack_id: String,
    pub revision: u32,
    pub title: String,
    pub rom_set_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaTargetChoice {
    pub target_id: String,
    pub label: String,
    /// Where it is currently attached. Evidence about this connection, never
    /// the target's identity — shown so the user can tell one device from
    /// another, not used to decide what is being synced.
    pub binding_locator: Option<String>,
    pub connected: bool,
}

/// Everything issue #34 requires a plan to display, in one payload so the UI
/// cannot render a partial plan by omission.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanView {
    pub plan_digest: String,
    pub target_id: String,
    pub binding_locator: String,
    pub profile_id: String,
    pub profile_revision: u32,
    pub rom_pack_revision: u64,
    /// False once anything the plan observed has changed. A stale plan is
    /// never executable.
    pub inventory_fresh: bool,
    pub inventory_digest: String,
    /// Capabilities the binding does not have, phrased for a person.
    pub transport_limitations: Vec<String>,
    pub actions: Vec<PlanAction>,
    pub preserved_unknowns: Vec<RelativePath>,
    pub preserved_duplicates: Vec<RelativePath>,
    pub preserved_unrepresentable: Vec<String>,
    pub missing_managed: Vec<RelativePath>,
    pub conflicts: Vec<BlockReason>,
    pub peak_capacity_required: u64,
    pub safety_margin: u64,
    /// What the user must acknowledge before this plan can run.
    pub permanent_removal_count: usize,
    pub executable: bool,
}

impl PlanView {
    pub fn of(plan: &SyncPlan, inventory_fresh: bool) -> Self {
        let transport_limitations = plan
            .blocked
            .iter()
            .filter_map(|reason| match reason {
                BlockReason::UnsupportedCapability { capability } => {
                    Some(format!("this connection cannot provide {capability}"))
                }
                _ => None,
            })
            .chain(
                (!plan.atomic_publication)
                    .then(|| "this connection cannot publish atomically".to_owned()),
            )
            .collect();

        Self {
            plan_digest: plan.digest.clone(),
            target_id: plan.target_id.clone(),
            binding_locator: plan.binding_locator.clone(),
            profile_id: plan.profile_id.clone(),
            profile_revision: plan.profile_revision,
            rom_pack_revision: plan.rom_pack_revision,
            inventory_fresh,
            inventory_digest: plan.inventory_digest.clone(),
            transport_limitations,
            actions: plan.actions.clone(),
            preserved_unknowns: plan.preserved_unknowns.clone(),
            preserved_duplicates: plan.preserved_duplicates.clone(),
            preserved_unrepresentable: plan.preserved_unrepresentable.clone(),
            missing_managed: plan.missing_managed.clone(),
            conflicts: plan.blocked.clone(),
            peak_capacity_required: plan.required_capacity,
            safety_margin: plan.safety_margin,
            permanent_removal_count: plan.removal_count(),
            // Freshness is part of executability, so a stale plan cannot run
            // even though nothing about its own contents changed.
            executable: plan.is_executable() && inventory_fresh,
        }
    }

    /// Whether `acknowledged` authorizes this plan's permanent removals.
    ///
    /// Compared against the count the user was *shown*, so an acknowledgement
    /// can never cover more destruction than was on screen.
    pub fn removal_acknowledgement_matches(&self, acknowledged: usize) -> bool {
        acknowledged == self.permanent_removal_count
    }
}

/// Which part of an operation is running. Named phases rather than a
/// percentage, because "verifying" and "writing" fail differently and the user
/// needs to know which one they are in.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Preparing,
    Writing,
    Verifying,
    Removing,
    Publishing,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub phase: Phase,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub artifacts_done: usize,
    pub artifacts_total: usize,
    /// The ROM Set currently being worked on, if any.
    pub current_rom_set: Option<String>,
    /// True while read-back verification is in flight, so the UI can say
    /// "verifying" rather than implying the write is already trustworthy.
    pub verifying: bool,
    pub cancellation: CancellationState,
    /// True only once the outcome has been written durably. Until then the UI
    /// must not tell the user the operation is safely finished.
    pub durably_recorded: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CancellationState {
    Running,
    /// The user asked to stop; the operation is finishing the step it is in.
    Requested,
    Stopped,
}

/// A terminal outcome as the UI shows it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutcomeView {
    pub kind: OutcomeKind,
    pub reason: Option<String>,
    pub performed: Vec<RelativePath>,
    pub not_attempted: Vec<RelativePath>,
    pub uncertain: Vec<RelativePath>,
    pub residue: Vec<RelativePath>,
    /// True when the user must refresh and build a new plan before doing
    /// anything else with this target.
    pub refresh_required: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutcomeKind {
    Completed,
    Cancelled,
    Incomplete,
    /// The application cannot establish what reached the target. Its own kind
    /// so the UI cannot present it as an ordinary failure.
    Indeterminate,
}

impl OutcomeView {
    pub fn of(outcome: &crate::ExecutionOutcome) -> Self {
        let report = outcome.report();
        let paths =
            |actions: &[PlanAction]| actions.iter().map(|action| action.path.clone()).collect();
        let (kind, reason) = match outcome {
            crate::ExecutionOutcome::Completed { .. } => (OutcomeKind::Completed, None),
            crate::ExecutionOutcome::Cancelled { .. } => (OutcomeKind::Cancelled, None),
            crate::ExecutionOutcome::Incomplete { reason, .. } => {
                (OutcomeKind::Incomplete, Some(reason.clone()))
            }
            crate::ExecutionOutcome::Indeterminate { reason, .. } => {
                (OutcomeKind::Indeterminate, Some(reason.clone()))
            }
        };
        Self {
            kind,
            reason,
            performed: paths(&report.performed),
            not_attempted: paths(&report.not_attempted),
            uncertain: paths(&report.uncertain),
            residue: report
                .residue
                .iter()
                .map(|residue| residue.path.clone())
                .collect(),
            // Anything but a clean completion leaves the target in a state the
            // application has not established.
            refresh_required: !matches!(kind, OutcomeKind::Completed),
        }
    }
}

/// The events the Rust core emits. Purely an optimization: every one of them
/// can be dropped without the UI becoming wrong, because the next command
/// returns a full [`Snapshot`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum AppEvent {
    ProgressChanged(Progress),
    StateChanged(Box<Snapshot>),
}
