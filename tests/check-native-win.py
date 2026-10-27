#!/usr/bin/env python3
"""Windows native loader smoke: only app-owned, compatible CPU DLLs."""
import ctypes
import os
from pathlib import Path
import sys

root = Path(sys.argv[1]).resolve()
# Keep this handle alive: otherwise Windows immediately removes the directory.
dll_directory = os.add_dll_directory(str(root))
base = ctypes.CDLL(str(root / 'ggml-base.dll'))
ggml = ctypes.CDLL(str(root / 'ggml.dll'))
ggml.ggml_backend_load.argtypes = [ctypes.c_char_p]
ggml.ggml_backend_load.restype = ctypes.c_void_p
variants = []
for path in root.glob('ggml-cpu-*.dll'):
    module = ctypes.CDLL(str(path))
    module.ggml_backend_score.restype = ctypes.c_int
    score = module.ggml_backend_score()
    if score > 0:
        variants.append((score, path))
if variants:
    _, cpu = max(variants)
else:
    cpu = root / 'ggml-cpu.dll'  # Existing ARM64 posture.
assert ggml.ggml_backend_load(str(cpu).encode('utf-8')), f'Cannot load {cpu}'
llama = ctypes.CDLL(str(root / 'llama.dll'))
llama.llama_backend_init()
llama.llama_backend_free()
print(f'PASS: app-owned CPU initialized: {cpu.name}')
