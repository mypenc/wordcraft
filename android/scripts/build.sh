#!/usr/bin/env bash
# Build the Rust library for Android and package the APK.
#
#   android/scripts/build.sh [--debug] [--abi arm64-v8a|x86_64] [--no-apk]
#
# Needs: Rust 1.95+ with the Android targets (rustup target add aarch64-linux-android
# x86_64-linux-android), cargo-ndk (cargo install cargo-ndk), ANDROID_NDK_HOME (r26+), and for
# the APK a JDK 17 and Gradle (or the wrapper: `cd android && gradle wrapper`).
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$here/.." && pwd)"
profile="--release"; gradle_task="assembleRelease"; abis=(arm64-v8a); apk=1
while [ $# -gt 0 ]; do
  case "$1" in
    --debug) profile=""; gradle_task="assembleDebug" ;;
    --abi) abis=("$2"); shift ;;
    --no-apk) apk=0 ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac; shift
done

"$here/scripts/vendor-winit.sh"
cd "$root"
# Launcher icon: copied from the repo icon set (git-ignored copy, no Gradle task needed).
icon_dir="$here/app/src/main/res/mipmap-xxxhdpi"
mkdir -p "$icon_dir"
cp "$root/assets/app-icon/hicolor/512x512/apps/ai.storyteller.wordcraft.png" "$icon_dir/ic_launcher.png"
targets=(); for a in "${abis[@]}"; do targets+=(-t "$a"); done
# The winit patch applies to this build only: desktop builds keep using crates.io's winit.
cargo ndk --platform 29 "${targets[@]}" -o "$here/app/src/main/jniLibs" build -p wordcraft-android $profile \
  --config "patch.crates-io.winit.path=\"$here/vendor/winit\""

if [ "$apk" = 1 ]; then
  cd "$here"
  if [ -x ./gradlew ]; then ./gradlew "$gradle_task"; else gradle "$gradle_task"; fi
  echo "APK: $(ls app/build/outputs/apk/*/*.apk 2>/dev/null | head -1)"
fi
