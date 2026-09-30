# Local speaker diarization experiment

This prototype runs three local steps: convert audio to mono 16 kHz WAV, run
Nemotron 3 Diarization with NeMo-Speech.cpp, then transcribe with Whisper using
word timestamps. It assigns each word to the speaker interval with the largest
time overlap. JSON retains the original words, speaker intervals, and ambiguous
overlaps; TXT is a readable draft. Any later correction step should process the
TXT only after the speaker labels have been reviewed, and must preserve them.

The standalone script uses `faster-whisper` for an independent baseline. The
desktop app keeps its existing `whisper-rs` engine; the Qwen correction stage was
removed in 0.2.0, so editing is manual. The Models view lists Nemotron below
Whisper, with download and delete controls. Speaker detection is **opt-in**: it
only runs when the user turns on "Identificar voces" *and* a current
NeMo-Speech.cpp runtime is configured, so having the model installed is not
enough. Deleting the model returns new transcriptions to Whisper alone. The
commercial bundle does not include the NeMo-Speech.cpp runtime.

## Local run

Use a recent NeMo-Speech.cpp build. Its v0.1.0 macOS binary can identify the
GGUF but fails on Nemotron 3 with `pre_ln transformer variant is not supported`.

Everything heavy lives in `local/`, which is ignored by git. The venv cannot be
moved after creation (absolute paths inside), so recreate it with `uv` if the
folder is relocated.

```sh
cd experiments/diarization
uv venv local/.venv --python 3.11
uv pip install --python local/.venv/bin/python faster-whisper
local/.venv/bin/python diarize.py input.wav \
  --nemo-bin local/nemo-speech-src/build/metal-diar/bin/nemo-speech \
  --diar-model local/models/Nemotron-3-Diarization.q8_0.gguf \
  --whisper-model /absolute/path/to/faster-whisper-small \
  --output /absolute/path/to/result.json
```

`local/` layout:

| Path | What it is | Notes |
| --- | --- | --- |
| `local/.venv` | Python env with `faster-whisper` | Recreate if moved. |
| `local/nemo-speech-src` | NeMo-Speech.cpp checkout (own `.git`) plus `build/metal-diar` | This is the build that works. |
| `local/bin/nemo-speech` | Upstream v0.1.0 install | **Does not work**: `pre_ln transformer variant is not supported`. |
| `local/models` | `Nemotron-3-Diarization.q8_0.gguf` | Symlink to the copy the app downloaded, so it is not duplicated. |
| `local/sample` | Test audio and previous results | `two-speakers-long.wav` is the 1 → 2 → 1 → 2 check. |

The `build/metal-diar/bin/nemo-speech` binary embeds its `@rpath`. If the folder
moves, fix it or dyld cannot load `libnemo_speech_asr.dylib`:

```sh
BIN=experiments/diarization/local/nemo-speech-src/build/metal-diar/bin
install_name_tool -add_rpath "$PWD/$BIN" "$BIN/nemo-speech"
codesign --force --sign - "$BIN/nemo-speech"
```

For product integration, bundle a current native runtime for each supported
platform, keep speaker IDs in the stored `Segment` schema, and preserve labels
through the editor and PDF export. Evaluate real recordings with two to eight speakers and overlap
before enabling the option by default.

## Desktop prototype

```sh
export BELEGAL_LOCAL_DEMO_NEMO_BIN="$PWD/experiments/diarization/local/nemo-speech-src/build/metal-diar/bin/nemo-speech"
pnpm tauri dev --config src-tauri/tauri.diarization.conf.json
```

The config gives the prototype a separate app identity and data directory. Put
a verified Whisper model in that app's `models` directory or download one
through its Models screen. Download Nemotron there as well; its SHA-256 is
verified against the pinned NVIDIA GGUF in `model-manifest.json`.

The app runs the diarizer on the same decoded 16 kHz mono samples used by
Whisper. With speaker mode enabled, it transcribes each detected speaker turn
separately with Whisper, so language detection can adapt to bilingual turns.
Stored segments gain
an optional `speaker` field, so old saved transcripts remain readable. The
backend prints the `Hablante N:` label only when the speaker changes, the editor
normalizes older transcripts, and per-transcript `speakerNames` let the user
replace the label with a real name that is reused in the editor and the PDF.

### Verified on this Mac (2026-09-24)

- Apple Silicon, 16 GB RAM; NeMo-Speech.cpp commit `97a15afa` compiled with
  `metal-diar`; Nemotron 3 Q8 GGUF from the official NVIDIA model repository.
- A generated 43-second, four-turn test recording produced the speaker sequence
  1 → 2 → 1 → 2. The independent Whisper Small pipeline also detected two
  speakers and produced JSON/TXT output.
- NeMo-Speech.cpp v0.1.0 recognized the model file but could not execute it
  (`pre_ln transformer variant is not supported`); use a current build.
- Synthetic voices and bilingual audio are insufficient to approve production
  accuracy. In the mixed Spanish/English test, Whisper sometimes transcribed
  English as Spanish because it chose one language for the full recording.

### Before shipping

1. Pin and verify the native runtime and model for macOS and Windows ARM64/x64;
   review the Nemotron OpenMDW 1.1 terms and add attribution/notices.
2. Keep the opt-in switch off by default and limited to installations with the
   verified model and native runtime until real-audio quality is measured.
3. Evaluate speaker error, missed words, overlaps, memory, and processing time
   on Spanish meetings with 2–8 participants. For severe overlap, compare the
   coupled Nemotron 3 + Nemotron 3.5 ASR pipeline against Whisper.
