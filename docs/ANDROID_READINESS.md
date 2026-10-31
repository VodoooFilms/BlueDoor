# Android readiness evidence — 2026-10-30

Requested scope remains **partially completed**. Nothing was published or pushed.
No release signing key was generated; the owner requested creation instructions.
The starting checkout already had Android/GLiNER/layout/version edits; these
were preserved and excluded from the new phase commits. Validation below uses
the complete working tree, including those pre-existing edits.

## PII evaluation

Production rules/merge only, on 246 authored synthetic development fixtures:

| Category | Gold | Fully covered | Recall | Strict precision |
|---|---:|---:|---:|---:|
| RUT | 24 | 24 | 100% | 100% |
| Cédula | 24 | 24 | 100% | 100% |
| Phone | 24 | 24 | 100% | 72.7% |
| IBAN | 24 | 24 | 100% | 100% |
| Address | 24 | 24 | 100% | 66.7% |
| Person | 36 | 36 | 100% | 100% |
| Email | 24 | 24 | 100% | 100% |
| Card | 24 | 24 | 100% | 100% |
| Explicit DOB | 24 | 24 | 100% | 100% |
| Total | 228 | 228 | 100% | 92.8% |

Rules-only initial coverage was 89/228 (39.0%). Final strict precision counts
220 exact true-positive predictions and 17 extra/incorrect spans. Eight gold
addresses were fully covered by oversized spans, so coverage and exact-span
true positives intentionally differ. Details, license and limitations live in
`tests/pii_eval/`. This is not an independent real-chat benchmark and does not
resolve the trust blocker. The historical 69%/61% figure was supplied by the
request and was not reproduced here. No full Qwen3-1.7B + GLiNER benchmark has
been run; no learned-model recall/precision is claimed. The Android build
compiles the new GLiNER-before-Qwen path; device runtime validation is pending.

## Verification and artifact

- `cargo test --locked --manifest-path src-tauri/Cargo.toml`: 144 passed,
  22 ignored, zero failures in the final run. Model-dependent opt-in tests were
  not counted as passed. A first run failed an updated name-advisory expectation
  and a transient layout-helper JSON read; the adjusted expectation and final
  whole-suite rerun passed.
- `pnpm check`: typecheck/build passed.
- `pnpm test:ui`: all requested UI workflows passed after reopening the settings
  menu between two language changes in the test, matching app behavior.
- `pnpm eval:pii`: passed the 99% gate; see synthetic-only limitation above.
- `BLUEDOOR_ANDROID_BUILD_MODE=unsigned ./scripts/build-android.sh` with JDK 17:
  passed. App ID/version 0.5.7/5007, retained ONNX JNI classes, 16 KiB ZIP/native
  alignment checked. Unsigned artifact: `installers/BlueDoor-android-arm64-unsigned.apk`,
  **69,406,077 bytes (66.19 MiB)**. It is not an installable signed release.
- Merged release manifest contains only INTERNET and AndroidX's signature-only
  internal receiver permission. ACCESS_NETWORK_STATE is removed.
- Default release build refuses absent signing credentials. Actual signing and
  its successful fingerprint verification remain untested without an owner key.
- No ADB devices were connected. Midrange-device cold/warm model load time,
  peak RAM, thermal behavior and full pipeline accuracy remain **unmeasured**.

## Remaining release blockers

1. Obtain a properly licensed/consented corpus of at least 200 real/anonymized
   chats and an independent holdout; measure actual full pipeline scores on it.
   Address any misses without weakening human review or export gates.
2. Replace proprietary ML Kit OCR/barcode code with FLOSS equivalents preserving
   text coordinates and reliable QR erasure. Metadata is intentionally disabled.
3. Verify a pinned clean Linux source build twice and supply a working immutable
   fdroiddata build recipe; finish transitive source/license checks.
4. Create the owner's release signing identity using docs/FDROID.md, validate
   the signed APK on a physical midrange phone and publish the signed beta plus
   checksum/certificate fingerprint on GitHub Releases. There is no public APK
   release URL from this work. The unsigned artifact was not uploaded.

See docs/ANDROID_BETA.md for installation and voluntary, manually redacted logs.
