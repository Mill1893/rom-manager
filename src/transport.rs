use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use serde::{Deserialize, Serialize};

use crate::{
    MANIFEST_PATH, MARKER_PATH, ManagedArtifactManifest, RelativePath, TargetMarker, sha256,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransportCapabilities {
    pub read_back: bool,
    pub leaf_delete: bool,
    pub reports_capacity: bool,
    pub atomic_publish: bool,
}

impl TransportCapabilities {
    pub fn filesystem() -> Self {
        Self {
            read_back: true,
            leaf_delete: true,
            reports_capacity: true,
            atomic_publish: false,
        }
    }

    pub fn wpd_like() -> Self {
        Self {
            read_back: true,
            leaf_delete: true,
            reports_capacity: true,
            atomic_publish: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InventoryArtifact {
    pub kind: EntryKind,
    pub size: u64,
    pub sha256: String,
}

/// Whether an observed entry is a file or a directory. A directory occupying a
/// desired file path is a distinct condition from a file occupying it, and is
/// never cleared to make room.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    File,
    Directory,
}

impl InventoryArtifact {
    pub fn file(size: u64, sha256: impl Into<String>) -> Self {
        Self {
            kind: EntryKind::File,
            size,
            sha256: sha256.into(),
        }
    }

    /// Directories carry no content identity; they exist in the inventory so
    /// planning can see that a desired path is occupied by one.
    pub fn directory() -> Self {
        Self {
            kind: EntryKind::Directory,
            size: 0,
            sha256: String::new(),
        }
    }

    pub fn is_directory(&self) -> bool {
        self.kind == EntryKind::Directory
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Inventory {
    pub generation: u64,
    pub free_bytes: u64,
    pub artifacts: BTreeMap<RelativePath, InventoryArtifact>,
    /// Names observed on the target that the namespace cannot represent — an
    /// NFD spelling, a trailing dot, a reserved basename. Reported verbatim so
    /// planning can preserve and disclose them. A transport never repairs a
    /// name, and an unrepresentable one is not an I/O error.
    pub unrepresentable: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum TransportError {
    #[error("transport disconnected")]
    Disconnected,
    #[error("transport operation cancelled")]
    Cancelled,
    #[error("insufficient capacity")]
    InsufficientCapacity,
    #[error("target path is ambiguous or occupied: {0}")]
    Conflict(RelativePath),
    #[error("transport operation is unsupported: {0}")]
    Unsupported(String),
    #[error("transport I/O failed: {0}")]
    Io(String),
}

impl From<io::Error> for TransportError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

pub trait Transport {
    fn locator(&self) -> String;
    fn capabilities(&self) -> TransportCapabilities;
    fn marker(&mut self) -> Result<Option<TargetMarker>, TransportError>;
    fn write_marker(&mut self, marker: &TargetMarker) -> Result<(), TransportError>;
    fn manifest(&mut self) -> Result<Option<ManagedArtifactManifest>, TransportError>;
    fn write_manifest(&mut self, manifest: &ManagedArtifactManifest) -> Result<(), TransportError>;
    fn inventory(&mut self) -> Result<Inventory, TransportError>;
    fn read(&mut self, path: &RelativePath) -> Result<Vec<u8>, TransportError>;
    fn write_new(
        &mut self,
        path: &RelativePath,
        bytes: &[u8],
        cancellation: &CancellationToken,
    ) -> Result<(), TransportError>;
    fn delete_leaf(&mut self, path: &RelativePath) -> Result<(), TransportError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FakeFault {
    DisconnectOnWrite,
    CorruptReadBack(RelativePath),
    CancelAfterWrite,
    DelayDelete(u64),
    DisconnectAfterManifestWrite,
    DisconnectOnManifestRead,
    RetryExhausted,
}

pub struct FakeTransport {
    locator: String,
    capabilities: TransportCapabilities,
    marker: Option<TargetMarker>,
    manifest: Option<ManagedArtifactManifest>,
    artifacts: BTreeMap<RelativePath, Vec<u8>>,
    directories: std::collections::BTreeSet<RelativePath>,
    unrepresentable: Vec<String>,
    capacity: u64,
    generation: u64,
    fault: Option<FakeFault>,
}

impl FakeTransport {
    pub fn new(locator: impl Into<String>, capacity: u64) -> Self {
        Self {
            locator: locator.into(),
            capabilities: TransportCapabilities::wpd_like(),
            marker: None,
            manifest: None,
            artifacts: BTreeMap::new(),
            directories: std::collections::BTreeSet::new(),
            unrepresentable: Vec::new(),
            capacity,
            generation: 0,
            fault: None,
        }
    }

    /// Presents a binding whose observed capabilities differ from the default,
    /// so per-action gating can be exercised without a real device.
    pub fn with_capabilities(mut self, capabilities: TransportCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    pub fn with_artifact(mut self, path: RelativePath, bytes: Vec<u8>) -> Self {
        self.artifacts.insert(path, bytes);
        self.generation += 1;
        self
    }

    /// Places a directory at `path`, so a desired file path can be observed as
    /// occupied by one.
    pub fn with_directory(mut self, path: RelativePath) -> Self {
        self.directories.insert(path);
        self.generation += 1;
        self
    }

    /// Plants a name the namespace cannot represent, as a real target could
    /// already hold — an NFD spelling, a trailing dot, a reserved basename.
    pub fn with_unrepresentable(mut self, name: impl Into<String>) -> Self {
        self.unrepresentable.push(name.into());
        self.generation += 1;
        self
    }

    pub fn set_fault(&mut self, fault: Option<FakeFault>) {
        self.fault = fault;
    }

    pub fn set_locator(&mut self, locator: impl Into<String>) {
        self.locator = locator.into();
    }

    pub fn set_capacity(&mut self, capacity: u64) {
        self.capacity = capacity;
        self.generation += 1;
    }

    pub fn mutate(&mut self, path: RelativePath, bytes: Vec<u8>) {
        self.artifacts.insert(path, bytes);
        self.generation += 1;
    }

    pub fn remove(&mut self, path: &RelativePath) {
        self.artifacts.remove(path);
        self.generation += 1;
    }

    pub fn set_manifest(&mut self, manifest: Option<ManagedArtifactManifest>) {
        self.manifest = manifest;
        self.generation += 1;
    }

    pub fn set_marker(&mut self, marker: Option<TargetMarker>) {
        self.marker = marker;
        self.generation += 1;
    }
}

impl Transport for FakeTransport {
    fn locator(&self) -> String {
        self.locator.clone()
    }

    fn capabilities(&self) -> TransportCapabilities {
        self.capabilities.clone()
    }

    fn marker(&mut self) -> Result<Option<TargetMarker>, TransportError> {
        Ok(self.marker.clone())
    }

    fn write_marker(&mut self, marker: &TargetMarker) -> Result<(), TransportError> {
        self.marker = Some(marker.clone());
        self.generation += 1;
        Ok(())
    }

    fn manifest(&mut self) -> Result<Option<ManagedArtifactManifest>, TransportError> {
        if self.fault == Some(FakeFault::DisconnectOnManifestRead) {
            return Err(TransportError::Disconnected);
        }
        Ok(self.manifest.clone())
    }

    fn write_manifest(&mut self, manifest: &ManagedArtifactManifest) -> Result<(), TransportError> {
        self.manifest = Some(manifest.clone());
        self.generation += 1;
        if self.fault == Some(FakeFault::DisconnectAfterManifestWrite) {
            self.fault = Some(FakeFault::DisconnectOnManifestRead);
        }
        Ok(())
    }

    fn inventory(&mut self) -> Result<Inventory, TransportError> {
        let used = self
            .artifacts
            .values()
            .map(|bytes| bytes.len() as u64)
            .sum::<u64>();
        Ok(Inventory {
            generation: self.generation,
            free_bytes: self.capacity.saturating_sub(used),
            unrepresentable: self.unrepresentable.clone(),
            artifacts: self
                .directories
                .iter()
                .map(|path| (path.clone(), InventoryArtifact::directory()))
                .chain(self.artifacts.iter().map(|(path, bytes)| {
                    (
                        path.clone(),
                        InventoryArtifact::file(bytes.len() as u64, sha256(bytes)),
                    )
                }))
                .collect(),
        })
    }

    fn read(&mut self, path: &RelativePath) -> Result<Vec<u8>, TransportError> {
        let mut bytes = self
            .artifacts
            .get(path)
            .cloned()
            .ok_or_else(|| TransportError::Io(format!("missing artifact: {path}")))?;
        if self.fault == Some(FakeFault::CorruptReadBack(path.clone())) {
            bytes.push(0xff);
        }
        Ok(bytes)
    }

    fn write_new(
        &mut self,
        path: &RelativePath,
        bytes: &[u8],
        cancellation: &CancellationToken,
    ) -> Result<(), TransportError> {
        match self.fault {
            Some(FakeFault::DisconnectOnWrite) => return Err(TransportError::Disconnected),
            Some(FakeFault::RetryExhausted) => {
                return Err(TransportError::Io("retry budget exhausted".into()));
            }
            _ => {}
        }
        if self.artifacts.contains_key(path) {
            return Err(TransportError::Conflict(path.clone()));
        }
        let used = self
            .artifacts
            .values()
            .map(|value| value.len() as u64)
            .sum::<u64>();
        if used + bytes.len() as u64 > self.capacity {
            return Err(TransportError::InsufficientCapacity);
        }
        self.artifacts.insert(path.clone(), bytes.to_vec());
        self.generation += 1;
        if self.fault == Some(FakeFault::CancelAfterWrite) {
            cancellation.cancel();
        }
        Ok(())
    }

    fn delete_leaf(&mut self, path: &RelativePath) -> Result<(), TransportError> {
        if let Some(FakeFault::DelayDelete(milliseconds)) = &self.fault {
            std::thread::sleep(std::time::Duration::from_millis(*milliseconds));
        }
        self.artifacts
            .remove(path)
            .ok_or_else(|| TransportError::Io(format!("missing artifact: {path}")))?;
        self.generation += 1;
        Ok(())
    }
}

pub struct FilesystemTransport {
    root: PathBuf,
    locator: String,
    generation: u64,
}

impl FilesystemTransport {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, TransportError> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        let root = fs::canonicalize(root)?;
        let home = std::env::var_os("HOME").and_then(|path| fs::canonicalize(path).ok());
        if root.parent().is_none() || home.as_ref() == Some(&root) {
            return Err(TransportError::Unsupported(
                "host roots and the user home cannot be Media Targets".into(),
            ));
        }
        let locator = root.to_string_lossy().into_owned();
        Ok(Self {
            root,
            locator,
            generation: 0,
        })
    }

    fn absolute(&self, path: &RelativePath) -> Result<PathBuf, TransportError> {
        let mut absolute = self.root.clone();
        for segment in path.as_str().split('/') {
            absolute.push(segment);
            match fs::symlink_metadata(&absolute) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(TransportError::Unsupported(format!(
                        "filesystem indirection at {path}"
                    )));
                }
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(absolute)
    }

    fn read_json<T: for<'de> Deserialize<'de>>(
        &self,
        relative: &str,
    ) -> Result<Option<T>, TransportError> {
        let relative =
            RelativePath::new(relative).map_err(|error| TransportError::Io(error.to_string()))?;
        let path = self.absolute(&relative)?;
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|error| TransportError::Io(error.to_string())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn write_json<T: Serialize>(
        &mut self,
        relative: &str,
        value: &T,
    ) -> Result<(), TransportError> {
        let relative =
            RelativePath::new(relative).map_err(|error| TransportError::Io(error.to_string()))?;
        let path = self.absolute(&relative)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(value)
            .map_err(|error| TransportError::Io(error.to_string()))?;
        fs::write(path, bytes)?;
        self.generation += 1;
        Ok(())
    }

    /// Path relative to the target root, or `None` for the marker area.
    fn relative(&self, path: &Path) -> Result<Option<String>, TransportError> {
        let relative = path
            .strip_prefix(&self.root)
            .map_err(|error| TransportError::Io(error.to_string()))?
            .to_string_lossy()
            .replace('\\', "/");
        if relative == MARKER_PATH || relative == MANIFEST_PATH {
            return Ok(None);
        }
        Ok(Some(relative))
    }

    fn visit_files(
        &self,
        directory: &Path,
        artifacts: &mut BTreeMap<RelativePath, InventoryArtifact>,
        unrepresentable: &mut Vec<String>,
    ) -> Result<(), TransportError> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                return Err(TransportError::Unsupported(format!(
                    "filesystem indirection at {}",
                    path.to_string_lossy()
                )));
            }
            if file_type.is_dir() {
                if let Some(relative) = self.relative(&path)? {
                    match RelativePath::new(relative.clone()) {
                        Ok(relative) => {
                            artifacts.insert(relative, InventoryArtifact::directory());
                        }
                        Err(_) => unrepresentable.push(relative),
                    }
                }
                self.visit_files(&path, artifacts, unrepresentable)?;
            } else {
                let relative = path
                    .strip_prefix(&self.root)
                    .map_err(|error| TransportError::Io(error.to_string()))?
                    .to_string_lossy()
                    .replace('\\', "/");
                if relative == MARKER_PATH || relative == MANIFEST_PATH {
                    continue;
                }
                // An unrepresentable name is observed content, not an I/O
                // error. Failing here would make one stray NFD or trailing-dot
                // file render the whole Media Target unusable, when the rule is
                // to preserve and disclose it.
                let Ok(path) = RelativePath::new(relative.clone()) else {
                    unrepresentable.push(relative);
                    continue;
                };
                let bytes = fs::read(entry.path())?;
                artifacts.insert(
                    path,
                    InventoryArtifact::file(bytes.len() as u64, sha256(&bytes)),
                );
            }
        }
        Ok(())
    }
}

impl FilesystemTransport {
    /// A confined view of the target root. Opened per operation; the handles it
    /// retains live for the walk.
    fn confined(&self) -> Result<crate::ConfinedRoot, TransportError> {
        Ok(crate::ConfinedRoot::open(&self.root)?)
    }
}

impl Transport for FilesystemTransport {
    fn locator(&self) -> String {
        self.locator.clone()
    }

    fn capabilities(&self) -> TransportCapabilities {
        TransportCapabilities::filesystem()
    }

    fn marker(&mut self) -> Result<Option<TargetMarker>, TransportError> {
        self.read_json(MARKER_PATH)
    }

    fn write_marker(&mut self, marker: &TargetMarker) -> Result<(), TransportError> {
        self.write_json(MARKER_PATH, marker)
    }

    fn manifest(&mut self) -> Result<Option<ManagedArtifactManifest>, TransportError> {
        self.read_json(MANIFEST_PATH)
    }

    fn write_manifest(&mut self, manifest: &ManagedArtifactManifest) -> Result<(), TransportError> {
        self.write_json(MANIFEST_PATH, manifest)
    }

    fn inventory(&mut self) -> Result<Inventory, TransportError> {
        let mut artifacts = BTreeMap::new();
        let mut unrepresentable = Vec::new();
        self.visit_files(&self.root, &mut artifacts, &mut unrepresentable)?;
        unrepresentable.sort();
        let free_bytes = fs2::available_space(&self.root)?;
        let fingerprint = artifacts
            .iter()
            .map(|(path, artifact)| format!("{path}\0{}\0{}\n", artifact.size, artifact.sha256))
            .chain(unrepresentable.iter().map(|name| format!("{name}\0?\n")))
            .collect::<String>();
        let generation = u64::from_str_radix(&sha256(fingerprint.as_bytes())[..16], 16)
            .expect("SHA-256 prefix is hexadecimal");
        Ok(Inventory {
            generation,
            free_bytes,
            artifacts,
            unrepresentable,
        })
    }

    fn read(&mut self, path: &RelativePath) -> Result<Vec<u8>, TransportError> {
        Ok(self.confined()?.read(path)?)
    }

    fn write_new(
        &mut self,
        path: &RelativePath,
        bytes: &[u8],
        cancellation: &CancellationToken,
    ) -> Result<(), TransportError> {
        if cancellation.is_cancelled() {
            return Err(TransportError::Cancelled);
        }
        // The confined walk resolves each segment without following
        // indirection, and its atomic create-if-absent is what proves the name
        // was free. A canonicalize-then-prefix-check would be a different
        // operation from the open that follows it.
        match self.confined()?.write_new(path, bytes) {
            Ok(()) => {
                self.generation += 1;
                Ok(())
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                Err(TransportError::Conflict(path.clone()))
            }
            Err(error) => Err(error.into()),
        }
    }

    fn delete_leaf(&mut self, path: &RelativePath) -> Result<(), TransportError> {
        let confined = self.confined()?;
        // Reparse rejection says nothing about hard links: a second name means
        // the bytes are reachable from somewhere this application cannot see.
        if confined.link_count(path)? > 1 {
            return Err(TransportError::Unsupported(format!(
                "{path} has more than one name"
            )));
        }
        confined.delete_leaf(path)?;
        self.generation += 1;
        Ok(())
    }
}
