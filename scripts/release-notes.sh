#!/usr/bin/env bash
# Prints the CHANGELOG.md section of <version>, which the release workflow
# publishes as the release's notes.
#
#   scripts/release-notes.sh <version>
set -euo pipefail

version="$1"
awk -v heading="## $version" '
  index($0, heading) == 1 { found = 1; next }
  found && /^## / { exit }
  found { print }
' CHANGELOG.md | sed -e '/./,$!d'
