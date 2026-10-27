[CmdletBinding()]
param(
    [ValidateSet("arm64", "x64")]
    [string]$Architecture = "arm64"
)

# Builds the file-only FFmpeg decoder from the pinned upstream source using an
# installed MSYS2 + LLVM/clang toolchain. It performs no runtime download and
# produces the exact resource consumed by `platform::decoder_path()`.
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$Bash = if ($env:MSYS2_BASH) { $env:MSYS2_BASH } else { "C:\msys64\usr\bin\bash.exe" }
if (-not (Test-Path -LiteralPath $Bash -PathType Leaf)) {
    throw "MSYS2 bash is required to build the pinned Windows FFmpeg runtime. Install MSYS2/LLVM on the build VM, or set MSYS2_BASH to its bash.exe path."
}

$Arch = if ($Architecture -eq "arm64") { "aarch64" } else { "x86_64" }
$Target = if ($Architecture -eq "arm64") { "aarch64-pc-windows-msvc" } else { "x86_64-pc-windows-msvc" }
$SourceScript = Join-Path $Root "scripts\build-ffmpeg.py"

# The Python builder owns the pinned archive checksum and feature list. On
# Windows it needs the MSYS2 POSIX shell for FFmpeg configure/make; use a
# dedicated platform implementation rather than pretending the macOS script is
# portable.
# MSYS2's own python only opens POSIX-style paths, so map C:\... to the MSYS
# /c/... form; passing a bare C:/... argument is treated as relative to $HOME.
$PosixScript = ($SourceScript -replace '\\','/' -replace '^([A-Za-z]):','/$1')
# MSYS2 login shells rebuild PATH and inherit nothing from Windows, so expose
# the caller's PATH (vcvars, clang, python, make, ...) to the build shell as
# POSIX entries: C:\... -> /c/...
$PosixPath = (($env:PATH -split ';' | Where-Object { $_ -ne "" } | ForEach-Object {
    if ($_ -match '^([A-Za-z]):\\') {
        '/{0}/{1}' -f $matches[1].ToLower(), ($_ -replace '^[A-Za-z]:\\','' -replace '\\','/')
    } else {
        $_ -replace '\\','/'
    }
}) -join ':')
# Drive bash via a script file: PowerShell 5.1 does not escape embedded quotes
# when passing -c strings to native processes, which silently truncates the
# command at the double quotes in PATH entries.
$BuildScript = Join-Path $env:TEMP "bluedoor-ffmpeg-$PID.sh"
$BuildPayload = @"
set -euo pipefail
# POSIX tools belong only to this subprocess. Globally prepending MSYS2's
# usr/bin would make Cargo resolve its `link.exe` instead of MSVC's linker.
export PATH="/usr/bin:$PosixPath"
export BLUEDOOR_FFMPEG_ARCH='$Arch'
export BLUEDOOR_FFMPEG_TARGET='$Target'
export BLUEDOOR_FFMPEG_WINDOWS=1
python '$PosixScript'
"@ -replace "`r`n", "`n"
try {
    [IO.File]::WriteAllText($BuildScript, $BuildPayload)
    & $Bash -lc ($BuildScript -replace '\\','/')
    if ($LASTEXITCODE -ne 0) { throw "Pinned FFmpeg build failed for Windows $Architecture." }
} finally {
    Remove-Item -LiteralPath $BuildScript -Force -ErrorAction SilentlyContinue
}
