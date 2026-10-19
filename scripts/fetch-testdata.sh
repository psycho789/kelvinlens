#!/usr/bin/env bash
# Downloads the sample thermograms the golden tests use into testdata/ and checks their hashes.
# Samples: MIT-licensed, from https://github.com/SanNianYiSi/thermal_parser (pinned commit).
set -euo pipefail
cd "$(dirname "$0")/.."
COMMIT=b513647ef2318ba99dd2e2a543fa0fb071fd9579
BASE="https://raw.githubusercontent.com/SanNianYiSi/thermal_parser/$COMMIT/images"
mkdir -p testdata
while read -r sha name; do
  file="testdata/$name"
  if [ ! -f "$file" ]; then
    curl -fsSL --retry 3 -o "$file" "$BASE/$name"
  fi
  if command -v sha256sum >/dev/null; then actual=$(sha256sum "$file" | cut -d' ' -f1)
  else actual=$(shasum -a 256 "$file" | cut -d' ' -f1); fi
  if [ "$actual" != "$sha" ]; then
    echo "checksum mismatch for $name: $actual" >&2
    rm -f "$file"
    exit 1
  fi
  echo "ok $name"
done <<'LIST'
7fc6f5a7000b7d52d48d716dea6b503d75608253f65e101fcbc767f27f9611cb FLIR_E40.jpg
6cb40ab3967ca890fc90b2f4a53e6c4974d1865b4274f15fc8710da684591409 FLIR_AX8.jpg
f1421dfca3181503871e6d233f2b4f1be3c774de1721d6f74f22bdaff6dd14e7 FLIR_B60.jpg
LIST
