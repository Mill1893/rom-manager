mod domain;
mod transport;
mod workflow;

pub use domain::{
    Action, BlockReason, DeviceProfile, ManagedArtifactManifest, ManagedEvidence, ManagementOrigin,
    PlanAction, RelativePath, SyncPlan, TargetArtifact, TargetMarker,
};
pub use transport::{
    CancellationToken, FakeFault, FakeTransport, FilesystemTransport, Inventory, InventoryArtifact,
    Transport, TransportCapabilities, TransportError,
};
pub use workflow::{ExecutionOutcome, SyncCore, SyncError};

pub fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    format!("{:x}", Sha256::digest(bytes))
}
