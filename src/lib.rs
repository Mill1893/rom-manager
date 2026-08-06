/// Marker-area locations. Behavior-bearing: they are part of the Device
/// Profile snapshot digest, so changing one requires a new profile revision.
pub(crate) const MARKER_PATH: &str = "ROMManager/target.json";
pub(crate) const MANIFEST_PATH: &str = "ROMManager/manifest.json";

mod confined;
mod domain;
mod transport;
mod workflow;

pub use confined::ConfinedRoot;
pub use domain::{
    Action, Approval, BlockReason, DeviceProfile, ManagedArtifactManifest, ManagedEvidence,
    ManagementOrigin, PathError, PlanAction, RelativePath, SyncPlan, TargetArtifact, TargetMarker,
};
pub use transport::{
    CancellationToken, EntryKind, FakeFault, FakeTransport, FilesystemTransport, Inventory,
    InventoryArtifact, Transport, TransportCapabilities, TransportError,
};
pub use workflow::{ExecutionOutcome, OperationReport, Residue, SyncCore, SyncError};

pub fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    format!("{:x}", Sha256::digest(bytes))
}
