-- Durable sync-core state (issue #33).
--
-- Every table here is written by short serialized transactions. No external
-- I/O — no transport call, no filesystem access — ever happens inside one.

CREATE TABLE media_target (
    target_id       TEXT PRIMARY KEY,
    marker_schema   INTEGER NOT NULL
) STRICT;

-- A Transport Binding is a *live* observation, not an identity. The target it
-- points at is identified by its marker, so a locator change never changes
-- which target this is.
CREATE TABLE transport_binding (
    target_id       TEXT NOT NULL REFERENCES media_target(target_id),
    locator         TEXT NOT NULL,
    capabilities    TEXT NOT NULL,
    observed_at     INTEGER NOT NULL,
    PRIMARY KEY (target_id, locator)
) STRICT;

CREATE TABLE device_profile (
    profile_id      TEXT NOT NULL,
    revision        INTEGER NOT NULL,
    snapshot_digest TEXT NOT NULL,
    body            TEXT NOT NULL,
    PRIMARY KEY (profile_id, revision)
) STRICT;

CREATE TABLE rom_pack (
    rom_pack_id     TEXT NOT NULL,
    revision        INTEGER NOT NULL,
    body            TEXT NOT NULL,
    PRIMARY KEY (rom_pack_id, revision)
) STRICT;

-- Plans are immutable and addressed by their own digest, so a reload can be
-- revalidated by identity rather than trusted because a caller supplied it.
CREATE TABLE sync_plan (
    digest          TEXT PRIMARY KEY,
    target_id       TEXT NOT NULL REFERENCES media_target(target_id),
    body            TEXT NOT NULL,
    created_at      INTEGER NOT NULL
) STRICT;

-- Single use is enforced here as well as in the type system: the row is
-- deleted when the approval is consumed.
CREATE TABLE plan_approval (
    plan_digest     TEXT PRIMARY KEY REFERENCES sync_plan(digest),
    body            TEXT NOT NULL,
    granted_at      INTEGER NOT NULL
) STRICT;

CREATE TABLE operation (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    plan_digest     TEXT NOT NULL REFERENCES sync_plan(digest),
    target_id       TEXT NOT NULL REFERENCES media_target(target_id),
    state           TEXT NOT NULL CHECK (state IN (
                        'running', 'completed', 'cancelled', 'incomplete', 'indeterminate')),
    reason          TEXT,
    report          TEXT,
    started_at      INTEGER NOT NULL,
    finished_at     INTEGER
) STRICT;

CREATE INDEX operation_running ON operation(state) WHERE state = 'running';

-- The application's mirror of what it believes is on the target. Never trusted
-- over the target's own copy; disagreement blocks destructive action.
CREATE TABLE managed_manifest (
    target_id       TEXT PRIMARY KEY REFERENCES media_target(target_id),
    generation      INTEGER NOT NULL,
    body            TEXT NOT NULL
) STRICT;

CREATE TABLE inventory_snapshot (
    target_id       TEXT PRIMARY KEY REFERENCES media_target(target_id),
    digest          TEXT NOT NULL,
    stale           INTEGER NOT NULL DEFAULT 0,
    observed_at     INTEGER NOT NULL
) STRICT;
