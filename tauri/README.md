# The ROM Manager desktop application

```sh
npm --prefix ui ci          # once
npm --prefix ui run build   # build the frontend into ui/dist
npx --prefix ui tauri build --bundles appimage   # or: nsis, on Windows
```

Run both commands from the **repository root**.

## Why the frontend is built as its own step

`tauri.conf.json` deliberately has no `beforeBuildCommand`.

Its working directory is not what you would expect. Invoked from the repository
root it resolves relative paths one way; invoked through `npx --prefix ui` it
resolves them relative to `ui/`. A single relative path written there is
therefore correct for one invocation and silently wrong for the next — the
failure is a missing `package.json` several directories away from anything you
edited, which is a poor way to spend an afternoon.

Building the frontend explicitly costs one line in CI and in this file, and the
directory is stated rather than inferred.

## What the WebView can reach

The twelve commands listed in `generate_handler!` in `src/main.rs`, and nothing
else. No Tauri plugin is depended on — not `fs`, `sql`, `shell`, or `http` — so
the capability file in `capabilities/` is withholding permissions from code that
was never compiled in. A permission cannot be re-granted by editing JSON.

Every command takes identifiers or a boolean. No path, no query, no URL: a
replaced frontend cannot express a request that reaches past the boundary,
because the vocabulary does not exist.

The three commands added for nominating folders and devices —
`pick_import_folder`, `pick_media_target`, `scan_import_folders` — do not weaken
that, and are worth being explicit about, because "we added a folder picker" is
exactly the change that usually does. They take **no arguments at all**. The
directory comes back from a native dialog the *user* drove, and goes straight
into the core; the WebView cannot propose a location, cannot pre-fill one, and
cannot ask for a folder it was not shown.

Paths do travel the other way. `MediaTargetChoice.bindingLocator` and the
`path` on each `DeclinedFile` are strings the interface has to render — you
cannot tell someone which card you are about to write to, or which file was
declined, without naming it. That is the outbound direction and it is not the
boundary. The boundary is what a request may *say*, and no command accepts a
path, a query, or a URL.

When that count changes, this line has to change with it — it went stale the
same day the three commands landed.

## Icons

Generated, not committed as artwork:

```sh
python3 tauri/icons/generate.py
```

The palette is duplicated there as Python literals because the script cannot
import TypeScript. `tests/desktop_icons.rs` compares the two copies so they
cannot drift.
