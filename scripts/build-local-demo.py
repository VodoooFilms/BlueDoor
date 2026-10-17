#!/usr/bin/env python3
"""Build and open the ad-hoc signed local macOS demo, without cloud deployment."""
from pathlib import Path
import importlib.util
import subprocess
import sys

root = Path(__file__).resolve().parent.parent
build = root / "src-tauri/target/debug"
app = build / "bundle/macos/Local Legal.app"
def run(args):
    subprocess.run([str(arg) for arg in args], cwd=root, check=True)

if sys.platform != "darwin":
    raise SystemExit("demo:mac requires macOS. Use pnpm tauri dev on other desktops.")
if not (root / "src-tauri/resources/runtime/ffmpeg").is_file():
    run([sys.executable, root / "scripts/build-ffmpeg.py"])
run(["pnpm", "tauri", "build", "--debug", "--bundles", "app", "--no-sign"])
run([sys.executable, root / "scripts/package-local-ai.py", app, build])
spec = importlib.util.spec_from_file_location("verify", root / "scripts/verify-bundle.py")
verify = importlib.util.module_from_spec(spec)
spec.loader.exec_module(verify)
for binary in sorted(verify.macho_files(app), key=lambda path:len(path.parts), reverse=True):
    run(["codesign", "--force", "--sign", "-", binary])
run(["codesign", "--force", "--sign", "-", app])
run(["codesign", "--verify", "--deep", "--strict", app])
# Runtime executables must be re-signed after install_name_tool changes
# before the verifier executes them (macOS rejects invalid signatures).
run([sys.executable, root / "scripts/verify-bundle.py", app])
if "--no-open" not in sys.argv:
    run(["open", app])
