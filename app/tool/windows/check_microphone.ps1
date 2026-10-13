# The microphone path on Windows (Windows plan W1, step 5): a tone plays into
# CABLE Input, the app's microphone is CABLE Output, so the tone is what the
# app captures; a Listener voicebot measures 440 Hz in what arrives. Run in
# three phases — voice activity, push-to-talk released, push-to-talk held —
# each with its own Listener, so each phase's level is measured on its own.
# Its own server, data folder and profile, so it never touches yours.
#
#   pwsh -File app/tool/windows/check_microphone.ps1
#
# The plan has the tone played into Hi-Fi CABLE Input, a second cable. This PC
# has one VB-CABLE pair, so the tone goes into CABLE Input and the app's
# speaker is a real one instead (windows.md §7). Nothing is audible either
# way: the only other bot is silent.
#
# Acceptance is the comparison between phases, not an absolute level: a pure
# sine is not speech, and how much the processing chain colours it is exactly
# what is unknown here. -Suppression says which chain to use; `off` isolates
# the device path, which is what step 5 is about.

param(
    [string]$InputDevice = 'CABLE Output',
    [string]$ToneInto = 'CABLE Input',
    [string]$Speaker = 'Realtek',
    [ValidateSet('off', 'standard', 'high')][string]$Suppression = 'off',
    [int]$HoldSeconds = 12
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. "$PSScriptRoot\e2e_server.ps1"
. "$PSScriptRoot\audio_devices.ps1"

# Not $speaker: PowerShell variable names ignore case, so that is the same
# variable as the [string]$Speaker parameter, and assigning an object to it
# would quietly turn it into a string.
$microphone = Get-E2EAudioDeviceId -Name $InputDevice -Kind Capture
$speakerDevice = Get-E2EAudioDeviceId -Name $Speaker -Kind Render
$toneTarget = Get-E2EAudioDeviceId -Name $ToneInto -Kind Render
Write-Host "the app's microphone: $($microphone.Name)  $($microphone.Id)"
Write-Host "the app's speaker   : $($speakerDevice.Name)"
Write-Host "the tone plays into : $($toneTarget.Name)"
Write-Host "noise suppression   : $Suppression"

# ffplay follows the default playback device, so the tone reaches the cable
# only while the cable is the default. Put the old default back afterwards.
Import-Module AudioDeviceCmdlets -ErrorAction Stop
$previousDefault = (Get-AudioDevice -Playback).ID

$levels = [ordered]@{}
try {
    Set-AudioDevice -ID ((Get-AudioDevice -List |
                Where-Object { $_.Type -eq 'Playback' -and $_.Name -like "*$ToneInto*" }).ID) | Out-Null
    Write-Host "default playback is now $((Get-AudioDevice -Playback).Name)"

    foreach ($phase in 'voice-activity', 'push-to-talk-released', 'push-to-talk-held') {
        Write-Host "===== $phase ====="
        Invoke-E2ECheck -Profile e2e_microphone {
            $manifest = Join-Path $E2E.Repo 'Cargo.toml'
            $env:CARGO_INCREMENTAL = '0'
            & cargo build --quiet --manifest-path $manifest -p opencord-voicebot
            if ($LASTEXITCODE -ne 0) { throw 'the voicebot did not build.' }
            $bot = Join-Path $E2E.Repo 'target\debug\opencord-voicebot.exe'

            $invite = Invoke-E2EClaimAndInvite -Bot $bot

            # The tone, for the whole phase and a little over.
            $tone = Start-Process -FilePath ffplay -ArgumentList @(
                '-nodisp', '-autoexit', '-loglevel', 'error',
                '-f', 'lavfi', '-i', "sine=frequency=440:duration=$($HoldSeconds + 40)"
            ) -PassThru -WindowStyle Hidden
            Add-E2EBackground -Id $tone.Id

            # The Listener measures 440 Hz in whatever each user sends it.
            $listenerLog = Join-Path $E2E.Work 'listener.log'
            $listener = Start-Process -FilePath $bot -ArgumentList @(
                '--server', $invite, '--name', 'Listener', '--tone', '0',
                '--listen-for', '440', '--seconds', ($HoldSeconds + 30)
            ) -RedirectStandardOutput $listenerLog -RedirectStandardError "$listenerLog.err" `
                -PassThru -WindowStyle Hidden
            Add-E2EBackground -Id $listener.Id

            Set-Location $E2E.App
            $testLog = Join-Path $E2E.Work 'microphone.log'
            & flutter test integration_test/windows/microphone_test.dart -d windows `
                --dart-define=OPENCORD_E2E_INVITE="$invite" `
                --dart-define=OPENCORD_E2E_INPUT_ID="$($microphone.Id)" `
                --dart-define=OPENCORD_E2E_OUTPUT_ID="$($speakerDevice.Id)" `
                --dart-define=OPENCORD_E2E_PHASE="$phase" `
                --dart-define=OPENCORD_E2E_HOLD="$HoldSeconds" `
                --dart-define=OPENCORD_E2E_SUPPRESSION="$Suppression" `
                *> $testLog
            if ($LASTEXITCODE -ne 0) {
                Get-Content $testLog | Write-Host
                throw "the app failed in $phase."
            }

            # The Listener reports once it stops; let it.
            $listener.WaitForExit(60000) | Out-Null
            if (-not $listener.HasExited) { Stop-E2ETree -Id $listener.Id }

            $heard = Select-String -Path $listenerLog -Pattern 'user \d+:.*' |
                ForEach-Object { $_.Matches.Value }
            foreach ($line in $heard) { Write-Host "  $line" }
            $level = $null
            foreach ($line in $heard) {
                $found = [regex]::Match($line, '440 Hz (-?[0-9.]+) dBFS')
                if ($found.Success) { $level = [double]$found.Groups[1].Value }
            }
            if ($null -eq $level) {
                # No packets from the app at all is itself a level: silence.
                Write-Host '  the Listener heard nothing from the app'
                $level = -200.0
            }
            $script:levels[$phase] = $level
            Write-Host "  440 Hz as the Listener heard it: $level dBFS"
        }
    }
} finally {
    if ($previousDefault) {
        Set-AudioDevice -ID $previousDefault | Out-Null
        Write-Host "default playback put back to $((Get-AudioDevice -Playback).Name)"
    }
}

Write-Host '----- what was measured -----'
foreach ($phase in $levels.Keys) { Write-Host ("{0,-22} {1,8:N1} dBFS" -f $phase, $levels[$phase]) }

$activity = $levels['voice-activity']
$released = $levels['push-to-talk-released']
$held = $levels['push-to-talk-held']

$problems = @()
# Released push-to-talk must be clearly quieter than a held one: that is the
# whole point of the mode, and it needs no absolute threshold.
if (($held - $released) -lt 20) {
    $problems += "push-to-talk held ($held dBFS) is not 20 dB above released ($released dBFS)"
}
if (($activity - $released) -lt 20) {
    $problems += "voice activity ($activity dBFS) is not 20 dB above a released push-to-talk ($released dBFS)"
}
if ($problems) {
    foreach ($problem in $problems) { Write-Host "FAILED: $problem" }
    exit 1
}

Write-Host 'Microphone path check passed.'
