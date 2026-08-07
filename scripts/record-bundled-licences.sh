#!/usr/bin/env bash
#
# Records what an AppImage actually bundles, and under which licences.
#
# The point is that it reads the *built artefact* rather than the dependency
# declarations. Those two disagree in practice: the hand-written manifest this
# replaces said webkit2gtk was "system-provided where present, bundled only as a
# fallback", and the AppImage bundles it unconditionally along with 150-odd
# other libraries. A licence manifest that describes intentions rather than
# contents is not evidence of anything.
#
# LGPL components are called out separately because they carry obligations the
# others do not: the user must be able to replace the library, which for a
# bundled build means shipping it dynamically linked and saying so.
#
#     scripts/record-bundled-licences.sh <path-to.AppImage> [output.md]

set -euo pipefail

APPIMAGE="${1:?usage: record-bundled-licences.sh <AppImage> [output.md]}"
OUTPUT="${2:-packaging/linux/bundled-licences.md}"

if [[ ! -f "$APPIMAGE" ]]; then
  echo "no AppImage at $APPIMAGE" >&2
  exit 1
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

APPIMAGE_ABS="$(readlink -f "$APPIMAGE")"
(cd "$WORK" && "$APPIMAGE_ABS" --appimage-extract >/dev/null 2>&1) || {
  echo "could not extract $APPIMAGE" >&2
  exit 1
}

LIBDIR="$WORK/squashfs-root/usr/lib"
[[ -d "$LIBDIR" ]] || { echo "the AppImage bundles no usr/lib" >&2; exit 1; }

# `dpkg -S` answers for the system copy; the bundled file is the same library
# under a different path, so it is matched by name.
package_for() {
  local name="$1" found=""
  for candidate in "/usr/lib/x86_64-linux-gnu/$name" "/usr/lib/$name"; do
    found="$(dpkg -S "$candidate" 2>/dev/null | head -1 | cut -d: -f1)" || true
    [[ -n "$found" ]] && { printf '%s' "$found"; return; }
  done
  printf 'unknown'
}

# Every distinct licence a package declares, not just the first.
#
# Taking the first line understates multi-licensed components, and it does so
# exactly where it matters most: webkit2gtk's copyright file opens with
# BSD-2-clause and goes on to declare LGPL twenty lines later. Reporting only
# the opening line would have described the largest bundled component as
# carrying no replacement obligation at all.
licence_for() {
  local package="$1" copyright="/usr/share/doc/$1/copyright"
  [[ -r "$copyright" ]] || { printf 'see %s' "$copyright"; return; }
  local all count
  all="$(grep -oP '^License:\s*\K.*' "$copyright" 2>/dev/null | tr -d '\r' | sort -u)" || true
  [[ -n "$all" ]] || { printf 'unstated in copyright file'; return; }
  count="$(wc -l <<<"$all")"

  # A package like webkit declares dozens. Listing them all makes the table
  # unreadable and hides the one fact a reader needs, so the summary leads with
  # the copyleft terms and says how many others there are.
  if (( count > 3 )); then
    local copyleft
    copyleft="$(grep -iE 'LGPL|MPL|GPL' <<<"$all" | head -2 | paste -sd '; ' -)"
    if [[ -n "$copyleft" ]]; then
      printf '%s (+%d more; see copyright)' "$copyleft" "$((count - 1))"
    else
      printf '%s (+%d more; see copyright)' "$(head -1 <<<"$all")" "$((count - 1))"
    fi
  else
    paste -sd '; ' - <<<"$all"
  fi
}

total_bytes="$(du -sb "$LIBDIR" | cut -f1)"
count="$(find "$LIBDIR" -maxdepth 1 -name '*.so*' | wc -l)"

{
  echo "# Bundled dependency licences"
  echo
  echo "**Generated from a built AppImage, not from dependency declarations.**"
  echo "Regenerate with \`scripts/record-bundled-licences.sh <AppImage>\`."
  echo
  echo "| Field | Value |"
  echo "| --- | --- |"
  echo "| Artefact | \`$(basename "$APPIMAGE")\` |"
  echo "| SHA-256 | \`$(sha256sum "$APPIMAGE" | cut -d' ' -f1)\` |"
  echo "| Bundled shared libraries | $count |"
  echo "| Bundled library payload | $(numfmt --to=iec "$total_bytes") |"
  echo "| Recorded | $(date -u +%Y-%m-%d) |"
  echo

  lgpl_rows="$WORK/lgpl.txt"
  all_rows="$WORK/all.txt"
  : > "$lgpl_rows"; : > "$all_rows"

  while IFS= read -r library; do
    name="$(basename "$library")"
    package="$(package_for "$name")"
    licence="$(licence_for "$package")"
    printf '| `%s` | %s | %s |\n' "$name" "$package" "$licence" >> "$all_rows"
    if grep -qi 'LGPL' <<<"$licence"; then
      printf '| `%s` | %s | %s |\n' "$name" "$package" "$licence" >> "$lgpl_rows"
    fi
  done < <(find "$LIBDIR" -maxdepth 1 -name '*.so*' | sort)

  echo "## Components declaring copyleft terms"
  echo
  echo "Each is bundled **dynamically linked**, so a user may replace it by"
  echo "substituting the file inside the extracted AppImage. Source for the"
  echo "exact versions is available from the distribution that packaged them,"
  echo "identified in the package column."
  echo
  echo "> **This table is generated, not reviewed.** It reports what each"
  echo "> package's copyright file declares. Several entries list GPL terms"
  echo "> alongside LGPL ones -- commonly because the package ships GPL tooling"
  echo "> beside an LGPL library, but *commonly* is not *always*, and which"
  echo "> applies to the bundled \`.so\` is a question this script cannot answer."
  echo "> A human must confirm before release."
  echo
  if [[ -s "$lgpl_rows" ]]; then
    echo "| Library | Package | Licence |"
    echo "| --- | --- | --- |"
    cat "$lgpl_rows"
  else
    echo "_None detected._"
  fi
  echo
  echo "## Everything bundled"
  echo
  echo "| Library | Package | Licence |"
  echo "| --- | --- | --- |"
  cat "$all_rows"
} > "$OUTPUT"

echo "wrote $OUTPUT ($count libraries)"
