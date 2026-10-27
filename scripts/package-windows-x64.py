#!/usr/bin/env python3
"""Stage only this build's CPU runtime and app-local MSVC CRT for NSIS."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parent.parent


def pe_machine(path):
    with path.open('rb') as stream:
        if stream.read(2) != b'MZ':
            raise ValueError(f'Not a Windows PE: {path}')
        stream.seek(0x3c)
        offset = int.from_bytes(stream.read(4), 'little')
        stream.seek(offset)
        if stream.read(4) != b'PE\0\0':
            raise ValueError(f'Invalid PE: {path}')
        return int.from_bytes(stream.read(2), 'little')


def stage(target_dir, redist_dir):
    release = target_dir.resolve() / 'x86_64-pc-windows-msvc/release'
    candidates = list((release / 'build').glob('llama-cpp-sys-2-*/out'))
    candidates = [p for p in candidates if (p / 'bin/llama.dll').is_file()]
    if len(candidates) != 1:
        raise ValueError(f'Expected one fresh llama build, found {len(candidates)}; use a clean target directory')
    native = candidates[0]
    libraries = [*(native / 'bin').glob('*.dll'), *(native / 'backends').glob('*.dll')]
    # MSVC also needs its C/C++ runtime on office PCs without Visual Studio.
    crt = list(redist_dir.glob('x64/Microsoft.VC*.CRT'))
    if len(crt) != 1:
        raise ValueError(f'Expected one x64 CRT directory under {redist_dir}')
    libraries.extend(crt[0].glob('*.dll'))
    names = [p.name for p in libraries]
    for name in ['llama.dll', 'ggml.dll', 'ggml-base.dll', 'ggml-cpu-x64.dll', 'vcruntime140.dll', 'msvcp140.dll']:
        if name not in names:
            raise ValueError(f'Missing required runtime {name}')
    if len(set(names)) != len(names):
        raise ValueError('Ambiguous duplicate runtime DLLs')
    for path in libraries:
        if path.name.startswith('ggml-') and path.name not in ('ggml-base.dll',) and not path.name.startswith('ggml-cpu-'):
            raise ValueError(f'Non-CPU backend rejected: {path}')
        if pe_machine(path) != 0x8664:
            raise ValueError(f'Not x64: {path}')
        destination = release / path.name
        # Upstream may already have hard-linked the file into release.
        if not destination.exists() or not os.path.samefile(path, destination):
            shutil.copy2(path, destination)
        deps = release / 'deps'
        deps.mkdir(exist_ok=True)
        destination = deps / path.name
        if not destination.exists() or not os.path.samefile(path, destination):
            shutil.copy2(path, destination)
    runtime = ROOT / 'src-tauri/resources/runtime'
    if any(p.suffix.lower() in ('.gguf', '.bin') for p in runtime.rglob('*')):
        raise ValueError('Model weights must never be bundled')
    for name in ['ffmpeg', 'bluedoor-ocr', 'bluedoor-layout']:
        if (runtime / name).exists():
            raise ValueError(f'Foreign macOS runtime {name}; use a clean Windows build checkout')
    resources = {str(runtime): 'runtime/'}
    resources.update({str(release / p.name): p.name for p in libraries})
    config = target_dir.resolve() / 'bundle-x64.json'
    config.write_text(json.dumps({'bundle': {
        'targets': ['nsis'], 'resources': resources,
        'windows': {'webviewInstallMode': {'type': 'embedBootstrapper'},
                    'nsis': {'installMode': 'currentUser'}}
    }}, indent=2), encoding='utf-8')
    inventory = [{'file': p.name, 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()} for p in libraries]
    (target_dir / 'runtime-x64.json').write_text(json.dumps(inventory, indent=2), encoding='utf-8')
    print(config)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--target-dir', required=True, type=Path)
    parser.add_argument('--redist-dir', required=True, type=Path)
    args = parser.parse_args()
    stage(args.target_dir, args.redist_dir)
