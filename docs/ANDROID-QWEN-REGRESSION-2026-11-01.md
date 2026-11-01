# Android Qwen regression and performance, 2026-11-01

## Repository evidence

- `57f21bb` (Android 0.5.5): independent Qwen3-0.6B Q4_K_M, 396,705,472 bytes.
- `f14bc40`: added a mandatory GLiNER Multi pass and changed the mobile prompt.
- `e530b0d`: bundled GLiNER Multi/tokenizer into Qwen readiness and download.

## Final behavior in 0.5.12

Qwen downloads and loads only its verified 397 MB GGUF. GLiNER Small and Multi
remain independently selectable. No existing user GLiNER files are deleted.

The mobile extraction instruction is compact and has no example names that can
be copied into answers. Entity generation uses a JSON grammar through host
sampling, preventing malformed key/value syntax from causing repeated model
calls. This is restricted to mobile entity extraction; desktop/generic generation
retain their sampler path. Exact-source grounding, review and export gates remain
mandatory. A valid JSON object is not a guarantee of complete detection.

Removed the extra terminal-fragment overflow retry introduced in 0.5.10.
Overflow splits immediately, as before. Malformed/ungrounded responses retain
the bounded exact-copy retry. Cancellation and the three-minute deadline remain.

## Explicit comparison of the mobile execution profile

| Parameter | 0.5.5 | 0.5.12 |
| --- | --- | --- |
| Context | 4096 tokens | 4096 tokens |
| Source fragment | 1200 characters | 1200 characters |
| Overlap | 300 characters | 300 characters |
| Extraction response ceiling | 384 tokens | 384 tokens |
| Prefill batch / microbatch | 128 / 128 | 128 / 128 |
| Qwen distribution | top-k 20, top-p .8, temperature .25, seed 42 | same distribution, JSON-constrained |

## Real Android ARM64 emulator regression

All GLiNER assets were temporarily moved out of the model directory. Catalog
reported only Qwen installed, at 397 MB. They were restored after standalone QA.

The 3156-character repeated legal fixture initially required 18 prompt decodes,
expanded from four source fragments to seven, took 96.10 seconds, and remained
incomplete. A larger output budget alone did not repair the invalid JSON.

The final build, with the original 384-token ceiling, completes all four source
fragments in exactly four prompt decodes, in 34.33 seconds, without warnings:

| Fragment | Input tokens | Generated tokens observed |
| --- | --- | --- |
| 1 | 301 | 138 |
| 2 | 569 | 103 |
| 3 | 557 | 120 |
| 4 | 556 | 103 |

The short fixture completes in 7.19 seconds with one real decode. Its repeated
run uses the verified extraction cache (0.16 seconds); this is not an inference
speed claim. The native receipt workflow also passes manual review and PDF
export with Qwen alone. These are emulator measurements, not physical Redmi
measurements or measurements of the user's actual document.

Reproduce with a debug Android WebView forwarded to port 9223:

```
BLUEDOOR_QA_QWEN_ONLY=1 node tests/check-android-qwen.mjs
node tests/check-android-qwen-fragments.mjs
BLUEDOOR_ANDROID_QA_MODELS=qwen3-fast node tests/check-android-model-engines.mjs
```

The standalone tests require GLiNER assets to be absent in the synthetic emulator.
They check redaction of person/company/email/ID values and preservation of money.
Opt-in Rust regressions also require one generation per source fragment and
complete coverage. Native suite: 154 passed, 24 ignored; both opt-in real mobile
regressions pass. Format/Clippy, typecheck and build pass.

The former desktop card-size CI failure is fixed. The full UI run previously
timed out in `check-welcome.mjs`; no claim is made that full CI is green.

Local preview: `installers/Bluedoor-0.5.12-android-arm64-preview.apk`.
No push or publication performed.
