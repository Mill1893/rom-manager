# Windows sync-core milestone report

Evidence for [Certify the Windows sync-core milestone](https://github.com/Mill1893/rom-manager/issues/39), assembled 2026-08-06.

## Disposition

**Blocked on physical validation only.**

CI is green on both hosts and both packaging jobs succeed, so the automated half of this gate is now evidenced rather than promised. What remains is physical: no packaged build has been installed and run on a real Windows host, and no AYN Odin 3 validation has been performed. The ticket is explicit that fake-transport, CI, and diagnostic results must not be generalized into packaged-host or physical claims, so this report **records what is established and names what is not** rather than certifying the gate.

The single remaining blocking cause is **access to physical hardware** — a Windows host to install on, and an AYN Odin 3 to sync to.

## The build this report describes

| Item | Value |
| --- | --- |
| Branch | `feat/durable-sync-state` (stacked on `feat/target-path-namespace` → `feat/sync-core-safety-foundation`) |
| Packaged build | Unsigned NSIS installer `ROM Manager_0.1.0_x64-setup.exe` (2.78 MB) and `ROM Manager_0.1.0_amd64.AppImage` (78.7 MB), both built in CI from commit `7bb4cfc` |
| Toolchain | Rust 1.97.1, pinned by `rust-toolchain.toml` |
| Durable schema version | 2 (`migrations/0001_initial.sql`, `migrations/0002_library.sql`) |
| NES fixture identity | `ac46556f3c6a5e3a0ed4ce7a4a09dd05ae8b01d012f473d29201b1ec2a200946`, reproduced by `fixtures/nes/generate.mjs` |
| Generic profile identity | `generic-folder` revision 1, snapshot digest in `fixtures/profiles/generic-folder.rev1.sha256` |

Both are published by CI as the `windows-unsigned-package` and
`linux-appimage-inputs` artifacts.

**The row above is a transcription, and it has gone stale.** It names commit
`7bb4cfc`, which is many merges old, and a SHA-256 pasted in by hand on the day
it was read. A digest copied once is evidence about a build that existed that
afternoon; it says nothing about the artefact a reader is holding, and there is
no way to tell from this page which of the two you have.

So it is no longer the source. Each packaging job now writes
`build-identity-linux.md` or `build-identity-windows.md` into its own artifact
via `scripts/record-build-identity.sh`, recording the commit, ref, workflow run
and attempt, runner image, toolchain versions, and a size and SHA-256 for every
file it produced — read from the build rather than typed into a document
afterwards. **Cite that file, not this table.** The table is kept as the record
of what this report was assembled against.

**No packaged build has been installed and run by a human.** Criterion 1 asks
for evidence from the installed application, and producing an installer is not
the same as installing one.

## Environments

| Environment | Used for | Status |
| --- | --- | --- |
| Linux dev host (WSL2, Ubuntu 24.04, ext4) | Full automated suite, confinement against real symlinks, scale baseline, network-denied run | Exercised |
| Windows 11 build 26200, NTFS, non-admin | Path and handle probes ([#52](https://github.com/Mill1893/rom-manager/issues/52)), **and the full tracer scenario matrix** | Exercised |
| `x86_64-pc-windows-gnu` cross-compilation | Type-checking the Windows code paths | Compiles |
| Windows CI runner, Server 2025 build 26100 | Native build, full suite, packaging | **Green** |
| Packaged Windows host | Installed-application behaviour | **Never run** |
| Linux desktop (WSLg, webkit2gtk 2.52.3) | The Tauri application and its AppImage | Exercised |
| AYN Odin 3 | Physical WPD validation | **Never run** |

## The desktop application

For most of this milestone `tauri.conf.json` described an application that did
not exist: no crate, no entry point, no icons, no frontend bundler. It exists
now, and the following were observed rather than asserted on a Linux desktop:

| Observation | Result |
| --- | --- |
| The application compiles and links | yes |
| It launches and creates its window | yes |
| It writes its database to the XDG data directory | yes — `~/.local/share/rom-manager/` |
| An AppImage bundles | yes — 78 MB |
| The AppImage runs with no system-wide install | yes |
| The AppImage writes nothing into its own mount point | yes — state went to XDG |

Re-observed on 2026-08-07 against the AppImage built from `c9b9b8a`, launched
with a clean `HOME` so that every path it touched was attributable:

| Observation | Result |
| --- | --- |
| Launches from the AppImage with no system-wide install | yes |
| Creates its own state under the XDG data root | yes — `~/.local/share/rom-manager/` holding `library.sqlite3`, its `-wal` and `-shm`, and `library/` |
| Cannot write into its own mount point | stronger than "did not" — the mount carries `ro,nosuid,nodev`, and an explicit `touch` inside it returns `Read-only file system` |
| WebKitGTK's separate per-application directory | `~/.local/share/dev.mill1893.rom-manager/` — `WebKitCache`, `CacheStorage`, `storage`, `mediakeys`, `hsts-storage.sqlite` |

That last row is recorded because it looks worse than it is, and someone
auditing this will find it. `hsts-storage.sqlite` is HTTP Strict Transport
Security state: WebKitGTK creates its network-session files when the web
process starts, whether or not anything is ever fetched. It is not evidence of
network access. It is also not evidence of the *absence* of network access —
that claim rests on the dependency audit below, not on this directory.

**What this does not establish.** An unsigned NSIS installer now builds in CI,
but nobody has run it. Nothing here speaks to WebView2 bootstrapping or to the
upgrade, uninstall, and reinstall boundaries
[#35](https://github.com/Mill1893/rom-manager/issues/35) requires — those are
observations about an *installed* application, and there has not been one.

Nominating ROM Packs and Media Targets is no longer unfinished:
[#78](https://github.com/Mill1893/rom-manager/pull/78) added
`pick_import_folder`, `pick_media_target`, and `scan_import_folders`, so a
fresh install can be given something to work with. But a fresh install still
starts empty by design, and **no end-to-end user journey through the interface
has been walked here.** The wizard's steps are covered at the command level by
`tests/desktop_session.rs` — including `what_the_plan_wanted_is_what_reached_the_device` —
which is a different claim: it exercises the commands the interface calls, not
the interface. Nobody has clicked through this application and watched a ROM
reach a device.

## The tracer on a real Windows desktop

`rom-manager-tracer` runs every scenario the sync core claims against a volume a
person points it at. On Windows 11 build 26200, NTFS, non-admin, from a
`x86_64-pc-windows-gnu` cross-build:

| Scenario | Result |
| --- | --- |
| Marker initialization | pass |
| Add and read-back verification | pass — 24,592 bytes placed and verified |
| Retain | pass — the second run wants no changes |
| Adoption | pass — pre-existing identical content adopted intact |
| Conflict | pass — blocked with `PathConflict` |
| Managed removal | pass |
| Cancellation | pass — 1 action reported not attempted |
| Post-plan mutation | pass — the stale approval was refused |
| Manifest agreement | pass — a disagreeing manifest produced a blocked plan |
| Refresh plus new-plan recovery | pass |
| Durable state across restart | pass — schema 5 reopened with bindings intact |
| Changed locator | pass — identity and manifest survived relocation |
| Capacity blocking | **skipped** — proving it means filling the volume |
| Disconnect | **skipped** — requires physically removing media mid-write |

**What this does and does not establish.** It is the sync core executing on a
real Windows desktop rather than a CI runner, which is the first evidence of
that kind. It is *not* the packaged application: there is no installer, the
tracer is a console program, and the two skipped scenarios were not attempted.
They are reported as skipped rather than counted, because a pass that did not
happen is worse than a gap that is named.

## Automated suites

123 Rust tests and 8 UI tests. Formatting, `clippy --all-targets -- -D warnings` on both the host and the Windows target, and `tsc --noEmit` under `strict` with `noUncheckedIndexedAccess` and `exactOptionalPropertyTypes`.

| Suite | Covers |
| --- | --- |
| `sync_core.rs` | The original reconciliation contract |
| `target_path_namespace.rs` | Namespace validation, equivalence key, frozen profile identity |
| `action_capabilities.rs` | Per-action capability gating, unreported capacity |
| `plan_approval.rs` | Single-use approval and its bindings |
| `occupied_paths.rs` | All ten occupied-path classifications |
| `failure_residue.rs` | Terminal outcomes, residue, operation reporting |
| `confinement.rs` | **Real symlinks** — leaf, intermediate directory, write, delete, root, hard link |
| `durable_state.rs` | Migrations, restart, recovery, single-use approval, pack selection |
| `app_boundary.rs` | Plan display completeness, staleness, acknowledgement matching |
| `wpd_contract.rs` | MTP semantics: ambiguity, partial write, locked device, retry exhaustion |
| `scale.rs` | 10,000-artifact planning, repeat stability, steady-state retain |
| `PlanReview.test.tsx` | Keyboard operation, labelling, announcements, blocked reasons |

## Fault matrix

Every row is covered by a deterministic automated test on the **fake and filesystem** transports. None is covered on a packaged host or a physical device.

| Fault | Outcome | Evidence |
| --- | --- | --- |
| Insufficient space | Blocked at planning; nothing written | `a_capacity_change_is_visible_and_blocks_an_oversized_write`, `InsufficientCapacity` |
| Cancellation | `Cancelled`; remaining actions reported not-attempted | `a_cancelled_operation_reports_what_it_did_not_attempt` |
| Restart mid-operation | `Indeterminate` on next start; inventory stale; no resume | `an_interrupted_operation_becomes_indeterminate_on_restart` |
| Disconnect during write | `Indeterminate`, action marked **uncertain** not failed | `a_disconnect_marks_the_action_uncertain_not_failed` |
| Stale inventory | Plan not executable; refresh required | `a_stale_plan_is_not_executable_even_when_nothing_about_it_is_wrong` |
| Changed locator | Target identity and manifest preserved | `a_relocated_target_keeps_its_identity_and_manifest` |
| Marker conflict | Blocks; marker never rewritten | `manifest_disagreement_blocks_destructive_authority`, `unsupported_marker_schema_blocks_refresh` |
| Post-plan mutation | Approval invalid; no mutation | `target_mutation_between_planning_and_execution_invalidates_the_approval` |
| Read-back mismatch | `Incomplete`; content **left in place** and recorded | `unverifiable_content_is_left_in_place_and_recorded` |
| Retry exhaustion | Stable transport error | `retry_exhaustion_maps_to_a_stable_error` |

No case produces a false success, and no case performs a removal that was not first verified. Any failure stops every remaining permanent removal.

## Migration and restart evidence

A store written at schema 1 migrates forward with its rows intact; a store at a higher version than the build understands is refused. Preserved across restart: Media Target identity and bindings, mirrored manifests, exact ROM Pack selection with content digests, operation history, and indeterminate recovery state.

**Gap:** Library *availability* — content reachable independently of its original source — belongs to the app-owned import work in [#22](https://github.com/Mill1893/rom-manager/issues/22) and is not part of this gate.

## Scale and resource baseline

Measured on the Linux dev host, release profile:

| Measurement | Result |
| --- | --- |
| Planning 10,000 Target Artifacts | **8.0 ms** |
| Refreshing the inventory for that set | 0.4 µs |
| Repeated planning | Identical plan digest — no growth |
| 1,000 already-managed artifacts | All retain, zero required capacity |

**This is not the declared reference-host threshold.** That threshold is a packaged-Windows figure and no packaged build exists. The result above establishes only that planning is linear rather than quadratic; it must not be cited as the Windows number.

Hashing and transfer baselines against real media are unmeasured.

## Privacy

| Check | Result | Enforced |
| --- | --- | --- |
| Full suite with the network denied | **513 tests pass** inside a network namespace with no interfaces | CI, every push — `unshare -rn cargo test --all-targets` |
| Network-capable crates in the dependency tree | **None** in the default feature set | CI — `scripts/assert-no-network-capability.sh` |
| Network APIs in application source | **None** — no `std::net`, `TcpStream`, `TcpListener`, `UdpSocket`, or `SocketAddr` | CI — same script |
| UI runtime dependencies | **None** — `dependencies` is absent; React and the test tools are dev-only | CI — same script |
| Telemetry-shaped identifiers | **None** found in Rust or TypeScript source | **Hand-checked 2026-08-06.** Nothing re-checks this |
| WebView capability | Events and window title only; no fs, sql, shell, http, or updater plugin is depended on | Structural — no such plugin is compiled in |

The application cannot reach the network, because nothing in it can open a socket. Provider lookup ([#30](https://github.com/Mill1893/rom-manager/issues/30)) will introduce the first network capability and is explicitly opt-in behind the `provider-http` feature.

**These were prose until 2026-08-07**, verified by hand once and never since — and the first row had already drifted, citing 123 tests against a suite that had grown to 513. Four of the six are now checked on every push. The check verifies itself as well as the tree: it re-runs its own crate detector against `--features provider-http`, where the answer is known to be yes, so a typo in the pattern cannot leave it passing while testing nothing.

**What this does not establish.** The crate check is a deny-list, and a deny-list cannot be complete — a network client under an unfamiliar name would pass it. The network-denied run is the check that does not depend on knowing any names, and it is the one to trust. Neither says anything about the *packaged* application's behaviour on a host, which is [#37](https://github.com/Mill1893/rom-manager/issues/37) and [#77](https://github.com/Mill1893/rom-manager/issues/77) and needs a person with the installer and a disconnected cable.

## Accessibility

Covered on the components that exist: full keyboard operation, accessible names, blocked reasons tied to the control they block via `aria-describedby`, conflicts announced as alerts, and status announcements distinguishing phase, cancellation, and an indeterminate outcome.

**Not covered:** WCAG 2.2 AA contrast and 100/200 percent scaling need a real browser with layout, and the criterion asks for evidence on the **packaged application**, which does not exist. Criterion 6 is unmet.

## Required before this gate can close

1. ~~Enable GitHub Actions.~~ **Done.** The suite is green on `ubuntu-latest` and `windows-latest`, both packaging jobs succeed, and the host-behaviour probe report is published as an artifact.
2. **Produce a packaged Windows build** ([#35](https://github.com/Mill1893/rom-manager/issues/35)) and validate it on a Windows host ([#37](https://github.com/Mill1893/rom-manager/issues/37)).
3. **Validate WPD on the AYN Odin 3** ([#38](https://github.com/Mill1893/rom-manager/issues/38)).
4. **Complete the desktop workflow** ([#34](https://github.com/Mill1893/rom-manager/issues/34)) — the Tauri shell and the remaining steps.
5. **Re-run the reparse probes elevated**, so tag-agnostic no-reparse confinement is observed rather than inferred from junctions alone.
6. **Measure the reference-host threshold** on the packaged Windows build.

## Deferred to the release candidate

Not required here, and not claimed: a second unrelated Android device, Linux MTP ([#27](https://github.com/Mill1893/rom-manager/issues/27)), macOS parity ([#28](https://github.com/Mill1893/rom-manager/issues/28)), format breadth ([#19](https://github.com/Mill1893/rom-manager/issues/19)), ES-DE metadata ([#21](https://github.com/Mill1893/rom-manager/issues/21), [#23](https://github.com/Mill1893/rom-manager/issues/23)), provider integration ([#30](https://github.com/Mill1893/rom-manager/issues/30)), and the visual language ([#31](https://github.com/Mill1893/rom-manager/issues/31)).

## What this report deliberately does not claim

- That anything works on a packaged Windows application.
- That anything works on a physical device, over MTP or otherwise.
- That the Windows confinement walk behaves *identically* to Unix. It now executes on a Windows CI host, which is real evidence, but a Server 2025 runner is not a user's desktop.
- That reparse rejection holds for tags other than junctions.
- That the measured planning figure is the Windows reference-host threshold.
- Resistance to a hostile same-privilege process, which is outside the stated threat model.
