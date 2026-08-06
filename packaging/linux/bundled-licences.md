# Bundled dependency licences

Recorded per build. CI regenerates this from the resolved dependency graph; the
entries below are the ones with obligations that affect how the bundle may be
built.

| Component | Licence | Obligation |
| --- | --- | --- |
| `libmtp` | LGPL-2.1-or-later | The user must be able to replace it. Bundled dynamically and relinkable; source and version recorded per build. |
| `libusb` | LGPL-2.1-or-later | As above. |
| `webkit2gtk` | LGPL-2.1-or-later, BSD-2-Clause | System-provided where present; bundled only as a fallback. |
| SQLite (via `libsqlite3-sys`, bundled) | Public domain | None. |
| Rust crates | MIT / Apache-2.0 | Attribution; the generated notice file carries it. |

**Not yet generated from a real build.** This table records what the
dependency choices oblige, not what a produced AppImage contains — no AppImage
has been built, because CI has never run.
