//! The workflow state machine behind the desktop commands (issue #34).
//!
//! `app.rs` defines what crosses the boundary. This is what happens when it
//! does — the eight commands the WebView can call, and the rules about when
//! each is allowed.
//!
//! # Nothing happens on its own
//!
//! No command here refreshes, re-plans, or resumes as a side effect of another.
//! That is a deliberate cost: it means the user has to press Refresh after a
//! device changes, rather than the application quietly re-observing and
//! carrying on.
//!
//! The alternative is worse in a specific way. An application that silently
//! re-observed would produce a plan the user never reviewed, and then execute
//! it under an approval granted for a different one. Every automatic step
//! between "the user looked at this" and "bytes were written" is a step where
//! what they approved and what happened can diverge.
//!
//! # The step is stored, not inferred
//!
//! [`WizardStep`] is held explicitly rather than derived from which fields are
//! populated. Inferring it would make two states indistinguishable that must
//! not be — "a plan was built and is awaiting review" and "a plan was built,
//! executed, and the result dismissed" both leave a plan in hand.
//!
//! # An approval is single-use, durably
//!
//! Approvals are taken from the store rather than held in memory, so a crash
//! between granting and executing cannot leave one available to be spent twice.

use crate::{
    Approval, CancellationToken, ExecutionOutcome, MediaTargetChoice, OutcomeView, Progress,
    RomPackChoice, Snapshot, Store, StoreError, SyncCore, SyncError, SyncPlan, Transport,
    WizardStep, app::PlanView,
};

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("that step is not available yet: {0}")]
    OutOfOrder(&'static str),
    #[error("no ROM Pack with that identifier is available")]
    UnknownRomPack,
    #[error("no Media Target with that identifier is available")]
    UnknownMediaTarget,
    #[error("that device is not connected")]
    NotConnected,
    #[error("this device has not been set up for ROM Manager yet")]
    NotInitialized,
    #[error("setting up a device must be confirmed")]
    NotConfirmed,
    #[error("this device changed since the plan was built. Refresh and build a new plan.")]
    PlanStale,
    #[error("the acknowledgement does not match what the plan displayed")]
    AcknowledgementMismatch,
    #[error("that approval has already been used")]
    ApprovalSpent,
    #[error("the sync core refused: {0}")]
    Core(String),
    #[error("durable state could not be read or written: {0}")]
    Store(String),
    #[error("the device could not be reached: {0}")]
    Transport(String),
}

impl From<StoreError> for SessionError {
    fn from(error: StoreError) -> Self {
        Self::Store(error.to_string())
    }
}

impl From<SyncError> for SessionError {
    fn from(error: SyncError) -> Self {
        Self::Core(format!("{error:?}"))
    }
}

/// Connects a Media Target identifier to a transport.
///
/// Supplied by the host so the session stays testable: the suites drive it with
/// a fake transport and the desktop application supplies a real one.
pub type Connect<T> = Box<dyn FnMut(&str) -> Result<T, String> + Send>;

pub struct Session<T: Transport> {
    store: Store,
    connect: Connect<T>,
    core: Option<SyncCore<T>>,
    step: WizardStep,
    rom_pack: Option<RomPackChoice>,
    media_target: Option<MediaTargetChoice>,
    plan: Option<SyncPlan>,
    progress: Option<Progress>,
    outcome: Option<OutcomeView>,
    recovery_disclosure: Vec<String>,
    cancellation: CancellationToken,
    packs: Vec<RomPackChoice>,
    targets: Vec<MediaTargetChoice>,
    /// Built by the host once a ROM Pack is chosen: what that pack wants on a
    /// target. Held here so `build_plan` does not have to re-derive it.
    desired: Vec<crate::TargetArtifact>,
}

impl<T: Transport> Session<T> {
    pub fn new(
        store: Store,
        connect: Connect<T>,
        packs: Vec<RomPackChoice>,
        targets: Vec<MediaTargetChoice>,
    ) -> Self {
        Self {
            store,
            connect,
            core: None,
            step: WizardStep::SelectRomPack,
            rom_pack: None,
            media_target: None,
            plan: None,
            progress: None,
            outcome: None,
            recovery_disclosure: Vec::new(),
            cancellation: CancellationToken::default(),
            packs,
            targets,
            desired: Vec::new(),
        }
    }

    /// What a chosen ROM Pack wants on a target. Set by the host.
    pub fn set_desired(&mut self, desired: Vec<crate::TargetArtifact>) {
        self.desired = desired;
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn available_packs(&self) -> &[RomPackChoice] {
        &self.packs
    }

    pub fn available_targets(&self) -> &[MediaTargetChoice] {
        &self.targets
    }

    /// The whole current state. Every command returns one of these.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            step: self.step.clone(),
            rom_pack: self.rom_pack.clone(),
            media_target: self.media_target.clone(),
            plan: self.plan_view(),
            progress: self.progress.clone(),
            outcome: self.outcome.clone(),
            recovery_disclosure: self.recovery_disclosure.clone(),
        }
    }

    /// The plan as the UI sees it, including whether the observation it rests
    /// on is still fresh.
    fn plan_view(&self) -> Option<PlanView> {
        let plan = self.plan.as_ref()?;
        let fresh = self
            .store
            .fresh_inventory_digest(&plan.target_id)
            .ok()
            .flatten()
            .is_some_and(|digest| digest == plan.inventory_digest);
        Some(PlanView::of(plan, fresh))
    }

    // ── Commands ────────────────────────────────────────────────────────────

    /// Re-reads authoritative state. Safe at any point, changes nothing.
    pub fn load_snapshot(&mut self) -> Result<Snapshot, SessionError> {
        Ok(self.snapshot())
    }

    pub fn select_rom_pack(
        &mut self,
        rom_pack_id: &str,
        revision: u32,
    ) -> Result<Snapshot, SessionError> {
        let choice = self
            .packs
            .iter()
            .find(|pack| pack.rom_pack_id == rom_pack_id && pack.revision == revision)
            .ok_or(SessionError::UnknownRomPack)?
            .clone();

        // Choosing a different pack invalidates any plan built for the old one.
        // Keeping it would let a user approve a plan for content they are no
        // longer syncing.
        self.rom_pack = Some(choice);
        self.plan = None;
        self.step = WizardStep::SelectMediaTarget;
        Ok(self.snapshot())
    }

    pub fn select_media_target(&mut self, target_id: &str) -> Result<Snapshot, SessionError> {
        if self.rom_pack.is_none() {
            return Err(SessionError::OutOfOrder("choose a ROM Pack first"));
        }
        let choice = self
            .targets
            .iter()
            .find(|target| target.target_id == target_id)
            .ok_or(SessionError::UnknownMediaTarget)?
            .clone();
        if !choice.connected {
            return Err(SessionError::NotConnected);
        }

        let transport = (self.connect)(target_id).map_err(SessionError::Transport)?;
        let profile = crate::DeviceProfile::generic_nes();
        let mut core = SyncCore::new(
            transport,
            target_id,
            profile,
            self.desired.clone(),
            self.rom_pack
                .as_ref()
                .map_or(1, |pack| u64::from(pack.revision)),
        );
        // A target the application already knows keeps its manifest; a fresh
        // one has none, and initialization is the user's decision, not ours.
        core.replace_local_manifest(self.store.load_manifest(target_id)?);

        self.media_target = Some(choice);
        self.core = Some(core);
        self.plan = None;
        self.step = WizardStep::ReviewPlan;
        Ok(self.snapshot())
    }

    /// Claims a device for ROM Manager by writing its marker.
    ///
    /// A separate, confirmed command rather than something `select_media_target`
    /// does quietly. Writing a marker is how this application takes
    /// responsibility for a device's contents, and a user who plugged in the
    /// wrong card should get a question, not a claim.
    pub fn initialize_target(&mut self, confirmed: bool) -> Result<Snapshot, SessionError> {
        if !confirmed {
            return Err(SessionError::NotConfirmed);
        }
        let core = self
            .core
            .as_mut()
            .ok_or(SessionError::OutOfOrder("choose a device first"))?;
        core.initialize_target(true)?;
        if let Some(target) = self.media_target.as_ref() {
            self.store.upsert_target(&target.target_id, 1)?;
        }
        self.plan = None;
        self.step = WizardStep::ReviewPlan;
        Ok(self.snapshot())
    }

    /// Observes the target afresh. Never called as a side effect of anything.
    pub fn refresh_target(&mut self) -> Result<Snapshot, SessionError> {
        let core = self
            .core
            .as_mut()
            .ok_or(SessionError::OutOfOrder("choose a device first"))?;
        // A device with no marker is not a broken device — it is one this
        // application has not been asked to manage. Saying so lets the UI offer
        // set-up instead of showing a conflict the user cannot act on.
        match core.refresh() {
            Ok(()) => {}
            Err(SyncError::Blocked) | Err(SyncError::MarkerConflict) => {
                if core.local_manifest().is_none() {
                    return Err(SessionError::NotInitialized);
                }
                return Err(SessionError::Core("MarkerConflict".into()));
            }
            Err(other) => return Err(other.into()),
        }

        // A refresh invalidates the plan that rested on the old observation.
        // Leaving it in place would let the user execute a plan describing a
        // target state that has just been superseded.
        self.plan = None;
        self.step = WizardStep::ReviewPlan;
        Ok(self.snapshot())
    }

    pub fn build_plan(&mut self) -> Result<Snapshot, SessionError> {
        let core = self
            .core
            .as_mut()
            .ok_or(SessionError::OutOfOrder("choose a device first"))?;
        let plan = core.build_plan()?;
        self.store.save_plan(&plan, now())?;
        // The plan carries the digest of the observation it was built from.
        // Recording it is what makes "is this plan still describing the device
        // I looked at?" answerable after a restart, rather than a question the
        // session can only answer while it happens to still be running.
        self.store
            .record_inventory(&plan.target_id, &plan.inventory_digest, now())?;
        self.plan = Some(plan);
        self.step = WizardStep::ReviewPlan;
        Ok(self.snapshot())
    }

    /// Approves and runs.
    ///
    /// `acknowledged_removals` must equal what the plan displayed. A caller
    /// that acknowledged three removals cannot authorize a plan that performs
    /// four, and one that acknowledged none cannot authorize any.
    pub fn approve_and_execute(
        &mut self,
        plan_digest: &str,
        acknowledged_removals: usize,
    ) -> Result<Snapshot, SessionError> {
        let plan = self
            .plan
            .clone()
            .ok_or(SessionError::OutOfOrder("build a plan first"))?;
        if plan.digest != plan_digest {
            // The UI is approving something other than what it holds, which
            // means the two have diverged. Refusing is the only safe answer.
            return Err(SessionError::PlanStale);
        }
        let view = self.plan_view().ok_or(SessionError::PlanStale)?;
        if !view.inventory_fresh {
            return Err(SessionError::PlanStale);
        }
        if !view.removal_acknowledgement_matches(acknowledged_removals) {
            return Err(SessionError::AcknowledgementMismatch);
        }

        // Granted and stored before execution, then taken. Storing it means a
        // crash mid-write cannot leave an approval that could be spent again.
        let approval = Approval::grant(&plan, acknowledged_removals);
        self.store.save_approval(&approval, now())?;
        let approval = self
            .store
            .take_approval(&plan.digest)?
            .ok_or(SessionError::ApprovalSpent)?;

        let core = self
            .core
            .as_mut()
            .ok_or(SessionError::OutOfOrder("choose a device first"))?;

        self.cancellation = CancellationToken::default();
        self.step = WizardStep::Executing;

        let outcome = core.execute(&plan, approval, &self.cancellation)?;
        self.recovery_disclosure = match &outcome {
            ExecutionOutcome::Completed { report }
            | ExecutionOutcome::Cancelled { report }
            | ExecutionOutcome::Incomplete { report, .. }
            | ExecutionOutcome::Indeterminate { report, .. } => report.recovery_disclosure(),
        };
        self.outcome = Some(OutcomeView::of(&outcome));

        // Whatever happened, the observation the plan rested on is no longer
        // trustworthy. Marking it stale forces a refresh before any further
        // claim about this target's contents.
        self.store.mark_inventory_stale(&plan.target_id)?;
        if let Some(manifest) = core.local_manifest() {
            self.store.save_manifest(manifest)?;
        }

        self.plan = None;
        self.progress = None;
        self.step = WizardStep::Result;
        Ok(self.snapshot())
    }

    /// Asks the running operation to stop. It stops at a safe point, not
    /// immediately — a write torn in half is worse than one that finishes.
    pub fn request_cancellation(&mut self) -> Result<Snapshot, SessionError> {
        self.cancellation.cancel();
        if let Some(progress) = self.progress.as_mut() {
            progress.cancellation = crate::CancellationState::Requested;
        }
        Ok(self.snapshot())
    }

    /// Clears a terminal result and returns to the start.
    pub fn dismiss_result(&mut self) -> Result<Snapshot, SessionError> {
        if self.step != WizardStep::Result {
            return Err(SessionError::OutOfOrder("there is no result to dismiss"));
        }
        self.outcome = None;
        self.recovery_disclosure.clear();
        self.step = WizardStep::SelectRomPack;
        Ok(self.snapshot())
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}
