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

# Reject stale Android overrides instead of distributing a mislabeled APK.
export BLUEDOOR_ANDROID_AAPT="$sdk_tools/aapt"
python3 - <<'PY_VERSION'
import json, os, re, subprocess
version = json.load(open("package.json"))["version"]
assert json.load(open("src-tauri/tauri.conf.json"))["version"] == version
assert json.load(open("src-tauri/tauri.android.conf.json"))["version"] == version
major, minor, patch = map(int, version.split("."))
expected_code = major * 1000000 + minor * 1000 + patch
badging = subprocess.check_output([os.environ["BLUEDOOR_ANDROID_AAPT"], "dump", "badging", "installers/BlueDoor-android-arm64-preview.apk"], text=True)
assert re.search(r"versionName='" + re.escape(version) + r"'", badging), "APK versionName mismatch"
assert f"versionCode='{expected_code}'" in badging, "APK versionCode mismatch"
print(f"APK version verified: {version}/{expected_code}")
PY_VERSION

# Every native dependency must support Android's 16 KiB page size.
export BLUEDOOR_ANDROID_READELF="$NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/llvm-readelf"
python3 - <<'PY_NATIVE'
import os, pathlib, subprocess, tempfile, zipfile
mapping = pathlib.Path("src-tauri/gen/android/app/build/outputs/mapping/universalRelease/mapping.txt").read_text()
for name in ("OnnxJavaType", "OnnxTensor", "TensorInfo", "OrtSession"):
    original = f"ai.onnxruntime.{name}"
    assert f"{original} -> {original}:" in mapping, f"ONNX JNI class renamed: {original}"
print("ONNX JNI classes preserved by R8")
with tempfile.TemporaryDirectory(prefix="bluedoor-elf-") as temporary:
    with zipfile.ZipFile("installers/BlueDoor-android-arm64-preview.apk") as apk:
        native = [name for name in apk.namelist() if name.startswith("lib/") and name.endswith(".so")]
        assert native and all(name.startswith("lib/arm64-v8a/") for name in native), "Unexpected APK ABI"
        for name in native:
            path = pathlib.Path(temporary) / pathlib.Path(name).name
            path.write_bytes(apk.read(name))
            headers = subprocess.check_output([os.environ["BLUEDOOR_ANDROID_READELF"], "-lW", str(path)], text=True)
            loads = [line.split() for line in headers.splitlines() if line.strip().startswith("LOAD")]
            assert loads and all(int(row[-1], 16) >= 16384 for row in loads), f"Unaligned ELF: {name}"
            print(f"16 KiB ELF verified: {name}")
PY_NATIVE
