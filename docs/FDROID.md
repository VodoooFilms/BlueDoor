# Android distribution: F-Droid and sideload

Status: **not ready for submission**. The current source uses Google ML Kit
text recognition and barcode scanning, including proprietary native OCR and
barcode binaries. Disabling uploader discovery does not make these FLOSS.
They must be replaced with source-built FLOSS equivalents while retaining OCR
coordinates and QR erasure; silently removing OCR would weaken protection.
See the [F-Droid inclusion policy](https://f-droid.org/docs/Inclusion_Policy/).
`metadata/com.bluedoor.desktop.yml` is intentionally disabled; it is not a
working fdroiddata build recipe. `Builds` remains empty until a public source
revision and a tested Linux build are available. Do not claim reproducibility
from a single successful macOS build.

Identity: `com.bluedoor.desktop` (unchanged). Version 0.5.7, versionCode 5007.
Code formula: major × 1,000,000 + minor × 1,000 + patch. Every shipped update
must increase it. Names and descriptions live under `metadata/{en-US,es-ES}`.
Launcher assets live in `src-tauri/gen/android/app/src/main/res/mipmap-*`;
the canonical door geometry remains `assets/branding/bluedoor-app-icon.svg`.

## Toolchain and build

Current successful local build inputs (not yet a hermetic F-Droid toolchain):

- JDK 17; set JAVA_HOME explicitly (Linux and macOS supported by the script).
- Android SDK platform 36, build-tools 35.0.0, target SDK 34, minimum SDK 24.
- NDK r27c / 27.2.12479018. Set NDK_HOME to the installed directory.
- Rust 1.98.1, target `aarch64-linux-android` (`rustup target add ...`).
- Gradle wrapper 8.14.3, Android Gradle Plugin 8.11.0, Kotlin plugin 1.9.25.
- pnpm 12.3.4; JS and Rust dependencies from pnpm-lock.yaml and Cargo.lock.

```sh
pnpm install --frozen-lockfile
pnpm check
cargo test --locked --manifest-path src-tauri/Cargo.toml
pnpm test:ui
pnpm eval:pii
BLUEDOOR_ANDROID_BUILD_MODE=unsigned ./scripts/build-android.sh
```

The unsigned mode uses no signing key and writes
`installers/Bluedoor-android-arm64-unsigned.apk`. It verifies identity/version,
16 KiB ELF and ZIP alignment and retained ONNX JNI names. It is for the F-Droid
signing pipeline, not direct installation. Weights are not bundled: inference
runs locally after opt-in downloads verified against model-manifest.json.
The Android Qwen path now requires the multilingual GLiNER model installed
first; missing/corrupt weights block analysis rather than skip the NER pass.
GLiNER is learned NER with fixed inference, not a rule-based detection guarantee.

Outstanding reproducibility work: pin a Linux build image/JDK/Node/Rust,
verify the Gradle distribution digest, vendor/cache allowed build dependencies,
replace ML Kit and audit transitive licenses/native sources (including ONNX),
record a clean build recipe against a public immutable commit, then compare
unsigned APKs from two clean independent builders. Local compiled caches are
not acceptable source inputs. No claim of bit-for-bit reproducibility yet.

## Upstream release signing

Never commit a keystore or passwords. Create a signing identity outside the
repository, retain an offline backup, and record its certificate fingerprint.
Losing this key prevents normal sideload updates. F-Droid normally manages its
own signing identity; switching channels may require uninstall/reinstall and
manual backup of user data.

```sh
mkdir -p "$HOME/.local/share/bluedoor-signing"
chmod 700 "$HOME/.local/share/bluedoor-signing"
keytool -genkeypair -keystore "$HOME/.local/share/bluedoor-signing/release.p12" \
  -storetype PKCS12 -alias bluedoor-release -keyalg RSA -keysize 4096 -validity 10000
keytool -list -v -keystore "$HOME/.local/share/bluedoor-signing/release.p12" \
  -alias bluedoor-release
export ANDROID_KEYSTORE_PATH="$HOME/.local/share/bluedoor-signing/release.p12"
export ANDROID_KEYSTORE_ALIAS=bluedoor-release
# Supply passwords through the local environment/secret manager, not git:
# ANDROID_KEYSTORE_PASSWORD and ANDROID_KEYSTORE_KEY_PASSWORD
# Set ANDROID_RELEASE_CERT_SHA256 to the printed SHA-256 certificate fingerprint.
./scripts/build-android.sh
```

Gradle reads all four ANDROID_KEYSTORE_* variables; partial configuration fails.
Release mode is the default and refuses missing keys, debug/preview certificates,
invalid signatures or a certificate SHA-256 different from the pinned fingerprint.
It writes `installers/Bluedoor-android-arm64-release.apk` and its SHA-256 sidecar.
It never generates a temporary signing identity. Keep secrets out of shell logs.
Add the signed APK and checksum to a GitHub beta release only after the full
PII pipeline and required suites pass. There is no signed public beta link yet.
