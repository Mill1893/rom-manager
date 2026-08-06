use std::{collections::BTreeMap, fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::sha256;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct RelativePath(String);

impl<'de> Deserialize<'de> for RelativePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

impl RelativePath {
    pub fn new(value: impl Into<String>) -> Result<Self, PathError> {
        let value = value.into();
        let normalized = value.replace('\\', "/");
        if normalized.is_empty()
            || normalized.starts_with('/')
            || normalized.contains(':')
            || normalized
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(PathError(value));
        }
        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn file_name(&self) -> &str {
        self.0
            .rsplit('/')
            .next()
            .expect("validated path is not empty")
    }

    pub fn parent(&self) -> Option<Self> {
        self.0
            .rsplit_once('/')
            .map(|(parent, _)| Self(parent.to_owned()))
    }
}

impl fmt::Display for RelativePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for RelativePath {
    type Err = PathError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("path must be a normalized relative path: {0}")]
pub struct PathError(String);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeviceProfile {
    pub id: String,
    pub revision: u32,
    pub platform: String,
    pub managed_root: RelativePath,
}

impl DeviceProfile {
    pub fn generic_nes() -> Self {
        Self {
            id: "generic-folder".into(),
            revision: 1,
            platform: "NES".into(),
            managed_root: RelativePath::new("ROMs/nes").expect("built-in path is valid"),
        }
    }

    pub fn target_path(&self, file_name: &str) -> Result<RelativePath, PathError> {
        if !file_name.to_ascii_lowercase().ends_with(".nes") || file_name.contains(['/', '\\']) {
            return Err(PathError(file_name.into()));
        }
        RelativePath::new(format!("{}/{}", self.managed_root, file_name))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetArtifact {
    pub rom_set_id: String,
    pub path: RelativePath,
    bytes: Vec<u8>,
    sha256: String,
}

impl TargetArtifact {
    pub fn new(rom_set_id: impl Into<String>, path: RelativePath, bytes: Vec<u8>) -> Self {
        let sha256 = sha256(&bytes);
        Self {
            rom_set_id: rom_set_id.into(),
            path,
            bytes,
            sha256,
        }
    }

    pub fn size(&self) -> u64 {
        self.bytes.len() as u64
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TargetMarker {
    pub schema_version: u32,
    pub target_id: String,
}

impl TargetMarker {
    pub fn new(target_id: impl Into<String>) -> Self {
        Self {
            schema_version: 1,
            target_id: target_id.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagementOrigin {
    Placed,
    Adopted,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ManagedEvidence {
    pub rom_set_id: String,
    pub size: u64,
    pub sha256: String,
    pub origin: ManagementOrigin,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ManagedArtifactManifest {
    pub schema_version: u32,
    pub target_id: String,
    pub generation: u64,
    pub profile_id: String,
    pub profile_revision: u32,
    pub artifacts: BTreeMap<RelativePath, ManagedEvidence>,
}

impl ManagedArtifactManifest {
    pub fn empty(target_id: impl Into<String>, profile: &DeviceProfile) -> Self {
        Self {
            schema_version: 1,
            target_id: target_id.into(),
            generation: 0,
            profile_id: profile.id.clone(),
            profile_revision: profile.revision,
            artifacts: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Add,
    Retain,
    Adopt,
    Remove,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlanAction {
    pub action: Action,
    pub path: RelativePath,
    pub rom_set_id: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockReason {
    MarkerConflict,
    ManifestDisagreement,
    StaleInventory,
    OutsideManagedRoot { path: RelativePath },
    EffectiveCaseCollision { path: RelativePath },
    PathConflict { path: RelativePath },
    ManagedContentChanged { path: RelativePath },
    InsufficientCapacity { required: u64, available: u64 },
    UnsupportedCapability { capability: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SyncPlan {
    pub schema_version: u32,
    pub target_id: String,
    pub binding_locator: String,
    pub profile_id: String,
    pub profile_revision: u32,
    pub rom_pack_revision: u64,
    pub inventory_generation: u64,
    pub actions: Vec<PlanAction>,
    pub preserved_unknowns: Vec<RelativePath>,
    pub preserved_duplicates: Vec<RelativePath>,
    pub blocked: Vec<BlockReason>,
    pub required_capacity: u64,
    pub safety_margin: u64,
    pub atomic_publication: bool,
    pub digest: String,
}

impl SyncPlan {
    pub(crate) fn seal(mut self) -> Self {
        self.digest.clear();
        let bytes = serde_json::to_vec(&self).expect("sync plan is serializable");
        self.digest = sha256(&bytes);
        self
    }

    pub fn removal_count(&self) -> usize {
        self.actions
            .iter()
            .filter(|action| action.action == Action::Remove)
            .count()
    }

    pub fn is_executable(&self) -> bool {
        self.blocked.is_empty()
    }

    pub(crate) fn has_valid_digest(&self) -> bool {
        let mut unsealed = self.clone();
        let digest = unsealed.digest.clone();
        unsealed.digest.clear();
        digest == sha256(&serde_json::to_vec(&unsealed).expect("sync plan is serializable"))
    }
}
