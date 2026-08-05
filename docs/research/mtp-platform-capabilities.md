# MTP transport capabilities across desktop platforms

Research date: 2026-08-05

## Scope and conclusion boundaries

This report establishes dependable implementation paths for discovering and identifying Media Transfer Protocol (MTP) devices and for reading, writing, deleting, reporting progress, cancelling work, and observing changes on Windows, macOS, and Linux. It does **not** select a final architecture.

The principal finding is that there is no equivalent native MTP API on all three hosts:

- Windows has an inbox, documented MTP/WPD stack and public COM API.
- macOS documents camera/scanner APIs and raw USB APIs, but no generic MTP client API. The established generic path is `libmtp` over `libusb`.
- Linux has `libmtp` plus two mature desktop brokers, GVfs/GIO and KDE KIO, but their availability and behavior depend on the installed desktop environment.

Stable support is realistic on all three hosts only if "stable" allows device-specific failures, locked-device states, exclusive-access conflicts, and cancellation that may leave partial work. Windows has the strongest host contract. Linux is strong when using an installed desktop broker and conditional when talking to USB directly. macOS is feasible but carries the most integration and hardware-validation risk.

## MTP is not an OS-mounted filesystem

MTP presents **objects**, object IDs, properties, storage IDs, and parent associations. It does not expose the device's filesystem or block device. `libmtp` documents consequences that materially affect a file-manager abstraction: object IDs can coexist with duplicate names under one parent, moves may be unsupported, and upload size must be known before a transactional write begins ([libmtp README, "API is obscure" and "I Want Streaming"](https://github.com/libmtp/libmtp/blob/v1.1.23/README)). Windows likewise defines an object ID that need not persist across sessions, a separate persistent unique ID, parent ID, original filename, and object size ([WPD object properties](https://learn.microsoft.com/en-us/windows/win32/wpd_sdk/object-properties)).

Therefore:

- A Windows Explorer portable-device entry is not a drive letter or Win32 path.
- A GVfs `mtp://` location is a GIO userspace virtual filesystem. GVfs describes its FUSE exposure as only "limited access" for non-GIO applications ([GVfs README](https://github.com/GNOME/gvfs/blob/bf0c67ca97facbf28a33e71d508e0fef64af25a6/README.md)).
- A KIO `mtp:/` URL is handled by a KIO worker, not mounted storage ([KIO MTP README](https://github.com/KDE/kio-extras/blob/ac14265b5339cef5eef5f09e9ea679d2b46afbb4/mtp/README)).
- A FUSE compatibility path such as `/run/user/$UID/gvfs/...` does not turn MTP into a block filesystem and must not be assumed to provide POSIX atomicity, random writes, rename, locking, or stable inode semantics.

An application should model transport objects and capabilities even if a host API presents path-shaped identifiers.

## Capability summary

| Host and path | Discovery and identity | Read / write / delete | Progress | Cancellation | Change tracking | Realistic host stability |
|---|---|---|---|---|---|---|
| Windows WPD | Inbox enumeration; friendly name, manufacturer, model, serial, protocol and transport properties | Documented object enumeration, property, stream, create/commit and delete APIs | Count bytes read/written against `WPD_OBJECT_SIZE`; no transfer-progress callback is required | `IPortableDevice::Cancel`, then `IStream::Revert`; partial/corrupt writes are explicitly possible | Device arrival/removal notifications plus WPD object events | **High**, subject to device firmware and user authorization |
| macOS `libmtp` + `libusb` | Raw USB enumeration and `libmtp` manufacturer/model/serial APIs | Full object read/write/delete APIs | `sent`/`total` callback | Callback return requests cancellation; some devices can lock up or require reset | USB hotplug plus optional MTP event API | **Medium/conditional**; no OS MTP broker and USB ownership must be validated |
| Linux GVfs/GIO | Host volume monitor/udev discovers and brokers MTP | GIO enumerate/stream/copy/delete over `mtp://` | GIO copy callback | `GCancellable`, but in-flight backend/libmtp interruption is not uniformly guaranteed | GIO file monitors backed by a subset of MTP events and USB removal | **Medium-high** on supported desktop installations |
| Linux KIO MTP | KDE Solid/daemon exposes `mtp:/` devices | KIO worker supports list/get/put/copy/delete | KIO processed-size signals | Listing checks worker death; current copy wait path does not propagate cancellation to the MTP daemon | Desktop/device notifications; not a general object event contract | **Medium-high** for KDE applications, lower as a portable app dependency |
| Linux direct `libmtp` | Raw USB/udev discovery and metadata APIs | Full object read/write/delete APIs | `sent`/`total` callback | Cooperative callback cancellation, with device-specific recovery risk | udev hotplug plus optional MTP events | **Medium**; the app owns permissions, serialization, conflicts and recovery |

## Windows

### Inbox path: Windows Portable Devices

Windows Portable Devices (WPD) is the public Windows framework for portable devices; Microsoft supplies standard drivers for MTP and mass-storage devices ([WPD application API](https://learn.microsoft.com/en-us/windows/win32/wpd_sdk/wpd-application-programming-interface)). It is a COM API using Windows SDK headers and `PortableDeviceGUIDs.lib` ([development requirements](https://learn.microsoft.com/en-us/windows/win32/wpd_sdk/general-requirements-for-application-development)). It is not a protocol-specific DLL that the application redistributes.

#### Discovery and identification

`IPortableDeviceManager::GetDevices` enumerates WPD Plug and Play IDs. The manager also returns friendly name, manufacturer and description, and the PnP ID is passed to `IPortableDevice::Open` ([enumerating devices](https://learn.microsoft.com/en-us/windows/win32/wpd_sdk/enumerating-devices)). Enumeration includes WPD devices other than MTP, so a client must inspect capabilities/properties instead of treating every result as MTP.

The WPD device object defines protocol, transport, manufacturer, model, serial number, firmware version, friendly name and optional functional unique ID properties ([device properties](https://learn.microsoft.com/en-us/windows/win32/wpd_sdk/device-properties)). These support display identity and matching, but the documentation does not guarantee that every physical device supplies a serial or functional unique ID. PnP ID, USB location, and friendly name should not alone be treated as permanent physical identity.

Arrival and removal are separate from WPD content events. Desktop applications can receive `WM_DEVICECHANGE` after `RegisterDeviceNotification`, then re-enumerate WPD devices ([device notification](https://learn.microsoft.com/en-us/windows/win32/devio/registering-for-device-notification)).

#### Object operations

- **List and identify objects:** `IPortableDeviceContent::EnumObjects` returns immediate child object IDs ([API](https://learn.microsoft.com/en-us/windows/win32/api/portabledeviceapi/nf-portabledeviceapi-iportabledevicecontent-enumobjects)). `IPortableDeviceProperties::GetValues` retrieves requested properties; asking for all properties may be significantly slower ([property retrieval](https://learn.microsoft.com/en-us/windows/win32/wpd_sdk/retrieving-properties-for-a-single-object)). Persistent unique object IDs are available only where the device/driver supplies them.
- **Read:** `IPortableDeviceResources::GetStream` returns an `IStream` for an object's resource and an optimal buffer size ([API](https://learn.microsoft.com/en-us/windows/win32/api/portabledeviceapi/nf-portabledeviceapi-iportabledeviceresources-getstream)). The application streams chunks and can report bytes completed against `WPD_OBJECT_SIZE`.
- **Write:** `IPortableDeviceContent::CreateObjectWithPropertiesAndData` returns an `IStream`; the object does not exist until synchronous `Commit`, `Revert` abandons the transfer, and `IPortableDeviceDataStream::GetObjectID` retrieves the created ID ([API](https://learn.microsoft.com/en-us/windows/win32/api/portabledeviceapi/nf-portabledeviceapi-iportabledevicecontent-createobjectwithpropertiesanddata)). This gives byte progress while writing, but commit latency has no finer documented progress.
- **Delete:** `IPortableDeviceContent::Delete` can delete a collection. `S_FALSE` means at least one object failed and the caller must inspect the failed-object collection ([delete example](https://learn.microsoft.com/en-us/windows/win32/wpd_sdk/deleting-content-from-the-device)). Deletion has no byte progress and is not rollback-safe as a multi-object transaction.

WPD exposes capability queries. Code must check supported content types, formats, commands, resources, and writable/deletable properties rather than infer support from a device name.

#### Threading, errors, progress and cancellation

WPD calls are synchronous at the operation/stream-call boundary and return `HRESULT`. They should run off the UI thread. `CLSID_PortableDeviceFTM`, available since Windows 7, aggregates the free-threaded marshaler; Microsoft still instructs a multithreaded application to create a separate `IPortableDevice` instance per thread so cancellation affects only that thread's I/O ([interface threading notes](https://learn.microsoft.com/en-us/windows/win32/api/portabledeviceapi/nn-portabledeviceapi-iportabledevice), [cancellation notes](https://learn.microsoft.com/en-us/windows/win32/api/portabledeviceapi/nf-portabledeviceapi-iportabledevice-cancel)).

`IPortableDevice::Cancel` cancels pending work on that interface. For an active write, Microsoft requires `IStream::Revert` and release afterward and warns that cancelling before `IStream::Write` completes may corrupt the data being written ([cancellation notes](https://learn.microsoft.com/en-us/windows/win32/api/portabledeviceapi/nf-portabledeviceapi-iportabledevice-cancel)). Cancellation is therefore a best-effort stop plus cleanup, not an atomic rollback guarantee. A client should treat the destination as indeterminate until it re-enumerates or verifies it.

`IPortableDevice::Advise` installs an asynchronous callback for device-generated events ([API](https://learn.microsoft.com/en-us/windows/win32/api/portabledeviceapi/nf-portabledeviceapi-iportabledevice-advise)). Object added/removed events can invalidate a cache, but event support is device capability-dependent. A post-operation refresh and reconnect refresh remain necessary.

#### Distribution and testability

- Runtime WPD components and MTP class support are part of Windows; only application code is shipped. The Windows SDK is a build dependency.
- The official COM sample exercises enumeration, capabilities, properties, transfers and event registration ([Microsoft PortableDeviceCOM sample](https://github.com/microsoft/Windows-classic-samples/tree/main/Samples/PortableDeviceCOM)). It is useful reference code, not a mockable transport.
- Microsoft's sample mentions an MTP Device Simulator, but this research did not find a current supported download or lifecycle statement. Its availability is an explicit unknown.
- Unit tests can isolate an application-owned adapter and use fake object graphs/streams. WPD behavior, driver return codes, disconnects, locked phones, and cancellation residue require Windows integration tests with physical devices.

### Alternative path: direct `libmtp`

`libmtp` can compile for MinGW, but this is not a dependable consumer path on normal Windows installations. `libusb` requires a supported generic USB driver when the device is not already using WinUSB, and its documentation describes installing WinUSB/libusbK/libusb-win32/UsbDk ([libusb Windows backend](https://github.com/libusb/libusb/wiki/Windows#driver-installation)). Replacing or bypassing the inbox MTP driver introduces installation, signing, device-ownership, and coexistence risk. WPD avoids those constraints.

## macOS

### Documented Apple APIs and the missing generic layer

Apple's `ICDeviceBrowser` is documented as finding **digital cameras and scanners**, and `ICDeviceTypeMask` contains only camera and scanner cases ([browser](https://developer.apple.com/documentation/imagecapturecore/icdevicebrowser), [type mask](https://developer.apple.com/documentation/imagecapturecore/icdevicetypemask)). `ICCameraDevice` can download and delete camera files and has cancellation hooks ([download](https://developer.apple.com/documentation/imagecapturecore/iccameradevice/requestdownloadfile(_:options:downloaddelegate:diddownloadselector:contextinfo:)), [delete](https://developer.apple.com/documentation/imagecapturecore/iccameradevice/requestdeletefiles(_:))). This can be useful for a device that macOS actually publishes as an image-capture camera, but it is not a documented contract for discovering arbitrary MTP storage, writing arbitrary objects, or traversing a generic MTP hierarchy.

Apple's `IOUSBHost` framework, available on macOS 10.15+, provides user-space access to custom and non-class-compliant USB devices and bulk/control/interrupt pipes ([IOUSBHost](https://developer.apple.com/documentation/iousbhost)). It is a raw USB mechanism, not an MTP object API; using it directly would require an MTP implementation and device-quirk work.

No public Apple documentation found in this research defines a generic MTP client, MTP mount, or File Provider integration for attached MTP devices. Absence from the reviewed public APIs is not proof that no private implementation exists; whether Apple has an undocumented/private MTP service is explicitly out of scope because it would not be a dependable distribution contract.

### Established generic path: `libmtp` over `libusb`

`libmtp` is an MTP initiator library over `libusb` ([upstream README](https://github.com/libmtp/libmtp/blob/v1.1.23/README)). Its public C API provides:

- raw-device detection and opening;
- manufacturer, model, serial, friendly-name and storage metadata;
- per-folder listing and file metadata;
- file download/upload through files, file descriptors, or callbacks;
- object delete and capability-gated move/copy/partial-edit operations;
- a progress callback with `sent` and `total`, where nonzero return requests cancellation;
- an error stack and a distinct cancelled error;
- synchronous and asynchronous event-reading APIs ([public header](https://github.com/libmtp/libmtp/blob/v1.1.23/src/libmtp.h.in)).

The high-level transfer functions are synchronous. They belong on a dedicated worker, and operations against one mutable `LIBMTP_mtpdevice_t` should be serialized unless a tested version/device combination proves otherwise. The public documentation does not state a general per-device thread-safety guarantee; the device structure contains mutable cache, storage and error-stack state.

Cancellation is cooperative through the progress/data callbacks. Upstream explicitly warns that while some devices tolerate a nonzero callback return, others lock up or require reset; it recommends consuming and discarding the remainder for robust read streaming ([libmtp README, "I Want Streaming"](https://github.com/libmtp/libmtp/blob/v1.1.23/README)). Delete has no progress callback, and a blocked synchronous USB transaction cannot be assumed to stop immediately.

macOS build viability is established rather than theoretical: upstream CI builds on `macos-latest` with Homebrew `libusb`, and Homebrew ships Intel and Apple Silicon bottles for `libmtp` 1.1.23 ([upstream CI](https://github.com/libmtp/libmtp/blob/v1.1.23/.github/workflows/main.yml), [Homebrew formula metadata](https://formulae.brew.sh/api/formula/libmtp.json)). This proves compilation and packaging, not compatibility with any particular MTP device.

#### USB ownership and sandboxing

When no macOS kernel extension owns a USB device, `libusb` says it can operate without root. If a driver owns the device, capture is whole-device, may require Apple-granted entitlement/provisioning, and upstream currently describes unresolved GUI entitlement constraints ([libusb macOS FAQ](https://github.com/libusb/libusb/wiki/FAQ#how-can-i-run-libusb-applications-under-macos-if-there-is-already-a-kernel-extension-installed-for-the-device-and-claim-exclusive-access)). Generic MTP interfaces normally have no Apple MTP driver, but composite and dual-mode devices must be tested.

A sandboxed macOS app needs the public `com.apple.security.device.usb` entitlement to use USB device-access APIs ([Apple entitlement documentation](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.device.usb)). A direct-distribution build must also sign embedded executables/libraries, enable hardened runtime, and notarize software built for Developer ID distribution ([Apple notarization requirements](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)). Bundled `libmtp` and `libusb` dylibs must be included in that signing/notarization process.

### Licensing, stability and testability

`libmtp` and `libusb` are LGPL-2.1-or-later according to their shipped license/formula metadata ([libmtp COPYING](https://github.com/libmtp/libmtp/blob/v1.1.23/COPYING), [libusb COPYING](https://github.com/libusb/libusb/blob/v1.0.30/COPYING)). Distribution in a proprietary app is possible under the LGPL's conditions, including notices/license and relinking/replacement obligations. Dynamic linking and publishing the exact corresponding library source and modifications are the straightforward compliance posture, but final packaging needs legal review.

The realistic stability rating is medium/conditional. `libmtp` is actively released and contains a large device/quirk database, but its own FAQ records locked-screen zero-storage behavior, competing process ownership, close-session bugs, unsupported operations and firmware-specific workarounds ([libmtp README](https://github.com/libmtp/libmtp/blob/v1.1.23/README)). Unit tests can cover an adapter over `libmtp`; end-to-end confidence requires Intel/Apple Silicon packaging tests and a physical-device matrix with lock/unlock, authorization, unplug, timeout, cancel, insufficient-space, duplicate-name and large-file cases.

## Linux

### Direct `libmtp`

The same public API described for macOS is available on Linux. Upstream documents udev/libgudev hotplug using `ID_MTP_DEVICE`, bus number and device number, while its default udev rules provide identification and access metadata ([libmtp README, "Autodetect with gudev"](https://github.com/libmtp/libmtp/blob/v1.1.23/README)). Shipping direct USB support therefore includes installing or relying on suitable udev rules, handling distro permissions, and supporting both hotplug and cold enumeration.

Only one process can normally claim an MTP USB interface. `libmtp` calls out GVfs and gphoto2 "hogging" the device, and `libusb` states a second application must exit before another can claim the interface ([libmtp FAQ](https://github.com/libmtp/libmtp/blob/v1.1.23/README), [libusb FAQ](https://github.com/libusb/libusb/wiki/FAQ#can-i-run-libusb-application-on-linux-when-there-is-already-another-application-which-has-already-claimed-the-interface)). A direct path must detect `BUSY`, explain the desktop-daemon conflict, and avoid racing an automounter.

Progress, cancellation, error, event and threading limits are the same as on macOS: synchronous high-level operations, cooperative transfer callbacks, an error stack, optional events, and no documented broad thread-safety guarantee. `libmtp`'s release API is easy to wrap and fake; actual protocol behavior remains hardware-dependent.

### GVfs/GIO broker path

GVfs is a userspace virtual filesystem for GIO with an optional limited FUSE bridge ([GVfs README](https://github.com/GNOME/gvfs/blob/bf0c67ca97facbf28a33e71d508e0fef64af25a6/README.md)). Its MTP backend uses GUdev for the selected USB device and `libmtp` for storage/object operations. The source serializes backend access with a mutex, translates `libmtp` errors to `G_IO_ERROR`, caches path-to-object mappings, listens for USB removal, and handles store/object add/remove events where the device emits them ([GVfs MTP backend](https://github.com/GNOME/gvfs/blob/bf0c67ca97facbf28a33e71d508e0fef64af25a6/daemon/gvfsbackendmtp.c)).

Applications address the device as non-native `GFile` URIs and use GIO's asynchronous enumerate, read, write/copy and delete APIs. GIO recommends asynchronous calls on a shared/main loop and worker threads for synchronous calls ([GFile](https://docs.gtk.org/gio/iface.File.html)). `g_file_copy_async` supplies completed/total byte progress in the caller's main context and accepts `GCancellable` ([copy API](https://docs.gtk.org/gio/method.File.copy_async.html)); enumerate and delete also have cancellable asynchronous forms ([enumerate](https://docs.gtk.org/gio/method.File.enumerate_children_async.html), [delete](https://docs.gtk.org/gio/method.File.delete_async.html)). `GCancellable` itself is thread-safe ([API](https://docs.gtk.org/gio/class.Cancellable.html)).

Those are GIO-level semantics, not a promise of immediate protocol abort or rollback. The MTP backend performs synchronous `libmtp` calls under its device mutex and checks job cancellation at safe points/callbacks. An app should wait for completion, then verify destination state after cancellation or disconnect. File monitoring is available, but device event support is incomplete and cache/path aliases remain possible; reconnect or foreground refresh should re-enumerate.

The broker avoids raw USB ownership and centralizes contention, permissions and quirks. Its deployment constraints are that GVfs and its MTP backend must be installed and running in the user session, and consumers must preserve URI/GIO semantics rather than convert the location to a native path. A Flatpak needs `org.gtk.vfs.*` D-Bus and `xdg-run/gvfsd` permissions for GIO access; direct USB instead needs raw USB permission/portal integration ([Flatpak GVfs and USB permissions](https://docs.flatpak.org/en/latest/sandbox-permissions.html#gvfs-access)).

The MTP backend source is LGPL-2.0-or-later ([source header](https://github.com/GNOME/gvfs/blob/bf0c67ca97facbf28a33e71d508e0fef64af25a6/daemon/gvfsbackendmtp.c)). Consuming the user's installed daemon over GIO does not require copying that backend into the application. Bundling a private GVfs stack would add GLib/D-Bus/daemon complexity and LGPL distribution duties.

### KDE KIO broker path

KDE's MTP worker exposes `mtp:/` and uses a session daemon and `libmtp` rather than a native filesystem ([README](https://github.com/KDE/kio-extras/blob/ac14265b5339cef5eef5f09e9ea679d2b46afbb4/mtp/README), [build dependencies](https://github.com/KDE/kio-extras/blob/ac14265b5339cef5eef5f09e9ea679d2b46afbb4/mtp/CMakeLists.txt)). The worker supports device/storage/folder listing, metadata, get, put, host-device copy, delete, create-directory and same-parent rename. It reports `totalSize`/`processedSize`, and its daemon emits copy progress ([worker source](https://github.com/KDE/kio-extras/blob/ac14265b5339cef5eef5f09e9ea679d2b46afbb4/mtp/kio_mtp.cpp)).

Cancellation is uneven in the inspected source: streamed listing checks `wasKilled()` and aborts its lister, but `waitForCopyOperation` only waits for progress/finished signals and does not issue a copy abort. Consequently KIO job cancellation must not be represented as guaranteed immediate MTP transfer cancellation without a targeted runtime test.

The worker is GPL-2.0-or-later ([source header](https://github.com/KDE/kio-extras/blob/ac14265b5339cef5eef5f09e9ea679d2b46afbb4/mtp/kio_mtp.cpp)). Using an installed worker through KIO is different from incorporating or redistributing its source/binary; bundling it requires GPL compliance review. Its current MTP autotest checks protocol classification and a desktop predicate, not object transfer behavior ([autotest](https://github.com/KDE/kio-extras/blob/ac14265b5339cef5eef5f09e9ea679d2b46afbb4/mtp/autotests/mtpprotocoltest.cpp)). Hardware integration tests remain required.

## Cross-cutting behavior and test requirements

### Identity

Use the strongest available device serial/functional ID plus transport metadata, but tolerate missing or changing values. Object handles are session-scoped unless the responder supplies a persistent unique ID. A cache key should retain storage ID and object identity and be invalidated on reconnect; path/name alone is ambiguous because duplicate sibling names are legal in the protocol model.

### Progress and cancellation contract

The dependable UI contract across hosts is:

1. Report byte progress for streamed reads/writes when total size is known.
2. Treat list/delete/commit as indeterminate operations where byte progress is unavailable.
3. Define cancellation as "request stop, await transport completion, clean up, then verify," not rollback.
4. Never kill a worker thread while it owns COM, `libmtp`, or `libusb` state.
5. On unplug/cancel/error, discard uncommitted host output, revert where the API supports it, and refresh the device before trusting cached destination state.

At the raw USB layer, even `libusb_cancel_transfer` is asynchronous and may report bytes already transferred; on macOS, cancelling one transfer cancels all transfers on that endpoint ([libusb asynchronous I/O](https://libusb.sourceforge.io/api-1.0/group__libusb__asyncio.html#async_cancel)). `libmtp` does not expose that raw asynchronous transfer object through its high-level file API.

### Minimum integration matrix

| Dimension | Required cases |
|---|---|
| Hosts | Supported Windows versions; macOS Intel and Apple Silicon if both ship; GNOME/GVfs and KDE/KIO Linux sessions; sandboxed package format if applicable |
| Devices | At least two current Android vendors, a device with multiple storages/SD card if supported, and any non-phone target category the product claims |
| Authorization | Locked, unlocked but not authorized, authorization accepted/denied/revoked, USB mode changed while connected |
| Object model | Deep trees, empty folders, duplicate names, Unicode names, missing metadata, nonpersistent object handles, read-only storage |
| Transfer | Zero-byte, large file, insufficient space, overwrite, delete nonempty folder, unsupported move/edit, device-side concurrent change |
| Failure | Unplug during enumerate/read/write/commit/delete, timeout/stall, app cancel at start/middle/end, daemon/device busy, reconnect at a new USB location |
| Packaging | Clean machine without development packages; signed/notarized macOS bundle; packaged Linux permissions; Windows without third-party USB drivers |

Adapter-level fakes can deterministically cover domain behavior and error mapping. They cannot establish MTP responder correctness. Upstream's current `libmtp` CI builds Linux and macOS but does not attach physical MTP hardware ([CI](https://github.com/libmtp/libmtp/blob/v1.1.23/.github/workflows/main.yml)), so product hardware testing cannot be delegated to upstream CI.

## Explicit unknowns and decision inputs

- The exact minimum Windows version the product will support is not specified. The current API is documented and available on supported Windows desktop releases, but any legacy minimum should be verified with a compiled probe and real driver.
- It is unknown whether the target device population always exposes serial numbers, persistent object IDs, object-change events, partial-object operations, or robust cancellation. These are responder capabilities, not universal MTP guarantees.
- It is unknown whether macOS App Store review accepts the intended raw-USB/MTP use and entitlement configuration for this product. The USB entitlement is public, but store policy should be validated before treating the store as a distribution channel.
- It is unknown whether a target macOS composite/dual-mode device has an Apple driver that claims another interface or the whole device. A USB-descriptor and open/claim probe on the hardware matrix is required.
- It is unknown which Linux package formats and desktop environments must work. GVfs/KIO broker availability, Flatpak permissions, and direct-USB fallback requirements depend on that distribution scope.
- There is no verified current MTP simulator that reproduces vendor firmware, authorization prompts, disconnect behavior and cancellation failures on all hosts. A physical-device lab is required for release confidence.
- Current KIO copy cancellation and GVfs in-flight cancellation need black-box timing/state tests before either is exposed as a hard cancellation guarantee.
- Licensing observations here identify upstream terms and packaging duties; they are not legal advice. Exact notices, source-offer/relink mechanism and dynamic-library replacement behavior need release-time legal review.

These facts leave multiple viable implementation paths. Architecture selection must separately weigh application framework, supported package channels, Linux desktop coverage, tolerance for native dependencies, and the device matrix; this report intentionally does not make that choice.
