#!/usr/bin/env python3
"""Positive and fault-injection checks of the real built bundle gate."""
import importlib.util
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("verify_bundle", root / "scripts/verify-bundle.py")
verifier = importlib.util.module_from_spec(spec)
spec.loader.exec_module(verifier)
original = Path(sys.argv[1]).resolve()
assert not verifier.verify(original)
with tempfile.TemporaryDirectory(prefix="vtp-bundle-") as directory:
    app = Path(directory) / "Espacios á 日本語.app"
    shutil.copytree(original, app)
    assert not verifier.verify(app)
    decoder = app / "Contents/Resources/runtime/ffmpeg"
    saved = decoder.read_bytes()
    decoder.unlink()
    assert any("Missing resource" in e for e in verifier.verify(app))
    decoder.write_bytes(saved)
    decoder.chmod(0o755)
    binary = app / "Contents/MacOS/belegal-local-demo"
    subprocess.run(["install_name_tool", "-add_rpath", "/opt/homebrew/lib", str(binary)], check=True, capture_output=True)
    assert any("forbidden LC_RPATH" in e for e in verifier.verify(app))
    subprocess.run(["install_name_tool", "-delete_rpath", "/opt/homebrew/lib", str(binary)], check=True, capture_output=True)
    # Mach-O disguised as a resource must also be inspected.
    hidden = app / "Contents/Resources/hidden-plugin.bin"
    shutil.copyfile(decoder, hidden)
    subprocess.run(["install_name_tool", "-change", "/usr/lib/libSystem.B.dylib", "/usr/local/lib/forbidden.dylib", str(hidden)], check=True, capture_output=True)
    assert any("hidden-plugin.bin: forbidden/unresolved dependency" in e for e in verifier.verify(app))
print("PASS: portable relocation; missing decoder, external rpath and hidden Mach-O dependencies rejected")
