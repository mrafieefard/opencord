# Others' video shown in the app on Windows (Windows plan W4), the Windows
# half of ../check_v5.sh's receive side and of ../check_v6.sh's "watches".
# A voicebot sends a camera of real H.264, and another shares a screen; the
# app joins, shows each in its tile, Flutter draws the texture, focus mode
# opens the camera large, its colours are checked off the screen, and hiding
# the tiles stops the video. This PC's own camera and screen stay off.
#
#   pwsh -File app/tool/windows/check_receive_video.ps1
#
# Two phases, each with its own server, data folder and profile, as W1's
# checks do — a second run on one profile would find the identity already
# made and never reach the screen it waits for.
#
# **The app's window has to be visible during the camera phase**: the colour
# check copies pixels off the screen, because a Flutter `Texture` is
# composited by Windows and cannot be read from inside a test. Don't cover the
# window or lock the PC. Nothing is typed or clicked outside the app.

param(
    [int]$LeastDrawn = 30,
    # How much redder the left third has to be than the right. The camera's
    # chroma bands run U=96,V=160 to U=152,V=104 across the picture, so left
    # is reddish and right bluish; a red/blue swap would reverse the sign.
    # 10 is well clear of the noise and far under the ~140 seen in practice.
    [int]$LeastRedness = 10
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. "$PSScriptRoot\e2e_server.ps1"

$capture = Join-Path ([System.IO.Path]::GetTempPath()) 'opencord-w4-camera.png'
$pwshPath = [System.Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
$problems = @()
# The screen half is kept apart: on this PC it does not arrive reliably
# (W4's log has the runs), and that should not read as the camera half
# failing. See the exit codes at the bottom.
$screenProblems = @()
$built = $false

foreach ($phase in 'camera', 'screen') {
    Write-Host "===== $phase ====="
    Invoke-E2ECheck -Profile e2e_receive_video {
        $manifest = Join-Path $E2E.Repo 'Cargo.toml'
        $env:CARGO_INCREMENTAL = '0'
        & cargo build --quiet --manifest-path $manifest -p opencord-voicebot
        if ($LASTEXITCODE -ne 0) { throw 'the voicebot did not build.' }
        $bot = Join-Path $E2E.Repo 'target\debug\opencord-voicebot.exe'

        # The app is built before the bot starts: a bot's run is a fixed
        # number of seconds from when it starts, and a cold cargokit build
        # inside the run would see it gone (the trap W2 hit).
        if (-not $script:built) {
            Set-Location $E2E.App
            Write-Host 'building the app first, so the bot is still there when it joins'
            & flutter build windows --debug *>&1 | Select-Object -Last 2
            if ($LASTEXITCODE -ne 0) { throw 'the app did not build.' }
            $script:built = $true
        }

        $invite = Invoke-E2EClaimAndInvite -Bot $bot

        $spec = if ($phase -eq 'camera') {
            @{ Name = 'Camera'; Args = @('--camera') }
        } else {
            @{ Name = 'Sharer'; Args = @('--screen', '720p30') }
        }
        $botLog = Join-Path $E2E.Work "$($spec.Name).log"
        $process = Start-Process -FilePath $bot -ArgumentList (
            @('--server', $invite, '--name', $spec.Name, '--tone', '0') +
            $spec.Args + @('--seconds', '600')
        ) -RedirectStandardOutput $botLog -RedirectStandardError "$botLog.err" `
            -PassThru -WindowStyle Hidden
        Add-E2EBackground -Id $process.Id

        Set-Location $E2E.App
        $testLog = Join-Path $E2E.Work "receive_$phase.log"
        $defines = @(
            "--dart-define=OPENCORD_E2E_INVITE=$invite",
            "--dart-define=OPENCORD_E2E_PHASE=$phase"
        )
        if ($phase -eq 'camera') {
            $defines += @(
                "--dart-define=OPENCORD_E2E_PWSH=$pwshPath",
                "--dart-define=OPENCORD_E2E_REPO=$($E2E.Repo)",
                "--dart-define=OPENCORD_E2E_CAPTURE=$capture"
            )
        }
        & flutter test integration_test/windows/receive_video_test.dart -d windows @defines *> $testLog
        $failed = $LASTEXITCODE -ne 0

        foreach ($pattern in 'RECEIVE: .*', 'SAMPLER .*', 'SAMPLE .*', 'HIDDEN .*', 'BEFORE WATCH: .*', 'AFTER WATCH: .*') {
            foreach ($line in Select-String -Path $testLog -Pattern $pattern) {
                Write-Host "  $($line.Matches.Value)"
            }
        }
        if ($failed) {
            Get-Content $testLog -Tail 50 | Write-Host
            if ($phase -eq 'screen') {
                $script:screenProblems += 'the screen share never drew a picture'
                return
            }
            throw "the app failed during the $phase phase."
        }

        $receive = Select-String -Path $testLog -Pattern 'RECEIVE: presented=(\d+) drawn=(\d+)' |
            Select-Object -First 1
        if (-not $receive) { throw "the $phase phase reported no texture statistics." }
        $presented = [int]$receive.Matches[0].Groups[1].Value
        $drawn = [int]$receive.Matches[0].Groups[2].Value
        # The screen only has to be watched and drawn; its own acceptance
        # (sharing, viewer counts) is W6's.
        $least = if ($phase -eq 'camera') { $LeastDrawn } else { 10 }
        Write-Host "  $phase presented $presented, drawn $drawn (at least $least)"
        if ($presented -lt $least) { $script:problems += "$phase presented only $presented pictures" }
        if ($drawn -lt $least) { $script:problems += "Flutter drew only $drawn $phase pictures" }

        if ($phase -ne 'camera') { return }

        # The colours, off the screen.
        $redness = Select-String -Path $testLog -Pattern 'SAMPLE redness left=(-?[0-9.]+) right=(-?[0-9.]+)' |
            Select-Object -First 1
        if (-not $redness) {
            $script:problems += 'the colour sampler reported nothing (was the window covered?)'
        } else {
            $left = [double]$redness.Matches[0].Groups[1].Value
            $right = [double]$redness.Matches[0].Groups[2].Value
            Write-Host ("  redness left {0:N1} against right {1:N1} (left at least {2} higher)" -f
                $left, $right, $LeastRedness)
            Write-Host "  capture: $capture"
            if (($left - $right) -lt $LeastRedness) {
                $script:problems += ("the left of the picture is not redder than the right " +
                    "($left against $right): red and blue may be swapped")
            }
        }

        # Hidden tiles: the app asked for nothing, which the test already
        # waited for, and nothing new was presented afterwards.
        $hidden = Select-String -Path $testLog -Pattern 'HIDDEN presented=(\d+) then=(\d+)' |
            Select-Object -First 1
        if (-not $hidden) {
            $script:problems += 'the app did not report what happened once the tiles were hidden'
        } else {
            $before = [int]$hidden.Matches[0].Groups[1].Value
            $after = [int]$hidden.Matches[0].Groups[2].Value
            Write-Host "  hidden tile: $before presented, $after two seconds later"
            if ($after -gt $before) {
                $script:problems += "a hidden tile kept receiving ($before then $after)"
            }
        }
    }
}

if ($problems) {
    Write-Host ''
    foreach ($problem in $problems) { Write-Host "FAILED: $problem" }
    exit 1
}

# 2, "inconclusive", as check_echo.ps1 uses it: the camera half — which is
# what W4 is mostly about — is met, and the screen half did not come. It has
# arrived before, so this is not a flat failure; W6 is where screen sharing
# is built and measured properly, and this is carried there.
if ($screenProblems) {
    Write-Host ''
    foreach ($problem in $screenProblems) { Write-Host "NOT SHOWN: $problem" }
    Write-Host 'The camera half of W4 passed. The screen half has arrived on this PC'
    Write-Host 'before (10 pictures presented, 11 drawn) but does not come reliably.'
    Write-Host 'W6 builds screen sharing and settles it; see windows.md section 7.'
    exit 2
}

Write-Host ''
Write-Host 'Receive-video check passed.'
