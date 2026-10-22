# Phase 1 M6 acceptance on Windows, the twin of app/tool/check_m6.sh: two
# identities chat in real time, and when the owner gives the other a role and
# restricts #general, that user's UI follows without reconnecting. The app is
# the member (integration_test/m6_test.dart); the owner is
# crates/opencord-core/examples/m6_peer.rs, on the same core. Its own server,
# data folder and profile, so it never touches yours.
#
#   pwsh -File app/tool/windows/check_m6.ps1
#
# Windows plan W0, step 4: this is how "the app opens a text channel and sends
# a message" is checked on Windows, rather than by hand.

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. "$PSScriptRoot\e2e_server.ps1"

Invoke-E2ECheck -Profile e2e_member {
    $manifest = Join-Path $E2E.Repo 'Cargo.toml'
    $env:CARGO_INCREMENTAL = '0'
    & cargo build --quiet --manifest-path $manifest -p opencord_core --example m6_peer
    if ($LASTEXITCODE -ne 0) { throw 'm6_peer did not build.' }

    # The owner claims the server, then leaves an invite for the app.
    $invitePath = Join-Path $E2E.Work 'invite'
    $ownerLog = Join-Path $E2E.Work 'owner.log'
    $owner = Start-Process -FilePath (Join-Path $E2E.Repo 'target\debug\examples\m6_peer.exe') `
        -ArgumentList "127.0.0.1:$($E2E.Port)", $E2E.Claim, $invitePath `
        -RedirectStandardOutput $ownerLog -RedirectStandardError "$ownerLog.err" `
        -PassThru -WindowStyle Hidden
    Add-E2EBackground -Id $owner.Id

    foreach ($attempt in 1..150) {
        if ((Test-Path $invitePath) -and (Get-Item $invitePath).Length -gt 0) { break }
        if ($owner.HasExited) { throw 'The owner stopped before inviting anyone.' }
        Start-Sleep -Milliseconds 200
    }
    if (-not ((Test-Path $invitePath) -and (Get-Item $invitePath).Length -gt 0)) {
        throw 'No invite from the owner.'
    }
    $invite = (Get-Content $invitePath -Raw).Trim()

    Set-Location $E2E.App
    & flutter test integration_test/m6_test.dart -d windows `
        --dart-define=OPENCORD_E2E_SERVER="127.0.0.1:$($E2E.Port)" `
        --dart-define=OPENCORD_E2E_INVITE="$invite"
    if ($LASTEXITCODE -ne 0) { throw 'The member app failed.' }

    # The owner's own steps must have gone through as well.
    $owner.WaitForExit(60000) | Out-Null
    if (-not $owner.HasExited) { throw 'The owner did not finish.' }
    if ($owner.ExitCode -ne 0) {
        throw ("The owner failed:`n" + ((Get-Content $ownerLog, "$ownerLog.err" -Tail 40 -ErrorAction SilentlyContinue) -join "`n"))
    }
}

Write-Host 'M6 check passed.'
