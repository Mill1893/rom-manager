use std::collections::{BTreeMap, BTreeSet};

use crate::{
    Action, BlockReason, CancellationToken, DeviceProfile, ManagedArtifactManifest,
    ManagedEvidence, ManagementOrigin, PlanAction, SyncPlan, TargetArtifact, TargetMarker,
    Transport, TransportError, sha256,
};

const CAPACITY_MARGIN: u64 = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutionOutcome {
    Succeeded,
    Cancelled,
    Failed { reason: String },
    Indeterminate { reason: String },
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
                });
                continue;
            }
            // Folds case *and* normalization, so an existing NFD spelling of a
            // planned NFC name is caught here rather than becoming a second
            // file that differs only by spelling.
            if inventory.artifacts.keys().any(|path| {
                path != &expected.path && path.equivalence_key() == expected.path.equivalence_key()
            }) {
                blocked.push(BlockReason::EffectiveCaseCollision {
                    path: expected.path.clone(),
                });
                continue;
            }
            expected_paths.insert(expected.path.clone());
            match inventory.artifacts.get(&expected.path) {
                None => {
                    required_capacity += expected.size();
                    actions.push(action(expected, Action::Add));
                }
                Some(actual) if actual.sha256 != expected.sha256() => {
                    blocked.push(BlockReason::PathConflict {
                        path: expected.path.clone(),
                    });
                }
                Some(_) if manifest.artifacts.contains_key(&expected.path) => {
                    let evidence = &manifest.artifacts[&expected.path];
                    if evidence.rom_set_id == expected.rom_set_id
                        && evidence.size == expected.size()
                        && evidence.sha256 == expected.sha256()
                    {
                        actions.push(action(expected, Action::Retain));
                    } else {
                        blocked.push(BlockReason::ManifestDisagreement);
                    }
                }
                Some(_) => actions.push(action(expected, Action::Adopt)),
            }
        }

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
                !expected_paths.contains(*path)
                    && !managed_paths.contains(*path)
                    && expected_hashes.contains(artifact.sha256.as_str())
            })
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>();
        let preserved_unknowns = inventory
            .artifacts
            .iter()
            .filter(|(path, artifact)| {
                !expected_paths.contains(*path)
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
        if inventory.free_bytes < required_with_margin {
            blocked.push(BlockReason::InsufficientCapacity {
                required: required_with_margin,
                available: inventory.free_bytes,
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
            actions,
            preserved_unknowns,
            preserved_duplicates,
            blocked,
            required_capacity: required_with_margin,
            safety_margin: CAPACITY_MARGIN,
            atomic_publication: capabilities.atomic_publish,
            digest: String::new(),
        }
        .seal())
    }

    pub fn execute(
        &mut self,
        plan: &SyncPlan,
        acknowledged_removals: usize,
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
        if plan.removal_count() != acknowledged_removals {
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
                            });
                        }
                        Err(error) => {
                            self.refreshed = None;
                            return Ok(ExecutionOutcome::Failed {
                                reason: error.to_string(),
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
                        let _ = self.transport.delete_leaf(&action.path);
                        self.refreshed = None;
                        return Ok(ExecutionOutcome::Failed {
                            reason: format!("read-back verification failed for {}", action.path),
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
                        return Ok(ExecutionOutcome::Failed {
                            reason: format!("adoption verification failed for {}", action.path),
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
                        return Ok(ExecutionOutcome::Failed {
                            reason: format!("retention verification failed for {}", action.path),
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
                Err(error) => {
                    self.refreshed = None;
                    return Ok(ExecutionOutcome::Indeterminate {
                        reason: error.to_string(),
                    });
                }
            };
            if sha256(&current_bytes) != action.sha256 {
                self.refreshed = None;
                return Ok(ExecutionOutcome::Failed {
                    reason: format!("managed content changed before removal: {}", action.path),
                });
            }
            if let Err(error) = self.transport.delete_leaf(&action.path) {
                self.refreshed = None;
                return Ok(ExecutionOutcome::Indeterminate {
                    reason: error.to_string(),
                });
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
            });
        }
        let published_manifest = match self.transport.manifest() {
            Ok(manifest) => manifest,
            Err(error) => {
                self.refreshed = None;
                return Ok(ExecutionOutcome::Indeterminate {
                    reason: format!("manifest publication cannot be confirmed: {error}"),
                });
            }
        };
        if published_manifest != Some(next_manifest.clone()) {
            self.refreshed = None;
            return Ok(ExecutionOutcome::Indeterminate {
                reason: "target manifest read-back disagreed".into(),
            });
        }
        self.local_manifest = Some(next_manifest);
        self.refreshed = None;
        Ok(ExecutionOutcome::Succeeded)
    }

    fn path_is_managed(&self, path: &crate::RelativePath) -> bool {
        path.as_str()
            .strip_prefix(self.profile.managed_root.as_str())
            .is_some_and(|suffix| suffix.starts_with('/'))
    }

    fn marker_is_valid(&self, marker: &TargetMarker) -> bool {
        marker.schema_version == 1 && marker.target_id == self.target_id
    }

    fn manifest_is_valid(&self, manifest: &ManagedArtifactManifest) -> bool {
        manifest.schema_version == 1
            && manifest.target_id == self.target_id
            && manifest.profile_id == self.profile.id
            && manifest.profile_revision == self.profile.revision
            && manifest
                .artifacts
                .keys()
                .all(|path| self.path_is_managed(path))
    }

    fn failed_after_side_effect(&mut self, error: TransportError) -> ExecutionOutcome {
        self.refreshed = None;
        match error {
            TransportError::Disconnected => ExecutionOutcome::Indeterminate {
                reason: "target disconnected after execution began".into(),
            },
            error => ExecutionOutcome::Failed {
                reason: error.to_string(),
            },
        }
    }
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
