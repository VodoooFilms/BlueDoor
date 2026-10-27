[CmdletBinding()]
param([Parameter(Mandatory)][string]$Release, [Parameter(Mandatory)][string]$Output)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force -Path $Output | Out-Null
$Destination = Join-Path $env:TEMP 'BlueDoor x64 QA'
$Process = $null
$Report = @{ installed = $false; windowVisible = $false; errors = @(); peakWorkingSetBytes = 0 }
$Timer = [Diagnostics.Stopwatch]::StartNew()
try {
    $Installer = @(Get-ChildItem "$Release\bundle\nsis\*-setup.exe")
    if ($Installer.Count -ne 1) { throw 'Expected one NSIS installer' }
    # /D must be the final argument and unquoted per NSIS parsing rules.
    $Install = Start-Process -FilePath $Installer[0].FullName -ArgumentList "/S /D=$Destination" -PassThru -Wait
    if ($Install.ExitCode -ne 0) { throw "NSIS exit code: $($Install.ExitCode)" }
    $Exe = Join-Path $Destination 'bluedoor.exe'
    if (-not (Test-Path $Exe)) { throw 'Installed executable missing' }
    $Report.installed = $true
    $InstalledModels = @(Get-ChildItem $Destination -Recurse -File | Where-Object { $_.Extension -in '.gguf', '.bin' })
    if ($InstalledModels.Count) { throw 'Installer contains model weights' }
    python "$PSScriptRoot\..\tests\check-native-win.py" $Destination
    if ($LASTEXITCODE -ne 0) { throw 'Installed CPU runtime did not initialize' }
    & "$Destination\runtime\ffmpeg.exe" -version | Out-File "$Output\decoder.log"
    if ($LASTEXITCODE -ne 0) { throw 'Installed decoder failed' }
    # Exclude build-machine DLL directories from PATH; run from an unrelated CWD.
    $SavedPath = $env:PATH
    try {
        $env:PATH = "$env:SystemRoot\System32;$env:SystemRoot"
        $Process = Start-Process -FilePath $Exe -WorkingDirectory $env:TEMP -PassThru
        for ($i = 0; $i -lt 45; $i++) {
            Start-Sleep -Seconds 1
            $Process.Refresh()
            if ($Process.HasExited) { throw "Application exited: $($Process.ExitCode)" }
            $Report.peakWorkingSetBytes = [Math]::Max($Report.peakWorkingSetBytes, $Process.PeakWorkingSet64)
            if ($Process.MainWindowHandle -ne 0 -and $Process.Responding) { $Report.windowVisible = $true; break }
        }
        if (-not $Report.windowVisible) { throw 'No responsive native window within 45 seconds' }
        Start-Sleep -Seconds 10
        $Process.Refresh()
        if ($Process.HasExited -or -not $Process.Responding) { throw 'Window failed startup stability check' }
    } finally { $env:PATH = $SavedPath }
} catch {
    $Report.errors += $_.Exception.Message
    throw
} finally {
    if ($Process -and -not $Process.HasExited) { Stop-Process -Id $Process.Id -Force }
    $Report.seconds = $Timer.Elapsed.TotalSeconds
    $Report | ConvertTo-Json -Depth 4 | Set-Content "$Output\startup.json" -Encoding utf8
}
