#!/usr/bin/env python3
"""Developer fixture generator + tests of the exact bundled decoder (no user tooling)."""
import argparse
from pathlib import Path
import math
import struct
import subprocess
import tempfile
import wave

parser = argparse.ArgumentParser()
parser.add_argument("--encoder", required=True, help="Developer-only full encoder used to create fixtures")
parser.add_argument("--decoder", default="src-tauri/resources/runtime/ffmpeg")
args = parser.parse_args()
decoder = str(Path(args.decoder).resolve())
with tempfile.TemporaryDirectory(prefix="vtp-media-") as directory:
    root = Path(directory)
    source = root / "source.wav"
    with wave.open(str(source), "wb") as wav:
        wav.setparams((1, 2, 16000, 16000, "NONE", "not compressed"))
        wav.writeframes(b"".join(struct.pack("<h", int(10000 * math.sin(i * 2 * math.pi * 440 / 16000))) for i in range(16000)))
    formats = [("wav", "pcm_s24le"), ("mp3", "libmp3lame"), ("m4a", "aac"), ("aac", "aac"), ("mp4", "aac"), ("mov", "aac"), ("flac", "flac"), ("ogg", "vorbis"), ("webm", "libopus"), ("mkv", "libopus"), ("wma", "wmav2"), ("aiff", "pcm_s16be")]
    for ext, codec in formats:
        media = root / ("voz á 日本語 space ' ; $() " + "long-" * 15 + "." + ext)
        subprocess.run([args.encoder, "-nostdin", "-loglevel", "error", "-i", str(source), "-ac", "2", "-c:a", codec, "-strict", "experimental", str(media)], check=True)
        output = root / "decoded.wav"
        subprocess.run([decoder, "-nostdin", "-y", "-loglevel", "error", "-protocol_whitelist", "file", "-i", str(media), "-map", "0:a:0", "-vn", "-sn", "-dn", "-ar", "16000", "-ac", "1", "-f", "wav", "-acodec", "pcm_s16le", str(output)], check=True, env={"PATH": "/usr/bin:/bin"})
        with wave.open(str(output), "rb") as wav:
            assert wav.getframerate() == 16000 and wav.getnchannels() == 1 and wav.getsampwidth() == 2
            assert wav.getnframes() >= 14000
        print("PASS", ext, "with Unicode/spaces/special characters")
    invalid = root / "invalid.mp3"
    invalid.write_text("not media")
    result = subprocess.run([decoder, "-nostdin", "-loglevel", "error", "-i", str(invalid), "-f", "wav", str(root / "bad.wav")], capture_output=True)
    assert result.returncode != 0
    protocols = subprocess.check_output([decoder, "-hide_banner", "-protocols"], text=True)
    assert {s.strip() for s in protocols.splitlines() if s.startswith("  ")} == {"file"}
    print("PASS invalid media rejected; only file protocol compiled")
