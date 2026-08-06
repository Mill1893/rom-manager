-- Import Folders (issue #62).
--
-- A remembered place to look, nothing more. Folders are scanned only when the
-- user asks; the application never walks the user's disks on its own schedule.

CREATE TABLE import_folder (
    folder_id       INTEGER PRIMARY KEY AUTOINCREMENT,
    path            TEXT NOT NULL UNIQUE,
    -- An authoritative default Platform for candidates found here, if the user
    -- set one.
    default_platform TEXT,
    last_scanned_at INTEGER
) STRICT;
