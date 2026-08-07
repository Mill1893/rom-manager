//! Ordering a combined-target sync (issue #72).
//!
//! # Why the order is the safety property
//!
//! On a target holding both ROMs and gamelists, three things happen: content is
//! added, metadata is published, and content no longer selected is removed. The
//! order is not a preference — it is what stops a metadata failure from costing
//! the user files:
//!
//! 1. **Add and verify ROM content.** Removals are only ever justified by
//!    additions already verified in the same operation.
//! 2. **Publish and reread metadata.** If this fails, the target still has
//!    every ROM it had before plus the new ones.
//! 3. **Remove managed ROMs last** — and only if step 2 succeeded.
//!
//! Removing before publishing would mean a metadata failure left the user with
//! fewer ROMs *and* a stale gamelist. Removing after a failed publish would
//! mean the gamelist still describes files that are gone.
//!
//! # Split targets are two operations, not one transaction
//!
//! When ROMs and metadata live on different Media Targets there is no
//! cross-target transaction, because there cannot be one — they are separate
//! devices that can be unplugged independently. Both plans bind the same ROM
//! Pack revision and Device Profile version, but each is approved on its own,
//! and the ROM-content target must converge first.
//!
//! A metadata failure then reports `ROM content synced; metadata pending`. That
//! wording matters: it is a true statement about a partially-complete outcome,
//! not a failure that implies the content did not land.

use crate::{DestinationRole, RoleAssignment};

/// A stage of a combined-target sync.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncStage {
    AddAndVerifyContent,
    PublishAndRereadMetadata,
    RemoveManagedContent,
}

/// The fixed order for a combined target.
pub const COMBINED_ORDER: [SyncStage; 3] = [
    SyncStage::AddAndVerifyContent,
    SyncStage::PublishAndRereadMetadata,
    SyncStage::RemoveManagedContent,
];

/// How a combined-target sync ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CombinedOutcome {
    /// Everything ran.
    Completed,
    /// Content is on the target; metadata did not publish. Removals were
    /// skipped, so nothing was lost.
    ContentSyncedMetadataPending {
        reason: String,
        removals_skipped: usize,
    },
    /// Content itself failed; metadata was never attempted.
    ContentFailed { reason: String },
}

impl CombinedOutcome {
    /// The message the user sees. Phrased as what is true, not as a failure —
    /// their ROMs did sync.
    pub fn summary(&self) -> String {
        match self {
            Self::Completed => "Sync complete.".into(),
            Self::ContentSyncedMetadataPending { .. } => {
                "ROM content synced; metadata pending.".into()
            }
            Self::ContentFailed { reason } => format!("Sync did not complete: {reason}"),
        }
    }

    /// Whether verified content was left in place despite the failure. Always
    /// true except when content itself failed — a metadata problem never rolls
    /// back ROMs that were already verified onto the device.
    pub fn content_retained(&self) -> bool {
        !matches!(self, Self::ContentFailed { .. })
    }
}

/// Runs the stages in order, stopping in the way each failure requires.
///
/// `content` and `metadata` return `Ok(())` or a reason. `removals` is invoked
/// only when both succeeded, and receives nothing to decide — by the time it
/// runs, its justification has already been established.
pub fn run_combined(
    pending_removals: usize,
    content: impl FnOnce() -> Result<(), String>,
    metadata: impl FnOnce() -> Result<(), String>,
    removals: impl FnOnce(usize),
) -> CombinedOutcome {
    if let Err(reason) = content() {
        // Metadata is never attempted: a gamelist describing content that did
        // not land would be wrong the moment it was written.
        return CombinedOutcome::ContentFailed { reason };
    }

    if let Err(reason) = metadata() {
        // Removals are skipped. Losing files *and* failing to update metadata
        // is the worst available outcome, and it is entirely avoidable.
        return CombinedOutcome::ContentSyncedMetadataPending {
            reason,
            removals_skipped: pending_removals,
        };
    }

    removals(pending_removals);
    CombinedOutcome::Completed
}

/// Whether two role-paired plans may be executed, and in what order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SplitReadiness {
    /// One target: a single plan, ordered by [`COMBINED_ORDER`].
    Combined,
    /// Two targets: the ROM-content plan must converge before metadata runs.
    ContentFirst {
        rom_content: String,
        frontend_metadata: String,
    },
    /// The pairing is unusable, so nothing may be planned against it.
    Blocked(&'static str),
}

/// Decides how a role assignment must be executed.
pub fn split_readiness(
    assignment: &RoleAssignment,
    rom_content_is_current: bool,
) -> SplitReadiness {
    if !assignment.is_usable() {
        return SplitReadiness::Blocked("the Destination Role pairing is not confirmed");
    }
    if assignment.is_combined() {
        return SplitReadiness::Combined;
    }
    if !rom_content_is_current {
        // Metadata describing content that has not converged would name files
        // that are not there.
        return SplitReadiness::Blocked("the ROM-content target is not current");
    }
    SplitReadiness::ContentFirst {
        rom_content: assignment
            .target_for(DestinationRole::RomContent)
            .expect("usable assignment resolves both roles")
            .to_owned(),
        frontend_metadata: assignment
            .target_for(DestinationRole::FrontendMetadata)
            .expect("usable assignment resolves both roles")
            .to_owned(),
    }
}

/// A preview row: one metadata action, by system, path, and field.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataPreviewRow {
    pub system_key: String,
    pub entry_path: String,
    pub field: Option<String>,
    pub action: MetadataAction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetadataAction {
    Add,
    Update,
    Adopt,
    Retire,
    OmitIneligible,
    Conflict,
    RecoveryChange,
    PreservedSharedState,
}

/// The preview the user approves, plus the limits of the transport carrying it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MetadataPreview {
    pub rows: Vec<MetadataPreviewRow>,
    /// Disclosed, never silently accepted: an MTP target cannot replace a
    /// document atomically, so an interruption can leave it torn.
    pub atomic_publication: bool,
}

impl MetadataPreview {
    pub fn count(&self, action: MetadataAction) -> usize {
        self.rows.iter().filter(|row| row.action == action).count()
    }

    /// Whether anything here needs the user before publication may proceed.
    pub fn requires_decision(&self) -> bool {
        self.count(MetadataAction::Conflict) > 0 || self.count(MetadataAction::Adopt) > 0
    }
}
