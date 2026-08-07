# Windows path and handle probes

Disposable probes backing [issue #52](https://github.com/Mill1893/rom-manager/issues/52). Results and
interpretation live in [`../windows-path-probes.md`](../windows-path-probes.md).

These are throwaway diagnostics, not production code and not part of the build.

## Running them

From a Windows host (or WSL with interop), non-admin is sufficient for everything except the
symbolic-link rows in probe D and the volume-wide `fsutil 8dot3name query` in probe B:

```
powershell -NoProfile -ExecutionPolicy Bypass -File probe-a-case-and-normalization.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File probe-b-namespace.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File probe-c-reserved-names.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File probe-d-root-relative-opens.ps1
```

Each writes under `%TEMP%\rm-probe*` and leaves the directories in place for inspection. Probe C
deliberately creates files with reserved device basenames; on hosts where those names still resolve
to devices the entries will be absent, which is itself the result being measured.

## `ntprobe`

Type-checks the root-handle-relative, `OBJ_DONT_REPARSE` open against `windows-sys`. Compilation is
the assertion — it is not linked or run:

```
rustup target add x86_64-pc-windows-gnu
cargo check --manifest-path ntprobe/Cargo.toml --target x86_64-pc-windows-gnu
```
