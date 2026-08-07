-- Library entities the sync core selects from (issue #33, first criterion).
--
-- A Platform-scoped Game is one Library row; a Release owns the
-- representation-specific ROM Sets; a ROM Pack selects complete exact sets.
-- The selection records the ROM Set by *content identity*, so a pack means the
-- same thing after a restart even if the Library is rebuilt around it.

CREATE TABLE game (
    game_id         TEXT PRIMARY KEY,
    platform        TEXT NOT NULL,
    title           TEXT NOT NULL
) STRICT;

CREATE TABLE release (
    release_id      TEXT PRIMARY KEY,
    game_id         TEXT NOT NULL REFERENCES game(game_id),
    label           TEXT NOT NULL
) STRICT;

CREATE TABLE rom_set (
    rom_set_id      TEXT PRIMARY KEY,
    release_id      TEXT NOT NULL REFERENCES release(release_id),
    -- Strong content identity over the whole set. This, not the row id, is
    -- what makes a selection mean the same thing later.
    content_digest  TEXT NOT NULL,
    file_name       TEXT NOT NULL,
    size            INTEGER NOT NULL
) STRICT;

CREATE INDEX rom_set_by_content ON rom_set(content_digest);

-- An exact selection: a ROM Pack revision naming exactly these ROM Sets.
-- Never silently changed — a different selection is a new pack revision.
CREATE TABLE rom_pack_selection (
    rom_pack_id     TEXT NOT NULL,
    revision        INTEGER NOT NULL,
    rom_set_id      TEXT NOT NULL REFERENCES rom_set(rom_set_id),
    content_digest  TEXT NOT NULL,
    PRIMARY KEY (rom_pack_id, revision, rom_set_id),
    FOREIGN KEY (rom_pack_id, revision) REFERENCES rom_pack(rom_pack_id, revision)
) STRICT;
