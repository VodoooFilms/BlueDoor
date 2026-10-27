#!/usr/bin/env python3
"""Minimal release gate for Windows installers and the native build output."""
import argparse
from pathlib import Path
import sys
import importlib.util

spec = importlib.util.spec_from_file_location("windows_package", Path(__file__).with_name("package-windows-x64.py"))
windows_package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(windows_package)
pe_machine = windows_package.pe_machine


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
    executable = release / "bluedoor.exe"
    if not executable.is_file():
        fail(f"missing executable: {executable}")
    bundle = release / "bundle"
    installers = [*bundle.rglob("*.exe"), *bundle.rglob("*.msi")]
    if not installers:
        fail("neither NSIS nor MSI installer was produced")
    if args.architecture == "x64":
        if not list(bundle.glob("nsis/*-setup.exe")):
            fail("missing x64 NSIS .exe installer")
        for binary in [executable, *release.glob("*.dll")]:
            if pe_machine(binary) != 0x8664:
                fail(f"not Windows x64: {binary}")
        for name in ["llama.dll", "ggml.dll", "ggml-base.dll", "ggml-cpu-x64.dll", "vcruntime140.dll", "msvcp140.dll"]:
            if not (release / name).is_file():
                fail(f"missing app-local runtime: {name}")
        runtime = Path(__file__).resolve().parent.parent / "src-tauri/resources/runtime"
        if pe_machine(runtime / "ffmpeg.exe") != 0x8664:
            fail("decoder is not Windows x64")
        if any(p.suffix.lower() in (".gguf", ".bin") for p in runtime.rglob("*")):
            fail("model weights must never be bundled")
    print(f"Windows bundle verification PASS: {args.architecture}, executable and installers present")


if __name__ == "__main__":
    main()
