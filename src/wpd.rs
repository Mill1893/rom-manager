//! Windows Portable Devices transport (issue #36).
//!
//! # Why a worker thread
//!
//! WPD is COM, and its interfaces are apartment-bound: they are not `Send`, and
//! using one from a thread other than the one that created it is undefined
//! behaviour rather than an error you get told about. So a single dedicated
//! thread initializes COM, creates every interface, and is the only thread that
//! ever touches one. Callers send [`Request`]s over a channel and get plain data
//! back.
//!
//! The discipline is enforced by the type system rather than by care: the
//! backend lives inside the worker closure and is never returned, so a COM
//! pointer has nowhere to escape to. [`Backend`] is deliberately **not** `Send`.
//!
//! # What MTP is not
//!
//! MTP is an object protocol, not a filesystem, and this adapter refuses to
//! pretend otherwise:
//!
//! - **No atomic rename or replacement.** There is no publish-by-rename; a write
//!   is a new object and is verified by reading it back.
//! - **No stable timestamps.** Nothing here reads or trusts a modification time.
//! - **No recursive deletion.** Deletion is leaf-only, one object at a time.
//! - **No reliable change tokens.** Freshness comes from re-observing the
//!   device, never from a device-reported "nothing changed".
//! - **Object IDs are session-scoped.** They are resolved fresh each session and
//!   never persisted as authority; durable identity is the marker and content
//!   digests, exactly as for a filesystem target.
//!
//! # What this module does not establish
//!
//! The COM backend compiles for Windows and has **never run against a device**.
//! No physical-support claim follows from anything here; that is issue #38's to
//! make, on hardware.

use std::sync::mpsc::{Receiver, Sender, channel};

use crate::{
    CancellationToken, Inventory, ManagedArtifactManifest, RelativePath, TargetMarker,
    TransportCapabilities, TransportError,
};

/// One unit of device work. Everything crossing the channel is plain data —
/// never a COM interface.
pub enum Request {
    Capabilities,
    Marker,
    WriteMarker(TargetMarker),
    Manifest,
    WriteManifest(Box<ManagedArtifactManifest>),
    Inventory,
    Read(RelativePath),
    /// Size is known up front: WPD wants the object size before the transfer,
    /// so a streaming write of unknown length is not expressible.
    WriteNew {
        path: RelativePath,
        bytes: Vec<u8>,
        cancellation: CancellationToken,
    },
    DeleteLeaf(RelativePath),
    Shutdown,
}

/// The reply to a [`Request`], again plain data only.
pub enum Reply {
    Capabilities(TransportCapabilities),
    Marker(Option<TargetMarker>),
    Manifest(Option<ManagedArtifactManifest>),
    Inventory(Box<Inventory>),
    Bytes(Vec<u8>),
    Done,
    Failed(TransportError),
}

/// Device access, as performed **on the worker thread only**.
///
/// Implementors may hold apartment-bound COM state. The trait is deliberately
/// not `Send`: the worker constructs its backend inside its own thread and never
/// lets it out, so this cannot be violated by accident.
pub trait Backend {
    fn capabilities(&mut self) -> TransportCapabilities;
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

/// A handle to the dedicated device thread.
///
/// This is what the rest of the application holds. It is `Send` because it
/// carries only channel endpoints; the COM state stays behind them.
pub struct Worker {
    requests: Sender<Request>,
    replies: Receiver<Reply>,
    locator: String,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Worker {
    /// Starts the worker. `make_backend` runs **on the new thread**, so a COM
    /// apartment is initialized and every interface created there.
    pub fn start<B, F>(locator: impl Into<String>, make_backend: F) -> Result<Self, TransportError>
    where
        B: Backend,
        F: FnOnce() -> Result<B, TransportError> + Send + 'static,
    {
        let (request_tx, request_rx) = channel::<Request>();
        let (reply_tx, reply_rx) = channel::<Reply>();

        let handle = std::thread::Builder::new()
            .name("wpd-worker".into())
            .spawn(move || {
                let mut backend = match make_backend() {
                    Ok(backend) => backend,
                    Err(error) => {
                        // The caller learns about it through the first reply
                        // rather than through a panic on an unrelated thread.
                        let _ = reply_tx.send(Reply::Failed(error));
                        return;
                    }
                };
                for request in request_rx {
                    let reply = match request {
                        Request::Shutdown => break,
                        request => dispatch(&mut backend, request),
                    };
                    if reply_tx.send(reply).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| TransportError::Io(error.to_string()))?;

        Ok(Self {
            requests: request_tx,
            replies: reply_rx,
            locator: locator.into(),
            handle: Some(handle),
        })
    }

    pub fn locator(&self) -> &str {
        &self.locator
    }

    /// Sends one request and waits for its reply.
    ///
    /// A dead worker reads as [`TransportError::Disconnected`]: from the
    /// caller's side a device that stopped answering and a thread that stopped
    /// running are the same situation, and both must fail closed.
    pub fn call(&self, request: Request) -> Result<Reply, TransportError> {
        self.requests
            .send(request)
            .map_err(|_| TransportError::Disconnected)?;
        self.replies
            .recv()
            .map_err(|_| TransportError::Disconnected)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.requests.send(Request::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn dispatch<B: Backend>(backend: &mut B, request: Request) -> Reply {
    fn done(result: Result<(), TransportError>) -> Reply {
        match result {
            Ok(()) => Reply::Done,
            Err(error) => Reply::Failed(error),
        }
    }

    match request {
        Request::Capabilities => Reply::Capabilities(backend.capabilities()),
        Request::Marker => match backend.marker() {
            Ok(marker) => Reply::Marker(marker),
            Err(error) => Reply::Failed(error),
        },
        Request::WriteMarker(marker) => done(backend.write_marker(&marker)),
        Request::Manifest => match backend.manifest() {
            Ok(manifest) => Reply::Manifest(manifest),
            Err(error) => Reply::Failed(error),
        },
        Request::WriteManifest(manifest) => done(backend.write_manifest(&manifest)),
        Request::Inventory => match backend.inventory() {
            Ok(inventory) => Reply::Inventory(Box::new(inventory)),
            Err(error) => Reply::Failed(error),
        },
        Request::Read(path) => match backend.read(&path) {
            Ok(bytes) => Reply::Bytes(bytes),
            Err(error) => Reply::Failed(error),
        },
        Request::WriteNew {
            path,
            bytes,
            cancellation,
        } => done(backend.write_new(&path, &bytes, &cancellation)),
        Request::DeleteLeaf(path) => done(backend.delete_leaf(&path)),
        Request::Shutdown => Reply::Done,
    }
}

/// Capabilities an MTP binding can honestly claim.
///
/// Every value here is a statement about what MTP *does not* offer, and each
/// one is load-bearing: absent capacity reporting blocks additions, and absent
/// atomic publication is disclosed on every plan.
pub fn mtp_capabilities(reports_capacity: bool) -> TransportCapabilities {
    TransportCapabilities {
        // Objects can be read back in full, which is what makes verification —
        // and therefore management authority — possible at all.
        read_back: true,
        // Single-object deletion exists; recursive deletion is never used.
        leaf_delete: true,
        // Some devices report free space and some do not. Never assumed.
        reports_capacity,
        // MTP has no rename-into-place. Publication is non-atomic, always.
        atomic_publish: false,
    }
}

#[cfg(windows)]
mod com {
    //! The real WPD backend.
    //!
    //! Constructed **inside** [`super::Worker::start`]'s closure so COM is
    //! initialized on the worker thread and every interface it creates stays
    //! there. This has never run against a device; see the module header.

    use windows_sys::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize,
    };

    use crate::TransportError;

    /// Owns the COM apartment for the worker thread's lifetime.
    ///
    /// Not `Send`, so it cannot be moved to another thread even by mistake —
    /// which is the whole point, since everything it transitively creates is
    /// apartment-bound.
    pub struct Apartment {
        _not_send: std::marker::PhantomData<*const ()>,
    }

    impl Apartment {
        pub fn initialize() -> Result<Self, TransportError> {
            // SAFETY: called once, on the thread that will use it, and undone
            // in Drop on that same thread.
            let status = unsafe { CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as u32) };
            if status < 0 {
                return Err(TransportError::Unsupported(format!(
                    "COM initialization failed: 0x{:08X}",
                    status as u32
                )));
            }
            Ok(Self {
                _not_send: std::marker::PhantomData,
            })
        }

        /// The class context WPD device enumeration is created with.
        pub const DEVICE_MANAGER_CONTEXT: u32 = CLSCTX_INPROC_SERVER;
    }

    impl Drop for Apartment {
        fn drop(&mut self) {
            // SAFETY: balances the CoInitializeEx above, on the same thread.
            unsafe { CoUninitialize() };
        }
    }
}

#[cfg(windows)]
pub use com::Apartment;

/// A backend that behaves the way MTP does, for contract tests on any host.
///
/// This is not a simulation of a device and proves nothing about one. It exists
/// so the *adapter's* contract — threading, error mapping, cancellation,
/// verification ordering — can be exercised deterministically, including the
/// awkward cases a real device produces rarely and unrepeatably.
pub struct WpdLikeBackend {
    objects: std::collections::BTreeMap<RelativePath, Vec<u8>>,
    marker: Option<TargetMarker>,
    manifest: Option<ManagedArtifactManifest>,
    capacity: Option<u64>,
    generation: u64,
    /// Session-scoped object ids, reassigned on every session, to catch any
    /// code that tried to treat one as durable.
    next_object_id: u64,
    object_ids: std::collections::BTreeMap<RelativePath, u64>,
    fault: Option<WpdFault>,
    /// Names that resolve ambiguously on this device, as MTP allows: a folder
    /// may legitimately contain two objects with the same name.
    ambiguous: std::collections::BTreeSet<RelativePath>,
}

/// Failure modes a real MTP stack produces and a filesystem does not.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WpdFault {
    /// The device vanished mid-transfer.
    DisconnectOnWrite,
    /// The device is present but locked or not authorized.
    Unauthorized,
    /// The transfer stopped partway; the object may or may not exist.
    PartialWrite,
    /// Read-back returns different bytes than were written.
    CorruptReadBack,
    /// The stack retried and gave up.
    RetryExhausted,
    /// Publication returned no usable answer either way.
    IndeterminatePublish,
}

impl WpdLikeBackend {
    pub fn new(capacity: Option<u64>) -> Self {
        Self {
            objects: Default::default(),
            marker: None,
            manifest: None,
            capacity,
            generation: 0,
            next_object_id: 1,
            object_ids: Default::default(),
            fault: None,
            ambiguous: Default::default(),
        }
    }

    pub fn with_object(mut self, path: RelativePath, bytes: Vec<u8>) -> Self {
        self.assign_id(&path);
        self.objects.insert(path, bytes);
        self.generation += 1;
        self
    }

    /// Marks `path` as resolving to more than one object, which MTP permits.
    pub fn with_ambiguous_name(mut self, path: RelativePath) -> Self {
        self.ambiguous.insert(path);
        self
    }

    pub fn with_fault(mut self, fault: WpdFault) -> Self {
        self.fault = Some(fault);
        self
    }

    fn assign_id(&mut self, path: &RelativePath) -> u64 {
        let id = self.next_object_id;
        self.next_object_id += 1;
        self.object_ids.insert(path.clone(), id);
        id
    }

    fn resolve(&self, path: &RelativePath) -> Result<u64, TransportError> {
        if self.ambiguous.contains(path) {
            // Two objects, one name: which one was meant cannot be decided, so
            // the adapter must refuse rather than pick.
            return Err(TransportError::Conflict(path.clone()));
        }
        self.object_ids
            .get(path)
            .copied()
            .ok_or_else(|| TransportError::Io(format!("no object at {path}")))
    }
}

impl Backend for WpdLikeBackend {
    fn capabilities(&mut self) -> TransportCapabilities {
        mtp_capabilities(self.capacity.is_some())
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
        Ok(self.manifest.clone())
    }

    fn write_manifest(&mut self, manifest: &ManagedArtifactManifest) -> Result<(), TransportError> {
        if self.fault == Some(WpdFault::IndeterminatePublish) {
            // Written or not — the device did not say. Reported as a disconnect
            // so the core treats the outcome as unestablished.
            self.manifest = Some(manifest.clone());
            return Err(TransportError::Disconnected);
        }
        self.manifest = Some(manifest.clone());
        self.generation += 1;
        Ok(())
    }

    fn inventory(&mut self) -> Result<Inventory, TransportError> {
        if self.fault == Some(WpdFault::Unauthorized) {
            return Err(TransportError::Unsupported("device is locked".into()));
        }
        Ok(Inventory {
            generation: self.generation,
            free_bytes: self.capacity.map(|capacity| {
                capacity.saturating_sub(self.objects.values().map(|b| b.len() as u64).sum::<u64>())
            }),
            artifacts: self
                .objects
                .iter()
                .map(|(path, bytes)| {
                    (
                        path.clone(),
                        crate::InventoryArtifact::file(bytes.len() as u64, crate::sha256(bytes)),
                    )
                })
                .collect(),
            unrepresentable: Vec::new(),
        })
    }

    fn read(&mut self, path: &RelativePath) -> Result<Vec<u8>, TransportError> {
        self.resolve(path)?;
        if self.fault == Some(WpdFault::CorruptReadBack) {
            return Ok(b"not what was written".to_vec());
        }
        self.objects
            .get(path)
            .cloned()
            .ok_or_else(|| TransportError::Io(format!("no object at {path}")))
    }

    fn write_new(
        &mut self,
        path: &RelativePath,
        bytes: &[u8],
        cancellation: &CancellationToken,
    ) -> Result<(), TransportError> {
        if self.object_ids.contains_key(path) || self.ambiguous.contains(path) {
            return Err(TransportError::Conflict(path.clone()));
        }
        match self.fault {
            Some(WpdFault::Unauthorized) => {
                return Err(TransportError::Unsupported("device is locked".into()));
            }
            Some(WpdFault::RetryExhausted) => {
                return Err(TransportError::Io("retries exhausted".into()));
            }
            Some(WpdFault::DisconnectOnWrite) => return Err(TransportError::Disconnected),
            Some(WpdFault::PartialWrite) => {
                // Some bytes landed. The object exists but is not what was
                // asked for, which read-back verification must catch.
                self.assign_id(path);
                self.objects
                    .insert(path.clone(), bytes[..bytes.len() / 2].to_vec());
                self.generation += 1;
                return Err(TransportError::Disconnected);
            }
            _ => {}
        }
        if cancellation.is_cancelled() {
            return Err(TransportError::Cancelled);
        }
        if let Some(capacity) = self.capacity {
            let used: u64 = self.objects.values().map(|b| b.len() as u64).sum();
            if used + bytes.len() as u64 > capacity {
                return Err(TransportError::InsufficientCapacity);
            }
        }
        self.assign_id(path);
        self.objects.insert(path.clone(), bytes.to_vec());
        self.generation += 1;
        Ok(())
    }

    fn delete_leaf(&mut self, path: &RelativePath) -> Result<(), TransportError> {
        self.resolve(path)?;
        self.objects.remove(path);
        self.object_ids.remove(path);
        self.generation += 1;
        Ok(())
    }
}
