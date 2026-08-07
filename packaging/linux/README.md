# Linux packaging

An x86-64 AppImage, built in CI from an exact commit.

## Why AppImage

It runs with no system-wide install, which matters for two reasons here. The
user can try the application without granting it root, and — more importantly —
**an AppImage cannot write to itself**. Its mount point is read-only and
disappears when the process exits, which makes the placement rules in
`src/paths.rs` enforceable rather than merely intended: there is nowhere inside
the bundle for state to accidentally land.

## What is bundled, and the licences that come with it

Every bundled dependency is recorded in `bundled-licences.md`. The obligation
that actually bites is LGPL: [issue #18](https://github.com/Mill1893/rom-manager/issues/18)
selected direct `libmtp` for experimental Linux MTP, and LGPL requires that a
user be able to replace the library. Bundling it in an AppImage satisfies that
only if the bundle is relinkable, so the licence file records the version and
the source it was built from.

## What removal does

Nothing. Deleting the AppImage removes the application and no user data: the
Library, database, and settings live under the XDG roots and survive. That is
the point of the split — see `src/paths.rs`.

## Building

CI runs the `package-linux` job. Locally, the build needs a Linux host with the
Tauri prerequisites (`webkit2gtk`, `libayatana-appindicator`), which is why it
is a CI job rather than a script anyone is expected to run by hand.
