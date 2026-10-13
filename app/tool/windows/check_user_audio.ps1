# The per-user voice menu (desktop UI plan §17.3) against a real server,
# measured at the speaker — the Windows twin of app/tool/check_user_audio.sh
# (Windows plan W1, step 4). Talker (a voicebot) sends a 440 Hz tone, Player
# plays an 880 Hz soundboard sound every 3 s. The app
# (integration_test/windows/user_audio_test.dart) plays into CABLE Input and
# takes a digitally silent microphone, so nothing reaches your speakers or
# comes from your microphone; CABLE Output, which hears CABLE Input back, is
# recorded with ffmpeg and each tone measured while the menu sets Talker to
# 50 % and 200 %, mutes and unmutes them, and mutes Player's soundboard. Its
# own server, data folder and profile, so it never touches yours.
#
#   pwsh -File app/tool/windows/check_user_audio.ps1
#
# Where Linux uses two PipeWire null sinks, this uses the one VB-CABLE pair
# plus any digitally silent capture device for the microphone (windows.md §7).
# -SilentInput names that device; the default is WO Mic Device, which is silent
# whenever its phone client is not connected.

param(
    [string]$SilentInput = 'WO Mic',
    [string]$OutputDevice = 'CABLE Input',
    [string]$RecordDevice = 'CABLE Output'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. "$PSScriptRoot\e2e_server.ps1"
. "$PSScriptRoot\audio_devices.ps1"

$output = Get-E2EAudioDeviceId -Name $OutputDevice -Kind Render
$silent = Get-E2EAudioDeviceId -Name $SilentInput -Kind Capture
$record = Get-E2EAudioDeviceId -Name $RecordDevice -Kind Capture
Write-Host "the app plays into : $($output.Name)  $($output.Id)"
Write-Host "its microphone     : $($silent.Name)  $($silent.Id)"
Write-Host "recorded with ffmpeg: $($record.Name)"

Invoke-E2ECheck -Profile e2e_user_audio {
    $manifest = Join-Path $E2E.Repo 'Cargo.toml'
    $env:CARGO_INCREMENTAL = '0'
    & cargo build --quiet --manifest-path $manifest -p opencord-voicebot
    if ($LASTEXITCODE -ne 0) { throw 'the voicebot did not build.' }
    $bot = Join-Path $E2E.Repo 'target\debug\opencord-voicebot.exe'

    # The 880 Hz soundboard sound Player uploads, written with ffmpeg rather
    # than Python's wave module, since ffmpeg is already a dependency here.
    $beep = Join-Path $E2E.Work 'beep.wav'
    & ffmpeg -hide_banner -loglevel error -f lavfi `
        -i 'sine=frequency=880:duration=1.5:sample_rate=48000' `
        -af 'volume=0.3' -ac 1 -y $beep
    if ($LASTEXITCODE -ne 0) { throw 'ffmpeg did not write the soundboard sound.' }

    $invite = Invoke-E2EClaimAndInvite -Bot $bot -BotArguments @(
        '--upload-sound', $beep, '--sound-name', 'Beep'
    )

    $claimLog = Get-Content (Join-Path $E2E.Work 'claim.log') -Raw
    $sound = [regex]::Match($claimLog, 'server:[0-9a-f]{64}').Value
    if (-not $sound) { throw "The owner did not upload the sound:`n$claimLog" }

    foreach ($bot_args in @(
            @('--name', 'Talker', '--tone', '440', '--seconds', '900'),
            @('--name', 'Player', '--tone', '0', '--sound', $sound, '--sound-every', '3', '--seconds', '900')
        )) {
        $name = $bot_args[1]
        $log = Join-Path $E2E.Work "$name.log"
        $process = Start-Process -FilePath $bot `
            -ArgumentList (@('--server', $invite) + $bot_args) `
            -RedirectStandardOutput $log -RedirectStandardError "$log.err" `
            -PassThru -WindowStyle Hidden
        Add-E2EBackground -Id $process.Id
    }

    Set-Location $E2E.App
    $testLog = Join-Path $E2E.Work 'user_audio.log'
    & flutter test integration_test/windows/user_audio_test.dart -d windows `
        --dart-define=OPENCORD_E2E_INVITE="$invite" `
        --dart-define=OPENCORD_E2E_OUTPUT_ID="$($output.Id)" `
        --dart-define=OPENCORD_E2E_INPUT_ID="$($silent.Id)" `
        --dart-define=OPENCORD_E2E_RECORD="$($record.Name)" `
        *> $testLog
    $status = $LASTEXITCODE
    Select-String -Path $testLog -Pattern 'USER AUDIO: .*' |
        ForEach-Object { $_.Matches.Value } | Write-Host
    if ($status -ne 0) {
        Get-Content $testLog | Write-Host
        throw "the app's measurements failed."
    }
}

Write-Host 'Per-user audio check passed.'
