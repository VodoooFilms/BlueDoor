#!/usr/bin/env python3
"""Package an already verified macOS app without rebuilding native dependencies."""
import argparse
import importlib.util
from pathlib import Path
import subprocess
import sys


def main():
    root = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("This installer requires macOS")
    app, output = args.app.resolve(), args.output.resolve()
    if not app.is_dir() or app.suffix != ".app":
        parser.error("--app must point to an existing app bundle")
    if output.suffix != ".dmg" or output.exists():
        parser.error("--output must be a new .dmg file")
    subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app)], check=True)
    subprocess.run([sys.executable, str(root / "scripts/verify-bundle.py"), str(app)], check=True)
    spec = importlib.util.spec_from_file_location("release_mac", root / "scripts/release-mac.py")
    release = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(release)
    release.APP = app
    release.create_dmg(output)
    subprocess.run(["hdiutil", "verify", str(output)], check=True)
    digest = release.write_checksum(output)
    print(f"Local installer: {output}\nSHA-256: {digest}")
    print("Uses the existing app signature. This operation does not notarize the app.")


if __name__ == "__main__":
    main()
