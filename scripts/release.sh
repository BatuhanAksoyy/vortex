#!/usr/bin/env bash
# Prepares a release on this machine: sets the version and writes what
# changed since the last release into CHANGELOG.md. Read and edit the
# changelog, then commit, tag and push yourself; pushing the tag makes the
# release workflow build the app and publish the release on GitHub.
#
#   scripts/release.sh <version>
#   e.g. scripts/release.sh 0.2.0
set -euo pipefail

version="${1:?usage: scripts/release.sh <version>}"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "the version must be x.y.z" >&2; exit 1; }

cd "$(git rev-parse --show-toplevel)"
if git rev-parse -q --verify "refs/tags/v$version" >/dev/null; then
  echo "v$version exists" >&2
  exit 1
fi

previous="$(git describe --tags --abbrev=0 --match 'v*' 2>/dev/null || true)"
range="${previous:+$previous..}HEAD"
changes="$(git log --no-merges --reverse --format='- %s' "$range")"
[ -n "$changes" ] || { echo "nothing changed since $previous" >&2; exit 1; }

# The workspace's version, which every crate takes: the first `version`
# line, under [workspace.package].
awk -v version="$version" '
  !done && /^version = "/ { print "version = \"" version "\""; done = 1; next }
  { print }
' Cargo.toml > Cargo.toml.new
mv Cargo.toml.new Cargo.toml
cargo update --workspace --quiet

section="## $version — $(date +%Y-%m-%d)"$'\n\n'"$changes"$'\n'
if [ -f CHANGELOG.md ]; then
  { head -n 2 CHANGELOG.md; printf '%s\n' "$section"; tail -n +3 CHANGELOG.md; } > CHANGELOG.md.new
else
  printf '# Changelog\n\n%s\n' "$section" > CHANGELOG.md.new
fi
mv CHANGELOG.md.new CHANGELOG.md

cat <<DONE
Kiln is now $version and CHANGELOG.md lists what changed. Edit it, then:

  git add Cargo.toml Cargo.lock CHANGELOG.md
  git commit -m "Release $version"
  git tag -a v$version -m "Kiln $version"
  git push origin main v$version
DONE
