# ES-DE metadata coexistence gate report

Evidence for [Certify the ES-DE metadata coexistence gate](https://github.com/Mill1893/rom-manager/issues/73), assembled 2026-08-06.

## Disposition

**Blocked on CI only.** Every behavioural criterion is implemented and covered by tests that pass locally. Like gate 2, this gate needs a combined **filesystem** target — not a device, not a packaged installer, not physical hardware. The one outstanding item is that **GitHub Actions has never executed on this repository**.

## Scope

The first release exports device metadata only for the version-pinned **ES-DE on Android** profile. The Generic Folder Tree emits ROM content only. Daijisho, Pegasus, RetroArch, and custom adapters are outside this contract, and provider or user artwork never reaches a Media Target.

| Item | Value |
| --- | --- |
| Profile | `esde-android` revision 1, pinned to ES-DE 3.1.1 |
| Gamelist location | `<metadata-root>/gamelists/<system-key>/gamelist.xml` |
| Entry key | `./`-relative path under the configured system ROM directory |
| Owned fields | `name`, `sortname`, `desc`, `releasedate`, `developer`, `publisher`, `genre`, `players` |

## Criteria

| Criterion | Evidence |
| --- | --- |
| Local or effective textual projections on a combined filesystem target | `a_projection_is_written_and_reread_on_a_combined_target` |
| Explicit adoption | `an_equal_pre_existing_field_is_adopted_then_owned` |
| Frontend-owned state preserved | `frontend_state_survives_every_operation` |
| Field-level three-way conflicts | `metadata_merge.rs`, 10 tests |
| Stopped-ES-DE confirmation | `publication_requires_confirmation_that_es_de_is_stopped` |
| Target-local recovery | `restoration_is_offered_before_regeneration`, `the_recovery_copy_rotates_only_after_a_verified_publication` |
| Verified reread | `Publication::prepare` reparses the staged replacement before it goes live |
| Interruption recovery | `the_fault_matrix_never_costs_content_or_user_state` |
| Pending removals blocked after metadata failure | `a_metadata_failure_skips_every_removal` |

## Suites

| Suite | Tests | Covers |
| --- | --- | --- |
| `esde_profile.rs` | 7 | Destination Roles, pairing confirmation, path rules, frozen identity |
| `metadata_projection.rs` | 11 | Field mapping, omission over approximation, title disambiguation |
| `gamelist_coexistence.rs` | 13 | Shared-document read/rewrite, refusal to touch frontend state |
| `metadata_merge.rs` | 10 | Three-way merge, adoption offers, conflicts |
| `metadata_retirement.rs` | 9 | Ledger-gated retirement, ineligible-field withdrawal |
| `metadata_publish.rs` | 10 | Publication refusals, recovery, copy rotation |
| `combined_target.rs` | 10 | Stage ordering, split-target readiness, preview |
| `metadata_gate.rs` | 6 | End-to-end and fault matrix |

## The load-bearing guarantees

1. **The document is not ours.** ROM Manager owns mapped fields and nothing else. `set_owned_field` *refuses* anything outside the mapped set, so a caller cannot accidentally claim favourites or play counts.
2. **Two values cannot tell a user edit from a stale export.** The ledger is the third value; a device-side change is a conflict resolved neither by overwriting nor importing.
3. **Removal must be correct about the past.** Retirement is gated on the ledger twice — recorded as ours, and still matching what was recorded.
4. **The order is the safety property.** Content, then metadata, then removals — so a metadata failure never costs files.
5. **Omission beats approximation.** A partial date or open-ended player count is omitted, because the user cannot tell an exported guess from an exported fact.

## Not covered here

- **Accessibility of the metadata workflow** — the UI does not exist yet ([#34](https://github.com/Mill1893/rom-manager/issues/34)).
- **A real ES-DE installation.** Every test uses synthetic gamelists shaped like ES-DE's. Nothing here has been read by ES-DE itself, which is [#38](https://github.com/Mill1893/rom-manager/issues/38)'s territory on the Odin.
- **MTP metadata publication.** The atomicity limit is modelled and disclosed but never exercised against a device.
- **Any CI result.**

## Required before this gate can close

1. **Enable GitHub Actions** and confirm these suites pass on both hosts.
2. Build the metadata workflow UI and cover its accessibility.
3. Read an exported gamelist with a real ES-DE installation, ideally on the Odin during [#38](https://github.com/Mill1893/rom-manager/issues/38).
