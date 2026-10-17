#!/usr/bin/env python3
"""Build the app-owned, file-only audio decoder from pinned upstream source."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
VERSION = "8.1.2"
SHA256 = "464beb5e7bf0c311e68b45ae2f04e9cc2af88851abb4082231742a74d97b524c"
URL = f"https://ffmpeg.org/releases/ffmpeg-{VERSION}.tar.xz"
CACHE = ROOT / ".build-cache"
OUT = ROOT / "src-tauri/resources/runtime"
COMMON_FLAGS = [
    "--disable-autodetect",
    "--disable-shared", "--enable-static", "--disable-gpl", "--disable-nonfree",
    "--disable-version3", "--disable-network", "--disable-doc", "--disable-debug",
    "--disable-programs", "--enable-ffmpeg", "--disable-everything", "--enable-small",
    "--disable-avdevice", "--enable-protocol=file",
    "--enable-demuxer=wav,mp3,mov,aac,ogg,flac,matroska,asf,aiff,ac3,eac3",
    "--enable-decoder=aac,aac_fixed,aac_latm,ac3,eac3,alac,flac,mp3,mp3float,opus,vorbis,wmav1,wmav2,wmapro,wmalossless,pcm_*",
    "--enable-parser=aac,aac_latm,ac3,flac,mpegaudio,opus,vorbis",
    "--enable-encoder=pcm_s16le", "--enable-muxer=wav",
    "--enable-filter=aresample,aformat,anull",
]


def target_config():
    """Return native build settings; Windows is built only on Windows/MSYS2."""
    if os.environ.get("BLUEDOOR_FFMPEG_WINDOWS") == "1":
        arch = os.environ["BLUEDOOR_FFMPEG_ARCH"]
        target = os.environ["BLUEDOOR_FFMPEG_TARGET"]
        if arch not in {"aarch64", "x86_64"} or target not in {
            "aarch64-pc-windows-msvc", "x86_64-pc-windows-msvc"
        }:
            raise SystemExit("Unsupported Windows FFmpeg target")
        # LLVM/clang is invoked from a Visual Studio developer environment.
        # Static linking keeps the decoder self-contained and prevents arbitrary
        # PATH DLLs from affecting user imports.
        # The ARM64 UCRT does not export the POSIX read/write/close/fdopen
        # symbols, only the _-prefixed variants, so map them at the compiler.
        defaultlibs = ["-defaultlib:libcmt", "-defaultlib:libucrt",
                       "-defaultlib:libvcruntime", "-defaultlib:kernel32"]
        msvc_machine = "arm64" if arch == "aarch64" else "x64"
        return {
            "binary": "ffmpeg.exe",
            "make_target": "ffmpeg.exe",
            "flags": [
                f"--arch={arch}", "--target-os=win64", "--cc=clang",
                "--host-extralibs=",
                "--ld=lld-link", "--ar=llvm-ar", "--nm=llvm-nm", "--ranlib=llvm-ranlib", "--strip=llvm-strip",
                f"--extra-cflags=--target={target} -Dread=_read -Dwrite=_write -Dclose=_close -Dfdopen=_fdopen",
                "--extra-ldflags=" + " ".join([f"-machine:{msvc_machine}", *defaultlibs]),
                *COMMON_FLAGS,
            ],
            "target": target,
            "compiler": "clang",
            "sdk": os.environ.get("WindowsSDKVersion", "unknown"),
        }
    if subprocess.check_output(["uname", "-m"], text=True).strip() != "arm64":
        raise SystemExit("Use an Apple Silicon build host")
    return {
        "binary": "ffmpeg",
        "make_target": "ffmpeg",
        "flags": [
            "--arch=aarch64", "--target-os=darwin", "--cc=clang",
            "--extra-cflags=-mmacosx-version-min=13.4",
            "--extra-ldflags=-mmacosx-version-min=13.4",
            *COMMON_FLAGS,
        ],
        "target": "aarch64-apple-darwin",
        "compiler": "clang",
        "sdk": subprocess.check_output(["xcrun", "--show-sdk-version"], text=True).strip(),
    }


def build():
    config = target_config()
    CACHE.mkdir(exist_ok=True)
    archive = CACHE / f"ffmpeg-{VERSION}.tar.xz"
    if not archive.exists():
        partial = archive.with_suffix(".download")
        with urllib.request.urlopen(URL, timeout=60) as response, partial.open("wb") as out:
            shutil.copyfileobj(response, out)
        partial.rename(archive)
    if hashlib.sha256(archive.read_bytes()).hexdigest() != SHA256:
        raise SystemExit("FFmpeg source checksum mismatch; remove the corrupt build-cache archive and retry")
    source = CACHE / f"ffmpeg-{VERSION}"
    if not source.exists():
        with tarfile.open(archive) as tar:
            # The verified upstream archive is still extracted with traversal checks.
            for member in tar.getmembers():
                destination = (CACHE / member.name).resolve()
                if not destination.is_relative_to(CACHE.resolve()) or member.issym() or member.islnk():
                    raise SystemExit("Unsafe source archive entry")
            tar.extractall(CACHE)
    binary = OUT / config["binary"]
    stamp = {"version": VERSION, "sha256": SHA256, "source": URL, "configure": config["flags"],
             "target": config["target"],
             "compiler": subprocess.check_output([config["compiler"], "--version"], text=True).splitlines()[0],
             "sdk": config["sdk"]}
    previous = OUT / "ffmpeg-build.json"
    if previous.exists() and binary.exists():
        saved = json.loads(previous.read_text())
        binary_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
        if all(saved.get(k) == v for k, v in stamp.items()) and saved.get("binary_sha256") == binary_hash:
            print("Verified cached app-owned decoder")
            return
    env = os.environ.copy()
    for key in ["CFLAGS", "LDFLAGS", "CPPFLAGS", "C_INCLUDE_PATH", "CPLUS_INCLUDE_PATH", "LIBRARY_PATH", "PKG_CONFIG_PATH"]:
        env.pop(key, None)
    if config["target"] == "aarch64-apple-darwin":
        env["MACOSX_DEPLOYMENT_TARGET"] = "13.4"
    # Windows python cannot exec POSIX scripts, so run configure under the MSYS
    # bash that drives the build (make is found as make.exe via PATH).
    configure = ["./configure", *config["flags"]]
    if config["target"].endswith("-pc-windows-msvc"):
        configure = [os.environ.get("MSYS2_BASH", r"C:\msys64\usr\bin\bash.exe"), *configure]
    subprocess.run(configure, cwd=source, env=env, check=True)
    subprocess.run(["make", f"-j{min(os.cpu_count() or 2, 8)}", config["make_target"]], cwd=source, env=env, check=True)
    OUT.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source / "ffmpeg.exe" if config["binary"].endswith(".exe") else source / "ffmpeg", binary)
    if not config["binary"].endswith(".exe"):
        binary.chmod(0o755)
    stamp["binary_sha256"] = hashlib.sha256(binary.read_bytes()).hexdigest()
    # The committed stamp may carry the Windows ReadOnly attribute; the build
    # must be able to overwrite it.
    if previous.exists() and os.name == "nt":
        os.chmod(previous, 0o600)
    previous.write_text(json.dumps(stamp, indent=2) + "\n")
    shutil.copy2(source / "COPYING.LGPLv2.1", OUT / "FFmpeg-LICENSE.txt")
    print("Built", binary)


if __name__ == "__main__":
    build()
