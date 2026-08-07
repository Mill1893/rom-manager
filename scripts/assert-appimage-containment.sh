#!/usr/bin/env bash
#
# Asserts that an AppImage cannot be written to while it is running.
#
#     scripts/assert-appimage-containment.sh <path-to.AppImage>
#
# This is the property packaging/linux/README.md leans on: the placement rules
# in `src/paths.rs` are enforceable rather than merely intended, because there
# is nowhere inside the bundle for state to accidentally land. The milestone
# report has recorded it since 2026-08-06, but as a one-time observation
# somebody made by hand — nothing would have caught a regression, and the
# regression that matters here is silent. An application that started keeping
# state next to itself would work perfectly on the developer's machine and lose
# the user's Library on every upgrade.
#
# It uses `--appimage-mount`, which mounts the bundle without starting the
# application, so this needs no display, no GPU and no window manager. That
# matters: the assertion runs in the packaging job, and a check that needs a
# desktop session would be a flaky check bolted onto the job whose flakiness
# this project has just finished removing.
#
# Requires FUSE. `--appimage-mount` is the only way to observe the real mount;
# APPIMAGE_EXTRACT_AND_RUN unpacks into a writable temp directory instead,
# which would make this assertion pass by testing nothing.

set -euo pipefail

APPIMAGE="${1:?usage: assert-appimage-containment.sh <AppImage>}"
[[ -f "$APPIMAGE" ]] || { echo "no AppImage at $APPIMAGE" >&2; exit 1; }

MOUNT_LOG="$(mktemp)"
MOUNT_PID=""

cleanup() {
  [[ -n "$MOUNT_PID" ]] && kill "$MOUNT_PID" 2>/dev/null || true
  rm -f "$MOUNT_LOG"
}
trap cleanup EXIT

"$APPIMAGE" --appimage-mount > "$MOUNT_LOG" 2>&1 &
MOUNT_PID=$!

# The runtime prints the mount point once it is ready and then holds it open
# until killed. Poll rather than sleep a fixed amount: on a loaded CI runner
# this can take a moment, and a fixed sleep would be either slow or flaky.
MOUNT_POINT=""
for _ in $(seq 1 50); do
  MOUNT_POINT="$(head -1 "$MOUNT_LOG" 2>/dev/null || true)"
  [[ -n "$MOUNT_POINT" && -d "$MOUNT_POINT" ]] && break
  if ! kill -0 "$MOUNT_PID" 2>/dev/null; then
    echo "--appimage-mount exited before it mounted anything:" >&2
    cat "$MOUNT_LOG" >&2
    exit 1
  fi
  sleep 0.2
done

if [[ -z "$MOUNT_POINT" || ! -d "$MOUNT_POINT" ]]; then
  echo "the AppImage did not report a mount point within 10s:" >&2
  cat "$MOUNT_LOG" >&2
  exit 1
fi

echo "mounted at: $MOUNT_POINT"

FAILED=0

# 1. The kernel's own view. `ro` here is the guarantee; everything else is
#    a consequence of it.
OPTIONS="$(findmnt --raw --noheadings --output OPTIONS -- "$MOUNT_POINT" || true)"
echo "mount options: ${OPTIONS:-<none reported>}"
case ",$OPTIONS," in
  *,ro,*) echo "PASS  the mount is read-only" ;;
  *) echo "FAIL  the mount is not read-only" >&2; FAILED=1 ;;
esac

# 2. What actually happens when something tries. A mount flag that the
#    filesystem did not honour would still fail this.
if WRITE_ERROR="$(touch "$MOUNT_POINT/.containment-probe" 2>&1)"; then
  echo "FAIL  a write into the mount point succeeded" >&2
  rm -f "$MOUNT_POINT/.containment-probe" 2>/dev/null || true
  FAILED=1
else
  echo "PASS  a write into the mount point is refused: ${WRITE_ERROR#touch: }"
fi

if [[ "$FAILED" -ne 0 ]]; then
  echo >&2
  echo "This AppImage can be written to while running. The placement rules in" >&2
  echo "src/paths.rs are no longer enforced by the bundle, and state written" >&2
  echo "beside the application would be lost on upgrade." >&2
  exit 1
fi

echo
echo "the bundle cannot be written to while it runs"
