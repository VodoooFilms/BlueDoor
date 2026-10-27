[CmdletBinding()]
param(
    [ValidateSet("arm64", "x64")]
    [string]$Architecture = "arm64"
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$Target = if ($Architecture -eq "arm64") { "aarch64-pc-windows-msvc" } else { "x86_64-pc-windows-msvc" }
$TargetDirectory = Join-Path $Root "src-tauri\target\windows-$Architecture"
$Decoder = Join-Path $Root "src-tauri\resources\runtime\ffmpeg.exe"
$PreviousTranscribeArgs = $env:TRANSCRIBE_CMAKE_ARGS
$PreviousTargetDir = $env:CARGO_TARGET_DIR
$PreviousNoticeTarget = $env:BLUEDOOR_NOTICE_TARGET

function Invoke-Checked([string[]]$Command) {
    Write-Host "> $($Command -join ' ')"
    & $Command[0] $Command[1..($Command.Length - 1)]
    if ($LASTEXITCODE -ne 0) { throw "Command failed: $($Command -join ' ')" }
}

Push-Location $Root
try {
    # The decoder is built from the same pinned FFmpeg source as macOS. It is
    # intentionally a separate executable: never ship the macOS Mach-O file
    # into a Windows installer.
    Invoke-Checked @("powershell", "-ExecutionPolicy", "Bypass", "-File", (Join-Path $Root "scripts\build-ffmpeg-windows.ps1"), "-Architecture", $Architecture)
    if (-not (Test-Path -LiteralPath $Decoder -PathType Leaf)) {
        throw "Missing Windows decoder after build: $Decoder"
    }
    Invoke-Checked @("pnpm", "build")
    $env:CARGO_TARGET_DIR = $TargetDirectory
    # clang-cl provides the ARM64 C/C++ frontend for native dependencies.
    if ($Architecture -eq "arm64") {
        $env:CC_aarch64_pc_windows_msvc = "clang-cl"
        $env:CXX_aarch64_pc_windows_msvc = "clang-cl"
        $env:CXXFLAGS_aarch64_pc_windows_msvc = "/EHsc"
    }
    if ($Architecture -eq "x64") {
        # Static transcribe.cpp has no ISA dispatch. Its own conservative
        # option disables native tuning and all mandatory AVX/SSE4 tiers.
        # Qwen's separate dynamic CPU matrix remains optimized at runtime.
        if ($PreviousTranscribeArgs -notmatch '(?:^|\s)-DTRANSCRIBE_X86_CONSERVATIVE=ON(?:\s|$)') {
            $env:TRANSCRIBE_CMAKE_ARGS = "$PreviousTranscribeArgs -DTRANSCRIBE_X86_CONSERVATIVE=ON".Trim()
        }
    }
    # Ship the current third-party/model license inventory.
    $env:BLUEDOOR_NOTICE_TARGET = $Target
    Invoke-Checked @("python", (Join-Path $Root "scripts\collect-notices.py"))
    # Invoke the tauri CLI binary directly: `pnpm tauri` collides with the
    # package.json "tauri" script and pnpm swallows the trailing `-- --locked`.
    $TauriCli = Join-Path $Root "node_modules\@tauri-apps\cli\tauri.js"
    if ($Architecture -eq "x64") {
        # Build first, then package DLLs from this exact native build. Tauri
        # resources are explicit: DLLs next to the exe, decoder under runtime/.
        Invoke-Checked @("node", $TauriCli, "build", "--target", $Target, "--no-bundle", "--", "--locked")
        if (-not $env:VCToolsRedistDir) { throw "Run inside an x64 Visual Studio developer environment (VCToolsRedistDir required)." }
        Invoke-Checked @("python", (Join-Path $Root "scripts\package-windows-x64.py"), "--target-dir", $TargetDirectory, "--redist-dir", $env:VCToolsRedistDir)
        Invoke-Checked @("node", $TauriCli, "bundle", "--target", $Target, "--bundles", "nsis", "--config", (Join-Path $TargetDirectory "bundle-x64.json"), "--ci")
    } else {
        Invoke-Checked @("node", $TauriCli, "build", "--target", $Target, "--bundles", "nsis,msi", "--", "--locked")
    }
    Invoke-Checked @("python", (Join-Path $Root "scripts\verify-windows-bundle.py"), "--target-dir", $TargetDirectory, "--architecture", $Architecture)
} finally {
    $env:CARGO_TARGET_DIR = $PreviousTargetDir
    $env:BLUEDOOR_NOTICE_TARGET = $PreviousNoticeTarget
    $env:TRANSCRIBE_CMAKE_ARGS = $PreviousTranscribeArgs
    Remove-Item Env:CC_aarch64_pc_windows_msvc -ErrorAction SilentlyContinue
    Remove-Item Env:CXX_aarch64_pc_windows_msvc -ErrorAction SilentlyContinue
    Remove-Item Env:CXXFLAGS_aarch64_pc_windows_msvc -ErrorAction SilentlyContinue
    Pop-Location
}
