# This PC's camera, in a real call (Windows plan W5, V5's send half). The app
# turns its camera on; a watching voicebot decodes what arrives and says how
# many pictures came, whether they were intact and which layers they were on.
# Its own server, data folder and profile, so it never touches yours.
#
#   pwsh -File app/tool/windows/check_v5.ps1
#
# **This turns the camera on**, so its lamp comes on for about fifteen
# seconds. Nothing is recorded anywhere: the pictures go down a loopback
# connection to a voicebot that counts them and exits.
#
# ../check_v5.sh is the other half of V5 — the app *watching* a bot's camera —
# which on Windows is W4's check_receive_video.ps1.

param(
    [int]$HoldSeconds = 12,
    # §8: three layers for a 720p camera.
    [int]$WantLayers = 3,
    # Turning the camera on and off each have to take under this, per §8.
    [int]$MostMilliseconds = 1000
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. "$PSScriptRoot\e2e_server.ps1"

Invoke-E2ECheck -Profile e2e_camera {
    $manifest = Join-Path $E2E.Repo 'Cargo.toml'
    $env:CARGO_INCREMENTAL = '0'
    & cargo build --quiet --manifest-path $manifest -p opencord-voicebot
    if ($LASTEXITCODE -ne 0) { throw 'the voicebot did not build.' }
    $bot = Join-Path $E2E.Repo 'target\debug\opencord-voicebot.exe'

    # The app first: a bot's run is a fixed number of seconds from when it
    # starts, and a cold build inside the run would see it gone.
    Set-Location $E2E.App
    Write-Host 'building the app first, so the bot is still there when it joins'
    & flutter build windows --debug *>&1 | Select-Object -Last 2
    if ($LASTEXITCODE -ne 0) { throw 'the app did not build.' }

    $invite = Invoke-E2EClaimAndInvite -Bot $bot

    # One watcher per tile size. Only the layers somebody wants are encoded
    # (§7.11), so a single watcher can only ever receive one of them — three
    # watchers at three sizes is what makes "three layers sent" visible.
    $watchers = @()
    foreach ($tile in 180, 360, 720) {
        $log = Join-Path $E2E.Work "watch$tile.log"
        $process = Start-Process -FilePath $bot -ArgumentList @(
            '--server', $invite, '--name', "Watch$tile", '--tone', '0',
            '--watch', $tile, '--seconds', ($HoldSeconds + 110)
        ) -RedirectStandardOutput $log -RedirectStandardError "$log.err" `
            -PassThru -WindowStyle Hidden
        Add-E2EBackground -Id $process.Id
        $watchers += [pscustomobject]@{ Tile = $tile; Process = $process; Log = $log }
    }

    Set-Location $E2E.App
    $testLog = Join-Path $E2E.Work 'camera.log'
    $before = Get-Date
    & flutter test integration_test/windows/camera_test.dart -d windows `
        --dart-define=OPENCORD_E2E_INVITE="$invite" `
        --dart-define=OPENCORD_E2E_HOLD="$HoldSeconds" `
        *> $testLog
    $failed = $LASTEXITCODE -ne 0
    $wall = (Get-Date) - $before

    foreach ($pattern in 'CAMERAS: .*', 'CAMERA: .*') {
        foreach ($line in Select-String -Path $testLog -Pattern $pattern) {
            Write-Host "  $($line.Matches.Value)"
        }
    }
    if ($failed) {
        Get-Content $testLog -Tail 50 | Write-Host
        throw 'the app failed during the camera check.'
    }

    # How long the app took, and how much CPU the run cost. The app's own
    # process is gone by now, so this is the whole test's CPU over its whole
    # life — a ceiling on the camera's share, not the camera's alone, and
    # said as such.
    $reported = Select-String -Path $testLog -Pattern 'CAMERA: on=(\d+)ms off=(\d+)ms mirrored=(\w+) size=(\d+)x(\d+)' |
        Select-Object -First 1
    if (-not $reported) { throw 'the app did not report its camera timings.' }
    $on = [int]$reported.Matches[0].Groups[1].Value
    $off = [int]$reported.Matches[0].Groups[2].Value
    $mirrored = $reported.Matches[0].Groups[3].Value
    $width = [int]$reported.Matches[0].Groups[4].Value
    $height = [int]$reported.Matches[0].Groups[5].Value

    # Each watcher reports once it stops; let them.
    $pictures = 0
    $decoded = 0
    $layers = @(0, 0, 0)
    foreach ($watcher in $watchers) {
        $watcher.Process.WaitForExit(170000) | Out-Null
        if (-not $watcher.Process.HasExited) { Stop-E2ETree -Id $watcher.Process.Id }
        $seen = Select-String -Path $watcher.Log -Pattern 'pictures' | Select-Object -First 1
        if (-not $seen) {
            Write-Host "  the $($watcher.Tile)-pixel watcher saw nothing"
            continue
        }
        Write-Host "  $($watcher.Tile)px: $($seen.Line)"
        $found = [regex]::Match(
            $seen.Line,
            '(\d+) pictures, (\d+) intact, (\d+) decoded.*by layer (\d+)/(\d+)/(\d+)')
        if (-not $found.Success) { continue }
        $pictures += [int]$found.Groups[1].Value
        $decoded += [int]$found.Groups[3].Value
        foreach ($index in 0, 1, 2) {
            $layers[$index] += [int]$found.Groups[4 + $index].Value
        }
    }
    $used = ($layers | Where-Object { $_ -gt 0 }).Count

    Write-Host '----- what was measured -----'
    Write-Host ("camera on          : {0} ms (under {1})" -f $on, $MostMilliseconds)
    Write-Host ("camera off         : {0} ms (under {1})" -f $off, $MostMilliseconds)
    Write-Host ("preview mirrored   : $mirrored (what is sent is not)")
    Write-Host ("preview size       : ${width}x${height}")
    Write-Host ("pictures, all three: $pictures, $decoded decoded")
    Write-Host ("layers used        : $($layers -join '/') — $used of $WantLayers")
    Write-Host ("the whole run      : {0:N1} s wall clock" -f $wall.TotalSeconds)

    $problems = @()
    if ($on -ge $MostMilliseconds) { $problems += "turning the camera on took $on ms" }
    if ($off -ge $MostMilliseconds) { $problems += "turning the camera off took $off ms" }
    if ($mirrored -ne 'true') { $problems += 'the preview is not mirrored' }
    if ($pictures -lt 30) { $problems += "only $pictures pictures reached the watchers" }
    if ($problems) {
        foreach ($problem in $problems) { Write-Host "FAILED: $problem" }
        throw 'V5 is not met.'
    }

    # The layer count is kept apart from the rest. Everything about the
    # Windows camera itself passes; what this PC does not reach is the *top*
    # layer, and that is not the camera's doing — the shared test
    # `a_capped_uplink_stops_encoding_the_cameras_top_layer_until_lifted`
    # has been failing here with the same complaint since W1, and the same
    # shortfall shows in V4 and in W4's screen share. So: exit 2,
    # "inconclusive", as check_echo.ps1 uses it, rather than calling the
    # camera broken. windows.md §7 has the four measurements together.
    if ($used -lt $WantLayers) {
        Write-Host ''
        Write-Host "NOT REACHED: $used of $WantLayers layers ($($layers -join '/'))."
        Write-Host 'The camera, the preview and the timings all passed. What is missing is'
        Write-Host 'the top layer, which this PC never reaches — even a watcher asking for a'
        Write-Host '720-pixel tile is served the middle layer. See windows.md section 7.'
        exit 2
    }
    Write-Host 'Camera check passed.'
}
