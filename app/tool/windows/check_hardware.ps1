# The §15 hardware checklist on Windows, the parts that need a person at the
# PC (Windows plan W8, step 1): a headset taken away and brought back mid-call,
# and the PC slept and woken mid-call.
#
#   pwsh -File app/tool/windows/check_hardware.ps1 -Phase headset
#   pwsh -File app/tool/windows/check_hardware.ps1 -Phase sleep
#
# One phase per run, each in its own call on its own server:
#
#   headset  The app uses the headset for the call, the person turns it off or
#            unplugs it, and the app must fall back to the default device with
#            a toast and keep playing (§7.6). Then the headset comes back and
#            the app must return to it (media.rs `switch_back`). Both
#            directions are *measured*, not watched: the fallback device is
#            CABLE Input, so CABLE Output hears whatever the app plays there.
#            The same run covers a Bluetooth headset — pass its name — and
#            prints the device list as the microphone opens, which is where a
#            profile switch shows.
#   sleep    The call is measured, the PC is suspended (SetSuspendState, which
#            returns only once it is back), the person wakes it, and the call
#            must come back and play again.
#
# Nothing is audible but the headset itself: the app's fallback speaker is the
# cable and its microphone is a silent capture device.
#
# **The person is never prompted.** This script's output goes to the session
# driving it, not to the screen in front of whoever is at the PC, so every step
# waits for the hardware event itself — a device going, a device returning, the
# system resuming — for up to four minutes each. Whoever is at the PC gets
# their instructions from the session, and can take as long as they like.
#
# Exit codes: 0 passed, 1 failed, 2 nothing was proven (the hardware never
# moved, so the run says nothing either way).

param(
    [ValidateSet('headset', 'sleep')][string]$Phase = 'headset',
    # Part of the headset's name, as Windows shows it.
    [string]$Headset = 'Razer Barracuda',
    # What the app falls back to: silent, and it hears itself back.
    [string]$Fallback = 'CABLE Input',
    [string]$RecordDevice = 'CABLE Output',
    # The app's microphone while the headset is away: anything digitally
    # silent, as in check_user_audio.ps1.
    [string]$SilentInput = 'WO Mic',
    # How long after the call is measured to suspend the PC.
    [int]$SleepAfter = 8,
    # Hibernate instead of sleeping. On a PC with Modern Standby and no S3 a
    # plain sleep tears nothing down, so the call survives it untouched and
    # the item proves little; hibernating cuts the power, which is the harsh
    # case the checklist is after (windows.md §7, W8).
    [switch]$Hibernate
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. "$PSScriptRoot\e2e_server.ps1"
. "$PSScriptRoot\audio_devices.ps1"

$fallbackDevice = Get-E2EAudioDeviceId -Name $Fallback -Kind Render
$silent = Get-E2EAudioDeviceId -Name $SilentInput -Kind Capture
$record = Get-E2EAudioDeviceId -Name $RecordDevice -Kind Capture
Write-Host "the fallback speaker: $($fallbackDevice.Name)"
Write-Host "the app's microphone: $($silent.Name)"
Write-Host "recorded with ffmpeg: $($record.Name)"
if ($Phase -eq 'headset') { Write-Host "the headset          : *$Headset*" }

# The app falls back to whatever Windows calls the default, so the default has
# to be the cable for the length of the run — otherwise the fallback plays the
# call out of the laptop's speakers and cannot be measured. Put the old one
# back whatever happens.
Import-Module AudioDeviceCmdlets -ErrorAction Stop
$previousDefault = (Get-AudioDevice -Playback).ID

# What the system can do about sleeping, for the log: this PC has Modern
# Standby, not S3.
if ($Phase -eq 'sleep') { & powercfg /a | Write-Host }

# Waits for the test to say it is in the call, suspends the PC, and writes how
# long the sleep lasted. SetSuspendState returns only once the system is back,
# which is a better witness than any clock in the app: under Modern Standby a
# desktop app is throttled rather than stopped.
$suspender = @'
param([string]$Marker, [int]$Delay, [switch]$Hibernate)
Set-StrictMode -Version Latest
while (-not (Test-Path "$Marker.sleep")) { Start-Sleep -Milliseconds 300 }
Start-Sleep -Seconds $Delay
Add-Type -Namespace Win32 -Name Power -MemberDefinition @"
[DllImport("powrprof.dll", SetLastError = true)]
public static extern bool SetSuspendState(bool hibernate, bool force, bool wakeupEventsDisabled);
"@
$started = Get-Date
[Win32.Power]::SetSuspendState([bool]$Hibernate, $false, $false) | Out-Null
Set-Content -Path "$Marker.woke" -Value ([int]((Get-Date) - $started).TotalSeconds)
'@

$status = 0
$inconclusive = $false

try {
    Set-AudioDevice -ID ((Get-AudioDevice -List |
                Where-Object { $_.Type -eq 'Playback' -and $_.Name -like "*$Fallback*" }).ID) | Out-Null
    Write-Host "default playback is now $((Get-AudioDevice -Playback).Name)"

    Invoke-E2ECheck -Profile e2e_w8 {
        $manifest = Join-Path $E2E.Repo 'Cargo.toml'
        $env:CARGO_INCREMENTAL = '0'
        & cargo build --quiet --manifest-path $manifest -p opencord-voicebot
        if ($LASTEXITCODE -ne 0) { throw 'the voicebot did not build.' }
        $bot = Join-Path $E2E.Repo 'target\debug\opencord-voicebot.exe'

        # Built before anything is started: a cold build takes minutes, and
        # the bots' --seconds run from the moment they start (windows.md §7).
        Set-Location $E2E.App
        & flutter build windows --debug *>&1 | Select-Object -Last 1
        if ($LASTEXITCODE -ne 0) { throw 'the app did not build.' }

        $invite = Invoke-E2EClaimAndInvite -Bot $bot

        # The voice in the call, and what every measurement here measures.
        # --seconds is a monotonic deadline, which a sleep does not advance.
        $talkerLog = Join-Path $E2E.Work 'talker.log'
        $talker = Start-Process -FilePath $bot -ArgumentList @(
            '--server', $invite, '--name', 'Talker', '--tone', '440',
            '--seconds', '1200'
        ) -RedirectStandardOutput $talkerLog -RedirectStandardError "$talkerLog.err" `
            -PassThru -WindowStyle Hidden
        Add-E2EBackground -Id $talker.Id

        $marker = Join-Path $E2E.Work 'w8'
        if ($Phase -eq 'sleep') {
            $script = Join-Path $E2E.Work 'suspend.ps1'
            Set-Content -Path $script -Value $suspender
            $childArguments = @('-NoProfile', '-File', $script,
                '-Marker', $marker, '-Delay', $SleepAfter)
            if ($Hibernate) { $childArguments += '-Hibernate' }
            $child = Start-Process -FilePath (Get-Process -Id $PID).Path `
                -ArgumentList $childArguments -PassThru -WindowStyle Hidden
            Add-E2EBackground -Id $child.Id
            $how = if ($Hibernate) { 'hibernate' } else { 'sleep' }
            Write-Host "the PC will $how $SleepAfter s after the call is measured."
        }

        Set-Location $E2E.App
        $testLog = Join-Path $E2E.Work "w8_$Phase.log"
        & flutter test integration_test/windows/w8_test.dart -d windows `
            --dart-define=OPENCORD_E2E_INVITE="$invite" `
            --dart-define=OPENCORD_E2E_PHASE="$Phase" `
            --dart-define=OPENCORD_E2E_HEADSET="$Headset" `
            --dart-define=OPENCORD_E2E_OUTPUT_ID="$($fallbackDevice.Id)" `
            --dart-define=OPENCORD_E2E_INPUT_ID="$($silent.Id)" `
            --dart-define=OPENCORD_E2E_RECORD="$($record.Name)" `
            --dart-define=OPENCORD_E2E_MARKER="$marker" `
            *> $testLog
        $script:status = $LASTEXITCODE
        Select-String -Path $testLog -Pattern 'W8[ :].*' |
            ForEach-Object { Write-Host "  $($_.Matches.Value)" }
        if ($script:status -ne 0) {
            # The hardware never moved: the person did not get to it, or the
            # PC did not sleep. That is not a failure of the app.
            $missed = Select-String -Path $testLog -Pattern `
                'Timed out: the headset|the PC never slept'
            if ($missed) {
                $script:inconclusive = $true
                foreach ($line in $missed) { Write-Host "  $($line.Matches.Value)" }
            } else {
                Get-Content $testLog -Tail 40 | Write-Host
            }
        }
        # The body's own exit code is the test's, which is handled above.
        $global:LASTEXITCODE = 0
    }
} finally {
    if ($previousDefault) {
        Set-AudioDevice -ID $previousDefault | Out-Null
        Write-Host "default playback put back to $((Get-AudioDevice -Playback).Name)"
    }
}

if ($inconclusive) {
    Write-Host ''
    Write-Host "The $Phase item proved nothing: the hardware never moved."
    exit 2
}
if ($status -ne 0) {
    Write-Host ''
    Write-Host "FAILED: the $Phase item."
    exit 1
}

Write-Host ''
Write-Host "The $Phase checklist item passed."
