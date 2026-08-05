# Project Library metadata into shared frontend documents

ROM Manager will manage export-eligible metadata as field-level Metadata Projections rather than owning complete frontend documents. The first-release ES-DE adapter reconciles mapped descriptive fields through a mirrored export ledger and three-way merge while preserving frontend-owned and unknown state; this complexity is accepted because whole-file generation would destroy favorites, play history, emulator choices, and user edits, while one-time export would leave device metadata stale.

## Consequences

Every metadata-capable Sync Plan applies to exactly one Media Target and must expose adoption, conflict, omission, retirement, recovery, and non-atomic publication behavior. Shared documents require stopped-frontend confirmation, current-state validation, verified staging, a target-local recovery copy, and reread verification; ROM Manager never silently imports or overwrites edits to managed fields. Separate ROM-content and frontend-metadata Destination Roles may use different explicitly paired Media Targets, but retain independent plans and cannot claim cross-target atomicity.
