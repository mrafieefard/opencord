# Global keybinds on Windows (Windows plan W2, step 7): push-to-talk on a
# keybind, held from outside the app while Notepad has the focus. A tone plays
# into CABLE Input, the app's microphone is CABLE Output, so the tone is what
# the app would send; the keybind decides whether it does. A Listener voicebot
# measures 440 Hz in what arrives. Two phases, each with its own Listener, so
# each phase's level is measured on its own. Its own server, data folder and
# profile, so it never touches yours.
#
#   pwsh -File app/tool/windows/check_keybinds.ps1
#
# Nothing is audible: the tone goes down the cable and the only other bot is
# silent. Nothing is typed anywhere either — F13 is a key the keyboard has a
# code for and no app uses.
#
# As in W1's microphone check, acceptance is the comparison between phases and
# not an absolute level: how much the chain colours a pure sine is exactly
# what is unknown. What the comparison does prove is that a key pressed while
# another app had the focus reached the app, which is what W2 is about.

param(
    [string]$InputDevice = 'CABLE Output',
    [string]$ToneInto = 'CABLE Input',
    [string]$Speaker = 'Realtek',
    [string]$Keys = 'F13',
    [int]$VirtualKey = 0x7C,
    [int]$HoldSeconds = 12
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. "$PSScriptRoot\e2e_server.ps1"
. "$PSScriptRoot\audio_devices.ps1"

# Not $speaker: PowerShell variable names ignore case, so that is the same
# variable as the [string]$Speaker parameter.
$microphone = Get-E2EAudioDeviceId -Name $InputDevice -Kind Capture
$speakerDevice = Get-E2EAudioDeviceId -Name $Speaker -Kind Render
Write-Host "the app's microphone: $($microphone.Name)  $($microphone.Id)"
Write-Host "the app's speaker   : $($speakerDevice.Name)"
Write-Host "the keybind         : $Keys (virtual key 0x$('{0:X2}' -f $VirtualKey))"

# The app is built first, outside the phases. A voicebot's run is a fixed
# number of seconds from when it starts, so a cold build inside a phase —
# cargokit rebuilding the core takes minutes — would have the Listener gone
# before the app ever joined.
Push-Location (Split-Path -Parent (Split-Path -Parent $PSScriptRoot))
try {
    Write-Host 'building the app, so no phase waits for a cold build'
    $env:CARGO_INCREMENTAL = '0'
    & flutter build windows --debug *>&1 | Select-Object -Last 3
    if ($LASTEXITCODE -ne 0) { throw 'the app did not build.' }
} finally {
    Pop-Location
}

# ffplay follows the default playback device, so the tone reaches the cable
# only while the cable is the default. Put the old default back afterwards.
Import-Module AudioDeviceCmdlets -ErrorAction Stop
$previousDefault = (Get-AudioDevice -Playback).ID

$levels = [ordered]@{}
$reports = [ordered]@{}
try {
    Set-AudioDevice -ID ((Get-AudioDevice -List |
                Where-Object { $_.Type -eq 'Playback' -and $_.Name -like "*$ToneInto*" }).ID) | Out-Null
    Write-Host "default playback is now $((Get-AudioDevice -Playback).Name)"

    foreach ($phase in 'released', 'held') {
        Write-Host "===== $phase ====="
        Invoke-E2ECheck -Profile e2e_keybinds {
            $manifest = Join-Path $E2E.Repo 'Cargo.toml'
            $env:CARGO_INCREMENTAL = '0'
            & cargo build --quiet --manifest-path $manifest -p opencord-voicebot
            if ($LASTEXITCODE -ne 0) { throw 'the voicebot did not build.' }
            $bot = Join-Path $E2E.Repo 'target\debug\opencord-voicebot.exe'

            $invite = Invoke-E2EClaimAndInvite -Bot $bot

            # The tone, for the whole phase and a little over.
            $tone = Start-Process -FilePath ffplay -ArgumentList @(
                '-nodisp', '-autoexit', '-loglevel', 'error',
                '-f', 'lavfi', '-i', "sine=frequency=440:duration=$($HoldSeconds + 130)"
            ) -PassThru -WindowStyle Hidden
            Add-E2EBackground -Id $tone.Id

            # The Listener measures 440 Hz in whatever each user sends it.
            $listenerLog = Join-Path $E2E.Work 'listener.log'
            $listener = Start-Process -FilePath $bot -ArgumentList @(
                '--server', $invite, '--name', 'Listener', '--tone', '0',
                '--listen-for', '440', '--seconds', ($HoldSeconds + 110)
            ) -RedirectStandardOutput $listenerLog -RedirectStandardError "$listenerLog.err" `
                -PassThru -WindowStyle Hidden
            Add-E2EBackground -Id $listener.Id

            Set-Location $E2E.App
            $testLog = Join-Path $E2E.Work 'keybinds.log'
            & flutter test integration_test/windows/keybinds_test.dart -d windows `
                --dart-define=OPENCORD_E2E_INVITE="$invite" `
                --dart-define=OPENCORD_E2E_INPUT_ID="$($microphone.Id)" `
                --dart-define=OPENCORD_E2E_OUTPUT_ID="$($speakerDevice.Id)" `
                --dart-define=OPENCORD_E2E_PHASE="$phase" `
                --dart-define=OPENCORD_E2E_KEYS="$Keys" `
                --dart-define=OPENCORD_E2E_VIRTUAL_KEY="$VirtualKey" `
                --dart-define=OPENCORD_E2E_HOLD="$HoldSeconds" `
                --dart-define=OPENCORD_E2E_PWSH="$([System.Diagnostics.Process]::GetCurrentProcess().MainModule.FileName)" `
                --dart-define=OPENCORD_E2E_REPO="$($E2E.Repo)" `
                *> $testLog
            if ($LASTEXITCODE -ne 0) {
                Get-Content $testLog | Write-Host
                throw "the app failed in $phase."
            }

            $reported = Select-String -Path $testLog -Pattern 'KEYBINDS: .*' |
                Select-Object -First 1
            if (-not $reported) { throw "the app reported nothing in $phase." }
            $script:reports[$phase] = $reported.Matches.Value
            Write-Host "  $($reported.Matches.Value)"

            # The Listener reports once it stops; let it.
            $listener.WaitForExit(170000) | Out-Null
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
foreach ($phase in $levels.Keys) { Write-Host ("{0,-10} {1,8:N1} dBFS" -f $phase, $levels[$phase]) }
foreach ($phase in $reports.Keys) { Write-Host "  $($reports[$phase])" }

$released = $levels['released']
$held = $levels['held']
$problems = @()

# A keybind held must be clearly louder than one never pressed. That is the
# whole acceptance, and it needs no absolute threshold.
if (($held - $released) -lt 20) {
    $problems += "the keybind held ($held dBFS) is not 20 dB above never pressing it ($released dBFS)"
}
# The key was pressed while something else had the focus, or the run proves
# nothing about *global* keybinds.
if ($reports['held'] -notmatch 'foreground=notepad') {
    $problems += "the key was not pressed while Notepad had the focus: $($reports['held'])"
}
# And the settings page has to be able to say so (step 7's second half).
foreach ($phase in $reports.Keys) {
    if ($reports[$phase] -notmatch 'support=global \(a Windows keyboard hook\)') {
        $problems += "in $phase the core did not report a global keybind: $($reports[$phase])"
    }
}

if ($problems) {
    foreach ($problem in $problems) { Write-Host "FAILED: $problem" }
    exit 1
}

Write-Host 'Global keybind check passed.'
