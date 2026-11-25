#!/bin/sh
# Builds OpenMultiband's macOS bundles into dist/: VST3, Audio Unit, CLAP, LV2 and, when an AAX
# SDK is found, AAX. Universal (arm64 and x86_64), macOS 11.0.
#   scripts/build.sh
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SDK="$(cd "$ROOT/../../Dependencies/OpenPluginSDK" && pwd)"
SDK_REV=c6fa6a03560443191e1045db47c45a1839c65696
DRI="$ROOT/../../Dependencies/GoodLookinDri"
DRI_REV=3fe04263a7e1eb21931717393d0f1d705417e884

find_sdk() {
  if [ -n "$1" ] && [ -d "$1" ]; then echo "$1"; return 0; fi
  shift
  for base in "$ROOT/.." "$ROOT/../.." "$ROOT/../Dependencies" "$ROOT/../../Dependencies" "$ROOT/../SDKs" "$ROOT/../../SDKs" "$ROOT/../Water Channel/third_party" "/Volumes/Developer/Dev Projects" "/Volumes/Developer 2/SDKs" "$HOME/Dependencies" "$HOME/SDKs"; do
    for rel in "$@"; do
      [ -d "$base/$rel" ] && { (cd "$base/$rel" && pwd); return 0; }
    done
  done
  return 1
}
VST3_SDK="$(find_sdk "$VST3_SDK" vst3sdk)" || { echo "VST3 SDK not found; set VST3_SDK" >&2; exit 1; }
AU_SDK="$(find_sdk "$AU_SDK" AudioUnitSDK)" || { echo "AudioUnitSDK not found; set AU_SDK" >&2; exit 1; }
FORMATS=vst3,au,clap,lv2
if AAX_SDK="$(find_sdk "$AAX_SDK" aax-sdk-2-8-1)"; then FORMATS="$FORMATS,aax"; else AAX_SDK=""; echo "no AAX SDK: AAX is not built" >&2; fi

[ "$(git -C "$SDK" rev-parse HEAD 2>/dev/null)" = "$SDK_REV" ] || echo "note: OpenPluginSDK is not at $SDK_REV" >&2
[ "$(git -C "$DRI" rev-parse 'HEAD^{tree}' 2>/dev/null)" = "$(git -C "$DRI" rev-parse "$DRI_REV^{tree}" 2>/dev/null)" ] \
  || echo "note: GoodLookinDri is not at $DRI_REV" >&2

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
VERSION="${VERSION%%-*}"

"$SDK/scripts/build_plugin.sh" --manifest-path "$ROOT/Cargo.toml" --name OpenMultiband --version "$VERSION" \
  --bundle-id com.opengrid.openmultiband --manufacturer OpenGrid --au-subtype OMbd --au-manufacturer Ogrd \
  --ttl-bin lv2_ttl --editor --aax-pages --formats "$FORMATS" --archs universal \
  --vst3-sdk "$VST3_SDK" --au-sdk "$AU_SDK" ${AAX_SDK:+--aax-sdk "$AAX_SDK"} \
  --out "$ROOT/dist" --work-dir "$ROOT/build"
