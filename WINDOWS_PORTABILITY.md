# Windows portability

Status: **validated**. The Windows ARM64 build is complete and runtime-verified
on Windows 11 ARM64. The first target is `aarch64-pc-windows-msvc`; the same
source and scripts also support `x86_64-pc-windows-msvc`.

## Verified on Windows ARM64 (11 Sep 2026)

- Release build `pnpm release:windows:arm64` produces NSIS and MSI installers:
  - `src-tauri/target/windows-arm64/aarch64-pc-windows-msvc/release/bundle/nsis/Local Legal_0.1.0_arm64-setup.exe`
  - `src-tauri/target/windows-arm64/aarch64-pc-windows-msvc/release/bundle/msi/Local Legal_0.1.0_arm64_en-US.msi`
- `scripts/verify-windows-bundle.py` PASS: `local-legal.exe`, `llama*.dll`,
  `ggml*.dll` and the dynamic `ggml-cpu*.dll` CPU backend sit next to the
  executable. An installer that loses the DLL closure is rejected.
- App smoke test PASS: models download into `%APPDATA%\com.locallegal.desktop\models\`,
  Spanish transcription with `large-v3-turbo` produces correct text, and Qwen3
  correction works (combo test `large-v3-turbo + qwen3-fast` → correct output).
- Rust tests compiled and executed on Windows ARM64 with the LLVM/clang+Ninja
  toolchain: `llm_generates_spanish_correction` and
  `transcribe_correct_all_model_combos` PASS.

## What is portable

| Area | Result |
| --- | --- |
| UI and product flow | Svelte/Tauri IPC is unchanged. Record → Transcribe → Refine → Edit → PDF remains one flow. |
| Microphone | The renderer uses Web Audio + `getUserMedia`, then writes PCM WAV itself. WebView2 provides the Windows permission prompt; no CoreAudio/TCC code is present. |
| App data, history, models and downloads | `AppHandle::path().app_data_dir()` maps to `%APPDATA%\com.locallegal.desktop` on Windows. Paths use `PathBuf`; model verification, atomic writes and temporary files are OS-neutral. |
| Model weights | Whisper and Qwen GGUF/BIN files are architecture-independent. The pinned manifest, TLS download and SHA-256 verification are reused without changes. |
| PDF layout | `genpdf` is portable. Font discovery uses `%WINDIR%\Fonts` on Windows (with the legacy short names `arialbd`, `arialbi`, …) and the existing Supplemental fonts on macOS. |
| Tests | `src-tauri/src/platform.rs` exposes `test_models_dir()` (macOS: App Support legacy path; Windows: `%APPDATA%\com.locallegal.desktop\models`) and `test_audio_fixture()` (`/tmp/vtp-es.wav` vs `%TEMP%\vtp-es.wav`). |

## Implemented platform boundary

`src-tauri/src/platform.rs` owns the only runtime differences:

- macOS uses `runtime/ffmpeg`, Windows uses `runtime/ffmpeg.exe`;
- macOS loads the bundled Apple M1 CPU + Metal GGML modules; Windows discovers
  and loads the bundled CPU DLLs beside the executable;
- PDF font directories are native to each OS.

`llama-cpp-sys-2` keeps dynamic linking on both systems. The `metal` feature is
macOS-only. The Windows packaging contract is explicit: `llama*.dll`,
`ggml*.dll` and a `ggml-cpu*.dll` backend must sit next to `local-legal.exe`.

## Windows ARM64 build host

Windows 11 ARM64 with: Visual Studio Build Tools (C++ ARM64 + Windows SDK),
CMake/Ninja, LLVM/clang at `C:\Program Files\LLVM`, MSYS2, Rust (with the
`aarch64-pc-windows-msvc` target), Node/pnpm and Python. WebView2 is supplied by
supported Windows 11.

The native libraries need clang, not MSVC, for ARM64 (GGML rejects the MSVC
compiler on ARM). The build environment must export exactly:

```
PATH=<C:\Program Files\LLVM\bin>;<BuildTools CMake Ninja>\Ninja;<node;<cargo>;%PATH%
CMAKE_C_COMPILER=C:/Program Files/LLVM/bin/clang-cl.exe
CMAKE_CXX_COMPILER=C:/Program Files/LLVM/bin/clang-cl.exe
CMAKE_GENERATOR=Ninja
WHISPER_DONT_GENERATE_BINDINGS=1
CARGO_TARGET_DIR=src-tauri\target\windows-arm64
```

Run `pnpm release:windows:arm64` from a native PowerShell after those tools are
available. It builds the pinned file-only FFmpeg `ffmpeg.exe`, builds NSIS and
MSI installers, then verifies the executable, CPU backend/DLL closure and both
installer outputs. It intentionally does not sign an installer yet.

## Windows x64 follow-up

The same source and script support `pnpm release:windows:x64`; it targets
`x86_64-pc-windows-msvc`. Build it on a real x64 Windows host or a configured
cross-compilation CI worker. Do not treat ARM64 Windows under Parallels as an
x64 release test: emulation may conceal installer, DLL, microphone and
performance problems.

## Remaining release blockers

1. Authenticode signing, timestamping and SmartScreen reputation policy for
   commercial Windows distribution, then repeat on clean ARM64 and x64 VMs.
2. Re-run the full smoke test after a real installer install (current smoke run
   executed from the build output directory).
3. Keep MSYS2 available on the build host: it is required by the FFmpeg
   configure/make build and was removed from the current VM after the final
   decoder artifact was produced.