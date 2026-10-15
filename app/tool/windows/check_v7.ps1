# Phase 2 V7 on Windows (Windows plan W7, step 4 — §10.7's "no double
# audio" test): the app shares a screen *with its sound*, and what goes out
# has the shared sound in it and the call's own voice out of it.
#
#   pwsh -File app/tool/windows/check_v7.ps1
#
# **This has never run.** Process loopback needs Windows build 20348 and
# this PC is 19045, so the script stops at the first check with the same
# words the app's disabled switch uses. It is written so that the first
# Windows 11 machine can run it unchanged; until one does, W7's live half is
# not done (windows.md §7).
#
# Nothing is audible: every device is a cable. The app plays the call to
# CABLE Input, ffplay plays its tones into the default device — which the
# script points at CABLE Input for the length of the run and puts back — and
# the voicebots are on the loopback.
#
# Four parts, as §10.7 asks:
#   1. share the screen with sound, with a Talker in the call and ffplay
#      playing 1 kHz. The stream must carry 1 kHz on both channels and the
#      Talker below -55 dBFS.
#   2. the mutation: exclude some other process instead of Opencord's. The
#      Talker is then in the stream and the check MUST fail — a check that
#      cannot fail proves nothing.
#   3. a window share of an ffplay playing 1 kHz while another plays
#      2.4 kHz. Only 1 kHz may be in the stream.
#   4. optional, with a person: the same with a video playing in Edge.

param(
    [string]$Speaker = 'CABLE Input',
    [string]$Loopback = 'CABLE Output',
    [int]$HoldSeconds = 14,
    # The call's own voice must be this far down in the shared sound.
    [double]$VoiceBelow = -55.0,
    # And the shared tone must be clearly present.
    [double]$ToneAbove = -50.0
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. "$PSScriptRoot\e2e_server.ps1"
. "$PSScriptRoot\audio_devices.ps1"

# The one thing to check before anything else: this is what the app's own
# `share_audio::support()` reports, and it is why the switch is disabled.
$build = [int](Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion').CurrentBuildNumber
Write-Host "Windows build: $build (process loopback needs 20348)"
if ($build -lt 20348) {
    Write-Host ''
    Write-Host 'Sharing sound needs Windows 11.'
    Write-Host 'W7 step 1 is written and unit-tested, and crates/opencord-media/tests/'
    Write-Host 'share_audio_windows.rs covers the capture itself; both skip on this build.'
    Write-Host 'Run this again on a PC with build 20348 or newer to finish W7 (windows.md §7).'
    exit 2
}

$speakerDevice = Get-E2EAudioDeviceId -Name $Speaker -Kind Render
$loopbackDevice = Get-E2EAudioDeviceId -Name $Loopback -Kind Capture
Write-Host "the app's speaker   : $($speakerDevice.Name)"
Write-Host "the app's microphone: $($loopbackDevice.Name)"

# ffplay follows the default playback device, so its tones reach the cable
# only while the cable is the default. Put the old default back afterwards.
Import-Module AudioDeviceCmdlets -ErrorAction Stop
$previousDefault = (Get-AudioDevice -Playback).ID
$problems = @()

function Start-Tone {
    param([Parameter(Mandatory)][int]$Hz, [Parameter(Mandatory)][int]$Seconds)
    $process = Start-Process -FilePath ffplay -ArgumentList @(
        '-hide_banner', '-loglevel', 'error', '-nodisp', '-autoexit',
        '-f', 'lavfi', '-i', "sine=frequency=$Hz`:duration=$Seconds"
    ) -PassThru -WindowStyle Hidden
    Add-E2EBackground -Id $process.Id
    $process
}

try {
    Set-AudioDevice -ID ((Get-AudioDevice -List |
                Where-Object { $_.Type -eq 'Playback' -and $_.Name -like "*$Speaker*" }).ID) | Out-Null
    Write-Host "default playback is now $((Get-AudioDevice -Playback).Name)"

    foreach ($phase in 'screen', 'mutation', 'window') {
        Write-Host "===== $phase ====="
        Invoke-E2ECheck -Profile e2e_v7 {
            $manifest = Join-Path $E2E.Repo 'Cargo.toml'
            $env:CARGO_INCREMENTAL = '0'
            & cargo build --quiet --manifest-path $manifest -p opencord-voicebot
            if ($LASTEXITCODE -ne 0) { throw 'the voicebot did not build.' }
            $bot = Join-Path $E2E.Repo 'target\debug\opencord-voicebot.exe'

            Set-Location $E2E.App
            & flutter build windows --debug *>&1 | Select-Object -Last 1
            if ($LASTEXITCODE -ne 0) { throw 'the app did not build.' }

            $invite = Invoke-E2EClaimAndInvite -Bot $bot

            # The call's own voice, which must NOT be in the shared sound.
            $talkerLog = Join-Path $E2E.Work 'talker.log'
            $talker = Start-Process -FilePath $bot -ArgumentList @(
                '--server', $invite, '--name', 'Talker', '--tone', '440',
                '--seconds', ($HoldSeconds + 90)
            ) -RedirectStandardOutput $talkerLog -RedirectStandardError "$talkerLog.err" `
                -PassThru -WindowStyle Hidden
            Add-E2EBackground -Id $talker.Id

            # The shared sound, which must be.
            Start-Tone -Hz 1000 -Seconds ($HoldSeconds + 80) | Out-Null
            if ($phase -eq 'window') {
                # And another app, which must not leak into a window share.
                Start-Tone -Hz 2400 -Seconds ($HoldSeconds + 80) | Out-Null
            }

            # The Watcher measures what the app actually sends.
            $watcherLog = Join-Path $E2E.Work 'watcher.log'
            $watcher = Start-Process -FilePath $bot -ArgumentList @(
                '--server', $invite, '--name', 'Watcher', '--tone', '0',
                '--listen-for', '1000', '--listen-for', '440',
                '--listen-for', '2400',
                '--watch', '360', '--seconds', ($HoldSeconds + 80)
            ) -RedirectStandardOutput $watcherLog -RedirectStandardError "$watcherLog.err" `
                -PassThru -WindowStyle Hidden
            Add-E2EBackground -Id $watcher.Id

            Set-Location $E2E.App
            $testLog = Join-Path $E2E.Work "v7_$phase.log"
            & flutter test integration_test/windows/v7_test.dart -d windows `
                --dart-define=OPENCORD_E2E_INVITE="$invite" `
                --dart-define=OPENCORD_E2E_PHASE="$phase" `
                --dart-define=OPENCORD_E2E_HOLD="$HoldSeconds" `
                --dart-define=OPENCORD_E2E_OUTPUT_ID="$($speakerDevice.Id)" `
                --dart-define=OPENCORD_E2E_INPUT_ID="$($loopbackDevice.Id)" `
                *> $testLog
            if ($LASTEXITCODE -ne 0) {
                Get-Content $testLog -Tail 40 | Write-Host
                throw "the app failed in the $phase phase."
            }
            foreach ($line in Select-String -Path $testLog -Pattern 'V7: .*') {
                Write-Host "  $($line.Matches.Value)"
            }

            $watcher.WaitForExit(200000) | Out-Null
            if (-not $watcher.HasExited) { Stop-E2ETree -Id $watcher.Id }
            $heard = Select-String -Path $watcherLog -Pattern 'user \d+:.*' |
                ForEach-Object { $_.Matches.Value }
            foreach ($line in $heard) { Write-Host "  $line" }

            $shared = $null
            $voice = $null
            $other = $null
            foreach ($line in $heard) {
                $at = [regex]::Match($line, '1000 Hz (-?[0-9.]+) dBFS')
                if ($at.Success) { $shared = [double]$at.Groups[1].Value }
                $at = [regex]::Match($line, '440 Hz (-?[0-9.]+) dBFS')
                if ($at.Success) { $voice = [double]$at.Groups[1].Value }
                $at = [regex]::Match($line, '2400 Hz (-?[0-9.]+) dBFS')
                if ($at.Success) { $other = [double]$at.Groups[1].Value }
            }
            if ($null -eq $shared -or $null -eq $voice) {
                throw "the watcher reported no levels in the $phase phase."
            }
            Write-Host ("  shared tone {0:N1} dBFS, the call's voice {1:N1} dBFS" -f $shared, $voice)

            switch ($phase) {
                'screen' {
                    if ($shared -lt $ToneAbove) {
                        $script:problems += "the shared sound is not in the stream ($shared dBFS)"
                    }
                    if ($voice -gt $VoiceBelow) {
                        $script:problems += "the call's own voice leaked into the share ($voice dBFS)"
                    }
                }
                'mutation' {
                    # Excluding the wrong process must put the voice back in
                    # the stream. If it does not, the screen phase proved
                    # nothing.
                    if ($voice -le $VoiceBelow) {
                        $script:problems += (
                            'the mutation did not change the result: excluding another ' +
                            "process still kept the voice out ($voice dBFS), so the check " +
                            'cannot tell right from wrong')
                    } else {
                        Write-Host '  the mutation puts the voice back, as it must'
                    }
                }
                'window' {
                    if ($shared -lt $ToneAbove) {
                        $script:problems += "the window's own sound is not in the stream ($shared dBFS)"
                    }
                    if ($null -ne $other -and ($shared - $other) -lt 20) {
                        $script:problems += (
                            "the other app leaked into the window share " +
                            "($shared against $other dBFS)")
                    }
                }
            }
        }
    }
} finally {
    if ($previousDefault) {
        Set-AudioDevice -ID $previousDefault | Out-Null
        Write-Host "default playback put back to $((Get-AudioDevice -Playback).Name)"
    }
}

if ($problems) {
    Write-Host ''
    foreach ($problem in $problems) { Write-Host "FAILED: $problem" }
    exit 1
}

Write-Host ''
Write-Host 'V7 check passed: the shared sound goes out and the call does not.'
