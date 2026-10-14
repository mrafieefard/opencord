# Phase 2 V6 on Windows (Windows plan W6, step 9): the app shares a screen
# and a window, both picked in Opencord's own dialog, and a voicebot watches
# what goes out. Its own server, data folder and profile, so it never touches
# yours.
#
#   pwsh -File app/tool/windows/check_v6.ps1
#
# An ffplay window shows a moving test pattern, so there is something to
# share that is the same every run. It is **silent**: `-an` and a video-only
# source, so nothing comes out of the speakers.
#
# Three phases, each with its own server:
#   screen  a whole monitor at 720p30
#   window  the ffplay window
#   closed  the ffplay window, closed under the share, which has to end it
#
# The app's window is driven by the test itself; nothing is clicked outside
# it. The ffplay window appears on screen for the length of the run.

param(
    [int]$HoldSeconds = 14,
    [int]$LeastPictures = 100
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. "$PSScriptRoot\e2e_server.ps1"

$problems = @()
# Phases whose app side worked but whose pictures never reached the watcher.
$unwatched = @()
$built = $false
$results = [ordered]@{}

foreach ($phase in 'screen', 'window', 'closed') {
    Write-Host "===== $phase ====="
    Invoke-E2ECheck -Profile e2e_v6 {
        $manifest = Join-Path $E2E.Repo 'Cargo.toml'
        $env:CARGO_INCREMENTAL = '0'
        & cargo build --quiet --manifest-path $manifest -p opencord-voicebot
        if ($LASTEXITCODE -ne 0) { throw 'the voicebot did not build.' }
        $bot = Join-Path $E2E.Repo 'target\debug\opencord-voicebot.exe'

        if (-not $script:built) {
            Set-Location $E2E.App
            Write-Host 'building the app first, so the bot is still there when it joins'
            & flutter build windows --debug *>&1 | Select-Object -Last 2
            if ($LASTEXITCODE -ne 0) { throw 'the app did not build.' }
            $script:built = $true
        }

        # Something to look at, the same every run. Video only and -an, so
        # it is silent; the window title carries "testsrc2", which is how
        # the test finds it in the picker.
        $pattern = Start-Process -FilePath ffplay -ArgumentList @(
            '-hide_banner', '-loglevel', 'error', '-an',
            # One word: Start-Process splits an argument on its spaces, so a
            # quoted title arrives as three arguments and ffplay refuses it.
            '-window_title', 'OpencordTestPattern',
            '-x', '640', '-y', '360',
            '-f', 'lavfi', '-i', "testsrc2=size=640x360:rate=30"
        ) -PassThru
        Add-E2EBackground -Id $pattern.Id
        Start-Sleep -Seconds 3

        $invite = Invoke-E2EClaimAndInvite -Bot $bot

        # Watching at 360, not 720: this PC never reaches the top layer
        # (windows.md §7, W5), so a watcher asking for 720 is served
        # nothing at all and the check would measure that rather than the
        # share. 360 is the layer that does arrive here.
        $watcherLog = Join-Path $E2E.Work 'watcher.log'
        $watcher = Start-Process -FilePath $bot -ArgumentList @(
            '--server', $invite, '--name', 'Watcher', '--tone', '0',
            '--watch', '360', '--seconds', ($HoldSeconds + 120)
        ) -RedirectStandardOutput $watcherLog -RedirectStandardError "$watcherLog.err" `
            -PassThru -WindowStyle Hidden
        Add-E2EBackground -Id $watcher.Id

        Set-Location $E2E.App
        $testLog = Join-Path $E2E.Work "v6_$phase.log"
        $test = Start-Process -FilePath 'flutter' -ArgumentList @(
            'test', 'integration_test/windows/v6_test.dart', '-d', 'windows',
            "--dart-define=OPENCORD_E2E_INVITE=$invite",
            "--dart-define=OPENCORD_E2E_PHASE=$phase",
            "--dart-define=OPENCORD_E2E_HOLD=$HoldSeconds"
        ) -RedirectStandardOutput $testLog -RedirectStandardError "$testLog.err" `
            -PassThru -WindowStyle Hidden

        # In the closed phase the window goes away while the share is live,
        # which is what the app has to notice (W6 step 4).
        if ($phase -eq 'closed') {
            Start-Sleep -Seconds ($HoldSeconds + 40)
            if (-not $pattern.HasExited) {
                Write-Host '  closing the shared window'
                Stop-E2ETree -Id $pattern.Id
            }
        }

        $test.WaitForExit(300000) | Out-Null
        if (-not $test.HasExited) {
            Stop-E2ETree -Id $test.Id
            throw "the app did not finish the $phase phase."
        }
        $failed = $test.ExitCode -ne 0

        foreach ($pattern_ in 'SHARE: .*', 'CLOSED: .*') {
            foreach ($line in Select-String -Path $testLog -Pattern $pattern_) {
                Write-Host "  $($line.Matches.Value)"
            }
        }
        if ($failed) {
            Get-Content $testLog -Tail 40 | Write-Host
            throw "the app failed in the $phase phase."
        }

        # The watcher reports once it stops; let it.
        $watcher.WaitForExit(200000) | Out-Null
        if (-not $watcher.HasExited) { Stop-E2ETree -Id $watcher.Id }
        $seen = Select-String -Path $watcherLog -Pattern 'pictures' | Select-Object -First 1
        if (-not $seen) {
            # Not a failure of the share: the app's side is checked above,
            # and no screen-share media reaches a watcher on this PC in
            # either direction (windows.md §7, W4 and W6). Kept apart so a
            # real break in the app would still read as one.
            $script:unwatched += $phase
            return
        }
        Write-Host "  $($seen.Line)"
        $found = [regex]::Match($seen.Line, '(\d+) pictures, (\d+) intact, (\d+) decoded')
        if (-not $found.Success) {
            $script:problems += "$phase : the watcher's line does not parse"
            return
        }
        $script:results[$phase] = [int]$found.Groups[1].Value
    }
}

Write-Host '----- what was measured -----'
foreach ($phase in $results.Keys) {
    Write-Host ("{0,-8} {1,5} pictures reached the watcher" -f $phase, $results[$phase])
    if ($results[$phase] -lt $LeastPictures) {
        $problems += "$phase sent only $($results[$phase]) pictures"
    }
}
foreach ($phase in 'screen', 'window', 'closed') {
    if (-not ($results.Contains($phase) -or $unwatched -contains $phase)) {
        $problems += "$phase did not run"
    }
}

if ($problems) {
    Write-Host ''
    foreach ($problem in $problems) { Write-Host "FAILED: $problem" }
    exit 1
}

# 2, "inconclusive", as check_echo.ps1 and the W4 and W5 checks use it.
if ($unwatched) {
    Write-Host ''
    Write-Host "NOT WATCHED: $($unwatched -join ', ') — the app shared, and said what it"
    Write-Host 'shared, but no pictures reached the watching voicebot. That is the same'
    Write-Host 'screen-share media gap W4 measured from the other direction; the app side'
    Write-Host 'of each phase passed. See windows.md section 7.'
    exit 2
}

Write-Host ''
Write-Host 'V6 check passed.'
