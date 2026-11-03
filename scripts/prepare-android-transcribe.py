#!/usr/bin/env python3
"""Build pinned transcribe.cpp with a private static ggml symbol namespace.

llama.cpp and transcribe.cpp embed different ggml versions. Android links both
statically, so neither may resolve symbols against the other's implementation.
Only generated archives are transformed; registry sources remain unchanged.
"""
import json
import os
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parent.parent
cargo = Path(os.environ.get('CARGO_HOME', Path.home() / '.cargo'))
sources = list((cargo / 'registry/src').glob('*/transcribe-cpp-sys-0.2.4'))
if len(sources) != 1:
    raise SystemExit('Run cargo fetch first: expected pinned transcribe-cpp-sys 0.2.4 source')
ndk = Path(os.environ['NDK_HOME'])
host = 'darwin-x86_64' if os.uname().sysname == 'Darwin' else 'linux-x86_64'
tools = ndk / 'toolchains/llvm/prebuilt' / host / 'bin'
out = root / 'src-tauri/target/android-transcribe-arm64'
build = out / 'build'
prefix = out / 'install'
marker = out / 'private-symbols-v1'
if marker.exists():
    raise SystemExit(0)
subprocess.run(['cmake', '-S', str(sources[0]), '-B', str(build),
    f'-DCMAKE_TOOLCHAIN_FILE={ndk}/build/cmake/android.toolchain.cmake',
    '-DANDROID_ABI=arm64-v8a', '-DANDROID_PLATFORM=android-24',
    '-DANDROID_STL=c++_static', '-DCMAKE_BUILD_TYPE=Release',
    '-DTRANSCRIBE_INSTALL=ON', '-DTRANSCRIBE_BUILD_SHARED=OFF',
    '-DTRANSCRIBE_BUILD_TESTS=OFF', '-DTRANSCRIBE_BUILD_EXAMPLES=OFF',
    '-DTRANSCRIBE_USE_OPENMP=OFF', '-DTRANSCRIBE_USE_SYSTEM_BLAS=OFF',
    '-DTRANSCRIBE_METAL=OFF', '-DGGML_NATIVE=OFF',
    f'-DCMAKE_INSTALL_PREFIX={prefix}'], check=True)
subprocess.run(['cmake', '--build', str(build), '--target', 'install', '-j', '4'], check=True)
archives = sorted((prefix / 'lib').glob('*.a'))
if not archives:
    raise SystemExit('No transcribe archives installed')
symbols = set()
for archive in archives:
    listing = subprocess.check_output([str(tools / 'llvm-nm'), '--defined-only', '--extern-only', '--format=posix', str(archive)], text=True)
    for line in listing.splitlines():
        parts = line.split()
        if len(parts) < 3 or parts[1] not in ('T', 'D', 'B', 'R', 'W', 'V', 'S'):
            continue
        name = parts[0]
        if name.startswith('transcribe_') or name.startswith('__'):
            continue
        if name.startswith('_Z') and 'ggml' not in name and 'gguf' not in name:
            continue
        symbols.add(name)
renames = out / 'symbols.map'
renames.write_text(''.join(f'{name} bluedoor_stt_{name}\n' for name in sorted(symbols)))
for archive in archives:
    subprocess.run([str(tools / 'llvm-objcopy'), f'--redefine-syms={renames}', str(archive)], check=True)
manifest_path = prefix / 'lib/transcribe-link.json'
manifest = json.loads(manifest_path.read_text())
# Bionic implements pthread in libc, and Android uses libc++ rather than GNU stdc++.
manifest['system_libs'] = ['c++_static', 'c++abi', 'm', 'dl']
manifest_path.write_text(json.dumps(manifest, indent=2) + '\n')
marker.write_text('transcribe-cpp-sys 0.2.4; private static symbols\n')
print(f'Prepared isolated Android transcription engine ({len(symbols)} symbols)')
