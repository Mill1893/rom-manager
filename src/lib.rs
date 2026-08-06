/// Marker-area locations. Behavior-bearing: they are part of the Device
/// Profile snapshot digest, so changing one requires a new profile revision.
pub(crate) const MARKER_PATH: &str = "ROMManager/target.json";
pub(crate) const MANIFEST_PATH: &str = "ROMManager/manifest.json";

mod app;
mod cache;
mod confined;
mod domain;
mod durable;
mod library;
mod store;
mod transport;
mod workflow;
mod wpd;

pub use app::{
    AppEvent, CancellationState, MediaTargetChoice, OutcomeKind, OutcomeView, Phase, PlanView,
    Progress, RomPackChoice, Snapshot, WizardStep,
};
pub use cache::{CacheError, Lease, MaterializationCache};
pub use confined::ConfinedRoot;
pub use domain::{
    Action, Approval, BlockReason, DeviceProfile, ManagedArtifactManifest, ManagedEvidence,
    ManagementOrigin, PathError, PlanAction, RelativePath, SyncPlan, TargetArtifact, TargetMarker,
};
pub use durable::{DurableError, approve, execute_approved};
pub use library::{Container, ImportError, Imported, Library};
pub use store::{OperationState, SCHEMA_VERSION, Store, StoreError};
pub use transport::{
    CancellationToken, EntryKind, FakeFault, FakeTransport, FilesystemTransport, Inventory,
    InventoryArtifact, Transport, TransportCapabilities, TransportError,
};
pub use workflow::{ExecutionOutcome, OperationReport, Residue, SyncCore, SyncError};
#[cfg(windows)]
pub use wpd::Apartment;
pub use wpd::{Backend, Reply, Request, Worker, WpdFault, WpdLikeBackend, mtp_capabilities};

pub fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    format!("{:x}", Sha256::digest(bytes))
}
