-- App-owned Library storage (issue #57).
--
-- The distinction this schema exists to hold: a *source object* is content the
-- application owns and can reproduce from, while an *origin observation* is
-- only a memory of where some bytes were once seen. Losing every observation
-- costs provenance; it never costs content.

CREATE TABLE source_object (
    -- Content identity. Two imports of the same bytes are one object.
    content_digest  TEXT PRIMARY KEY,
    size            INTEGER NOT NULL,
    -- Path within app-owned storage, relative to the Library root.
    stored_path     TEXT NOT NULL UNIQUE,
    imported_at     INTEGER NOT NULL,
    -- 'healthy' until a verification finds bytes that are not what was stored.
    health          TEXT NOT NULL DEFAULT 'healthy'
                        CHECK (health IN ('healthy', 'quarantined')),
    verified_at     INTEGER
) STRICT;

-- Where bytes were seen. Provenance and discovery only: an observation may go
-- missing, move, or change without affecting the managed object it produced.
CREATE TABLE origin_observation (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    content_digest  TEXT NOT NULL REFERENCES source_object(content_digest),
    external_path   TEXT NOT NULL,
    file_name       TEXT NOT NULL,
    observed_at     INTEGER NOT NULL,
    -- Set when a rescan can no longer find these bytes at this path.
    unavailable_at  INTEGER,
    UNIQUE (content_digest, external_path)
) STRICT;

CREATE INDEX origin_by_path ON origin_observation(external_path);
