#!/usr/bin/env python3
"""Make Tauri's debug .app launchable using the project's native bundle rules."""

import importlib.util
import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEBUG = ROOT / "src-tauri/target/debug"
APP = DEBUG / "bundle/macos/Local Legal Diarization Test.app"

spec = importlib.util.spec_from_file_location("release_mac", ROOT / "scripts/release-mac.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)
release.APP = APP

frameworks = APP / "Contents/Frameworks"
frameworks.mkdir(exist_ok=True)
for native in [*DEBUG.glob("libllama*.dylib"), *DEBUG.glob("libggml*.dylib")]:
    shutil.copy2(native, frameworks / native.name)
backends = sorted((DEBUG / "build").glob("llama-cpp-sys-2-*/out/backends"), key=lambda p: p.stat().st_mtime)
if not backends:
    raise SystemExit("Native ggml backends missing")
for native in backends[-1].glob("*.so"):
    shutil.copy2(native, frameworks / native.name)

verifier = release.load_verifier()
binary = APP / "Contents/MacOS/local-legal"
for native in verifier.macho_files(APP):
    for rpath in verifier.rpaths(native):
        if rpath.startswith("/"):
            subprocess.run(["install_name_tool", "-delete_rpath", rpath, str(native)], check=True)
if "@executable_path/../Frameworks" not in verifier.rpaths(binary):
    subprocess.run(
        ["install_name_tool", "-add_rpath", "@executable_path/../Frameworks", str(binary)],
        check=True,
    )
release.sign_bundle(verifier, binary, "-", hardened_runtime=False)
print(APP)
