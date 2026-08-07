#!/usr/bin/env bash
#
# Places the pinned AppImage bundler tools where the bundler will find them.
#
# Without this, `tauri build --bundles appimage` performs six downloads from
# three hosts partway through the build, and one of them — the AppImage type-2
# runtime, fetched by linuxdeploy's plugin rather than by Tauri — is not cached
# anywhere, so it happens on *every* build. Tauri also discards the tool's
# output unless `--verbose` is passed, so when packaging broke in CI the only
# diagnostic was `failed to run linuxdeploy` after a two-minute stall.
#
# Running this first makes the bundler's own downloads unnecessary: it finds
# every file already present and fetches nothing. What is left is this script,
# which retries, and which checks what it got against
# packaging/bundler-tools.lock.
#
#     scripts/fetch-bundler-tools.sh            # populate the store
#     scripts/fetch-bundler-tools.sh --repin    # adopt current upstream bytes
#
# It prints the export line the build needs; see packaging/linux/README.md.
#
# ## Why there are two directories
#
# The verified copies live in a store of our own, and are copied into Tauri's
# cache from there. That indirection is not tidiness. Tauri *modifies* the
# AppImages in its cache: it zeroes the three magic bytes at offset 8 so they
# can be executed without FUSE. A digest taken from Tauri's cache therefore
# stops matching upstream the first time a build runs, and a script that
# verified in place would decide its cache was corrupt and re-download on every
# single build — the exact problem it was written to remove.
#
# So: our store holds pristine upstream bytes and is the thing that gets
# verified and cached. Tauri's cache is scratch, rebuilt from the store, and
# Tauri may do as it likes to it.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOCK="$ROOT/packaging/bundler-tools.lock"

CACHE_ROOT="${XDG_CACHE_HOME:-$HOME/.cache}"
STORE="$CACHE_ROOT/rom-manager-bundler"
TAURI_CACHE="$CACHE_ROOT/tauri"

REPIN=""
[[ "${1:-}" == "--repin" ]] && REPIN=1

[[ -f "$LOCK" ]] || { echo "no lock file at $LOCK" >&2; exit 1; }

mkdir -p "$STORE" "$TAURI_CACHE"

# Ten attempts with a widening gap. The failures worth surviving here are a
# stalled connection and an HTTP 500 from a release host, both of which have
# happened to this project; neither is helped by trying twice in quick
# succession.
fetch() {
  local url="$1" dest="$2" attempt
  for attempt in 1 2 3 4 5 6 7 8 9 10; do
    if curl --proto '=https' --tlsv1.2 --location --fail --silent --show-error \
            --connect-timeout 20 --max-time 300 \
            --output "$dest.partial" "$url"; then
      mv "$dest.partial" "$dest"
      return 0
    fi
    rm -f "$dest.partial"
    echo "  attempt $attempt failed" >&2
    # No wait after the last one; nothing follows it.
    if [[ "$attempt" -lt 10 ]]; then
      sleep $(( attempt * 5 ))
    fi
  done
  return 1
}

declare -a REPINNED=()
STATUS=0

while read -r kind name want url; do
  if [[ -z "${kind:-}" || "$kind" == \#* ]]; then
    continue
  fi

  case "$kind" in
    tauri|runtime) ;;
    *) echo "unknown destination '$kind' for $name" >&2; exit 1 ;;
  esac

  held="$STORE/$name"

  # An already-correct file is left alone: this is what makes a warm store
  # touch the network zero times.
  if [[ -z "$REPIN" && -f "$held" ]]; then
    got="$(sha256sum "$held" | cut -d' ' -f1)"
    if [[ "$got" == "$want" ]]; then
      echo "ok       $name (held)"
      [[ "$kind" == tauri ]] && install -m 0755 "$held" "$TAURI_CACHE/$name"
      continue
    fi
    echo "stale    $name — refetching" >&2
  fi

  echo "fetch    $name"
  if ! fetch "$url" "$held"; then
    echo "FAILED   $name could not be downloaded from $url" >&2
    STATUS=1
    continue
  fi

  got="$(sha256sum "$held" | cut -d' ' -f1)"

  if [[ -n "$REPIN" ]]; then
    REPINNED+=("$kind $name $got $url")
    if [[ "$got" == "$want" ]]; then echo "         unchanged"; else echo "         $want -> $got"; fi
  elif [[ "$got" != "$want" ]]; then
    # Deliberately fatal. Four of these URLs track a moving branch or tag, so
    # a mismatch means the bytes in the release path changed without notice.
    # That is a thing to read a changelog about, not to build on top of.
    echo "MISMATCH $name" >&2
    echo "         expected $want" >&2
    echo "         actually $got" >&2
    echo "         from     $url" >&2
    echo "         If this change is expected, re-pin it deliberately:" >&2
    echo "           scripts/fetch-bundler-tools.sh --repin" >&2
    rm -f "$held"
    STATUS=1
    continue
  fi

  chmod 0755 "$held"
  [[ "$kind" == tauri ]] && install -m 0755 "$held" "$TAURI_CACHE/$name"
done < "$LOCK"

if [[ -n "$REPIN" ]]; then
  # Only ever rewrite from a complete set. The rewrite is built from what was
  # fetched, so re-pinning through a transient failure would drop that tool
  # from the lock file entirely — a silent deletion, in the file whose whole
  # job is to say what the release path is allowed to use.
  if [[ "$STATUS" -ne 0 ]]; then
    echo >&2
    echo "not re-pinning: ${#REPINNED[@]} of $(grep -cvE '^[[:space:]]*(#|$)' "$LOCK") entries fetched." >&2
    echo "Rewriting now would delete the ones that failed. Fix the fetch and retry." >&2
    exit 1
  fi
  # Rewrite only the entry lines; the commentary above them is the part that
  # explains why this file exists, so it is preserved verbatim. `|| true`
  # because grep reports "no matches" as failure, which is not one here.
  {
    grep -E '^[[:space:]]*(#|$)' "$LOCK" || true
    printf '%s\n' "${REPINNED[@]}"
  } > "$LOCK.new"
  mv "$LOCK.new" "$LOCK"
  echo
  echo "re-pinned $LOCK — review the diff before committing"
  exit 0
fi

if [[ "$STATUS" -ne 0 ]]; then
  echo >&2
  echo "bundler tools are not ready; packaging would fall back to downloading them" >&2
  exit "$STATUS"
fi

# In Actions, set it rather than describe it. The workflow spelling the path
# out itself would be a second copy of a location this script already decides —
# and it would be the wrong copy anywhere XDG_CACHE_HOME is set.
if [[ -n "${GITHUB_ENV:-}" ]]; then
  echo "LDAI_RUNTIME_FILE=$STORE/runtime-x86_64" >> "$GITHUB_ENV"
fi

echo
echo "bundler tools ready. The AppImage plugin needs to be told about the runtime:"
echo "  export LDAI_RUNTIME_FILE=\"$STORE/runtime-x86_64\""
