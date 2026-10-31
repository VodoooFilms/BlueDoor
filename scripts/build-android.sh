#!/bin/bash
# Release ARM64 build. No keys are generated; release identity comes from env.
set -euo pipefail
cd "$(dirname "$0")/.."
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
export NDK_HOME="${NDK_HOME:-$ANDROID_HOME/ndk/android-ndk-r27c}"
if [[ -z "${JAVA_HOME:-}" && -x /usr/libexec/java_home ]]; then
  export JAVA_HOME="$(/usr/libexec/java_home -v 17)"
fi
mode="${BLUEDOOR_ANDROID_BUILD_MODE:-release}"
case "$mode" in
  release)
    : "${ANDROID_KEYSTORE_PATH:?Set an absolute path to your release keystore}"
    : "${ANDROID_KEYSTORE_PASSWORD:?Set release store password}"
    : "${ANDROID_KEYSTORE_ALIAS:?Set release key alias}"
    : "${ANDROID_KEYSTORE_KEY_PASSWORD:?Set release key password}"
    : "${ANDROID_RELEASE_CERT_SHA256:?Pin the expected release certificate SHA-256}"
    ;;
  unsigned)
    if env | grep -q '^ANDROID_KEYSTORE_'; then
      echo 'Unsigned mode requires ANDROID_KEYSTORE_* to be unset' >&2; exit 1
    fi
    ;;
  *) echo "Unknown build mode: $mode" >&2; exit 1 ;;
esac
pnpm tauri android build --target aarch64 --apk --ci
sdk_tools="$ANDROID_HOME/build-tools/35.0.0"
output_dir=src-tauri/gen/android/app/build/outputs/apk/universal/release
mkdir -p installers
if [[ "$mode" == unsigned ]]; then
  source_apk="$output_dir/app-universal-release-unsigned.apk"
  output_apk=installers/BlueDoor-android-arm64-unsigned.apk
  test -f "$source_apk"
  "$sdk_tools/zipalign" -f -P 16 4 "$source_apk" "$output_apk"
else
  source_apk="$output_dir/app-universal-release.apk"
  output_apk=installers/BlueDoor-android-arm64-release.apk
  test -f "$source_apk"
  certificate=$("$sdk_tools/apksigner" verify --verbose --print-certs "$source_apk")
  if printf '%s' "$certificate" | grep -Eiq 'android debug|local preview|androiddebugkey'; then
    echo 'Refusing debug/preview certificate' >&2; exit 1
  fi
  actual=$(printf '%s\n' "$certificate" | sed -n 's/^Signer #1 certificate SHA-256 digest: //p' | tr '[:upper:]' '[:lower:]' | tr -d ': ')
  expected=$(printf '%s' "$ANDROID_RELEASE_CERT_SHA256" | tr '[:upper:]' '[:lower:]' | tr -d ': ')
  [[ ${#expected} == 64 && "$actual" == "$expected" ]] || { echo 'Release certificate mismatch' >&2; exit 1; }
  cp "$source_apk" "$output_apk"
fi
"$sdk_tools/zipalign" -c -P 16 4 "$output_apk"
shasum -a 256 "$output_apk" > "$output_apk.sha256"
export BLUEDOOR_ANDROID_APK="$output_apk"

# Reject stale Android overrides instead of distributing a mislabeled APK.
export BLUEDOOR_ANDROID_AAPT="$sdk_tools/aapt"
python3 - <<'PY_VERSION'
import json, os, re, subprocess
version = json.load(open("package.json"))["version"]
assert json.load(open("src-tauri/tauri.conf.json"))["version"] == version
assert json.load(open("src-tauri/tauri.android.conf.json"))["version"] == version
major, minor, patch = map(int, version.split("."))
expected_code = major * 1000000 + minor * 1000 + patch
badging = subprocess.check_output([os.environ["BLUEDOOR_ANDROID_AAPT"], "dump", "badging", os.environ["BLUEDOOR_ANDROID_APK"]], text=True)
assert "package: name='com.bluedoor.desktop'" in badging, "APK applicationId mismatch"
assert "application-debuggable" not in badging, "Release APK is debuggable"
assert re.search(r"versionName='" + re.escape(version) + r"'", badging), "APK versionName mismatch"
assert f"versionCode='{expected_code}'" in badging, "APK versionCode mismatch"
print(f"APK version verified: {version}/{expected_code}")
PY_VERSION

# Every native dependency must support Android's 16 KiB page size.
case "$(uname -s)" in
  Darwin) ndk_host=darwin-x86_64 ;;
  Linux) ndk_host=linux-x86_64 ;;
  *) echo 'Unsupported NDK host' >&2; exit 1 ;;
esac
export BLUEDOOR_ANDROID_READELF="$NDK_HOME/toolchains/llvm/prebuilt/$ndk_host/bin/llvm-readelf"
python3 - <<'PY_NATIVE'
import os, pathlib, subprocess, tempfile, zipfile
mapping = pathlib.Path("src-tauri/gen/android/app/build/outputs/mapping/universalRelease/mapping.txt").read_text()
for name in ("OnnxJavaType", "OnnxTensor", "TensorInfo", "OrtSession"):
    original = f"ai.onnxruntime.{name}"
    assert f"{original} -> {original}:" in mapping, f"ONNX JNI class renamed: {original}"
print("ONNX JNI classes preserved by R8")
with tempfile.TemporaryDirectory(prefix="bluedoor-elf-") as temporary:
    with zipfile.ZipFile(os.environ["BLUEDOOR_ANDROID_APK"]) as apk:
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
