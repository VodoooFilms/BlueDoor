# Bluedoor

<p align="center">
  <img src="assets/branding/bluedoor-app-icon.svg" alt="Bluedoor" width="128" />
</p>

**Prepare sensitive documents for sharing, right on your Android phone.**

Bluedoor is a mobile-first document privacy workspace. Import a document, detect sensitive information with an on-device model, review and edit the proposed aliases, and export a protected PDF. Document processing stays on your device.

[Download Android preview](https://github.com/VodoooFilms/BlueDoor/releases/tag/v0.5.13-android-preview) · [Product page](https://www.prompthea.com/projects/bluedoor) · [Report an issue](https://github.com/VodoooFilms/BlueDoor/issues/new?template=android-beta.yml)

## Android first

- **Local document analysis:** choose Qwen, GLiNER PII Small or GLiNER Multi. Each model downloads and runs independently; Qwen is a separate 397 MB download.
- **Import and OCR:** work with PDFs, supported images, DOCX and text. Review the extracted content and the original document before sharing.
- **Editable privacy map:** inspect detected people, organizations, identifiers and contact details; correct omissions and choose readable aliases.
- **Document editing and PDF export:** review the protected result, approve it and export. Changes invalidate the previous approval.
- **Batch review:** process several documents and manage shared aliases within the batch.
- **Encrypted Vault:** optionally save work locally, protected by a password and recovery phrase.
- **Offline processing:** download the chosen model first, then analyze documents without an internet connection. No automatic document uploads, telemetry or remote crash reporting.
- **Mobile interface:** portrait-friendly review, source zoom, stable detection progress, and English/Spanish UI.

Detection can miss information. Human review of the privacy map and document pages is required; Bluedoor does not guarantee anonymity.

## Install on Android

Current preview: **0.5.13**, for **ARM64 Android devices, Android 7.0 or later**.

1. Download the APK and its SHA-256 checksum from the [0.5.13 release](https://github.com/VodoooFilms/BlueDoor/releases/tag/v0.5.13-android-preview).
2. Open the APK on your phone and allow installation from that source when prompted.
3. For an existing preview with the same signature, install as an update **without uninstalling**.
4. Open **Models** and download your chosen model. Qwen does not require GLiNER; model files are not bundled in the APK.
5. Start with a fictional document, detect, review, approve and export.

This is a testing preview signed with the existing local preview certificate. It is not a Google Play production release. F-Droid distribution is planned but has [documented blockers](docs/FDROID.md).

[Installation and voluntary bug reports](docs/ANDROID_BETA.md) · [Android validation](docs/ANDROID-VALIDATION.md) · [Qwen performance and regression evidence](docs/ANDROID-QWEN-REGRESSION-2026-11-01.md)

## Privacy and local data

Documents are processed locally. Model downloads are explicit actions; reporting a bug or sharing a PDF is also a user action. The Vault provides optional encrypted storage rather than automatically saving original documents.

Bluedoor uses its own application identity, `com.bluedoor.desktop`, and independent preferences and storage. It does not automatically migrate personal documents from other applications. Uninstalling clears app storage; preserve any files you need before removing the app.

See [privacy information](PRIVACY.md), the [Vault threat model](docs/VAULT_2026-10-23.md), and [third-party notices](THIRD_PARTY_NOTICES.md).

## Desktop development

The repository also contains desktop work for macOS and Windows. Desktop includes audio transcription and speaker-related workflows that are **not available in the Android app**. The current public download above is the Android preview.

Built with **Svelte 5, Tauri 2 and Rust**, with local llama.cpp and ONNX model execution.

```sh
pnpm install --frozen-lockfile
pnpm dev
# Native desktop development, with platform prerequisites installed:
pnpm tauri dev
```

For Android toolchain, source builds and signing, see [the build guide](docs/FDROID.md#toolchain-and-build). Release builds require an explicit signing identity; preview signing is not a production release identity.

## Verification

```sh
pnpm typecheck
pnpm build
pnpm check:native
pnpm test:native
pnpm test:ui
node tests/run-ui.mjs --android-models
node tests/run-ui.mjs --model-switch
```

Real Android inference regressions use synthetic documents and an explicitly prepared emulator. Their setup and measured limits are documented in the [Qwen regression report](docs/ANDROID-QWEN-REGRESSION-2026-11-01.md). Emulator timings do not establish performance on every phone. Check the [Actions runs](https://github.com/VodoooFilms/BlueDoor/actions) for current CI status.

Model weights, installers, personal documents and local QA artifacts stay outside Git. Downloadable APKs and checksums are attached to releases. Historical desktop milestones are retained in `docs/archive/` and `docs/milestones/`.
