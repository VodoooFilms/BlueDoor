#!/usr/bin/env python3
"""Copy the proven llama/ggml runtime into a macOS app; no model weights bundled."""
from pathlib import Path
import importlib.util
import hashlib
import shutil
import subprocess
import sys


def prepare(app, build):
    spec = importlib.util.spec_from_file_location('bundle_verifier', Path(__file__).with_name('verify-bundle.py'))
    verifier = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(verifier)
    frameworks = app / 'Contents/Frameworks'
    frameworks.mkdir(exist_ok=True)
    libraries = sorted([*build.glob('libllama*.dylib'), *build.glob('libggml*.dylib')])
    if not libraries:
        raise RuntimeError('Missing compiled local AI libraries')
    for library in libraries:
        target = frameworks / library.name
        if target.is_symlink():
            target.unlink()
        shutil.copy2(library, target)
    # Use the current build's emitted modules, not an arbitrary system directory.
    candidates = list((build / 'build').glob('llama-cpp-sys-2-*/out/backends'))
    main = app / 'Contents/MacOS/belegal-local-demo'
    binary_data = main.read_bytes()
    candidates = [p for p in candidates if (p / 'libggml-metal.so').is_file()]
    embedded = [p for p in candidates if str(p.resolve()).encode() in binary_data]
    if embedded:
        candidates = embedded
    else:
        # Release builds omit the development fallback path. Match the exact
        # dylib produced by this build, rather than picking a stale backend.
        expected = hashlib.sha256((build / 'libllama.0.dylib').read_bytes()).digest()
        candidates = [p for p in candidates
                      if (p.parent / 'lib/libllama.0.dylib').is_file()
                      and hashlib.sha256((p.parent / 'lib/libllama.0.dylib').read_bytes()).digest() == expected]
    if len(candidates) != 1:
        raise RuntimeError('Expected exactly one built llama backend directory')
    for module in candidates[0].glob('*.so'):
        shutil.copy2(module, frameworks / module.name)
    main = app / 'Contents/MacOS/belegal-local-demo'
    for binary in verifier.macho_files(app):
        for path in verifier.rpaths(binary):
            if path.startswith('/'):
                subprocess.run(['install_name_tool', '-delete_rpath', path, str(binary)], check=True)
    if '@executable_path/../Frameworks' not in verifier.rpaths(main):
        subprocess.run(['install_name_tool', '-add_rpath', '@executable_path/../Frameworks', str(main)], check=True)


if __name__ == '__main__':
    if len(sys.argv) != 3:
        raise SystemExit('Usage: package-local-ai.py APP BUILD_DIR')
    prepare(Path(sys.argv[1]), Path(sys.argv[2]))
