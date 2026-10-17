#!/usr/bin/env python3
"""Verify that the macOS bundle uses the checked-in Local Legal app icon."""
import hashlib
import plistlib
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
MASTER = ROOT / "assets/branding/local-legal-app-icon.png"
SOURCE_ICNS = ROOT / "src-tauri/icons/icon.icns"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def image_size(path: Path) -> tuple[int, int]:
    result = subprocess.run(
        ["sips", "-g", "pixelWidth", "-g", "pixelHeight", str(path)],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    values = {}
    for line in result.splitlines():
        if ": " in line:
            key, value = line.strip().split(": ", 1)
            values[key] = value
    return int(values["pixelWidth"]), int(values["pixelHeight"])


def verify(app: Path) -> None:
    expected = {
        MASTER: (1024, 1024),
        ROOT / "src-tauri/icons/icon.png": (512, 512),
        ROOT / "src-tauri/icons/32x32.png": (32, 32),
        ROOT / "src-tauri/icons/128x128.png": (128, 128),
        ROOT / "src-tauri/icons/128x128@2x.png": (256, 256),
    }
    for path, size in expected.items():
        if not path.is_file() or image_size(path) != size:
            raise SystemExit(f"Invalid icon asset or dimensions: {path} (expected {size})")
    if not SOURCE_ICNS.is_file() or SOURCE_ICNS.stat().st_size < 10_000:
        raise SystemExit(f"Missing or invalid macOS icon: {SOURCE_ICNS}")

    plist_path = app / "Contents/Info.plist"
    bundled = app / "Contents/Resources/icon.icns"
    if not plist_path.is_file() or not bundled.is_file():
        raise SystemExit("Bundle is missing Info.plist or icon.icns")
    with plist_path.open("rb") as handle:
        plist = plistlib.load(handle)
    if plist.get("CFBundleIconFile") != "icon.icns":
        raise SystemExit("CFBundleIconFile does not select icon.icns")
    if sha256(bundled) != sha256(SOURCE_ICNS):
        raise SystemExit("Bundled icon.icns differs from src-tauri/icons/icon.icns")
    print(
        "Icon verification PASS: master/PNGs valid and bundled icon.icns matches "
        f"source ({sha256(SOURCE_ICNS)})"
    )


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("Usage: scripts/verify-icons.py /path/to/Local Legal.app")
    verify(Path(sys.argv[1]).resolve())
