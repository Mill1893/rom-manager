# macOS packaging

A DMG containing the application bundle, built in CI. Unsigned and un-notarized:
code signing and notarization are out of scope for the first release, and issue
#1 lists them as such.

## What that costs the user

Gatekeeper will refuse an unsigned bundle on first launch. The user must
right-click → Open, or clear the quarantine attribute. That is a real friction
and it should be documented in the release notes rather than discovered.

## Where data lives

macOS has its own convention and the application follows it rather than forcing
XDG onto the platform:

| Purpose | Location |
| --- | --- |
| Library and database | `~/Library/Application Support/dev.mill1893.rom-manager` |
| Materialization Cache | `~/Library/Caches/dev.mill1893.rom-manager` |
| Settings | `~/Library/Preferences/dev.mill1893.rom-manager` |

The guarantee is the same as on Linux, expressed in the platform's terms:
Application Support is backed up by Time Machine and survives; **Caches is
explicitly disposable and macOS will purge it under disk pressure without
asking**. Putting the Library in Caches would let the operating system itself
delete the user's content.

## Filesystem confinement

macOS needs no separate implementation. The `openat` with `O_NOFOLLOW`
per-segment walk in `src/confined.rs` is the Unix path, and macOS is Unix — the
same code that is exercised against real symlinks on Linux applies here.

What *is* macOS-specific and unverified: APFS is case-insensitive by default but
can be formatted case-sensitive, exactly the situation the effective-equivalence
key exists to handle conservatively. That has never been exercised on an APFS
volume.

## Not built

No DMG exists. Building one needs a macOS host — Apple's SDK is not
redistributable, so this cannot be cross-compiled from Linux, which is why
there is no `package-macos` CI job on a Linux runner.
