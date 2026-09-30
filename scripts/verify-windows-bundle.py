#!/usr/bin/env python3
"""Minimal release gate for Windows installers and the native build output."""
import argparse
from pathlib import Path
import sys


def fail(message: str) -> None:
    raise SystemExit(f"Windows bundle verification FAILED: {message}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--target-dir", required=True)
    parser.add_argument("--architecture", choices=("arm64", "x64"), required=True)
    args = parser.parse_args()
    release = Path(args.target_dir) / (
        "aarch64-pc-windows-msvc" if args.architecture == "arm64" else "x86_64-pc-windows-msvc"
    ) / "release"
    executable = release / "belegal-local-demo.exe"
    if not executable.is_file():
        fail(f"missing executable: {executable}")
    bundle = release / "bundle"
    installers = [*bundle.rglob("*.exe"), *bundle.rglob("*.msi")]
    if not installers:
        fail("neither NSIS nor MSI installer was produced")
    print(f"Windows bundle verification PASS: {args.architecture}, executable and installers present")


if __name__ == "__main__":
    main()
