-- Archives as Source Containers (issue #58).
--
-- An archive is preserved byte-for-byte as an ordinary source object. What this
-- adds is the *reading* of it: which ROMs it contains, at which member paths.
--
-- Extracted ROM bytes are derived materializations, not stored Library content,
-- so nothing here holds member bytes — only the identity needed to reproduce
-- them from the container.

CREATE TABLE source_container (
    -- The archive's own content digest; it is a source_object like any other.
    content_digest  TEXT PRIMARY KEY REFERENCES source_object(content_digest),
    format          TEXT NOT NULL CHECK (format IN ('zip')),
    member_count    INTEGER NOT NULL,
    read_at         INTEGER NOT NULL
) STRICT;

-- One ROM inside a container. The same ROM content may appear in many
-- containers; each remains a distinct member, because differently packaged
-- sources are distinct Source Containers even when their ROMs are identical.
CREATE TABLE container_member (
    container_digest TEXT NOT NULL REFERENCES source_container(content_digest),
    member_path      TEXT NOT NULL,
    content_digest   TEXT NOT NULL,
    size             INTEGER NOT NULL,
    PRIMARY KEY (container_digest, member_path)
) STRICT;

CREATE INDEX container_member_by_content ON container_member(content_digest);
