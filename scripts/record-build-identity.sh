#!/usr/bin/env bash
#
# Records exactly which build produced a set of artefacts.
#
#     scripts/record-build-identity.sh <output.md> <artefact> [artefact...]
#
# [#35](https://github.com/Mill1893/rom-manager/issues/35) asks CI to retain
# installer hashes and exact build identity "for the milestone report". Until
# now the report carried a SHA-256 somebody pasted in by hand, against a commit
# that is many merges old. A hash transcribed once is not evidence of the build
# anyone is holding; it is evidence of a build that existed on the afternoon it
# was copied, and there is no way to tell from the report which.
#
# So this writes the same facts from inside the job that produced the bytes.
# It is deliberately dull: identity of the commit, identity of the toolchain,
# and a digest per artefact. Nothing derived, nothing summarised.
#
# Runs under bash on both hosts — Windows runners have it, and one script that
# both jobs share cannot drift the way two would.

set -euo pipefail

OUTPUT="${1:?usage: record-build-identity.sh <output.md> <artefact>...}"
shift
[[ $# -gt 0 ]] || { echo "no artefacts given" >&2; exit 1; }

mkdir -p "$(dirname "$OUTPUT")"

{
  echo "# Build identity"
  echo
  echo "Written by \`scripts/record-build-identity.sh\` inside the job that"
  echo "produced these files. Everything here is read from the build, not"
  echo "transcribed into it."
  echo
  echo "| Item | Value |"
  echo "| --- | --- |"
  echo "| Commit | \`${GITHUB_SHA:-unknown}\` |"
  echo "| Ref | \`${GITHUB_REF:-unknown}\` |"
  echo "| Workflow run | \`${GITHUB_RUN_ID:-unknown}\` attempt \`${GITHUB_RUN_ATTEMPT:-unknown}\` |"
  echo "| Runner | \`${RUNNER_OS:-unknown}\` / \`${ImageOS:-unknown}\` |"
  echo "| Built at (UTC) | \`$(date -u +%Y-%m-%dT%H:%M:%SZ)\` |"
  echo "| Rust | \`$(rustc --version 2>/dev/null || echo unavailable)\` |"
  echo "| Tauri CLI | \`$(npx --prefix ui tauri --version 2>/dev/null | tr -d '\r' || echo unavailable)\` |"
  echo
  echo "## Artefacts"
  echo
  echo "| File | Bytes | SHA-256 |"
  echo "| --- | --- | --- |"

  for artefact in "$@"; do
    if [[ ! -f "$artefact" ]]; then
      # Named but absent is a fact worth recording, not a reason to stop: the
      # report should show a gap rather than quietly omit a row.
      echo "| \`$(basename "$artefact")\` | **missing** | — |"
      continue
    fi
    size="$(wc -c < "$artefact" | tr -d ' ')"
    digest="$(sha256sum "$artefact" | cut -d' ' -f1)"
    echo "| \`$(basename "$artefact")\` | $size | \`$digest\` |"
  done

  echo
  echo "## What this does not establish"
  echo
  echo "That these bytes were built from the commit above, and nothing else."
  echo "It is not a signature: the artefacts are unsigned by design for the"
  echo "first release, so this records provenance for a reader who already"
  echo "trusts the CI logs, and proves nothing to one who does not."
} > "$OUTPUT"

cat "$OUTPUT"
