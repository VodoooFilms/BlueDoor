# Belegal _ Local demo — commercial readiness audit

Audit date: 10 Sep 2026. Scope: current `main` after the Qwen3 migration plus
the commercial-release changes in this working session. Version: 0.1.0, arm64,
minimum macOS 13.4. User-owned edits in `TranscriptView.svelte`, `i18n.ts` and
`tests/check-editor.mjs` were preserved and are outside this change set.

## Verdict

**Engineering state: READY FOR DEVELOPER ID SIGNING.** The current model catalog
has an engineering license gate of PASS with obligations, the app has a
self-contained deterministic bundle path, and the canonical release command now
implements hardened-runtime signing, notarization, stapling and Gatekeeper checks.

**Commercial distribution state: NOT YET APPROVED.** This Mac has no usable
`Developer ID Application` identity or notarization credentials, so a notarized
candidate cannot be produced here. A browser-downloaded candidate also has not
passed the strict clean-Mac journey. Those are evidence gaps, not missing product
features.

## Final release matrix

| Gate | Result | Evidence / remaining action |
|---|---|---|
| App and dependency license inventory | PASS with obligations | 303 runtime and 86 conservative build-only packages enumerated from locked graphs; every third-party runtime entry has an SPDX expression and collected or checked-in upstream license evidence. Owner/counsel review remains. |
| Current correction models | PASS with obligations | Qwen3 1.7B and 4B are Apache-2.0. Ollama blob digests match the manifest; official Qwen license revisions and texts are pinned. |
| Restricted legacy model | PASS after fix | Qwen2.5-3B stays recognizable only for deletion/integrity cleanup. Backend activation and download now accept current Qwen3 IDs only; regression test covers the boundary. |
| FFmpeg redistribution | PASS with obligations | Pinned FFmpeg 8.1.2 file-only build; LGPL source archive, build script and license ship beside the executable. Market-specific codec patent review remains. |
| Runtime portability | PASS on developer Mac | Recursive Mach-O/dependency/rpath/resources verifier; bundled FFmpeg and ggml CPU/Metal modules; no Homebrew/OpenSSL runtime dependency. |
| App icon | PASS on inspected bundle | Source icon set and bundled `Assets.car` are checked by hash/render-aware verifier so the new padded dock icon cannot silently regress to an old asset. |
| Hardened runtime pipeline | IMPLEMENTED, awaiting credentialed run | Nested Mach-O files are signed first, then app with secure timestamp, runtime option and audio-input entitlement. No broad library-validation exception or secret is stored. |
| Notarization and stapling | IMPLEMENTED, not executed | `notarytool submit --wait` for zipped app and DMG; rejection logs retained; app/DMG stapled and validated. Requires credentials below. |
| Gatekeeper validation | IMPLEMENTED, not executed on production signature | `codesign`, `spctl` app/open assessment and `hdiutil verify` are mandatory after notarization. |
| Clean Apple Silicon Mac | OPEN | Execute `MACOS_CLEAN_TEST.md` using a browser-downloaded notarized DMG on macOS 13.4 and a current macOS version, including real TCC microphone flow and offline inference. |
| CI visibility | OPEN | `.github/workflows/ci.yml` exists locally but remains ignored because the current GitHub OAuth token lacks `workflow` scope. Local equivalents all pass; remote enforcement still needs a credential authorized to publish workflow files. |

## Findings corrected in this audit

The Qwen3 migration correctly removed legacy choices from the UI, but a caller
could still pass an old ID directly to `download_llm` or `set_active_llm` because
the shared lookup included cleanup variants. Current-model lookup and
cleanup-only lookup are now separate. Old files can be deleted without making
the research-licensed Qwen2.5-3B part of the commercial catalog.

The new manifest used two nonexistent `Qwen3-*-Instruct` license URLs. They now
point to immutable revisions of the official `Qwen/Qwen3-1.7B` and
`Qwen/Qwen3-4B` repositories. The matching Apache-2.0 texts are checked in. The
Ollama pages identify digest `3d0b790…` as Qwen3 1.7B Q4_K_M and digest
`163553a…` as Qwen3 4B Q4_K_M.

The first signing implementation inserted `--timestamp` between `--options` and
its `runtime` value. The argument order is corrected. A failed submission now
points to the submit response when Apple returns no submission ID, and stale
production-named DMGs are removed before a commercial attempt so an old ad-hoc
artifact cannot be mistaken for a release.

Dependency notice collection now distinguishes runtime from build-only closure,
records source/authors, and supplies audited upstream texts for 27 crates whose
published archives omit standalone files. `audit-licenses.py` fails a commercial
run if a runtime grant/text is missing, Qwen evidence disagrees, or the
restricted legacy model becomes exposed again.

## Canonical commands and credential boundary

Local development package:

```sh
pnpm release:mac:dev
```

Commercial candidate:

```sh
pnpm release:mac
```

The commercial command builds and verifies everything before the credential
boundary. It then requires one valid keychain identity named `Developer ID
Application: …` with its private key. If several exist, select one through
`BELEGAL_LOCAL_DEMO_SIGNING_IDENTITY`. Notarization uses either a preconfigured
`BELEGAL_LOCAL_DEMO_NOTARY_PROFILE` or the three App Store Connect API environment
values documented in `MACOS_PORTABILITY.md`. Secrets stay outside the repository.

## Open risks after a credentialed build

- A real clean Mac must prove first-launch Gatekeeper, microphone TCC, model
  downloads, restart persistence, offline transcription/correction and PDF
  export. Automated inspection cannot substitute for this.
- Counsel/owner should accept the dependency/model obligations and determine
  codec patent exposure in the intended sales regions.
- Version 0.1.0 remains an alpha product marker. Do not relabel it 1.0 merely
  because packaging succeeds.
- Current CI does not produce or publish a signed artifact. Keep signing keys in
  a controlled release environment and add automated publication only after the
  manual credentialed flow passes.

## Release decision

`READY FOR SIGNING: YES`

`READY FOR COMMERCIAL DISTRIBUTION: NO — pending Developer ID/notarization and a
strict clean-Mac PASS.`
