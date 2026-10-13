# Phase 1 M5 acceptance on Windows, the twin of app/tool/check_m5.sh: a fresh
# install creates an identity, joins a local server as its owner (TOFU),
# restarts and is still connected. Runs the app twice through
# integration_test/m5_test.dart, with its own server, data folder and profile,
# so it never touches yours.
#
#   pwsh -File app/tool/windows/check_m5.ps1
#
# Windows plan W0, step 4: this is how "the app runs and signs in" is checked
# on Windows, rather than by hand.

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. "$PSScriptRoot\e2e_server.ps1"

Invoke-E2ECheck -Profile e2e {
    Set-Location $E2E.App
    foreach ($phase in 'join', 'restart') {
        & flutter test integration_test/m5_test.dart -d windows `
            --dart-define=OPENCORD_E2E_SERVER="127.0.0.1:$($E2E.Port)" `
            --dart-define=OPENCORD_E2E_CLAIM="$($E2E.Claim)" `
            --dart-define=OPENCORD_E2E_PHASE="$phase"
        if ($LASTEXITCODE -ne 0) { throw "The $phase phase failed." }
    }
}

Write-Host 'M5 check passed.'
