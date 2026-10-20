#!/usr/bin/env bash
# Downloads the sample thermograms the golden tests use into testdata/ and checks their hashes.
# They are only downloaded for testing, never shipped:
# - Images: MIT-licensed, from https://github.com/SanNianYiSi/thermal_parser (pinned commit).
# - Videos: from the Thermimage R package, https://github.com/gtatters/Thermimage (pinned commit).
set -euo pipefail
cd "$(dirname "$0")/.."
IMAGES="https://raw.githubusercontent.com/SanNianYiSi/thermal_parser/b513647ef2318ba99dd2e2a543fa0fb071fd9579/images"
VIDEOS="https://raw.githubusercontent.com/gtatters/Thermimage/cb244dac8fffaac50b1ba8bbbf58b7d37b5d96e1/inst/extdata"
mkdir -p testdata
while read -r base sha name; do
  file="testdata/$name"
  if [ ! -f "$file" ]; then
    case "$base" in
      images) url="$IMAGES/$name" ;;
      videos) url="$VIDEOS/$name" ;;
    esac
    curl -fsSL --retry 3 -o "$file" "$url"
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
images 7fc6f5a7000b7d52d48d716dea6b503d75608253f65e101fcbc767f27f9611cb FLIR_E40.jpg
images 6cb40ab3967ca890fc90b2f4a53e6c4974d1865b4274f15fc8710da684591409 FLIR_AX8.jpg
images f1421dfca3181503871e6d233f2b4f1be3c774de1721d6f74f22bdaff6dd14e7 FLIR_B60.jpg
images 0bd4c13e95317ac6c5904d61bd8ba1bd1d07e19caef127669a3e04d089618d64 FLIR_T640.jpg
images 7cbe5d9a04fb4daea8d750cbf8ea050f3c3a2fd5b530e756dee4d6b8f6615b58 FLIR.jpg
images efdbc4e51a87a5f38943055fcd04dfb2bdd97ef549b1cdef034c0d7f5b1e6eaa DJI_XT2.jpg
images c2ae58509119695cea72c27a344569e6e53196e968e5e091671e8f7d1813a74f DJI_XTR.jpg
videos abbcc3070b158d630fd737c2be2a7129dba89540284173b569f7ab1876a05de9 SampleSEQ.seq
videos 7c8505a40790615bb67bc264c45366bb9e8b3e94bd74b217e4b06c6ea8146151 SampleCSQ.csq
LIST
