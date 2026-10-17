#!/usr/bin/env python3
"""Copy the proven llama/ggml runtime into a macOS app; no model weights bundled."""
from pathlib import Path
import importlib.util
import hashlib
import re
import shutil
import subprocess
import sys

FRAMEWORKS_RPATH = '@executable_path/../Frameworks'


def required_backends(root):
    """Backend module names the app insists on, read from platform.rs itself.

    platform.rs refuses to run the model when any of these files is absent in
    Contents/Frameworks, so the packaging step must satisfy that list instead
    of trusting whatever the build happened to emit.
    """
    source = (root / 'src-tauri/src/platform.rs').read_text()
    names = sorted(set(re.findall(r'"(libggml[A-Za-z0-9_.-]*\.so)"', source)))
    if not names:
        raise RuntimeError('Could not read the required backend list from platform.rs')
    return names


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
    main = app / 'Contents/MacOS/local-legal'
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
    root = Path(__file__).resolve().parent.parent
    missing = [name for name in required_backends(root) if not (frameworks / name).is_file()]
    if missing:
        raise RuntimeError('Bundle lacks the backends platform.rs requires: ' + ', '.join(missing))
    main = app / 'Contents/MacOS/local-legal'
    for binary in verifier.macho_files(app):
        for path in verifier.rpaths(binary):
            if path.startswith('/'):
                subprocess.run(['install_name_tool', '-delete_rpath', path, str(binary)], check=True)
        if FRAMEWORKS_RPATH not in verifier.rpaths(binary):
            subprocess.run(['install_name_tool', '-add_rpath', FRAMEWORKS_RPATH, str(binary)], check=True)


if __name__ == '__main__':
    if len(sys.argv) != 3:
        raise SystemExit('Usage: package-local-ai.py APP BUILD_DIR')
    prepare(Path(sys.argv[1]), Path(sys.argv[2]))
