#!/usr/bin/env bash
# Sets the app version everywhere. Usage: scripts/bump-version.sh 0.2.0
# Commit and push to main; the Release workflow then builds and publishes v0.2.0.
set -euo pipefail
cd "$(dirname "$0")/.."
new=${1:?usage: scripts/bump-version.sh X.Y.Z}
[[ "$new" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "not a version: $new" >&2; exit 1; }
perl -0pi -e "s/(\[workspace\.package\]\nversion = \")[^\"]+/\${1}$new/" Cargo.toml
perl -pi -e "s/^  \"version\": \"[^\"]+\",/  \"version\": \"$new\",/" package.json src-tauri/tauri.conf.json
npm install --package-lock-only --silent
cargo update --workspace --quiet
scripts/check-version.sh
