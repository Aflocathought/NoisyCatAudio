#!/usr/bin/env bash
# Build a native CLAP bundle. This script never installs into a user's plugin folder.
set -euo pipefail

if [[ "$(uname -s)" != Darwin ]]; then
    echo "This build requires macOS and Xcode Command Line Tools." >&2
    exit 1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
case "$(uname -m)" in
    arm64) target=aarch64-apple-darwin; arch=arm64 ;;
    x86_64) target=x86_64-apple-darwin; arch=x86_64 ;;
    *) echo "Unsupported macOS architecture" >&2; exit 1 ;;
esac

# CI currently validates macOS 15. Do not advertise compatibility with older
# systems merely because a compiler would accept a lower deployment target.
export MACOSX_DEPLOYMENT_TARGET=15.0
build_args=(build -p spectral-resonator-plugin --release --locked --target "$target")
if [[ "${1:-}" == --offline && $# == 1 ]]; then
    build_args+=(--offline)
elif [[ $# != 0 ]]; then
    echo "Usage: bash scripts/bundle-clap-macos.sh [--offline]" >&2
    exit 1
fi
cargo "${build_args[@]}"

version="$(python3 -c 'import pathlib,tomllib; print(tomllib.loads(pathlib.Path("crates/spectral-resonator-plugin/Cargo.toml").read_text())["package"]["version"])')"
output="$repo_root/target/artifacts/$version/macOS-$arch"
mkdir -p "$output"
# Stage in a fresh directory to prevent stale resources from entering a new ZIP.
stage="$(mktemp -d "$output/staging.XXXXXX")"
trap 'rm -rf -- "$stage"' EXIT
bundle="$stage/my_spectral_resonator.clap"
executable=my_spectral_resonator
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp "target/$target/release/libspectral_resonator_plugin.dylib" "$bundle/Contents/MacOS/$executable"
chmod 755 "$bundle/Contents/MacOS/$executable"
cp LICENSE "$bundle/Contents/Resources/COPYING.txt"
cp COPYRIGHT.md "$bundle/Contents/Resources/"

python3 - "$bundle" "$version" "$executable" <<'PY'
from pathlib import Path
import plistlib
import subprocess
import sys

bundle, version, executable = sys.argv[1:]
contents = Path(bundle) / "Contents"
with (contents / "Info.plist").open("wb") as stream:
    plistlib.dump({
        "CFBundleExecutable": executable,
        "CFBundleIdentifier": "org.spectral-resonator.dev",
        "CFBundleName": "Specatral Resonator",
        "CFBundlePackageType": "BNDL",
        "CFBundleShortVersionString": version,
        "CFBundleVersion": version,
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundleSupportedPlatforms": ["MacOSX"],
        "LSMinimumSystemVersion": "15.0",
    }, stream)
commit = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
(contents / "Resources" / "BUILD.txt").write_text(
    f"Specatral Resonator {version}\nSource commit: {commit}\n"
    f"Source: https://github.com/Aflocathought/NoisyCatAudio/tree/{commit}\n"
    "License: GPL-3.0-only; dependencies retain their own licenses.\n"
    "Experimental macOS 15+ build; ad-hoc signed, not notarized.\n"
    "CI validation is not DAW, interactive UI or real-time audio acceptance.\n",
    encoding="utf-8",
)
PY

plutil -lint "$bundle/Contents/Info.plist"
actual_arch="$(lipo -archs "$bundle/Contents/MacOS/$executable")"
[[ "$actual_arch" == "$arch" ]] || { echo "Wrong Mach-O architecture: $actual_arch" >&2; exit 1; }
nm -gU "$bundle/Contents/MacOS/$executable" > "$stage/exports.txt"
grep -q ' _clap_entry$' "$stage/exports.txt"

# Ad-hoc signing makes the test bundle internally consistent on Apple Silicon.
# It does not provide a Developer ID identity or Gatekeeper/notarization approval.
codesign --force --sign - --timestamp=none "$bundle"
codesign --verify --strict --verbose=2 "$bundle"
archive="$output/SpecatralResonator-$version-macOS-$arch-test.zip"
ditto -c -k --sequesterRsrc --keepParent "$bundle" "$archive"
# Keep the checksum relative so it remains usable after downloading the ZIP.
(cd "$output" && shasum -a 256 "$(basename "$archive")" > "$(basename "$archive").sha256")

# The smoke tests extract this very archive, so their evidence covers the
# packaged executable rather than an unrelated target/release library.
if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
    printf 'archive=%s\n' "$archive" >> "$GITHUB_OUTPUT"
fi
printf 'Test bundle: %s\n' "$archive"
