# Durable Library import gate report

Evidence for [Certify the durable Library import gate](https://github.com/Mill1893/rom-manager/issues/65), assembled 2026-08-06.

## Disposition

**Evidenced.** Every behavioural criterion for this gate is implemented and covered by automated tests. Unlike the [Windows sync-core gate](../windows-sync-core/milestone-report.md), nothing here needs a device, a packaged installer, or physical hardware — the gate is Library import into app-owned storage, and a filesystem is the only environment it requires.

**Superseded on 2026-08-07.** This section previously read "Blocked on CI only", because GitHub Actions had never executed on this repository — `actions/runs` reported zero across every branch. That is no longer true. CI runs on every push and is green on both hosts:

| Evidence | Value |
| --- | --- |
| Run | [`31221804858`](https://github.com/Mill1893/rom-manager/actions/runs/31221804858) on `main` at `af857fb` |
| Jobs | `test (ubuntu-latest)`, `test (windows-latest)`, `host-behaviour`, `package-linux`, `package-windows` — all green |
| Rust tests | 513 on Linux, 503 natively on Windows Server 2025 |
| UI tests | 62 across 6 files |

The ten tests that do not run on Windows are gated `cfg(unix)` because they need Unix symlink semantics: the whole of `tests/confinement.rs`, two cases in `tests/import_rescan.rs`, and one in `tests/sync_core.rs`. They are skipped there, not failing.

The single outstanding item named by this report is therefore closed. Nothing else about the gate changed — the criteria below were already met when it was written.

## Schema and fixtures

| Item | Value |
| --- | --- |
| Durable schema version | 5 |
| Migrations | `0001_initial`, `0002_library`, `0003_library_storage`, `0004_source_containers`, `0005_import_folders` |
| NES fixture | `ac46556f3c6a5e3a0ed4ce7a4a09dd05ae8b01d012f473d29201b1ec2a200946` |
| Generic profile | `generic-folder` revision 1 |

## Criteria

| Criterion | Evidence |
| --- | --- |
| Exact ROM Pack references survive metadata, representation, and availability changes | `an_exact_selection_survives_availability_changes` — content is quarantined and the selection is byte-identical afterwards |
| Cache eviction has no effect on selection | `cache_eviction_has_no_effect_on_a_selection` |
| Unavailable selected content blocks planning and sync | `SetState::Unavailable` from `set_availability`, plus `quarantined_content_is_not_available` |
| End-to-end import → dedup → materialize → exact selection → successful sync | `the_whole_path_runs_import_to_successful_sync` — with the source file deleted mid-path |
| Import fault matrix without false success | `the_import_fault_matrix_never_produces_a_false_success` |
| Privacy | Inherited from the sync-core gate: the suite passes inside a network namespace with no interfaces, and no network-capable crate exists in the tree |
| Migration | `an_old_store_migrates_forward_preserving_its_rows`, `a_store_from_a_newer_build_is_refused` |

## Suites

| Suite | Tests | Covers |
| --- | --- | --- |
| `library_import.rs` | 10 | App-owned storage, provenance, dedup, partial-batch failure |
| `archive_import.rs` | 7 | Source Containers, derived materializations, malformed and hostile archives |
| `materialization_cache.rs` | 8 | Content addressing, verification, atomicity, LRU, leases, safe clearing |
| `library_integrity.rs` | 6 | Verification, quarantine, exact-match recovery |
| `import_rescan.rs` | 8 | Import Folders, moved/changed/vanished origins, indirection refusal |
| `set_availability.rs` | 7 | Incomplete vs unavailable vs available |
| `library_removal.rs` | 8 | Impact preview, confirmation, retained identities, blocked cascades |
| `import_gate.rs` | 5 | The end-to-end path and selection stability |

## The load-bearing guarantees

Four properties the rest depends on, each asserted directly rather than argued:

1. **Content outlives its source.** `an_imported_rom_outlives_its_source` deletes the source file *and* the folder around it, then reads the content back.
2. **A mismatch is corruption, never an update.** The record is never rewritten to match the disk, and unexpected bytes are moved aside rather than deleted — they may be the user's only copy.
3. **The cache never establishes availability.** `the_cache_alone_never_establishes_availability` — because if it did, clearing the cache could make content unavailable, and clearing would stop being safe.
4. **Nothing cascades.** Deleting an identity is refused while a ROM Pack selects it; removing bytes retains the identity so reimporting makes it whole again.

## Not covered here

- **Accessibility on the import workflow.** The UI components for import do not exist yet; the plan-review step's coverage does not extend to them.
- **Performance at Library scale.** The 10,000-artifact figure in the sync-core report measures planning, not import throughput or hashing.
- **Formats beyond loose files and ZIP** — deferred to [#19](https://github.com/Mill1893/rom-manager/issues/19).
- ~~**Any CI result.**~~ **Superseded 2026-08-07** — see the disposition above.

## Required before this gate can close

1. ~~**Enable GitHub Actions**, and confirm these suites pass on both `ubuntu-latest` and `windows-latest`.~~ **Done.** Green on both, on every push.
2. Build the import UI and cover its accessibility, alongside the remaining work on [#34](https://github.com/Mill1893/rom-manager/issues/34).
3. Measure import and hashing throughput against a declared threshold.
