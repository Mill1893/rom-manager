# Windows path and handle probe results

Probes for [issue #52](https://github.com/Mill1893/rom-manager/issues/52), run 2026-08-06. This note turns the documented facts gathered in [Establish Windows target-path and reparse facts](https://github.com/Mill1893/rom-manager/issues/41) into observed evidence. It records what one host actually did; it does not select the target-path policy, implement the production adapter, or establish the removable-filesystem matrix.

## Probe environment

| Property | Value |
| --- | --- |
| OS | Microsoft Windows NT 10.0.26200.0 (Windows 11 client) |
| Volume | `C:`, NTFS, fixed internal disk |
| Elevation | **Non-admin** |
| Reached from | WSL2 via `powershell.exe` interop |
| 8.3 policy | `NtfsDisable8dot3NameCreation = 2` (per-volume); short names observed present |
| Probe scripts | `docs/research/windows-path-probes/` |

**This is one client host, not the CI host.** CI runs `windows-latest` (Windows Server). Every result below that is version-sensitive is flagged; the reserved-name result in particular must not be generalized.

## Executive answer

- The documented picture from #41 is **confirmed** on the observed points, with one exception and several sharpenings.
- Case folding is **narrower than full Unicode case mapping**. NTFS folded only simple 1:1 mappings. It did *not* fold Kelvin sign, Angstrom sign, sharp-s, final sigma, Turkish dotted/dotless i, the `ﬀ` ligature, or any non-BMP character. An application rule built on Rust `to_lowercase` therefore *over*-folds relative to NTFS, which is the fail-closed direction, but it is not the same relation and must never be described as one.
- The kernel's relative-open path is **stronger than assumed**: `NtCreateFile` with `OBJECT_ATTRIBUTES.RootDirectory` rejects `..`, absolute names, and rooted names outright. Navigation segments cannot be smuggled through a relative name at all.
- `OBJ_DONT_REPARSE` behaved exactly as documented, returning `STATUS_REPARSE_POINT_ENCOUNTERED` for a reparse point both **mid-path** and **at the leaf**, while the same open without it silently escaped the root.
- **Contradiction:** on this build, `CON`, `PRN`, `AUX`, `COM1`, `LPT1` and their extension forms created **real files**, not device handles. Only `NUL` still resolved to a character device. The reserved-name rule is therefore not a stable Windows invariant, which strengthens rather than weakens the case for rejecting those names unconditionally.
- Rust can express the required primitive, but not from `windows-sys` alone: version 0.61 ships the types and constants and **no `Nt*File` entry points**. A manual `extern "system"` declaration against `ntdll` is required, and it compiles cleanly for the Windows target.

## P1 — Case-insensitive lookup versus comparison rules

Method: create `<name>.nes` in a fresh directory, then (a) try to open the case-variant spelling, and (b) try to `CREATE_NEW` the variant to see whether both can coexist.

| Pair | Folded by NTFS? | Both coexist? |
| --- | --- | --- |
| `tracers.nes` / `TRACERS.NES` (ASCII) | yes | no |
| `é` (U+00E9) / `É` (U+00C9) | yes | no |
| `σ` (U+03C3) / `Σ` (U+03A3) | yes | no |
| `ａ` (U+FF41) / `Ａ` (U+FF21) fullwidth | yes | no |
| `ı` (U+0131) dotless i / `I` | **no** | yes |
| `İ` (U+0130) dotted I / `i` | **no** | yes |
| `K` (U+212A) Kelvin sign / `K` | **no** | yes |
| `Å` (U+212B) Angstrom sign / `Å` (U+00C5) | **no** | yes |
| `ß` (U+00DF) / `SS` | **no** | yes |
| `ς` (U+03C2) final sigma / `σ` (U+03C3) | **no** | yes |
| `ﬀ` (U+FB00) ligature / `ff` | **no** | yes |
| `а` (U+0430) Cyrillic / `a` Latin | **no** | yes |
| `𐐨` (U+10428) / `𐐀` (U+10400) Deseret | **no** | yes |

**Observed limit.** NTFS applied a simple, code-point-wise uppercase table restricted to the BMP. Multi-character expansions (`ß`→`SS`), locale-sensitive mappings (Turkish i), compatibility singletons (Kelvin, Angstrom), contextual forms (final sigma), and supplementary-plane characters were all left unfolded.

**Consequence for the collision key.** Rust's `str::to_lowercase` folds Kelvin sign, Deseret, and final sigma, so it maps *more* names together than NTFS does. Under-folding — the dangerous direction, where the application believes two names are distinct but the filesystem resolves them to one object — was not observed for any tested pair. The conservative superset is therefore usable as a *planning* collision key, but the atomic `FILE_CREATE` disposition remains the only thing that proves a spelling did not select an existing object.

## P2 — Per-directory case sensitivity

`fsutil file setCaseSensitiveInfo <dir> enable` **succeeded without elevation**. Afterwards `rom.nes` and `ROM.nes` both created successfully and coexisted in that directory.

**Observed limit.** Any unprivileged process — including the user, another tool, or WSL — can flip a directory on a Media Target into case-sensitive mode. Case-insensitivity is a per-directory property discovered at runtime, never an assumption. A managed root can contain two entries that the application's collision key maps together, and that state is reachable without malice or elevation.

## P3 — Unicode normalization

Created NFC `é` (U+00E9), then probed the NFD spelling (`e` + U+0301).

- Opening the NFD spelling after creating NFC: **failed**.
- Creating the NFD spelling alongside: **succeeded**.
- Directory enumeration returned both, stored byte-exact: `U+0065,U+0301` and `U+00E9`.

**Observed limit.** NTFS performs no normalization and preserves the exact UTF-16 sequence. Canonically equivalent spellings are distinct objects. Normalization is entirely an application concern, and a normalization collision is an application ambiguity rather than a filesystem one — exactly as #41 concluded.

## P4 — Trailing dots and spaces

| Name | Win32 path create | `\\?\` verbatim create | Win32 path resolved to |
| --- | --- | --- | --- |
| `rom.nes.` | succeeded | succeeded | `rom.nes` |
| `rom.nes ` | failed, `ERROR_FILE_EXISTS` | succeeded | `rom.nes` |
| `rom.nes. ` | failed, `ERROR_FILE_EXISTS` | succeeded | `rom.nes` |

After the probe the directory held four distinct entries: `rom.nes`, `rom.nes `, `rom.nes.`, `rom.nes. `.

**Observed limit.** This is a live aliasing hazard, and it is worse than "such names are awkward". A Win32-path write to `rom.nes.` **silently retargets `rom.nes`** and overwrites a different managed artifact than the one named. The trailing variants that verbatim paths create are then permanently unaddressable through ordinary Win32 paths — an inventory built on Win32 paths cannot even see them as distinct, while enumeration reports them as separate entries. Rejecting trailing dots and spaces is necessary in both directions: refuse to plan them, and treat their presence on a target as unknown content rather than a match.

## P5 — Reserved device names

Two probes, the second verifying with `GetFileType`, directory enumeration, and `GetFinalPathNameByHandle`.

| Name | `GetFileType` | Real directory entry? | Final path |
| --- | --- | --- | --- |
| `CON` | 1 (`FILE_TYPE_DISK`) | **yes** | `…\r_CON\CON` |
| `PRN` | 1 (`FILE_TYPE_DISK`) | **yes** | `…\r_PRN\PRN` |
| `COM1` | 1 (`FILE_TYPE_DISK`) | **yes** | `…\r_COM1\COM1` |
| `CON.nes` | 1 (`FILE_TYPE_DISK`) | **yes** | `…\r_CON_nes\CON.nes` |
| `LPT1.nes` | 1 (`FILE_TYPE_DISK`) | **yes** | `…\r_LPT1_nes\LPT1.nes` |
| `NUL` | 2 (`FILE_TYPE_CHAR`) | **no** | *(none)* |

`AUX`, `CONOUT$`, and `clock$` also became real directory entries in the first probe.

**This contradicts the documented rule** that these basenames are reserved regardless of extension. On this build only `NUL` retained device semantics when it appeared as a component beneath a directory path.

**Observed limit, and why the policy does not change.** Reserved-name handling is evidently *build-dependent*. Windows Server hosts used by CI, and older clients, are expected to still redirect these names to devices — where the same plan would write to a console or printer instead of a file, and where a later inventory would find nothing. The application cannot detect which behavior a host has without probing, and probing per host is not acceptable. The correct policy is therefore to **reject the documented reserved set unconditionally at planning time**, on portability and unpredictability grounds rather than on a claim about what any one Windows version does. This result should not be cited as evidence that these names are safe.

*(Unresolved detail: after a 4-byte write to the `CON` file handle, a subsequent size query reported 0. The probe did not chase this down; it does not affect the policy conclusion.)*

## P6 — Short-name (8.3) aliases

- `GetShortPathNameW("…\Super Long Tracer Name.nes")` returned `…\P6_SHO~1\SUPERL~1.NES` — note the **directory component was aliased too**.
- `dir /x` reported `SUPERL~1.NES` and `SUPERL~2.NES` for the two long names.
- `fsutil 8dot3name query C:` **requires elevation** and failed with access denied; the per-volume registry policy read `2`.

**Observed limit.** Aliases were present and enumerable on this volume, and they alias *every* path component, not just the leaf. Alias existence could not be established as a volume-wide guarantee without elevation, which matches the documented position that aliases are optional and must be read from the filesystem rather than predicted. An effective-namespace inventory must therefore source actual short names from directory enumeration; synthesizing a `~1` pattern is not sound, and neither is assuming absence.

## P7 — Root-handle-relative, no-reparse opens

Setup: root `R` containing `managed/ok.nes`, `outside.txt`, and a junction `managed/evil` → `R`. `R` opened with `FILE_FLAG_BACKUP_SEMANTICS`; every open below is `NtCreateFile` with `OBJECT_ATTRIBUTES.RootDirectory = R`.

| Relative name | Flags | Result |
| --- | --- | --- |
| `managed\ok.nes` | — | opened, final path correct |
| `MANAGED\OK.NES` | — | opened, resolved to same object |
| `managed\..\outside.txt` | — | `0xC0000033` `STATUS_OBJECT_NAME_INVALID` |
| `\managed\ok.nes` | — | `0xC000000D` `STATUS_INVALID_PARAMETER` |
| `C:\Windows\notepad.exe` | — | `0xC0000033` `STATUS_OBJECT_NAME_INVALID` |
| `managed\evil\outside.txt` | — | **opened — escaped to `R\outside.txt`** |
| `managed\evil\outside.txt` | `OBJ_DONT_REPARSE` | `0xC000050B` `STATUS_REPARSE_POINT_ENCOUNTERED` |
| `managed\evil` | `FILE_OPEN_REPARSE_POINT` | opened the junction itself, no traversal |
| `managed\evil` | `OBJ_DONT_REPARSE` | `0xC000050B` `STATUS_REPARSE_POINT_ENCOUNTERED` |
| `managed\ok.nes.` | — | `0xC0000034` `STATUS_OBJECT_NAME_NOT_FOUND` |

**Observed limits, in order of importance.**

1. **Navigation segments are rejected by the kernel.** `..`, a leading separator, and a fully qualified name were all refused when `RootDirectory` was set. Confinement does not depend on the application lexically stripping `..` — though the application should still reject it during validation, since planning must not emit such a name in the first place.
2. **`OBJ_DONT_REPARSE` is the real confinement primitive, and it is needed.** Without it the junction was followed and the open escaped the managed root while still "looking" relative. With it, the same name failed closed, both when the reparse point was an intermediate component and when it was the leaf.
3. **The NT namespace is literal.** `ok.nes.` did not resolve to `ok.nes`, unlike the Win32 path layer in P4. Relative opens see exactly the name given, which is what makes a validated segment meaningful.
4. **Case-insensitive resolution applied** to relative names via `OBJ_CASE_INSENSITIVE`, consistent with P1.

**Gap.** Symbolic-link creation required privileges this non-admin session lacked, so the symlink rows are untested; only junctions were exercised. `OBJ_DONT_REPARSE` is documented as reparse-tag-agnostic and rejected the junction at both positions, so the conclusion is expected to hold for other tags — but that generalization is **inferred, not observed**, and a privileged or Developer-Mode host should re-run it before the guarantee is claimed in a milestone report.

## Rust bindability

`cargo check --target x86_64-pc-windows-gnu` against `windows-sys` 0.61.2:

- `OBJECT_ATTRIBUTES` is present but gated behind the **`Win32_Security`** feature (it references `SECURITY_DESCRIPTOR` and `SECURITY_QUALITY_OF_SERVICE`).
- `UNICODE_STRING`, `IO_STATUS_BLOCK`, `OBJ_CASE_INSENSITIVE`, `OBJ_DONT_REPARSE`, `FILE_OPEN`, `FILE_OPEN_REPARSE_POINT`, and `FILE_SYNCHRONOUS_IO_NONALERT` are all available.
- **`windows-sys` 0.61 exports no `Nt*File` functions at all.** `NtCreateFile` must be declared by the application:

```rust
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtCreateFile(/* … */) -> NTSTATUS;
}
```

With that declaration a root-relative, `OBJ_DONT_REPARSE` open **compiles cleanly** for the Windows target. Cross-compilation needed only `rustup target add x86_64-pc-windows-gnu`; no MSVC toolchain was required to type-check. The probe crate is at `docs/research/windows-path-probes/ntprobe/`.

**Observed limit.** Linking and running the Rust binary was not attempted — no mingw linker is present on this host, and the pinned toolchain (1.97.1) targets MSVC in CI. Compilation proves the binding is expressible and type-correct; it does **not** prove runtime behavior from Rust. The P7 runtime evidence above came from the equivalent C# P/Invoke, which uses the same structures and flags. Confirming that a Rust-linked binary produces the same status codes is a CI-side check, not something this host can supply.

## What this does not establish

- Nothing about removable media, SD cards, exFAT/FAT32, network shares, or ReFS. Every result is one fixed NTFS volume.
- Nothing about MTP, which has no filesystem path semantics at all.
- Nothing about hostile same-privilege processes. Junction traversal was exercised as a *benign* hazard; no adversarial timing was attempted.
- Nothing about hard links, which reparse rejection does not address.
- No runtime evidence from a Rust-linked binary, and no evidence from a Windows Server / CI host.

## Recommended follow-through

1. Treat the reserved-name and case-fold results as **inputs to a fail-closed policy**, not as capabilities to exploit.
2. Re-run these probes on the CI host as part of the safety-foundation evidence, so the reserved-name divergence is recorded rather than assumed — this is a candidate obligation for [Define the safety-foundation evidence and CI contract](https://github.com/Mill1893/rom-manager/issues/51).
3. Re-run P7's symlink rows on a Developer-Mode or elevated host before any milestone report claims tag-agnostic no-reparse confinement.
