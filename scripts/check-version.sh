#!/usr/bin/env bash
# Fails if Cargo.toml, package.json and tauri.conf.json disagree on the app version.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo=$(sed -n '/^\[workspace.package\]/,/^\[/{s/^version = "\(.*\)"/\1/p;}' Cargo.toml)
npm=$(sed -n 's/^  "version": "\(.*\)",/\1/p' package.json)
tauri=$(sed -n 's/^  "version": "\(.*\)",/\1/p' src-tauri/tauri.conf.json)
echo "Cargo.toml=$cargo package.json=$npm tauri.conf.json=$tauri"
[ -n "$cargo" ] && [ "$cargo" = "$npm" ] && [ "$cargo" = "$tauri" ]
