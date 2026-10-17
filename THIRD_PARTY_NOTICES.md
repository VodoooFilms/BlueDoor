# Third-party components and commercial-use audit

Scope: the arm64 macOS application bundle, its audio sidecar and the model files
the current UI/backend can download. Application source is MIT; dependencies and
model weights keep their own terms. This is an engineering audit for the owner
and counsel, not a legal opinion.

| Component | Exact version/source | Delivery | License and attribution | Redistribution / commercial conclusion |
|---|---|---|---|---|
| BlueDoor | repository version 0.1.0 | Bundled | MIT; original Belegal _ Local demo contributors attribution retained in LICENSE | **Allowed**; retain MIT notice. |
| FFmpeg audio decoder | 8.1.2; SHA-256 pinned official tarball | Bundled executable | LGPL-2.1-or-later; FFmpeg project | **Allowed with obligations**. The build disables GPL, nonfree, version3, network and external autodetection. Bundle the unmodified corresponding source archive, build recipe and LGPL text. Codec patent review remains market-specific. |
| whisper.cpp and Rust bindings | `whisper-rs` 0.12.0 / `whisper-rs-sys` 0.10.0 | Linked code | whisper.cpp MIT; bindings Unlicense | **Allowed with obligations**; retain notices. |
| Tauri, Svelte and remaining runtime packages | exact versions in `runtime/licenses/inventory.json` generated from Cargo.lock and pnpm lock | Linked/bundled | Per-package SPDX expression, source, authors and license files in inventory | **Allowed with recorded obligations** for the current closure. The release gate fails if a runtime package lacks an expression or license evidence. MPL-covered `selectors` source remains available at its recorded upstream revision. |
| NVIDIA Nemotron 3 Diarization weights | Pinned revision, size and SHA-256 in `model-manifest.json` | Downloaded on demand | License source recorded in manifest | Review model terms before commercial distribution. |
| Whisper weights | exact immutable Hugging Face revision, size and SHA-256 in `model-manifest.json` | Downloaded on demand; absent from DMG | MIT provenance through OpenAI Whisper / whisper.cpp; copyright notice retained | **Allowed with obligations**; keep model notices and pinned provenance. |
| Qwen3-1.7B Q4_K_M | Pinned historical Ollama blob `qwen3-fast`, exact SHA-256 in model manifest | Public weights downloaded on demand | Apache-2.0; Qwen3-1.7B license retained in `licenses/models` | Local privacy extraction only; not transcript correction. |
| llama.cpp / llama-cpp-2 | `llama-cpp-2` and `llama-cpp-sys-2` 0.1.156, exact closure in Cargo.lock | Native runtime bundled | Upstream MIT and Rust binding MIT/Apache-2.0 | Notices retained; CPU/Metal modules and dylibs packaged locally. |
| PDF/DOCX extraction | pdf-extract, zip and quick-xml, exact versions in Cargo.lock | Linked code | Per-package expressions in lockfile/inventory | Local parsing only; no cloud converters. |
| Newsreader font | Google Fonts regular 400 and italic 300 | Local UI assets | SIL OFL 1.1; `public/fonts/OFL.txt` | Retain the font license with distribution. |
| macOS frameworks and fonts | supplied by supported macOS | OS supplied | Apple/system terms | Not redistributed as separate repository assets. Review PDF font embedding behavior before adding fonts. |

The release process copies this file, model license evidence, locked package
inventory, package license evidence, the FFmpeg source archive and the FFmpeg
build script into `Contents/Resources/runtime`. The checked-in fallbacks under
`licenses/dependencies` cover crates whose published archive omitted a standalone
license file; their provenance is recorded in that directory.

The current automated engineering gate classifies the present commercial
catalog as **PASS with obligations**. Human review still needs to accept the
open-source/model obligations and decide codec patent exposure for sale markets.
No certificate, signing credential, AI weight or private key is stored here.

Primary sources:

- Whisper license: https://github.com/openai/whisper/blob/main/LICENSE
- FFmpeg legal information: https://ffmpeg.org/legal.html
- Apple notarization workflow: https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution
