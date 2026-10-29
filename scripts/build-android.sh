#!/bin/bash
# Local ARM64 preview only; no publication or production signing identity.
set -euo pipefail
cd "$(dirname "$0")/.."
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
export NDK_HOME="${NDK_HOME:-$ANDROID_HOME/ndk/android-ndk-r27c}"
export JAVA_HOME="${JAVA_HOME:-$(/usr/libexec/java_home -v 17)}"
pnpm tauri android build --target aarch64 --apk --ci
sdk_tools="$ANDROID_HOME/build-tools/35.0.0"
source_apk="src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release-unsigned.apk"
test -f "$source_apk"
mkdir -p .build-cache installers
preview_key=.build-cache/android-preview.keystore
if [[ ! -f "$preview_key" ]]; then
  "$JAVA_HOME/bin/keytool" -genkeypair -keystore "$preview_key" -storepass android -keypass android -alias androiddebugkey -dname 'CN=BlueDoor local preview' -keyalg RSA -keysize 2048 -validity 10000
fi
"$sdk_tools/zipalign" -f -P 16 4 "$source_apk" .build-cache/android-aligned.apk
"$sdk_tools/apksigner" sign --ks "$preview_key" --ks-key-alias androiddebugkey --ks-pass pass:android --key-pass pass:android --out installers/BlueDoor-android-arm64-preview.apk .build-cache/android-aligned.apk
"$sdk_tools/apksigner" verify --verbose installers/BlueDoor-android-arm64-preview.apk
"$sdk_tools/zipalign" -c -P 16 4 installers/BlueDoor-android-arm64-preview.apk
shasum -a 256 installers/BlueDoor-android-arm64-preview.apk > installers/BlueDoor-android-arm64-preview.apk.sha256
