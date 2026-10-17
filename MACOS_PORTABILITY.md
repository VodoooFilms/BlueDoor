# macOS portability

## Dependency map before this change

| Operation | Runtime dependency outside the app | Previous resolution |
|---|---|---|
| Microphone | FFmpeg for MediaRecorder's compressed blob | Homebrew, MacPorts, /usr/local, then PATH |
| WAV import | hound linked into Rust; no FFmpeg for supported PCM | direct file read; limited integer formats |
| MP3/M4A/AAC/MP4/MOV/OGG/FLAC/WebM/MKV/WMA/AIFF | FFmpeg demux/decoding/resampling | same system lookup |
| Local AI | llama/ggml dynamic libraries | manually copied into Frameworks |
| llama common | OpenSSL libssl/libcrypto | absolute /opt/homebrew paths in Mach-O |
| PDF | Arial/Times/etc. system fonts | /System/Library/Fonts/Supplemental (macOS-owned) |
| Models/history | app-owned downloaded models and user JSON | Application Support/com.bluedoor.desktop |
| Frontend | WKWebView and embedded JS/CSS | macOS framework and app executable |

No runtime ffprobe call exists. Rust, Node, pnpm, Python, CMake and Xcode are build tools only. The previous scripts' Homebrew include/library variables were build-time inputs whose OpenSSL linkage leaked into the runtime bundle. Repository-relative runtime lookup is not acceptable.

Implementation and validation results are recorded below as the work completes. A dependency inspection is not a clean-Mac test, and ad-hoc signing does not satisfy downloaded-app Gatekeeper qualification.

## Implemented architecture

### Microphone

`getUserMedia` → WKWebView Web Audio / AudioWorklet → mono signed 16-bit PCM WAV → Tauri `transcribe_blob` → Rust hound → transcribe.cpp (Whisper or Parakeet). The browser requests a 16 kHz context and records its actual sample rate in the WAV header. The worklet flushes its final samples before stop completes. Rust accepts PCM WAV directly in memory: no FFmpeg, external executable or recording temporary file. This is Web Audio backed by macOS capture, not a new native Rust capture subsystem. Existing orb and controls remain.

`public/pcm-worklet.js` is embedded by the frontend build. Missing worklet/capture initialization yields an application-level retry/reinstall message. Real WKWebView microphone/TCC testing remains pending; the WebKit synthetic-stream test does not grant or test microphone permission.

### Imports

Standard PCM WAV (including 8/16/24/32-bit integer and float32) is decoded directly by hound; other or unsupported WAV variants use the bundled decoder. Compressed/imported media uses **only** `AppHandle.path().resource_dir()/runtime/ffmpeg`. No production PATH, Homebrew, MacPorts, /usr/local or repository fallback. No ffprobe is needed.

FFmpeg 8.1.2 is built from a SHA-256 pinned official source tarball. It contains the needed demuxers/audio decoders and PCM WAV encoder, with networking, GPL/nonfree/version3 features, automatic external-library discovery and unnecessary programs disabled. Only the file protocol exists. No video decoding is needed: the app selects the first audio stream from MP4/MOV/MKV/WebM. Supported containers/codecs include MP3, AAC/M4A, ALAC, FLAC, OGG/Vorbis/Opus, AIFF/PCM, WMA family and WAV. Encrypted/DRM inputs and arbitrary codecs are not supported.

Rust canonicalizes the selected file, requires a real file, uses direct `Command.args`/`arg`, and writes into a private RAII temporary directory. Shell punctuation is never interpreted. Errors describe an invalid file, missing audio, storage failure or reinstall need. Runtime subprocess stderr is not displayed verbatim. The decoder process is not sandboxed; keeping a pinned decoder patched for untrusted media remains a release responsibility. Long imports still decode fully into memory, and inference cancellation is outside this pass.

### AI native libraries

OpenSSL was enabled by llama's newer `LLAMA_OPENSSL` setting despite the old `LLAMA_CURL=OFF`. `.cargo/config.toml` supplies `scripts/native-options.cmake` to disable OpenSSL/BoringSSL/LibreSSL at configuration time. No SSL library is copied from Homebrew. Rust's model downloader retains its own HTTPS implementation.

Keep dynamic linking to avoid the existing Whisper/llama ggml collision. The package contains llama/ggml `.dylib` dependencies **and** ggml CPU/Metal MODULE binaries ending in `.so`. CPU uses the Apple M1 baseline, compatible with later Apple Silicon, while Metal is loaded explicitly. Production loads exactly the known modules from `Contents/Frameworks` before initializing llama, not its compiled-in backend directory/cwd/environment search. Debug/tests may use the crate's build directory; that fallback is compiled out of release builds. M2/M3/M4 variants are packaged from the native build but not selected by the current explicit baseline loader.

Standard macOS frameworks and `/usr/lib` are the only permitted external native dependencies. System PDF fonts remain OS-provided, not developer-installed resources. Shader source is embedded in the Metal backend (`GGML_METAL_EMBED_LIBRARY=ON`); users do not need Xcode shader tools.

## Application-owned assets and repair

`src-tauri/model-manifest.json` pins each catalog download to an immutable repository revision, exact length and SHA-256 from Hugging Face LFS metadata. URLs, hashes and filenames are part of application code, not supplied by the webview. No multi-GB model is bundled.

Downloads require HTTPS (including redirects), check free space plus a 64 MiB margin, stream to an exclusive `.part` temporary, verify exact bytes/hash, sync and atomically publish. In-process downloads are mutually exclusive. Failed/canceled downloads never replace an installed model; retry stays in the app. The Cancel button is bounded by a 15-second socket timeout while a read/connect is stalled. No resumable range downloads yet. Multiple app instances can duplicate work but unique temporaries and immutable hashes prevent partial files from being published as valid models.

Installed-state checks use expected length for cheap UI refresh. Full SHA-256 is checked before loading an uncached model and before treating a download request as already satisfied. Hash checks/model initialization now occur in blocking workers. A missing/truncated model exposes Download automatically. Same-length corruption is rejected at load; remove and download again through Models. Full hash checks increase cold-load latency but avoid silently parsing modified models. Recommended assets total ~2.89 GB plus operating headroom (Whisper Large v3 Turbo and Qwen3 1.7B).

`check_installation` verifies the decoder is present/executable and application data is writable, returning UI guidance before history hydration. Essential loader dylibs can fail before Rust runs: the release gate must catch those omissions; an OS launch failure is repaired by reinstalling the app. Signed application code is never redownloaded or modified in place. Models can be downloaded/repaired in-app; user data remains fail-closed on corruption. There is no general updater or automatic history recovery yet. Microphone permission problems direct the user to System Settings, never Terminal.

## Canonical build

Developer-only prerequisites: Apple Silicon macOS, Xcode command-line compiler/SDK, CMake, Rust, Node, pnpm 12.3.4, Python >=3.9 and make. None is an end-user requirement. No system FFmpeg or OpenSSL is required to build the app-owned runtime. Builds download pinned source/crates/packages only; model weights are not needed for packaging.

```sh
pnpm install --frozen-lockfile
pnpm release:mac
```

The command builds/caches the pinned decoder, gathers and audits dependency notices/source, builds the locked arm64 Tauri app in a dedicated `src-tauri/target/portable` tree, bundles native dependencies, removes build rpaths, adds the Frameworks rpath and verifies the icon/bundle. It then signs nested code and the app with Developer ID plus hardened runtime and secure timestamp, notarizes/staples the app and DMG, runs Gatekeeper assessments, verifies the image and emits SHA-256. `pnpm release:mac:dev` produces a separately named ad-hoc local test image.

Artifact: `src-tauri/target/portable/aarch64-apple-darwin/release/bundle/dmg/BlueDoor_0.2.0_aarch64.dmg` (commercial), or `BlueDoor_0.2.0_development_aarch64.dmg` (development). Dated GitHub previews use an additional timestamp in the asset name.

`build-dmg.sh` is a compatibility wrapper for that same command. `build-install.sh` uses it too, stages and verifies the app, keeps rollback until replacement succeeds, and launches. It is a developer convenience, not an end-user install requirement. No stale “newest app” discovery or skip-build distribution path. Tauri is configured to build only `.app`; plain `tauri build` is not the release entry point because post-processing remains necessary.

A source/config/compiler/SDK stamp and hash validate the cached decoder. The release is repeatable from pinned inputs on the same toolchain; **not claimed bit-for-bit reproducible** across SDKs, toolchains or signing timestamps. CMake/Rust/Xcode still need a pinned CI image for reproducibility across hosts. Preserve the source/license payload when publishing the DMG.

## Automated release gate

`scripts/verify-bundle.py` scans every Mach-O file recursively, including `.so` and executables hidden among resources. It verifies arm64, deployment metadata <=13.4, dylib resolution, contained symlinks, permitted rpaths, known resources, decoder version/source integrity and file-only protocol configuration. Non-system absolute dependencies (Homebrew, /usr/local, repository/build/home paths) fail. No arbitrary dependency is silently copied to make the gate green.

The gate runs before and after nested/app signing. `codesign --verify --deep --strict` and `hdiutil verify` are mandatory. Fault-injection tests remove the decoder, inject a Homebrew rpath and disguise a Mach-O with /usr/local linkage as a resource; all must fail. Relocation into a directory with spaces and Unicode must pass. These checks are necessary but do not replace exercising the signed app on supported OS/hardware.

## Licensing and sources

See `THIRD_PARTY_NOTICES.md`. The bundle carries FFmpeg's exact unmodified source archive, configure/build script, LGPL text, dependency license texts and version inventory. The compiled FFmpeg prints LGPL version 2.1-or-later; no GPL/nonfree/version3 options are enabled. Patent/codec and overall commercial distribution review remain human responsibilities.

The offered correction models are Qwen3 1.7B and 4B Q4_K_M blobs identified by Ollama and pinned by exact SHA-256. Their official Qwen3 repositories use Apache-2.0; immutable license URLs and exact texts are checked in. The research-licensed Qwen2.5-3B remains recognized only so an existing installation can delete it: current activation/download commands reject its legacy id. Sources: [Qwen3-1.7B license](https://huggingface.co/Qwen/Qwen3-1.7B/blob/70d244cc86ccca08cf5af4e1e306ecf908b1ad5e/LICENSE), [Qwen3-4B license](https://huggingface.co/Qwen/Qwen3-4B/blob/1cfa9a7208912126459214e8b04321603b3df60c/LICENSE), [Ollama Qwen3 tags](https://ollama.com/library/qwen3/tags), [FFmpeg licensing](https://ffmpeg.org/legal.html).

## Signing and remaining gates

The canonical production pipeline is implemented but has not completed on this Mac because no valid Developer ID Application identity or notarization credential is installed. It signs FFmpeg and ggml modules/dylibs first, then the app with hardened runtime, secure timestamp and the audio-input entitlement; it adds no broad library-validation exception. It submits a zipped app and the final DMG with `notarytool --wait`, saves Apple rejection evidence, staples/validates both, then requires `codesign`, `spctl` and `hdiutil` checks. Configure either `BLUEDOOR_NOTARY_PROFILE` or the documented API-key environment variables; credentials are never stored in the repository.

For App Store Connect API authentication, set `BLUEDOOR_NOTARY_KEY_ID`, `BLUEDOOR_NOTARY_ISSUER_ID` and `BLUEDOOR_NOTARY_KEY_PATH`. For a Keychain profile, create it outside the repository with `xcrun notarytool store-credentials` and expose only its name as `BLUEDOOR_NOTARY_PROFILE`. If more than one Developer ID identity exists, set `BLUEDOOR_SIGNING_IDENTITY` to a unique name substring.

The clean-machine procedure and results ledger are in `MACOS_CLEAN_TEST.md`. **Runtime dependency and license engineering gates PASS; complete plug-and-play acceptance remains open.** Missing evidence: credentialed notarization and a genuine clean Apple Silicon Mac covering actual microphone/TCC, minimum-OS Metal, offline real inference/import/export, restart persistence, recovery scenarios and browser-downloaded Gatekeeper acceptance.

## Validation recorded on the developer Mac

- Frozen pnpm install, typecheck (0/0), frontend build, Cargo fmt/check, strict clippy: PASS.
- Rust tests: 16 PASS. Initially the audio fixture was absent; a synthetic Spanish WAV was then generated and the suite rerun. Final run includes all 4 installed Whisper × 2 Qwen combinations and the Spanish transcription test; no audio/model skips in that final run. Tiny/base are not matrix targets.
- SHA-256 checks: all six installed model files match the committed manifest (tiny/base absent).
- WebKit editor/history tests and synthetic AudioWorklet PCM test: PASS. These mock IPC or synthetic streams and are not a packaged hardware microphone test.
- Bundled decoder: twelve generated format fixtures PASS, with Unicode/spaces/quotes/punctuation filenames; malformed media rejected, protocol inventory file-only. Fixture encoding used a developer tool supplied explicitly to the test; decoding used only the bundled executable with a system-only PATH.
- Real bundle-native CPU/Metal modules and llama initialized successfully on the developer M4; Metal loaded its embedded shader library.
- Bundle inspection, relocated path test, deliberate missing decoder/external-rpath/hidden-Mach-O fault tests, nested signature and DMG checks: PASS.
- Final commercial acceptance remains pending because no truly clean Mac, real microphone permission journey or notarized production artifact was available for this run.
