#!/usr/bin/env python3
"""Windows native loader smoke: app-owned ggml/llama DLLs load in-process."""
import ctypes
from pathlib import Path
import sys

dlls = Path(sys.argv[1]).resolve()
os.add_dll_directory(str(dlls)) if hasattr(os := __import__("os"), "add_dll_directory") else None

base = ctypes.CDLL(str(dlls / "ggml-base.dll"))
ggml = ctypes.CDLL(str(dlls / "ggml.dll"), mode=ctypes.RTLD_GLOBAL)
ggml.ggml_backend_load.argtypes = [ctypes.c_char_p]
ggml.ggml_backend_load.restype = ctypes.c_void_p
for name in ["ggml-cpu.dll"]:
    assert ggml.ggml_backend_load(bytes(str(dlls / name), "utf-8")), f"Cannot load {name}"
llama = ctypes.CDLL(str(dlls / "llama.dll"), mode=ctypes.RTLD_GLOBAL)
llama.llama_backend_init()
llama.llama_backend_free()
print("PASS: app-owned CPU ggml/llama DLLs initialize in-process without external backend search")