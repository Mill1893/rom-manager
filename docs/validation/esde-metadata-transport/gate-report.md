# ES-DE metadata transport coverage report

Evidence for [Complete ES-DE metadata transport coverage](https://github.com/Mill1893/rom-manager/issues/21), assembled 2026-08-07 against `main` at `113e086`.

## Disposition

**Blocked on physical validation only.**

Every behavioural criterion this ticket names is implemented and covered by tests that pass in CI on both hosts. What remains is not code and not CI: it is whether ES-DE itself reads what we wrote, and whether a card reader and an MTP connection to the same device are recognised as one Media Target. Both need the AYN Odin 3 and belong to [#38](https://github.com/Mill1893/rom-manager/issues/38).

This report exists because the ticket asks for release-candidate evidence and there was none. [#65](https://github.com/Mill1893/rom-manager/issues/65) and [#73](https://github.com/Mill1893/rom-manager/issues/73) each have a report under `docs/validation/`; the transport half of the metadata work did not, which meant its coverage could only be established by reading test names.

## Scope

The coexistence contract — field ownership, three-way merge, retirement, publication safety — is [gate 3's](../esde-metadata/gate-report.md) and is not restated here. This report covers only what changes when the destination is reached over MTP, or is split across two Media Targets.

| Item | Value |
| --- | --- |
| Profile | `esde-android` revision 1, pinned to ES-DE 3.1.1 |
| ROM root | `ROMs/<system-key>/` |
| Metadata root | `ES-DE/gamelists/<system-key>/gamelist.xml` |
| Role model | `RoleAssignment::Combined { target_id }` or `Split { rom_content, frontend_metadata, confirmed }` |
| Transport under test | `WpdLikeBackend` |

## Criteria

| Criterion | Evidence |
| --- | --- |
| Independent per-target planning | `split_targets_plan_independently_with_no_cross_target_atomicity` |
| ROM-first convergence | `metadata_waits_for_rom_content_to_converge`, `split_targets_require_the_content_target_to_converge_first` |
| No false cross-target atomicity | `mtp_never_claims_atomic_metadata_publication`, `a_metadata_failure_on_a_split_target_never_rolls_back_content` |
| MTP identity across reconnection | `a_reconnect_at_a_new_locator_keeps_the_target_identity` |
| Unicode and nested paths | `unicode_and_nested_gamelist_paths_survive_the_transport` |
| Cancellation | `cancellation_before_a_metadata_write_leaves_the_document_untouched` |
| Partial failure | `a_locked_device_fails_metadata_cleanly_rather_than_partially`, `an_indeterminate_manifest_publication_is_reported_as_unestablished`, `a_disconnect_during_metadata_publication_leaves_the_prior_copy_recoverable` |
| Round trip over the transport | `a_gamelist_round_trips_through_an_mtp_like_transport` |
| Both roles resolved on one device | `the_esde_profile_resolves_both_roles_on_one_device` |
| Unconfirmed pairing blocks export | `an_unconfirmed_pairing_blocks_planning_entirely` |

## Suites

| Suite | Tests | Covers |
| --- | --- | --- |
| `metadata_over_mtp.rs` | 12 | Everything above that concerns the transport |
| `combined_target.rs` | 10 | Stage ordering, split-target readiness, action preview |

Both run on `ubuntu-latest` and `windows-latest` in CI on every push.

## The load-bearing guarantees

1. **Metadata never rolls back content.** A metadata failure on a split target leaves verified ROM content exactly where it is. The two targets are separate destinations that happen to be planned together, and a failure on one must not reach backwards into the other.
2. **Ordering is the safety property, and it survives being split.** Content, then metadata, then removals — a rule gate 3 establishes on one filesystem target, and which `split_targets_require_the_content_target_to_converge_first` re-establishes when the two roles live on different devices.
3. **MTP is never described as atomic.** `atomic_publish` is unconditionally false, so no plan can be built on a publication guarantee the transport does not have.
4. **A pairing is not a configuration detail.** An unconfirmed split assignment blocks planning outright rather than defaulting to one target, because guessing which device should receive a user's gamelist is not a recoverable mistake.
5. **Identity is not the locator.** A device that reappears at a different address is the same Media Target; this is what makes MTP's unstable addressing survivable.

## What this does not establish

- **That ES-DE reads any of it.** Every gamelist here is synthetic — shaped like ES-DE's, produced by this codebase. Nothing in this report has been read by an ES-DE installation. Frontend discovery on the Odin is [#38](https://github.com/Mill1893/rom-manager/issues/38).
- **Card-reader identity.** The ticket asks that the same Media Target reached over MTP and over a card reader be recognised as one target. Only the MTP half is covered: `a_reconnect_at_a_new_locator_keeps_the_target_identity` changes the locator within one transport. Two *different* transports resolving to one identity is untested and needs the hardware.
- **Any real MTP publication.** `WpdLikeBackend` is documented in `src/wpd.rs` as "not a simulation of a device", and it proves nothing about one. It exercises the adapter's contract — error mapping, cancellation, verification ordering — deterministically, including cases a real device produces rarely and unrepeatably. That is a different claim from working against an Odin, and the difference is the whole of what is left.
- **Accessibility of the metadata workflow.** The UI for it does not exist.

## Required before this ticket can close

1. Read an exported gamelist with a real ES-DE installation on the AYN Odin 3 ([#38](https://github.com/Mill1893/rom-manager/issues/38)).
2. Reach one Media Target over both MTP and a card reader, and confirm it resolves to a single identity.
3. Run a metadata publication over real MTP, including a disconnect mid-write, and confirm the recorded outcomes match what the deterministic backend predicts.
