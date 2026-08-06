# Frozen Device Profile identities

Each file here holds the canonical snapshot digest for one `(profile id,
revision)`, per [issue #46](https://github.com/Mill1893/rom-manager/issues/46).

`(id, revision)` identifies exactly one snapshot of the profile's
behavior-bearing fields — platform, managed root, accepted extensions, marker
and manifest locations, and the target-path construction rule. Presentational
fields are excluded and never force a revision.

The digest is asserted by `tests/target_path_namespace.rs`, so drift without a
revision bump is a **build failure** rather than a silent behaviour change.

## Changing a profile

Do not edit a digest to match new behaviour. A published revision is immutable:
add the next `<id>.rev<N>.sha256` and bump `DeviceProfile::revision`. Correcting
a mistake means publishing the next revision, not rewriting the previous one.
