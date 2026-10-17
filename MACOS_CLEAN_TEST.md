# Strict clean-Mac commercial acceptance

Status: **NOT EXECUTED**. Run only with the notarized production DMG from
`pnpm release:mac`. Any Terminal command, quarantine removal, Homebrew package,
manual model copy, Rosetta or developer-tool install makes the primary journey
a failure.

## Test record

Record tester/date, Mac model/chip/RAM, exact macOS build, source commit, DMG
filename and SHA-256. Test at least macOS 13.4 and one current macOS release on
Apple Silicon. Use a fresh macOS user or machine with no prior Local Legal data.
Download the DMG through a browser so quarantine is authentic.

## Primary journey

1. Mount the DMG, drag Local Legal to Applications, eject it, then open from
   Applications. Gatekeeper must allow normal launch and show no missing-library
   or damaged-app dialog.
2. Start recording and follow the native microphone prompt. Record 30 seconds
   containing a name, date and number; stop once by click and once by keyboard.
3. From Modelos, download Whisper Large v3 Turbo (~1.62 GB) and Qwen3 1.7B
   (~1.27 GB). Progress, cancel/retry and insufficient-space errors must remain
   inside the app.
4. Transcribe, correct, edit and save. Names, numbers, language and meaning must
   survive correction. A failed/incomplete correction must preserve prior text.
5. Quit and reopen. Confirm history, original segments, corrected editor content,
   selected models, language and theme persist.
6. Import known-good PCM WAV, MP3, M4A/AAC, MP4/MOV with audio, FLAC, OGG,
   WebM/Opus, MKV/Opus, AIFF and WMA fixtures. A video without audio and a
   malformed file must produce a clear app error without a crash.
7. Export PDF through the save dialog and inspect text, Unicode, formatting,
   pagination and replacement of an existing destination.
8. Disable all networking after models are installed. Recording, transcription,
   correction, save/reopen, import and PDF export must still work.
9. Move the intact app through Finder to a folder containing spaces, accents and
   Japanese characters; repeat transcription, correction and one compressed
   import.

## Recovery checks

| Scenario | Expected result |
|---|---|
| Deny microphone, then enable it in System Settings | Clear guidance; no stuck recording; retry works, with relaunch only if macOS requires it. |
| Cancel or interrupt a model download | Partial data never appears installed; retry succeeds; existing complete model remains intact. |
| Missing/truncated/same-size-corrupt model in an isolated QA account | App refuses inference and offers in-app remove/download repair; no crash or user-document change. |
| Low disk space on a safe test volume | Preflight/write error is understandable and existing models/documents remain intact. |
| Corrupt history in an isolated account | Original bytes remain preserved; new autosave is blocked and the error remains visible. |
| Incomplete app copy created before signing in QA | Release verifier fails. No incomplete package may reach customers. |

## Acceptance rule

Every primary step must be PASS with screenshots or a short observation log.
Mark unsafe recovery simulations NOT TESTED; never run them against production
data. Commercial acceptance requires the notarization log to say Accepted,
stapler validation to pass, local `spctl` checks to pass, and this clean-Mac
journey to pass without workarounds.
