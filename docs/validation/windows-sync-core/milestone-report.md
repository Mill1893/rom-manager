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
| Packaged build | Linux AppImage, 78 MB, built locally. **No Windows installer has been produced.** |
| Toolchain | Rust 1.97.1, pinned by `rust-toolchain.toml` |
| Durable schema version | 2 (`migrations/0001_initial.sql`, `migrations/0002_library.sql`) |
| NES fixture identity | `ac46556f3c6a5e3a0ed4ce7a4a09dd05ae8b01d012f473d29201b1ec2a200946`, reproduced by `fixtures/nes/generate.mjs` |
| Generic profile identity | `generic-folder` revision 1, snapshot digest in `fixtures/profiles/generic-folder.rev1.sha256` |

**This report does not link one exact packaged build, because none exists.** Criterion 1 is unmet on that basis alone.

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

**What this does not establish.** No Windows installer has been produced, so
nothing here speaks to NSIS, WebView2 bootstrapping, or the upgrade, uninstall,
and reinstall boundaries [#35](https://github.com/Mill1893/rom-manager/issues/35)
requires. The application's ROM Pack and Media Target catalogues are also
deliberately empty — nominating those is unfinished work — so the wizard starts
with nothing to choose and no end-to-end user journey has been walked.

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

| Check | Result |
| --- | --- |
| Full suite with the network denied | **123 tests pass** inside a network namespace with no interfaces |
| Network-capable crates in the dependency tree | **None** — no reqwest, hyper, tokio, curl, ureq, rustls, or native-tls |
| Network APIs in application source | **None** — no `std::net`, `TcpStream`, or `UdpSocket` |
| UI runtime dependencies | **None** — `dependencies` is empty; React and the test tools are dev-only |
| Telemetry-shaped identifiers | **None** found in Rust or TypeScript source |
| WebView capability | Events and window title only; no fs, sql, shell, http, or updater plugin is depended on |

The application cannot reach the network, because nothing in it can open a socket. Provider lookup ([#30](https://github.com/Mill1893/rom-manager/issues/30)) will introduce the first network capability and is explicitly opt-in.

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
