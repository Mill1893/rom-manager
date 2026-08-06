# Windows sync-core tracer validation

## Disposition

**Blocked.** The transport-independent fake and filesystem tracer is implemented, but this milestone cannot close until the Tauri desktop, durable SQLite operation state, Windows WPD adapter, packaged Windows checks, accessibility checks, and physical AYN Odin 3 matrix are implemented and pass. Simulation must not certify those boundaries.

## Implemented evidence

- Project-owned deterministic loose iNES fixture with frozen SHA-256 `ac46556f3c6a5e3a0ed4ce7a4a09dd05ae8b01d012f473d29201b1ec2a200946`.
- Generic folder Device Profile revision 1 mapping NES content to `ROMs/nes` with relative-path confinement.
- Marker-based Media Target identity independent of the current Transport Binding locator.
- Immutable, content-digested Sync Plans with inventory, binding, profile, ROM Pack, capability, capacity, and permanent-removal prerequisites.
- Explicit add, retain, adopt, and managed-remove actions; canonical-path conflicts and unknown content are preserved or blocked rather than overwritten.
- Strong read-back verification before management authority; fresh strong verification immediately before permanent leaf removal.
- Target/local Managed Artifact Manifest agreement blocks destructive authority.
- Deterministic cancellation, disconnect, read-back mismatch, insufficient-capacity, locator-change, manifest-disagreement, and post-plan-mutation cases.
- Real filesystem contract test using the same public sync workflow as the deterministic fake.
- Linux and Windows CI definitions for formatting, linting, fixture reproducibility, and automated tests.

Run the currently available automated evidence with:

```sh
cargo test --all-targets
```

## Required before closure

- Integrate the sync workflow into the selected Tauri 2, React, strict TypeScript, and SQLite application architecture.
- Persist exact plans, approvals, operation journals, inventory freshness, target identity, bindings, and mirrored manifest bytes across restart and migration.
- Implement Windows WPD on a dedicated COM-initialized worker without assuming filesystem rename, stable object IDs, timestamps, or atomic publication.
- Add durable progress, cancellation acknowledgement, restart-to-indeterminate recovery, bounded retries, and manifest reconciliation UI.
- Produce an unsigned Windows NSIS package and record its exact commit and SHA-256.
- On clean Windows hosts, verify install, launch, upgrade, uninstall without application-data deletion, offline operation, application-data placement, dependencies, and notices.
- Run packaged filesystem sync on required Windows and storage combinations, including changed locator, capacity, conflict, cancellation, disconnect, restart, mutation, mismatch, and retry exhaustion.
- Run packaged WPD against AYN Odin 3 internal and portable storage, including authorization, marker read-back, add/read-back, retain, adoption, managed removal, unplug/cancel recovery, manifest agreement, and refresh plus new-plan recovery.
- Verify keyboard completion, visible focus, assistive labels and announcements, WCAG 2.2 AA contrast, and 100 and 200 percent scaling on the packaged build.
- Record the 10,000 Target Artifact planning result on the declared reference host and resource-growth baselines.
- Publish a final versioned report containing exact build, fixture, environment, firmware, storage, automated, packaged-host, physical, accessibility, performance, privacy, known-limitation, and deferred-coverage evidence.

## Current environment

The implementation session ran under Linux WSL without a native Windows package host or connected AYN Odin 3. Those missing required environments block the affected milestone claims by design.
