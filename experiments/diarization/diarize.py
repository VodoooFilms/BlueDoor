#!/usr/bin/env python3
"""Local VoiceToPDF experiment: Whisper words + Nemotron 3 speaker activity.

Requires faster-whisper, ffmpeg, a current NeMo-Speech.cpp CLI, and the
Nemotron-3-Diarization GGUF. No audio or transcript is sent to a server.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import tempfile
from pathlib import Path

from faster_whisper import WhisperModel


def run_checked(args: list[str]) -> None:
    result = subprocess.run(args, capture_output=True, text=True, check=False)
    if result.returncode:
        raise RuntimeError(f"{' '.join(args[:2])} failed: {result.stderr.strip()}")


def speaker_for_word(word: dict, intervals: list[dict]) -> tuple[int | None, bool]:
    """Choose the largest temporal overlap; retain ambiguity for simultaneous talk."""
    if word["end"] <= word["start"]:
        containing = [
            int(interval["speaker"])
            for interval in intervals
            if interval["start"] - 0.05 <= word["start"] <= interval["end"] + 0.05
        ]
        return (containing[0], len(set(containing)) > 1) if containing else (None, False)
    scores: dict[int, float] = {}
    active: list[dict] = []
    for interval in intervals:
        overlap = max(
            0.0,
            min(word["end"], interval["end"]) - max(word["start"], interval["start"]),
        )
        if overlap:
            active.append(interval)
            speaker = int(interval["speaker"])
            scores[speaker] = scores.get(speaker, 0.0) + overlap
    if not scores:
        return None, False
    ranked = sorted(scores.items(), key=lambda entry: entry[1], reverse=True)
    # A word crossing a turn boundary is not itself overlapping speech.
    ambiguous = any(
        left["speaker"] != right["speaker"]
        and min(word["end"], left["end"], right["end"])
        - max(word["start"], left["start"], right["start"]) >= 0.05
        for index, left in enumerate(active)
        for right in active[index + 1 :]
    )
    return ranked[0][0], ambiguous


def group_words(words: list[dict]) -> list[dict]:
    turns: list[dict] = []
    for word in words:
        if (
            turns
            and turns[-1]["speaker"] == word["speaker"]
            and turns[-1]["overlap"] == word["overlap"]
            and word["start"] - turns[-1]["end"] <= 1.0
        ):
            turns[-1]["text"] += word["text"]
            turns[-1]["end"] = word["end"]
        else:
            turns.append(
                {
                    "start": word["start"],
                    "end": word["end"],
                    "speaker": word["speaker"],
                    "overlap": word["overlap"],
                    "text": word["text"],
                }
            )
    for turn in turns:
        turn["text"] = turn["text"].strip()
    return turns


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("audio", type=Path)
    parser.add_argument("--nemo-bin", type=Path, required=True)
    parser.add_argument("--diar-model", type=Path, required=True)
    parser.add_argument("--whisper-model", required=True, help="Local faster-whisper model directory")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--language", default=None, help="For example es; omit for automatic detection")
    parser.add_argument("--device", default="metal", choices=["metal", "cpu"])
    args = parser.parse_args()

    for path in (args.audio, args.nemo_bin, args.diar_model):
        if not path.is_file():
            parser.error(f"Missing file: {path}")
    if not Path(args.whisper_model).is_dir():
        parser.error(f"Missing Whisper model directory: {args.whisper_model}")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="belegal-local-demo-diar-") as temp:
        wav = Path(temp) / "audio.wav"
        diar_json = Path(temp) / "diarization.json"
        run_checked(
            [
                "ffmpeg", "-nostdin", "-y", "-loglevel", "error", "-i", str(args.audio),
                "-vn", "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le", str(wav),
            ]
        )
        run_checked(
            [
                str(args.nemo_bin), "diarize", str(wav), "--model", str(args.diar_model),
                "--device", args.device, "--format", "json", "--output", str(diar_json),
            ]
        )
        intervals = json.loads(diar_json.read_text())["segments"]
        recognizer = WhisperModel(args.whisper_model, device="cpu", compute_type="int8")
        segments, info = recognizer.transcribe(
            str(wav), language=args.language, word_timestamps=True, vad_filter=False
        )
        words = []
        for segment in segments:
            for item in segment.words or []:
                word = {"start": float(item.start), "end": float(item.end), "text": item.word}
                word["speaker"], word["overlap"] = speaker_for_word(word, intervals)
                words.append(word)

    turns = group_words(words)
    payload = {
        "audio": str(args.audio.resolve()),
        "language": info.language,
        "speaker_count": len({item["speaker"] for item in intervals}),
        "speaker_intervals": intervals,
        "words": words,
        "turns": turns,
    }
    args.output.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n")
    transcript = args.output.with_suffix(".txt")
    transcript.write_text(
        "\n".join(
            f"[{turn['start']:.1f}-{turn['end']:.1f}] "
            f"Hablante {turn['speaker'] if turn['speaker'] is not None else '?'}"
            f"{' (solapamiento)' if turn['overlap'] else ''}: {turn['text']}"
            for turn in turns
        ) + "\n"
    )
    print(f"Detected {payload['speaker_count']} speakers; wrote {args.output} and {transcript}")


if __name__ == "__main__":
    main()
