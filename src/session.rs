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
    Approval, CancellationToken, ExecutionOutcome, IntakeReport, MediaTargetChoice, OutcomeView,
    Progress, RomPackChoice, Snapshot, Store, StoreError, SyncCore, SyncError, SyncPlan, Transport,
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
    #[error("no Library is open, so nothing can be taken in")]
    NoLibrary,
    #[error("that folder is not one this application was asked to remember")]
    UnknownImportFolder,
    #[error("the folder could not be taken in: {0}")]
    Intake(String),
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

/// Opens a transport at a **locator** — for the filesystem, a directory path.
///
/// Keyed by locator rather than by target identity on purpose. A Media Target's
/// identity lives in its marker and never changes; the locator is merely the
/// last place it was seen. Connecting by identity would be circular, because
/// reading the marker is how the identity is discovered in the first place.
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
    last_scan: Option<crate::app::ScanSummary>,
    cancellation: CancellationToken,
    library: Option<crate::Library>,
    packs: Vec<RomPackChoice>,
    targets: Vec<MediaTargetChoice>,
    /// Built by the host once a ROM Pack is chosen: what that pack wants on a
    /// target. Held here so `build_plan` does not have to re-derive it.
    desired: Vec<crate::TargetArtifact>,
}

impl<T: Transport> Session<T> {
    pub fn new(store: Store, connect: Connect<T>) -> Self {
        let mut session = Self {
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
            last_scan: None,
            cancellation: CancellationToken::default(),
            library: None,
            packs: Vec::new(),
            targets: Vec::new(),
            desired: Vec::new(),
        };
        // Catalogues come from durable state, not from the caller. A session
        // constructed with a list handed in would show whatever the host
        // believed rather than what the user actually nominated, and the two
        // diverge the moment anything is added in another window.
        let _ = session.reload_catalogues();
        session
    }

    /// Re-reads the ROM Pack and Media Target catalogues from durable state.
    ///
    /// Connectedness is decided by *trying* the last known locator. A target
    /// whose card is unplugged is listed and marked disconnected rather than
    /// hidden — the user nominated it, and its absence is information.
    pub fn reload_catalogues(&mut self) -> Result<(), SessionError> {
        self.packs = self
            .store
            .rom_packs()?
            .into_iter()
            .map(|row| RomPackChoice {
                // An untitled pack shows its identifier. Worse than a name, and
                // better than a blank row the user cannot tell apart.
                title: row.title.unwrap_or_else(|| row.rom_pack_id.clone()),
                rom_pack_id: row.rom_pack_id,
                revision: row.revision,
                rom_set_count: row.rom_set_count,
            })
            .collect();

        let known = self.store.media_targets()?;
        let mut targets = Vec::with_capacity(known.len());
        for row in known {
            let connected = match row.last_locator.as_deref() {
                Some(locator) => (self.connect)(locator).is_ok(),
                None => false,
            };
            targets.push(MediaTargetChoice {
                label: row.label.unwrap_or_else(|| row.target_id.clone()),
                target_id: row.target_id,
                binding_locator: row.last_locator,
                connected,
            });
        }
        self.targets = targets;
        Ok(())
    }

    /// Scans a remembered folder and gathers what it finds into a ROM Pack.
    ///
    /// Separate from nominating on purpose. Remembering a folder is cheap and
    /// reversible; reading every file in it is neither, and #62 is explicit
    /// that the application never walks the user's disks on its own schedule.
    pub fn scan_import_folder(&mut self, folder_id: i64) -> Result<IntakeReport, SessionError> {
        let library = self.library.as_ref().ok_or(SessionError::NoLibrary)?;
        let path = self
            .store
            .import_folders()?
            .into_iter()
            .find(|(id, _)| *id == folder_id)
            .map(|(_, path)| path)
            .ok_or(SessionError::UnknownImportFolder)?;

        let report = crate::take_in(library, &self.store, std::path::Path::new(&path), now())
            .map_err(|error| SessionError::Intake(error.to_string()))?;
        self.store.mark_folder_scanned(folder_id, now())?;

        // A scan that produced a pack changes what the user can choose, so the
        // catalogue is refreshed here rather than leaving the UI to guess that
        // it should ask again.
        self.reload_catalogues()?;
        Ok(report)
    }

    /// Scans every remembered folder.
    ///
    /// One command rather than one per folder, because "look for my games now"
    /// is the whole of the user's intention and the boundary stays narrower for
    /// it — nothing has to name a folder, so nothing has to carry a path or an
    /// identifier the frontend could have invented.
    ///
    /// A folder that has gone missing is reported and skipped rather than
    /// failing the run. An unplugged drive must not stop the folders that are
    /// still there from being scanned.
    pub fn scan_all_import_folders(&mut self) -> Result<Vec<IntakeReport>, SessionError> {
        let folders = self.store.import_folders()?;
        let mut reports = Vec::new();
        for (folder_id, _) in folders {
            match self.scan_import_folder(folder_id) {
                Ok(report) => reports.push(report),
                Err(SessionError::NoLibrary) => return Err(SessionError::NoLibrary),
                Err(_) => continue,
            }
        }

        // Recorded on the snapshot so the UI can show what was refused. A scan
        // that reported only its successes would leave a user whose collection
        // is missing a game with no way to find out which one.
        self.last_scan = Some(crate::app::ScanSummary {
            folders_scanned: reports.len(),
            rom_sets_added: reports.iter().map(|report| report.rom_sets.len()).sum(),
            declined: reports
                .iter()
                .flat_map(|report| &report.declined)
                .map(|diagnostic| crate::app::DeclinedFile {
                    path: diagnostic
                        .location
                        .source
                        .clone()
                        .unwrap_or_else(|| "an unnamed file".to_owned()),
                    code: diagnostic.reason.as_str().to_owned(),
                    remediation: diagnostic.remediation().to_owned(),
                })
                .collect(),
        });
        Ok(reports)
    }

    /// Remembers a folder to look in for ROMs. Scanned only when asked.
    pub fn nominate_import_folder(&mut self, path: &str) -> Result<i64, SessionError> {
        Ok(self.store.remember_import_folder(path, None)?)
    }

    pub fn import_folders(&self) -> Result<Vec<(i64, String)>, SessionError> {
        Ok(self.store.import_folders()?)
    }

    /// Remembers a directory as a Media Target.
    ///
    /// If it already carries a marker, that marker's identity wins — this is
    /// the same target the application managed before, whatever drive letter it
    /// arrived on this time. Only a directory with no marker gets a new
    /// identity, and even then nothing is written to it here: nomination
    /// records intent, and [`Self::initialize_target`] is what claims the
    /// device.
    pub fn nominate_media_target(
        &mut self,
        locator: &str,
        label: &str,
    ) -> Result<MediaTargetChoice, SessionError> {
        let mut transport = (self.connect)(locator).map_err(SessionError::Transport)?;
        let capabilities = transport.capabilities();
        let existing = transport
            .marker()
            .map_err(|error| SessionError::Transport(error.to_string()))?;

        let target_id = match existing {
            Some(marker) => marker.target_id,
            None => mint_target_id(locator),
        };

        self.store.upsert_target(&target_id, 1)?;
        self.store
            .record_binding(&target_id, locator, &capabilities, now())?;
        self.store.set_target_label(&target_id, label)?;
        self.reload_catalogues()?;

        self.targets
            .iter()
            .find(|target| target.target_id == target_id)
            .cloned()
            .ok_or(SessionError::UnknownMediaTarget)
    }

    /// Attaches the Library that owns imported content.
    ///
    /// Optional because most of the workflow does not need one — a session that
    /// only syncs an already-populated Library never imports anything — and the
    /// suites that exercise ordering rules should not have to build storage
    /// they never touch.
    pub fn set_library(&mut self, library: crate::Library) {
        self.library = Some(library);
    }

    /// Replaces how transports are opened. Exists for tests that need a device
    /// to become unreachable partway through.
    pub fn set_connect(&mut self, connect: Connect<T>) {
        self.connect = connect;
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
            last_scan: self.last_scan.clone(),
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

        let locator = choice
            .binding_locator
            .clone()
            .ok_or(SessionError::NotConnected)?;
        let transport = (self.connect)(&locator).map_err(SessionError::Transport)?;
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

/// A fresh Media Target identity for a directory with no marker.
///
/// Derived from the locator and the clock so two cards nominated from the same
/// mount point at different times are different targets. It is written into the
/// marker at initialization, and from then on the marker is the identity — this
/// value is never re-derived, so a later change of locator cannot change who
/// the target is.
fn mint_target_id(locator: &str) -> String {
    let seed = format!("{locator}:{}", now());
    format!("target-{}", &crate::sha256(seed.as_bytes())[..16])
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}
