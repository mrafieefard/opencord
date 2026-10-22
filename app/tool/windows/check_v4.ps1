# Phase 2 V4 on Windows (Windows plan W3, step 7), the twin of
# ../check_v4.sh: an opencord-server of its own, one voicebot sending the
# synthetic three-layer camera, another watching it in a 360-pixel tile.
# Every picture must arrive intact, none taller than the tile. Its own server
# and data folders, so it never touches yours.
#
#   pwsh -File app/tool/windows/check_v4.ps1
#
# Nothing here touches a real camera or a screen: both ends are voicebots, and
# the pictures are the test pattern the engine generates. What it measures is
# the codec layer W3 turned on — this machine's hardware encoder on one side,
# FFmpeg's decoder on the other.

param(
    [int]$Tile = 360,
    [int]$CameraSeconds = 14,
    [int]$WatcherSeconds = 10,
    [int]$LeastPictures = 100
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. "$PSScriptRoot\e2e_server.ps1"

Invoke-E2ECheck -Profile e2e_v4 {
    $manifest = Join-Path $E2E.Repo 'Cargo.toml'
    $env:CARGO_INCREMENTAL = '0'
    & cargo build --quiet --manifest-path $manifest -p opencord-voicebot
    if ($LASTEXITCODE -ne 0) { throw 'the voicebot did not build.' }
    $bot = Join-Path $E2E.Repo 'target\debug\opencord-voicebot.exe'

    $invite = Invoke-E2EClaimAndInvite -Bot $bot

    $cameraLog = Join-Path $E2E.Work 'camera.log'
    $camera = Start-Process -FilePath $bot -ArgumentList @(
        '--server', $invite, '--name', 'Camera', '--test-pattern',
        '--seconds', $CameraSeconds
    ) -RedirectStandardOutput $cameraLog -RedirectStandardError "$cameraLog.err" `
        -PassThru -WindowStyle Hidden
    Add-E2EBackground -Id $camera.Id

    # Let the camera publish before anyone asks for it, as check_v4.sh does.
    Start-Sleep -Seconds 2

    $watcherLog = Join-Path $E2E.Work 'watcher.log'
    $watcher = Start-Process -FilePath $bot -ArgumentList @(
        '--server', $invite, '--name', 'Watcher', '--tone', '0',
        '--watch', $Tile, '--seconds', $WatcherSeconds
    ) -RedirectStandardOutput $watcherLog -RedirectStandardError "$watcherLog.err" `
        -PassThru -WindowStyle Hidden
    Add-E2EBackground -Id $watcher.Id

    $watcher.WaitForExit(120000) | Out-Null
    if (-not $watcher.HasExited) { throw 'the watcher did not finish.' }
    $camera.WaitForExit(120000) | Out-Null

    Get-Content $watcherLog | Write-Host
    $seen = Select-String -Path $watcherLog -Pattern 'pictures' | Select-Object -First 1
    if (-not $seen) {
        Get-Content "$watcherLog.err" -ErrorAction SilentlyContinue | Write-Host
        throw 'the watcher saw no video.'
    }

    $found = [regex]::Match(
        $seen.Line,
        '(\d+) pictures, (\d+) intact.*by layer \d+/\d+/(\d+)$')
    if (-not $found.Success) { throw "the watcher's line does not parse: $($seen.Line)" }
    $pictures = [int]$found.Groups[1].Value
    $intact = [int]$found.Groups[2].Value
    $high = [int]$found.Groups[3].Value

    Write-Host '----- what was measured -----'
    Write-Host "pictures      : $pictures (at least $LeastPictures)"
    Write-Host "intact        : $intact (all of them)"
    Write-Host "top layer     : $high (none: the tile is $Tile pixels)"

    $problems = @()
    if ($pictures -lt $LeastPictures) { $problems += "only $pictures pictures arrived" }
    if ($intact -ne $pictures) { $problems += "$($pictures - $intact) pictures were damaged" }
    if ($high -ne 0) { $problems += "$high pictures came from the top layer, too big for the tile" }
    if ($problems) {
        foreach ($problem in $problems) { Write-Host "FAILED: $problem" }
        throw 'the V4 acceptance was not met.'
    }

    Write-Host 'V4 check passed.'
}
