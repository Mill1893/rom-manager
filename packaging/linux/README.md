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

```sh
scripts/fetch-bundler-tools.sh
export LDAI_RUNTIME_FILE="${XDG_CACHE_HOME:-$HOME/.cache}/rom-manager-bundler/runtime-x86_64"
npm --prefix ui run build
npx --prefix ui tauri build --bundles appimage --verbose
```

## The bundler's own downloads

Building the AppImage is not a hermetic operation, and this took two CI
failures to pin down. `tauri build --bundles appimage` fetches six files from
three hosts in the middle of the build:

| file | fetched by | pinned upstream? |
| --- | --- | --- |
| `linuxdeploy-x86_64.AppImage` | Tauri | tag `linuxdeploy` |
| `AppRun-x86_64` | Tauri | tag `apprun-old` |
| `linuxdeploy-plugin-gtk.sh` | Tauri | no — `master` |
| `linuxdeploy-plugin-gstreamer.sh` | Tauri | no — `master` |
| `linuxdeploy-plugin-appimage.AppImage` | Tauri | no — `continuous` |
| `runtime-x86_64` | **linuxdeploy's plugin** | no — `continuous` |

Tauri caches the first five. It does not know about the sixth, which its plugin
fetches on its own, so that one was downloaded on **every** build — including
the run where packaging hung for two minutes and failed.

`scripts/fetch-bundler-tools.sh` fetches all six ahead of the build, with
retries, verifying each against `packaging/bundler-tools.lock`, and hands the
runtime to the plugin through `LDAI_RUNTIME_FILE`. A build that runs it first
downloads nothing. In CI the store is also restored from `actions/cache`, so
the usual run does not fetch them either.

Two things worth knowing before changing this:

- **The store is deliberately not Tauri's cache directory.** Tauri zeroes the
  three AppImage magic bytes at offset 8 of `linuxdeploy-x86_64.AppImage` so it
  can execute it without FUSE. Verifying digests in place would therefore see
  its own cache as corrupt and re-download on every build.
- **A digest mismatch is fatal, on purpose.** Three of those URLs track a
  moving branch or tag, so the bytes can change with no version number
  changing. Adopt a change deliberately with
  `scripts/fetch-bundler-tools.sh --repin` and review the diff.

None of this is a claim about the application, which contains no
network-capable crate at all. It is about the build that produces it.
