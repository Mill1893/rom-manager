use std::collections::{BTreeMap, BTreeSet};

use crate::{
    Action, Approval, BlockReason, CancellationToken, DeviceProfile, ManagedArtifactManifest,
    ManagedEvidence, ManagementOrigin, PlanAction, RelativePath, SyncPlan, TargetArtifact,
    TargetMarker, Transport, TransportError, sha256,
};

const CAPACITY_MARGIN: u64 = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutionOutcome {
    /// Every planned action performed and verified.
    Completed,
    /// Stopped by the user at a point where target state is established.
    Cancelled,
    /// An action failed, and what reached the target *is* established.
    Incomplete {
        reason: String,
        residue: Vec<Residue>,
    },
    /// The application cannot establish what reached the target. Never
    /// downgraded to `Incomplete` to produce a tidier report; a refresh is
    /// mandatory before any subsequent claim about target contents.
    Indeterminate {
        reason: String,
        residue: Vec<Residue>,
    },
}

/// Something left at a named path that the application could not verify as its
/// own, and therefore did not delete.
///
/// This is a disclosure to the user, not privileged state. On the next planning
/// pass the path is simply content the manifest does not name — unknown content,
/// preserved and classified like any other. The record never grants authority
/// the failed operation did not establish.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Residue {
    pub path: RelativePath,
    /// The action that was in flight when the outcome became uncertain.
    pub attempted: Action,
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error("the Media Target marker conflicts with the selected target")]
    MarkerConflict,
    #[error("refresh the Media Target before planning")]
    RefreshRequired,
    #[error("the Sync Plan is blocked")]
    Blocked,
    #[error("the Sync Plan has changed since approval")]
    PlanChanged,
    #[error("permanent removal acknowledgement does not match the Sync Plan")]
    RemovalAcknowledgement,
    #[error("the approval does not authorize this Sync Plan against this binding: {0}")]
    ApprovalInvalid(&'static str),
    #[error("initializing or replacing a Media Target marker requires explicit confirmation")]
    ConfirmationRequired,
}

pub struct SyncCore<T: Transport> {
    transport: T,
    target_id: String,
    profile: DeviceProfile,
    desired: Vec<TargetArtifact>,
    rom_pack_revision: u64,
    local_manifest: Option<ManagedArtifactManifest>,
    refreshed: Option<crate::Inventory>,
    observed_marker: Option<TargetMarker>,
}

impl<T: Transport> SyncCore<T> {
    pub fn new(
        transport: T,
        target_id: impl Into<String>,
        profile: DeviceProfile,
        desired: Vec<TargetArtifact>,
        rom_pack_revision: u64,
    ) -> Self {
        Self {
            transport,
            target_id: target_id.into(),
            profile,
            desired,
            rom_pack_revision,
            local_manifest: None,
            refreshed: None,
            observed_marker: None,
        }
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    pub fn local_manifest(&self) -> Option<&ManagedArtifactManifest> {
        self.local_manifest.as_ref()
    }

    pub fn replace_local_manifest(&mut self, manifest: Option<ManagedArtifactManifest>) {
        self.local_manifest = manifest;
    }

    pub fn initialize_target(&mut self, confirmed: bool) -> Result<(), SyncError> {
        match self.transport.marker()? {
            Some(marker) if self.marker_is_valid(&marker) => Ok(()),
            Some(_) => Err(SyncError::MarkerConflict),
            None => {
                if !confirmed {
                    return Err(SyncError::ConfirmationRequired);
                }
                let marker = TargetMarker::new(&self.target_id);
                self.transport.write_marker(&marker)?;
                if self.transport.marker()? == Some(marker) {
                    Ok(())
                } else {
                    Err(SyncError::MarkerConflict)
                }
            }
        }
    }

    pub fn refresh(&mut self) -> Result<(), SyncError> {
        let marker = self.transport.marker()?;
        if !marker
            .as_ref()
            .is_some_and(|marker| self.marker_is_valid(marker))
        {
            self.refreshed = None;
            return Err(SyncError::MarkerConflict);
        }
        self.observed_marker = marker;
        self.refreshed = Some(self.transport.inventory()?);
        Ok(())
    }

    pub fn build_plan(&mut self) -> Result<SyncPlan, SyncError> {
        let inventory = self.refreshed.clone().ok_or(SyncError::RefreshRequired)?;
        let target_manifest = self.transport.manifest()?;
        let mut blocked = Vec::new();
        let manifest_is_valid = target_manifest
            .as_ref()
            .is_none_or(|manifest| self.manifest_is_valid(manifest));
        if target_manifest != self.local_manifest || !manifest_is_valid {
            blocked.push(BlockReason::ManifestDisagreement);
        }
        if let Some(target_manifest) = target_manifest.as_ref()
            && self.profile_revision_differs(target_manifest)
        {
            blocked.push(BlockReason::ProfileRevisionChanged {
                recorded: target_manifest.profile_revision,
                active: self.profile.revision,
            });
        }

        let manifest = target_manifest
            .filter(|_| manifest_is_valid)
            .unwrap_or_else(|| ManagedArtifactManifest::empty(&self.target_id, &self.profile));
        let mut actions = Vec::new();
        let mut expected_paths = BTreeSet::new();
        let mut effective_paths = BTreeSet::new();
        let mut required_capacity = 0;

        for expected in &self.desired {
            if !self.path_is_managed(&expected.path) {
                blocked.push(BlockReason::OutsideManagedRoot {
                    path: expected.path.clone(),
                });
                continue;
            }
            if !effective_paths.insert(expected.path.equivalence_key()) {
                blocked.push(BlockReason::EffectiveCaseCollision {
                    path: expected.path.clone(),
                    existing: None,
                });
                continue;
            }
            // Folds case *and* normalization, so an existing NFD spelling of a
            // planned NFC name is caught here rather than becoming a second
            // file that differs only by spelling.
            if let Some(existing) = inventory.artifacts.keys().find(|path| {
                *path != &expected.path && path.equivalence_key() == expected.path.equivalence_key()
            }) {
                blocked.push(BlockReason::EffectiveCaseCollision {
                    path: expected.path.clone(),
                    existing: Some(existing.clone()),
                });
                continue;
            }
            expected_paths.insert(expected.path.clone());
            // Exactly one classification per desired path. Nothing falls
            // through, and no branch overwrites, relocates, or deletes content
            // the application did not both place and re-verify.
            match (
                inventory.artifacts.get(&expected.path),
                manifest.artifacts.get(&expected.path),
            ) {
                // Row 1 — free.
                (None, _) => {
                    required_capacity += expected.size();
                    actions.push(action(expected, Action::Add));
                }
                // Row 7 — a directory is never cleared to make room, empty or
                // not. Emptiness is not evidence of unimportance.
                (Some(actual), _) if actual.is_directory() => {
                    blocked.push(BlockReason::PathOccupiedByDirectory {
                        path: expected.path.clone(),
                    });
                }
                // Row 4 — managed content was changed outside the application,
                // so the recorded evidence no longer describes reality. Blocked
                // rather than overwritten.
                (Some(actual), Some(evidence)) if actual.sha256 != evidence.sha256 => {
                    blocked.push(BlockReason::ManagedContentChanged {
                        path: expected.path.clone(),
                    });
                }
                // Row 2 — managed and already current.
                (Some(actual), Some(evidence))
                    if actual.sha256 == expected.sha256()
                        && evidence.rom_set_id == expected.rom_set_id
                        && evidence.size == expected.size() =>
                {
                    actions.push(action(expected, Action::Retain));
                }
                // Row 3 — managed, content still matches the manifest, but a
                // different ROM Set is now wanted here. A legitimate
                // replacement, reachable only because row 4 passed first.
                (Some(_), Some(_)) => {
                    required_capacity += expected.size();
                    actions.push(action(expected, Action::Add));
                }
                // Row 5 — unrecognized content that exactly matches what was
                // planned. Strong evidence of identity, but not management
                // authority: offered as an adoption the approval authorizes.
                (Some(actual), None) if actual.sha256 == expected.sha256() => {
                    actions.push(action(expected, Action::Adopt));
                }
                // Row 6 — unknown content. Preserved, never overwritten.
                (Some(_), None) => {
                    blocked.push(BlockReason::PathConflict {
                        path: expected.path.clone(),
                    });
                }
            }
        }

        // Row 9 — the target already holds two entries whose keys collide, so
        // its namespace is ambiguous and planning refuses to guess which entry
        // it is looking at. Reachable without malice: an unprivileged process
        // can flip a directory case-sensitive.
        let mut seen_keys: BTreeMap<String, RelativePath> = BTreeMap::new();
        for path in inventory.artifacts.keys() {
            if let Some(previous) = seen_keys.insert(path.equivalence_key(), path.clone()) {
                blocked.push(BlockReason::EffectiveCaseCollision {
                    path: previous,
                    existing: Some(path.clone()),
                });
            }
        }

        // Names the target holds that the namespace cannot represent. They are
        // preserved and disclosed like any other unknown content; they block
        // only where one contends with a desired path, because then the
        // application cannot tell which object a planned spelling would select.
        let mut preserved_unrepresentable = Vec::new();
        for name in &inventory.unrepresentable {
            let key = RelativePath::key_of(name);
            if expected_paths
                .iter()
                .any(|path| path.equivalence_key() == key)
            {
                blocked.push(BlockReason::InvalidTargetPath { path: name.clone() });
            } else {
                preserved_unrepresentable.push(name.clone());
            }
        }

        // Row 10 — managed content the manifest names but the target no longer
        // holds. Disclosed, and never converted into a removal elsewhere.
        let missing_managed = manifest
            .artifacts
            .keys()
            .filter(|path| !inventory.artifacts.contains_key(*path))
            .cloned()
            .collect::<Vec<_>>();

        for (path, evidence) in &manifest.artifacts {
            if expected_paths.contains(path) {
                continue;
            }
            if let Some(actual) = inventory.artifacts.get(path) {
                if actual.sha256 == evidence.sha256 {
                    actions.push(PlanAction {
                        action: Action::Remove,
                        path: path.clone(),
                        rom_set_id: evidence.rom_set_id.clone(),
                        size: evidence.size,
                        sha256: evidence.sha256.clone(),
                    });
                } else {
                    blocked.push(BlockReason::ManagedContentChanged { path: path.clone() });
                }
            }
        }

        let managed_paths = manifest.artifacts.keys().cloned().collect::<BTreeSet<_>>();
        let expected_hashes = self
            .desired
            .iter()
            .map(|artifact| artifact.sha256())
            .collect::<BTreeSet<_>>();
        let preserved_duplicates = inventory
            .artifacts
            .iter()
            .filter(|(path, artifact)| {
                !artifact.is_directory()
                    && !expected_paths.contains(*path)
                    && !managed_paths.contains(*path)
                    && expected_hashes.contains(artifact.sha256.as_str())
            })
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>();
        let preserved_unknowns = inventory
            .artifacts
            .iter()
            .filter(|(path, artifact)| {
                !artifact.is_directory()
                    && !expected_paths.contains(*path)
                    && !managed_paths.contains(*path)
                    && !expected_hashes.contains(artifact.sha256.as_str())
            })
            .map(|(path, _)| path.clone())
            .collect();

        let required_with_margin = if required_capacity == 0 {
            0
        } else {
            required_capacity.saturating_add(CAPACITY_MARGIN)
        };
        // Only meaningful where capacity is actually reported. Where it is
        // not, the absence itself blocks any plan containing an addition (see
        // the capability gating below) rather than being guessed at here.
        if let Some(free_bytes) = inventory.free_bytes
            && free_bytes < required_with_margin
        {
            blocked.push(BlockReason::InsufficientCapacity {
                required: required_with_margin,
                available: free_bytes,
            });
        }
        // Capability requirements are per action, and are evaluated only when
        // the plan actually contains that action: a plan with no removals is not
        // blocked by a binding that cannot delete.
        let capabilities = self.transport.capabilities();
        let contains = |wanted: Action| actions.iter().any(|action| action.action == wanted);
        let places = contains(Action::Add);
        let verifies = places || contains(Action::Adopt);

        // An addition must be verifiable before it can ever justify a permanent
        // removal, and an adoption without read-back would fabricate authority.
        if verifies && !capabilities.read_back {
            blocked.push(BlockReason::UnsupportedCapability {
                capability: "read-back verification".into(),
            });
        }
        // Capacity safety is a pre-flight guarantee; a binding that cannot
        // report free space cannot support it, and guessing is not permitted.
        if places && !capabilities.reports_capacity {
            blocked.push(BlockReason::UnsupportedCapability {
                capability: "capacity reporting".into(),
            });
        }
        if contains(Action::Remove) && !capabilities.leaf_delete {
            blocked.push(BlockReason::UnsupportedCapability {
                capability: "leaf deletion".into(),
            });
        }
        // `atomic_publish` never blocks. It is disclosed on the plan below, and
        // publication falls back to a documented non-atomic path.

        Ok(SyncPlan {
            schema_version: 1,
            target_id: self.target_id.clone(),
            binding_locator: self.transport.locator(),
            profile_id: self.profile.id.clone(),
            profile_revision: self.profile.revision,
            rom_pack_revision: self.rom_pack_revision,
            inventory_generation: inventory.generation,
            inventory_digest: inventory_digest(
                &inventory,
                &self.transport.locator(),
                &capabilities,
                &manifest,
                self.observed_marker.as_ref(),
            ),
            actions,
            preserved_unknowns,
            preserved_duplicates,
            missing_managed,
            preserved_unrepresentable,
            blocked,
            required_capacity: required_with_margin,
            safety_margin: CAPACITY_MARGIN,
            atomic_publication: capabilities.atomic_publish,
            digest: String::new(),
        }
        .seal())
    }

    /// Executes `plan` under `approval`, which is consumed whether execution
    /// succeeds, fails, or is cancelled. No mutation happens before every
    /// binding below has been validated.
    pub fn execute(
        &mut self,
        plan: &SyncPlan,
        approval: Approval,
        cancellation: &CancellationToken,
    ) -> Result<ExecutionOutcome, SyncError> {
        if !plan.has_valid_digest()
            || plan.target_id != self.target_id
            || plan.profile_id != self.profile.id
            || plan.profile_revision != self.profile.revision
        {
            return Err(SyncError::PlanChanged);
        }
        if !plan.is_executable() {
            return Err(SyncError::Blocked);
        }
        // The approval must name this exact plan and this exact binding. An
        // approval granted for a different, smaller plan can never authorize a
        // larger one.
        for (matches, mismatch) in [
            (approval.plan_digest == plan.digest, "plan digest"),
            (
                approval.target_id == plan.target_id,
                "Media Target identity",
            ),
            (
                approval.profile_id == plan.profile_id,
                "Device Profile identity",
            ),
            (
                approval.profile_revision == plan.profile_revision,
                "Device Profile revision",
            ),
            (
                approval.binding_locator == plan.binding_locator,
                "Transport Binding locator",
            ),
            (
                approval.inventory_digest == plan.inventory_digest,
                "inventory evidence",
            ),
        ] {
            if !matches {
                return Err(SyncError::ApprovalInvalid(mismatch));
            }
        }
        if plan.removal_count() != approval.removals_acked {
            return Err(SyncError::RemovalAcknowledgement);
        }
        if !self
            .transport
            .marker()?
            .as_ref()
            .is_some_and(|marker| self.marker_is_valid(marker))
        {
            return Err(SyncError::PlanChanged);
        }
        let current = self.transport.inventory()?;
        self.refreshed = Some(current);
        let current_plan = self.build_plan()?;
        if current_plan.digest != plan.digest {
            return Err(SyncError::PlanChanged);
        }

        let previous = self
            .transport
            .manifest()?
            .unwrap_or_else(|| ManagedArtifactManifest::empty(&self.target_id, &self.profile));
        if Some(previous.clone()) != self.local_manifest
            && !(previous.generation == 0 && self.local_manifest.is_none())
        {
            return Err(SyncError::PlanChanged);
        }

        let mut next_entries = BTreeMap::new();
        for action in plan
            .actions
            .iter()
            .filter(|action| action.action != Action::Remove)
        {
            if cancellation.is_cancelled() {
                self.refreshed = None;
                return Ok(ExecutionOutcome::Cancelled);
            }
            let origin = match action.action {
                Action::Add => {
                    let expected = self
                        .desired
                        .iter()
                        .find(|artifact| artifact.path == action.path)
                        .expect("plan action came from desired artifacts");
                    match self
                        .transport
                        .write_new(&action.path, expected.bytes(), cancellation)
                    {
                        Ok(()) => {}
                        Err(TransportError::Disconnected) => {
                            self.refreshed = None;
                            return Ok(ExecutionOutcome::Indeterminate {
                                reason: "target disconnected during write".into(),
                                residue: Vec::new(),
                            });
                        }
                        Err(error) => {
                            self.refreshed = None;
                            return Ok(ExecutionOutcome::Incomplete {
                                reason: error.to_string(),
                                residue: Vec::new(),
                            });
                        }
                    }
                    if cancellation.is_cancelled() {
                        self.refreshed = None;
                        return Ok(ExecutionOutcome::Cancelled);
                    }
                    let read_back = match self.transport.read(&action.path) {
                        Ok(bytes) => bytes,
                        Err(error) => return Ok(self.failed_after_side_effect(error)),
                    };
                    if sha256(&read_back) != action.sha256 {
                        // Deliberately NOT deleted. The bytes at this path are
                        // not what was written, so the application cannot prove
                        // it created what is there — it may be another tool's
                        // file at a colliding name. Deleting on the basis that
                        // "this operation owned that path" is exactly the
                        // inference the safety rules exclude everywhere else.
                        // It is left in place and recorded, becoming ordinary
                        // unknown content on the next planning pass.
                        self.refreshed = None;
                        return Ok(ExecutionOutcome::Incomplete {
                            reason: format!("read-back verification failed for {}", action.path),
                            residue: vec![Residue {
                                path: action.path.clone(),
                                attempted: Action::Add,
                            }],
                        });
                    }
                    ManagementOrigin::Placed
                }
                Action::Adopt => {
                    let read_back = match self.transport.read(&action.path) {
                        Ok(bytes) => bytes,
                        Err(error) => return Ok(self.failed_after_side_effect(error)),
                    };
                    if sha256(&read_back) != action.sha256 {
                        self.refreshed = None;
                        return Ok(ExecutionOutcome::Incomplete {
                            reason: format!("adoption verification failed for {}", action.path),
                            residue: Vec::new(),
                        });
                    }
                    ManagementOrigin::Adopted
                }
                Action::Retain => {
                    let origin = previous
                        .artifacts
                        .get(&action.path)
                        .map(|evidence| evidence.origin.clone())
                        .ok_or(SyncError::PlanChanged)?;
                    let read_back = match self.transport.read(&action.path) {
                        Ok(bytes) => bytes,
                        Err(error) => return Ok(self.failed_after_side_effect(error)),
                    };
                    if sha256(&read_back) != action.sha256 {
                        self.refreshed = None;
                        return Ok(ExecutionOutcome::Incomplete {
                            reason: format!("retention verification failed for {}", action.path),
                            residue: Vec::new(),
                        });
                    }
                    origin
                }
                Action::Remove => unreachable!(),
            };
            next_entries.insert(
                action.path.clone(),
                ManagedEvidence {
                    rom_set_id: action.rom_set_id.clone(),
                    size: action.size,
                    sha256: action.sha256.clone(),
                    origin,
                },
            );
        }

        if cancellation.is_cancelled() {
            self.refreshed = None;
            return Ok(ExecutionOutcome::Cancelled);
        }
        for action in plan
            .actions
            .iter()
            .filter(|action| action.action == Action::Remove)
        {
            if cancellation.is_cancelled() {
                self.refreshed = None;
                return Ok(ExecutionOutcome::Cancelled);
            }
            let current_bytes = match self.transport.read(&action.path) {
                Ok(bytes) => bytes,
                Err(error) => return Ok(self.stopped_before_removal(error)),
            };
            if sha256(&current_bytes) != action.sha256 {
                self.refreshed = None;
                return Ok(ExecutionOutcome::Incomplete {
                    reason: format!("managed content changed before removal: {}", action.path),
                    residue: Vec::new(),
                });
            }
            if let Err(error) = self.transport.delete_leaf(&action.path) {
                return Ok(self.stopped_before_removal(error));
            }
        }

        if cancellation.is_cancelled() {
            self.refreshed = None;
            return Ok(ExecutionOutcome::Cancelled);
        }

        let next_manifest = ManagedArtifactManifest {
            schema_version: 1,
            target_id: self.target_id.clone(),
            generation: previous.generation + 1,
            profile_id: self.profile.id.clone(),
            profile_revision: self.profile.revision,
            artifacts: next_entries,
        };
        if let Err(error) = self.transport.write_manifest(&next_manifest) {
            self.refreshed = None;
            return Ok(ExecutionOutcome::Indeterminate {
                reason: format!("manifest publication status is unknown: {error}"),
                residue: Vec::new(),
            });
        }
        let published_manifest = match self.transport.manifest() {
            Ok(manifest) => manifest,
            Err(error) => {
                self.refreshed = None;
                return Ok(ExecutionOutcome::Indeterminate {
                    reason: format!("manifest publication cannot be confirmed: {error}"),
                    residue: Vec::new(),
                });
            }
        };
        if published_manifest != Some(next_manifest.clone()) {
            self.refreshed = None;
            return Ok(ExecutionOutcome::Indeterminate {
                reason: "target manifest read-back disagreed".into(),
                residue: Vec::new(),
            });
        }
        self.local_manifest = Some(next_manifest);
        self.refreshed = None;
        Ok(ExecutionOutcome::Completed)
    }

    fn path_is_managed(&self, path: &crate::RelativePath) -> bool {
        path.as_str()
            .strip_prefix(self.profile.managed_root.as_str())
            .is_some_and(|suffix| suffix.starts_with('/'))
    }

    fn marker_is_valid(&self, marker: &TargetMarker) -> bool {
        marker.schema_version == 1 && marker.target_id == self.target_id
    }

    /// Whether the manifest can be trusted to describe what this application
    /// manages on this target.
    ///
    /// Deliberately excludes the Device Profile *revision*. A revision mismatch
    /// means the managed layout was produced under different rules, which forces
    /// disclosed re-planning — but the content the manifest names is still
    /// managed content, and discarding the manifest would reclassify every
    /// managed artifact as unknown and strand it.
    fn manifest_is_valid(&self, manifest: &ManagedArtifactManifest) -> bool {
        manifest.schema_version == 1
            && manifest.target_id == self.target_id
            && manifest.profile_id == self.profile.id
            && manifest
                .artifacts
                .keys()
                .all(|path| self.path_is_managed(path))
    }

    /// A manifest written under a different revision of the same profile. Not a
    /// safety failure by itself, but the layout rules have changed, so it is
    /// disclosed and re-planned rather than silently reused.
    fn profile_revision_differs(&self, manifest: &ManagedArtifactManifest) -> bool {
        manifest.profile_id == self.profile.id && manifest.profile_revision != self.profile.revision
    }

    /// A failure during the removal phase. Any failure halts the operation and
    /// no further removals are attempted — their justification was the
    /// verified additions of *this* operation, and it lapses the moment the
    /// operation stops proceeding as planned.
    fn stopped_before_removal(&mut self, error: TransportError) -> ExecutionOutcome {
        self.refreshed = None;
        match error {
            // Only a disconnect leaves it unestablished whether the deletion
            // landed; a definite error means the target state is known.
            TransportError::Disconnected => ExecutionOutcome::Indeterminate {
                reason: "target disconnected during removal".into(),
                residue: Vec::new(),
            },
            error => ExecutionOutcome::Incomplete {
                reason: error.to_string(),
                residue: Vec::new(),
            },
        }
    }

    fn failed_after_side_effect(&mut self, error: TransportError) -> ExecutionOutcome {
        self.refreshed = None;
        match error {
            TransportError::Disconnected => ExecutionOutcome::Indeterminate {
                reason: "target disconnected after execution began".into(),
                residue: Vec::new(),
            },
            error => ExecutionOutcome::Incomplete {
                reason: error.to_string(),
                residue: Vec::new(),
            },
        }
    }
}

/// Digest over everything a planning decision reads.
///
/// Anything not covered here must not be a planning input, and anything covered
/// here invalidates a plan and its approval when it changes. A re-observation of
/// a genuinely unchanged target reproduces this digest exactly, which is what
/// lets a refresh leave an existing approval intact.
fn inventory_digest(
    inventory: &crate::Inventory,
    locator: &str,
    capabilities: &crate::TransportCapabilities,
    manifest: &ManagedArtifactManifest,
    marker: Option<&TargetMarker>,
) -> String {
    #[derive(serde::Serialize)]
    struct Observed<'a> {
        artifacts: BTreeMap<&'a str, (u64, &'a str)>,
        capabilities: &'a crate::TransportCapabilities,
        locator: &'a str,
        manifest: &'a ManagedArtifactManifest,
        marker: Option<&'a TargetMarker>,
        unrepresentable: &'a [String],
    }

    // Raw free bytes are deliberately excluded. On a shared volume they drift
    // constantly for reasons that have nothing to do with this target, and
    // binding them would invalidate approvals on unrelated disk activity. What
    // matters — whether capacity is *reported*, and whether it is *sufficient* —
    // is already covered: the capability is in `capabilities`, and insufficiency
    // becomes an `InsufficientCapacity` entry in the plan's `blocked` list,
    // which the plan digest covers.
    let observed = Observed {
        artifacts: inventory
            .artifacts
            .iter()
            .map(|(path, artifact)| (path.as_str(), (artifact.size, artifact.sha256.as_str())))
            .collect(),
        capabilities,
        locator,
        manifest,
        marker,
        unrepresentable: &inventory.unrepresentable,
    };
    sha256(&serde_json::to_vec(&observed).expect("observed evidence is serializable"))
}

fn action(expected: &TargetArtifact, action: Action) -> PlanAction {
    PlanAction {
        action,
        path: expected.path.clone(),
        rom_set_id: expected.rom_set_id.clone(),
        size: expected.size(),
        sha256: expected.sha256().to_owned(),
    }
}
