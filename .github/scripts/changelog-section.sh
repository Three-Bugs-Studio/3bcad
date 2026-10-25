#!/usr/bin/env sh
# Prints the CHANGELOG.md section for version $1 (without its heading).
# Exits non-zero when the section is missing or empty, so a release cannot ship without notes.
set -eu
notes=$(awk -v v="$1" '
  index($0, "## [" v "]") == 1 { found = 1; next }
  found && /^## \[/ { exit }
  found && /^\[[^]]+\]: / { exit }
  found { print }
' "${2:-CHANGELOG.md}")
if [ -z "$(printf '%s' "$notes" | tr -d '[:space:]')" ]; then
  echo "CHANGELOG.md has no notes for version $1" >&2
  exit 1
fi
printf '%s\n' "$notes"
