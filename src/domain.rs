use std::{collections::BTreeMap, fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfc_quick};

use crate::sha256;

/// Longest single segment, in UTF-16 code units.
const MAX_SEGMENT_UNITS: usize = 255;
/// Longest whole relative path, in UTF-16 code units. A deliberate application
/// bound chosen to sit under every supported host and transport limit, so the
/// namespace is never defined by the weakest target.
const MAX_PATH_UNITS: usize = 1024;

/// Basenames Windows may resolve to a device rather than a file. Rejected
/// regardless of extension, and regardless of whether any given Windows build
/// still honours them — probe evidence showed the behaviour differs between
/// builds, so the namespace cannot depend on it.
const RESERVED_BASENAMES: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "com¹", "com²", "com³", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8",
    "lpt9", "lpt¹", "lpt²", "lpt³", "conin$", "conout$",
];

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
    /// Accepts an already-canonical relative path. Input is validated, never
    /// repaired: a separator, reserved name, or trailing dot is a rejection and
    /// not something to rewrite. Use [`RelativePath::canonicalize`] to bring a
    /// source-derived name into canonical form first.
    pub fn new(value: impl Into<String>) -> Result<Self, PathError> {
        let value = value.into();
        Self::validate(&value)?;
        Ok(Self(value))
    }

    /// Normalizes to NFC and then validates. NFC is the only transformation the
    /// namespace permits; nothing else about the name is altered.
    pub fn canonicalize(value: impl AsRef<str>) -> Result<Self, PathError> {
        Self::new(nfc(value.as_ref()))
    }

    fn validate(value: &str) -> Result<(), PathError> {
        let reject = || PathError(value.to_owned());
        if value.is_empty() || utf16_len(value) > MAX_PATH_UNITS {
            return Err(reject());
        }
        // Rejected rather than translated: silently rewriting a separator would
        // repair a name the caller could not prove.
        if value.contains('\\') || value.starts_with('/') || value.ends_with('/') {
            return Err(reject());
        }
        if !is_nfc(value) {
            return Err(reject());
        }
        for segment in value.split('/') {
            Self::validate_segment(segment).map_err(|_| reject())?;
        }
        Ok(())
    }

    fn validate_segment(segment: &str) -> Result<(), PathError> {
        let reject = || PathError(segment.to_owned());
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(reject());
        }
        if utf16_len(segment) > MAX_SEGMENT_UNITS {
            return Err(reject());
        }
        if segment
            .chars()
            .any(|c| c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
        {
            return Err(reject());
        }
        // Win32 path parsing trims these, so a name carrying one resolves to a
        // *different* file than the one written down.
        if segment.ends_with('.') || segment.ends_with(' ') || segment.starts_with(' ') {
            return Err(reject());
        }
        let stem = segment.split('.').next().unwrap_or(segment);
        if RESERVED_BASENAMES.contains(&fold(stem).as_str()) {
            return Err(reject());
        }
        Ok(())
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

    /// The effective-equivalence key: two paths whose keys are equal may resolve
    /// to the same object on some supported host.
    ///
    /// This folds case *and* normalization, and is a deliberately conservative
    /// superset of any host's lookup relation — NTFS folds only simple 1:1 BMP
    /// mappings, so this maps together some names it would keep distinct. Over-
    /// folding yields a disclosed block; under-folding would be a silent
    /// overwrite. It is never a substitute for an atomic create-if-absent.
    pub fn equivalence_key(&self) -> String {
        self.0.split('/').map(fold).collect::<Vec<_>>().join("/")
    }
}

fn utf16_len(value: &str) -> usize {
    value.chars().map(char::len_utf16).sum()
}

/// `is_nfc_quick` answers `Maybe` whenever it cannot decide from the leading
/// combining classes alone — a string in that state is often already canonical,
/// so treating `Maybe` as "not NFC" would reject valid names.
fn is_nfc(value: &str) -> bool {
    match is_nfc_quick(value.chars()) {
        IsNormalized::Yes => true,
        IsNormalized::No => false,
        IsNormalized::Maybe => value.nfc().eq(value.chars()),
    }
}

fn nfc(value: &str) -> String {
    if is_nfc(value) {
        value.to_owned()
    } else {
        value.nfc().collect()
    }
}

fn fold(value: &str) -> String {
    nfc(value).to_lowercase()
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
    pub extensions: Vec<String>,
}

/// The behavior-bearing fields of a [`DeviceProfile`], in canonical form.
///
/// `(id, revision)` identifies exactly one of these. Serializing it with sorted
/// keys and hashing the result gives the snapshot digest that freezes the
/// profile's behaviour; any change here requires a new revision.
#[derive(Serialize)]
struct ProfileSnapshot<'a> {
    extensions: &'a [String],
    managed_root: &'a str,
    manifest_path: &'a str,
    marker_path: &'a str,
    platform: &'a str,
}

impl DeviceProfile {
    pub fn generic_nes() -> Self {
        Self {
            id: "generic-folder".into(),
            revision: 1,
            platform: "NES".into(),
            managed_root: RelativePath::new("ROMs/nes").expect("built-in path is valid"),
            extensions: vec![".nes".into()],
        }
    }

    /// Digest over the behavior-bearing fields only. Presentational fields are
    /// excluded and never force a revision.
    pub fn snapshot_digest(&self) -> String {
        let snapshot = ProfileSnapshot {
            extensions: &self.extensions,
            managed_root: self.managed_root.as_str(),
            manifest_path: crate::MANIFEST_PATH,
            marker_path: crate::MARKER_PATH,
            platform: &self.platform,
        };
        sha256(&serde_json::to_vec(&snapshot).expect("profile snapshot is serializable"))
    }

    pub fn target_path(&self, file_name: &str) -> Result<RelativePath, PathError> {
        let canonical = nfc(file_name);
        // A source name is one segment. Without this, a name carrying a
        // separator would silently place the artifact in a subdirectory the
        // profile never described.
        if canonical.contains(['/', '\\']) {
            return Err(PathError(file_name.into()));
        }
        let accepted = self
            .extensions
            .iter()
            .any(|extension| fold(&canonical).ends_with(&fold(extension)));
        if !accepted {
            return Err(PathError(file_name.into()));
        }
        RelativePath::canonicalize(format!("{}/{}", self.managed_root, canonical))
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
    OutsideManagedRoot {
        path: RelativePath,
    },
    EffectiveCaseCollision {
        path: RelativePath,
    },
    /// A name that fails namespace validation outright — distinct from two
    /// valid names colliding.
    InvalidTargetPath {
        path: String,
    },
    /// A directory occupies a desired file path. Never cleared to make room,
    /// empty or not.
    PathOccupiedByDirectory {
        path: RelativePath,
    },
    PathConflict {
        path: RelativePath,
    },
    ManagedContentChanged {
        path: RelativePath,
    },
    InsufficientCapacity {
        required: u64,
        available: u64,
    },
    UnsupportedCapability {
        capability: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SyncPlan {
    pub schema_version: u32,
    pub target_id: String,
    pub binding_locator: String,
    pub profile_id: String,
    pub profile_revision: u32,
    pub rom_pack_revision: u64,
    /// Observation counter, for ordering and diagnostics only.
    pub inventory_generation: u64,
    /// Digest over everything the plan observed. This — not the counter — is the
    /// freshness identity, so re-observing an unchanged target does not
    /// gratuitously invalidate an approval.
    pub inventory_digest: String,
    pub actions: Vec<PlanAction>,
    pub preserved_unknowns: Vec<RelativePath>,
    pub preserved_duplicates: Vec<RelativePath>,
    /// Managed content the manifest names that the target no longer holds.
    /// Disclosed only — absence is never licence to remove anything else.
    pub missing_managed: Vec<RelativePath>,
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

/// Authority to execute one exact Sync Plan.
///
/// Held by value and consumed by the execute call, so it is single-use at the
/// type level — a retry after any outcome needs a fresh plan and a fresh
/// approval. It carries no expiry: an approval is invalidated by *evidence of
/// change*, which is strictly stronger than a clock. An approval an hour old
/// against a provably unchanged target is not stale; one a second old against a
/// changed target is.
///
/// Because it binds the plan digest, and that digest covers every action's path
/// and content hash, approving a plan *is* approving exactly the adoptions it
/// names — adoption needs no separate consent, and execution can never widen it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Approval {
    pub plan_digest: String,
    pub removals_acked: usize,
    pub target_id: String,
    pub profile_id: String,
    pub profile_revision: u32,
    pub binding_locator: String,
    pub inventory_digest: String,
}

impl Approval {
    /// Grants authority for `plan`, acknowledging `removals_acked` permanent
    /// managed removals. Every other binding is taken from the plan, so an
    /// approval cannot be assembled for a plan the caller has not seen.
    pub fn grant(plan: &SyncPlan, removals_acked: usize) -> Self {
        Self {
            plan_digest: plan.digest.clone(),
            removals_acked,
            target_id: plan.target_id.clone(),
            profile_id: plan.profile_id.clone(),
            profile_revision: plan.profile_revision,
            binding_locator: plan.binding_locator.clone(),
            inventory_digest: plan.inventory_digest.clone(),
        }
    }
}
