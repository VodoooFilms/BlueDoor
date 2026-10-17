#!/usr/bin/env python3
"""Release gate: inspect every Mach-O (including resources), not just .dylib names."""
import json
import hashlib
import os
from pathlib import Path
import plistlib
import re
import subprocess
import sys

MAGIC = {bytes.fromhex(x) for x in ("feedface", "cefaedfe", "feedfacf", "cffaedfe", "cafebabe", "bebafeca", "cafebabf")}


def macho_files(app):
    return [p for p in app.rglob("*") if p.is_file() and p.open("rb").read(4) in MAGIC]


def rpaths(binary):
    text = subprocess.check_output(["otool", "-l", str(binary)], text=True)
    return re.findall(r"cmd LC_RPATH\s+cmdsize \d+\s+path (.*?) \(offset", text)


def verify(app):
    app = app.resolve()
    main = app / "Contents/MacOS/bluedoor"
    resources = app / "Contents/Resources"
    errors = []
    required = [main, resources / "runtime/ffmpeg", resources / "runtime/bluedoor-ocr", resources / "runtime/bluedoor-layout", resources / "runtime/ffmpeg-build.json",
                resources / "runtime/FFmpeg-LICENSE.txt", resources / "runtime/ffmpeg-source.tar.xz",
                resources / "runtime/THIRD_PARTY_NOTICES.md", resources / "runtime/licenses/inventory.json"]
    for p in required:
        if not p.is_file(): errors.append(f"Missing resource: {p.relative_to(app)}")
    if not main.is_file(): return errors
    with (app / "Contents/Info.plist").open("rb") as f:
        info = plistlib.load(f)
    if info.get("CFBundleExecutable") != "bluedoor": errors.append("Unexpected bundle executable")
    if info.get("LSMinimumSystemVersion") != "13.4": errors.append("Unexpected deployment target")
    for p in app.rglob("*"):
        if p.is_symlink() and not p.resolve().is_relative_to(app): errors.append(f"External bundle symlink: {p}")
    main_rpaths = rpaths(main)
    def expand(value, binary):
        for prefix, base in [("@loader_path/", binary.parent), ("@executable_path/", main.parent)]:
            if value.startswith(prefix): return (base / value[len(prefix):]).resolve()
        return Path(value).resolve() if value.startswith("/") else None
    binaries = macho_files(app)
    for binary in binaries:
        arch = subprocess.check_output(["lipo", "-archs", str(binary)], text=True).split()
        if arch != ["arm64"]: errors.append(f"{binary.name}: expected only arm64, found {arch}")
        if not os.access(binary, os.X_OK) and binary in {resources / "runtime/ffmpeg", resources / "runtime/bluedoor-ocr"}: errors.append(f"Runtime helper is not executable: {binary.name}")
        loads = subprocess.check_output(["otool", "-l", str(binary)], text=True)
        minimums = re.findall(r"\bminos ([\d.]+)", loads) + re.findall(r"cmd LC_VERSION_MIN_MACOSX\s+cmdsize \d+\s+version ([\d.]+)", loads)
        if not minimums: errors.append(f"{binary.name}: missing deployment metadata")
        for minimum in minimums:
            if tuple(map(int, minimum.split('.'))) > (13, 4, 0): errors.append(f"{binary.name}: requires macOS {minimum}, above 13.4")
        paths = rpaths(binary)
        for value in paths:
            resolved = expand(value, binary)
            if resolved is None or not resolved.is_relative_to(app): errors.append(f"{binary.name}: forbidden LC_RPATH {value}")
        deps = subprocess.check_output(["otool", "-L", str(binary)], text=True)
        for line in deps.splitlines()[1:]:
            dep = line.strip().split(" (compatibility version", 1)[0]
            if dep.startswith(("/System/Library/", "/usr/lib/")): continue
            candidates = []
            if dep.startswith("@rpath/"):
                for value, owner in [(v, binary) for v in paths] + [(v, main) for v in main_rpaths]:
                    base = expand(value, owner)
                    if base is not None: candidates.append((base / dep[len("@rpath/"):]).resolve())
            else:
                resolved = expand(dep, binary)
                if resolved is not None: candidates.append(resolved)
            if dep.startswith("/") or not any(p.is_relative_to(app) and p.is_file() for p in candidates):
                errors.append(f"{binary.name}: forbidden/unresolved dependency {dep}")
    decoder = resources / "runtime/ffmpeg"
    if decoder.is_file() and not errors:
        metadata = json.loads((resources / "runtime/ffmpeg-build.json").read_text())
        source_hash = hashlib.sha256((resources / "runtime/ffmpeg-source.tar.xz").read_bytes()).hexdigest()
        if source_hash != metadata["sha256"]: errors.append("Decoder source archive integrity mismatch")
        version = subprocess.check_output([str(decoder), "-version"], text=True)
        if f"ffmpeg version {metadata['version']} " not in version: errors.append("Decoder version mismatch")
        protocols = subprocess.check_output([str(decoder), "-hide_banner", "-protocols"], text=True)
        active = [line.strip() for line in protocols.splitlines() if line.startswith("  ")]
        if set(active) != {"file"}: errors.append(f"Decoder protocols must be file-only: {active}")
        license_text = subprocess.check_output([str(decoder), "-hide_banner", "-L"], text=True, stderr=subprocess.STDOUT)
        if "GNU Lesser General Public License" not in " ".join(license_text.split()): errors.append("Unexpected decoder license")
    return errors


if __name__ == "__main__":
    if len(sys.argv) != 2: sys.exit("Usage: verify-bundle.py /path/to/BlueDoor.app")
    failures = verify(Path(sys.argv[1]))
    if failures: sys.exit("Bundle verification FAILED:\n" + "\n".join(failures))
    print("Bundle verification PASS: arm64, resources, deployment target, local dependencies and file-only decoder")
