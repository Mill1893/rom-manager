# Descriptor fixtures

**Structurally valid, containing no game content.**

A CUE sheet or GDI is plain text naming other files; an M3U is a list of paths.
None of them contains ROM data, so these can be checked in honestly — and the
`.bin` files they name are project-generated zero-filled placeholders, not
dumps.

That distinction is what makes this testable at all: proving the *parser* is
correct needs the descriptor's structure, never the game's bytes.

| Fixture | What it exercises |
| --- | --- |
| `single-track.cue` | The ordinary one-file case |
| `multi-track.cue` | Several tracks in order |
| `unquoted.cue` | A `FILE` name without quotes, which real sheets contain |
| `escaping.cue` | A reference climbing out of the import root — must be refused |
| `absolute.cue` | An absolute path — must be refused |
| `no-files.cue` | Valid syntax naming nothing — incomplete, never "complete and empty" |
| `unterminated.cue` | A `FILE` line with one quote |
| `two-disc.m3u` | A playlist with comments and blank lines |
| `escaping.m3u` | A playlist entry climbing out |
| `dreamcast.gdi` | A GDI with its leading track count |
