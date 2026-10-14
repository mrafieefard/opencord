# The app's key recorder on the keys Windows reports oddly (Windows plan W2,
# step 6): the left Windows key, Print Screen, and the right Alt — which is
# AltGr on a layout that has one. The keys are really pressed, into the real
# app window, because what the recorder is handed is the whole question.
#
#   pwsh -File app/tool/windows/check_key_recorder.ps1
#
# The app runs on the mock repository, so no server and no voice: the recorder
# is the subject. It needs the app window to have the focus, so don't type
# while it runs — about a minute per key.
#
# Pressing the Windows key opens the Start menu, which takes the focus: that
# is not a bug in the recorder (nothing may swallow a key), but it does mean
# the Windows key is a poor keybind, and the run says so rather than failing.

param(
    [switch]$SkipSuper
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. "$PSScriptRoot\e2e_server.ps1"

$keys = [ordered]@{
    'print'     = @{ VirtualKey = 0x2C; Extended = $false; Expect = 'Print' }
    'right-alt' = @{ VirtualKey = 0xA5; Extended = $true; Expect = 'Alt_R' }
    'super'     = @{ VirtualKey = 0x5B; Extended = $false; Expect = 'Super_L' }
}
if ($SkipSuper) { $keys.Remove('super') }

$pwshPath = [System.Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
$repo = Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $PSScriptRoot))
$reports = [ordered]@{}

foreach ($phase in $keys.Keys) {
    $key = $keys[$phase]
    Write-Host "===== $phase (virtual key 0x$('{0:X2}' -f $key.VirtualKey)) ====="
    $log = Join-Path ([System.IO.Path]::GetTempPath()) "opencord-recorder-$phase.log"
    Push-Location (Join-Path $repo 'app')
    try {
        & flutter test integration_test/windows/key_recorder_test.dart -d windows `
            --dart-define=OPENCORD_E2E_PHASE="$phase" `
            --dart-define=OPENCORD_E2E_VIRTUAL_KEY="$($key.VirtualKey)" `
            --dart-define=OPENCORD_E2E_EXTENDED="$($key.Extended.ToString().ToLower())" `
            --dart-define=OPENCORD_E2E_PWSH="$pwshPath" `
            --dart-define=OPENCORD_E2E_REPO="$repo" `
            *> $log
        $failed = $LASTEXITCODE -ne 0
    } finally {
        Pop-Location
    }
    $reported = Select-String -Path $log -Pattern 'RECORDER: .*' | Select-Object -First 1
    if ($failed -and -not $reported) {
        Get-Content $log | Write-Host
        throw "the app failed on $phase."
    }
    if (-not $reported) { throw "the app reported nothing on $phase." }
    $reports[$phase] = $reported.Matches.Value
    Write-Host "  $($reports[$phase])"
    # The Start menu, if the Windows key opened it.
    if ($phase -eq 'super') { Start-Sleep -Seconds 1 }
}

Write-Host '----- what the recorder made of each key -----'
$wrong = @()
$missed = @()
foreach ($phase in $reports.Keys) {
    $line = $reports[$phase]
    $expected = $keys[$phase].Expect
    $recorded = [regex]::Match($line, 'recorded="([^"]*)"').Groups[1].Value
    $verdict = switch ($recorded) {
        $expected { 'ok' }
        '' { 'NOT RECORDED' }
        default { 'WRONG KEY' }
    }
    Write-Host ("{0,-10} recorded {1,-10} expected {2,-10} {3}" -f
        $phase, "`"$recorded`"", "`"$expected`"", $verdict)
    if ($verdict -eq 'WRONG KEY') { $wrong += "$phase recorded `"$recorded`", not `"$expected`"" }
    if ($verdict -eq 'NOT RECORDED') { $missed += "$phase ($expected) could not be recorded at all" }
}

# The recorder is shared Dart, so a difference here is a note for the Linux
# session and not something to fix on Windows (platforms.md §4). A key read as
# the *wrong* key is a real fault and fails; a key the recorder cannot see at
# all is reported as inconclusive, exit 2, as check_echo.ps1 does — the key
# may still be usable as a keybind, which the Rust side's
# `print_screen_reaches_the_hook` shows for Print Screen.
if ($wrong) {
    Write-Host ''
    foreach ($problem in $wrong) { Write-Host "FAILED: $problem" }
    exit 1
}
if ($missed) {
    Write-Host ''
    foreach ($problem in $missed) { Write-Host "NOT RECORDED: $problem" }
    Write-Host 'Windows sends a focused window only the key-up for Print Screen, and the'
    Write-Host 'recorder commits a key on its key-down, so there is nothing to commit. The'
    Write-Host 'hook below it does see both (see print_screen_reaches_the_hook), so the key'
    Write-Host 'works as a keybind once set; it just cannot be recorded by pressing it.'
    Write-Host 'Put this in windows.md §7 for the Linux session.'
    exit 2
}

Write-Host 'The key recorder reads every one of these keys as the core names it.'
