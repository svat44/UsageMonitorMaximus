# Native lifecycle smoke test for Windows (run on a Windows machine or the CI runner).
# Verifies with real Windows APIs: window close hides to tray, engine keeps running, autostart
# enable/disable round-trips through the registry, and an intentional quit exits with code 0.
param([string]$Bin = "target\debug\usage-monitor-app.exe")
$ErrorActionPreference = "Stop"
$data = Join-Path $env:TEMP ("umm-smoke-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $data | Out-Null
$env:USAGE_MONITOR_DATA_DIR = $data
$env:USAGE_MONITOR_SMOKE_TEST = "1"
$p = Start-Process -FilePath $Bin -PassThru -WindowStyle Hidden
if (-not $p.WaitForExit(60000)) { $p.Kill(); throw "app did not exit within 60s (quit path broken)" }
"exit code: $($p.ExitCode)"
$r = Get-Content (Join-Path $data "smoke-result.json") -Raw | ConvertFrom-Json
$r | Format-List
$ok = $r.visibleBeforeClose -and $r.windowHiddenAfterClose -and $r.engineRunningBeforeClose `
      -and $r.autostartEnabledAfterEnable -and $r.autostartDisabledAfterDisable
if ($r.trayAvailable) { $ok = $ok -and $r.engineRunningAfterClose }
if (-not $ok -or $p.ExitCode -ne 0) { throw "Windows smoke test FAILED" }
"WINDOWS SMOKE OK"
