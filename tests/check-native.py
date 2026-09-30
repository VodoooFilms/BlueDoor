#!/usr/bin/env python3
"""Developer-only native loader smoke, using the actual app-owned modules."""
import ctypes
from pathlib import Path
import sys

fw = Path(sys.argv[1]).resolve() / "Contents/Frameworks"
base = ctypes.CDLL(str(fw / "libggml-base.0.dylib"), mode=ctypes.RTLD_GLOBAL)
ggml = ctypes.CDLL(str(fw / "libggml.0.dylib"), mode=ctypes.RTLD_GLOBAL)
ggml.ggml_backend_load.argtypes = [ctypes.c_char_p]
ggml.ggml_backend_load.restype = ctypes.c_void_p
for name in ["libggml-cpu-apple_m1.so", "libggml-metal.so"]:
    assert ggml.ggml_backend_load(bytes(fw / name)), f"Cannot load {name}"
llama = ctypes.CDLL(str(fw / "libllama.0.dylib"), mode=ctypes.RTLD_GLOBAL)
llama.llama_backend_init()
llama.llama_backend_free()
print("PASS: app-owned CPU/Metal and llama initialize without external backend search")
