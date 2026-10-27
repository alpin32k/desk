#!/bin/sh
# Notes for the GitHub release of a tag: its section of CHANGELOG.md, then
# how to install. Usage: sh .github/release-notes.sh v0.1.8
set -eu
version="${1#v}"
changes=$(awk -v head="## $version" '
  /^## / { if (found) exit; found = ($0 == head || index($0, head " ") == 1); next }
  found' CHANGELOG.md | sed '/./,$!d')
if [ -n "$changes" ]; then
  printf '%s\n\n' "$changes"
else
  echo "::warning::CHANGELOG.md has no section for $version" >&2
fi
cat <<'NOTES'
macOS (Apple Silicon): `desk_*_aarch64.dmg`. The app is not signed: after copying it to
Applications run `xattr -dr com.apple.quarantine /Applications/desk.app` once.
Linux: `.deb` or `.AppImage`. Requires tmux.
Windows: `desk_*_x64-setup.exe`. Sessions run without tmux and end when desk closes.
NOTES
