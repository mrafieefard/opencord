# The PowerShell 7 twin of app/tool/e2e_server.sh, for the Windows check
# scripts: an opencord-server of their own on a free port, in a temporary
# folder that goes when the check exits, and a data folder of its own for the
# app, so a check never touches your server, your data or your identity.
#
# Dot-source it, then run the check's body through Invoke-E2ECheck, which
# stops everything it started even when the body throws:
#
#   . "$PSScriptRoot\e2e_server.ps1"
#   Invoke-E2ECheck -Profile e2e_example {
#     flutter test integration_test/windows/example_test.dart -d windows `
#       --dart-define=OPENCORD_E2E_SERVER="127.0.0.1:$($E2E.Port)"
#   }
#
# Inside the body $E2E has App, Repo, Work, Port, Claim and Server; anything
# the body starts can be handed to Add-E2EBackground to be stopped at exit.
#
# Windows plan W0, step 5. The differences from the bash original, and why:
#   - A free port comes from a TcpListener on port 0, not python3: Git Bash's
#     python3 is unreliable on Windows (windows.md §3, decision F).
#   - The app's data folder is isolated with OPENCORD_PROFILE alone. There is
#     no XDG_DATA_HOME on Windows; getApplicationSupportDirectory is
#     %APPDATA%\dev.opencord\Opencord, and a profile is a folder inside it
#     (app/lib/app_start.dart). The profile folder is removed before and
#     after the check, so a run starts as fresh as the Linux one does.
#   - Processes are stopped with their whole tree (taskkill /T), because a
#     cargo-run or flutter child outlives a bare Stop-Process.

Set-StrictMode -Version Latest

# The check's state, filled in by Start-E2EServer.
$script:E2E = [ordered]@{
    App        = ''
    Repo       = ''
    Work       = ''
    Port       = 0
    Claim      = ''
    Server     = ''
    Profile    = ''
    Background = [System.Collections.Generic.List[int]]::new()
}

# A loopback port nothing is listening on. The listener is closed before the
# server is told to bind it, so there is a small race, as in the bash original.
function Get-E2EFreePort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    $listener.Start()
    try { $listener.LocalEndpoint.Port } finally { $listener.Stop() }
}

# Adds a process to the list stopped when the check exits.
function Add-E2EBackground {
    param([Parameter(Mandatory)][int]$Id)
    $script:E2E.Background.Add($Id)
}

# Stops one process and everything it started, and waits for it to go.
# taskkill returns before the process has died, and a dead process's working
# folder stays locked for a moment after; without the wait the work folder
# cannot be removed. Already gone is not a failure.
function Stop-E2ETree {
    param([Parameter(Mandatory)][int]$Id)
    & taskkill.exe /PID $Id /T /F *> $null
    foreach ($attempt in 1..100) {
        if (-not (Get-Process -Id $Id -ErrorAction SilentlyContinue)) { return }
        Start-Sleep -Milliseconds 50
    }
}

# Removes a folder, giving Windows a moment to let go of it.
function Remove-E2EFolder {
    param([Parameter(Mandatory)][string]$Path)
    foreach ($attempt in 1..20) {
        if (-not (Test-Path $Path)) { return $true }
        Remove-Item $Path -Recurse -Force -ErrorAction SilentlyContinue
        if (-not (Test-Path $Path)) { return $true }
        Start-Sleep -Milliseconds 100
    }
    $false
}

# The app's data folder for a profile (app/lib/app_start.dart).
function Get-E2EProfileDir {
    param([Parameter(Mandatory)][string]$Profile)
    Join-Path $env:APPDATA "dev.opencord\Opencord\profiles\$Profile"
}

# Starts the server the check talks to and fills in $E2E. -Profile is the
# app's OPENCORD_PROFILE; it is exported and its data folder is emptied.
function Start-E2EServer {
    param([Parameter(Mandatory)][string]$Profile)

    $app = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
    $repo = Split-Path $app -Parent

    # A check killed outright — Ctrl-C, or a harness stopping it — runs no
    # finally, so it leaves its work folder behind. Clear the old ones first;
    # one still in use simply refuses and is left alone.
    foreach ($stale in Get-ChildItem ([System.IO.Path]::GetTempPath()) -Directory -Filter 'opencord-e2e-*' -ErrorAction SilentlyContinue) {
        Remove-Item $stale.FullName -Recurse -Force -ErrorAction SilentlyContinue
    }

    $work = Join-Path ([System.IO.Path]::GetTempPath()) ("opencord-e2e-" + [guid]::NewGuid().ToString('N').Substring(0, 12))
    New-Item -ItemType Directory -Path $work | Out-Null

    $script:E2E.App = $app
    $script:E2E.Repo = $repo
    $script:E2E.Work = $work
    $script:E2E.Port = Get-E2EFreePort
    $script:E2E.Profile = $Profile

    # A check that measures sets E2E_SERVER_PROFILE=release before this: a
    # debug build's processing delays distort the node's bandwidth estimates
    # (docs/decisions.md D28).
    $serverProfile = if ($env:E2E_SERVER_PROFILE) { $env:E2E_SERVER_PROFILE } else { 'debug' }
    $manifest = Join-Path $repo 'Cargo.toml'
    $build = @('build', '--quiet', '--manifest-path', $manifest, '-p', 'opencord-server')
    if ($serverProfile -eq 'release') { $build += '--release' }
    # Incremental compilation is off on Windows: it makes cargokit's deep
    # target folders deeper still and buys nothing in a check (windows.md §0).
    $previousIncremental = $env:CARGO_INCREMENTAL
    $env:CARGO_INCREMENTAL = '0'
    try { & cargo @build } finally { $env:CARGO_INCREMENTAL = $previousIncremental }
    if ($LASTEXITCODE -ne 0) { throw "opencord-server did not build." }

    $script:E2E.Server = Join-Path $repo "target\$serverProfile\opencord-server.exe"
    if (-not (Test-Path $script:E2E.Server)) { throw "No server at $($script:E2E.Server)." }

    # The app's profile folder, fresh. Its keychain entries are the credential
    # manager's and are left alone; the app replaces them when it signs in.
    $profileDir = Get-E2EProfileDir -Profile $Profile
    if (Test-Path $profileDir) { Remove-Item $profileDir -Recurse -Force }
    $env:OPENCORD_PROFILE = $Profile

    Start-E2EServerProcess -Log 'server.log' | Out-Null

    # The token is in the first start's log; drop colour codes before reading it.
    $script:E2E.Claim = Wait-E2ELogMatch -Log 'server.log' -Pattern 'claim_token=([A-Za-z0-9_-]+)' `
        -What 'The server did not start'
}

# Starts opencord-server on $E2E.Port, logging to $E2E.Work\$Log, and returns
# its process id.
function Start-E2EServerProcess {
    param([Parameter(Mandatory)][string]$Log)

    $serverDir = Join-Path $script:E2E.Work 'server'
    New-Item -ItemType Directory -Path $serverDir -Force | Out-Null

    $env:OPENCORD_BIND = "127.0.0.1:$($script:E2E.Port)"
    $env:OPENCORD_PUBLIC_HOST = '127.0.0.1'
    $env:OPENCORD_VOICE_UDP_PORT = '0'

    $logPath = Join-Path $script:E2E.Work $Log
    $process = Start-Process -FilePath $script:E2E.Server -WorkingDirectory $serverDir `
        -RedirectStandardOutput $logPath -RedirectStandardError "$logPath.err" `
        -PassThru -WindowStyle Hidden
    Add-E2EBackground -Id $process.Id
    $process.Id
}

# Waits up to 30 s for a log line to match, and returns the first group.
function Wait-E2ELogMatch {
    param(
        [Parameter(Mandatory)][string]$Log,
        [Parameter(Mandatory)][string]$Pattern,
        [Parameter(Mandatory)][string]$What
    )
    $logPath = Join-Path $script:E2E.Work $Log
    foreach ($attempt in 1..150) {
        if (Test-Path $logPath) {
            # Colour codes first, as the bash original's sed does.
            $text = (Get-Content $logPath -Raw -ErrorAction SilentlyContinue) -replace "`e\[[0-9;]*m", ''
            if ($null -ne $text) {
                $match = [regex]::Match($text, $Pattern)
                if ($match.Success) { return $match.Groups[1].Value }
            }
        }
        Start-Sleep -Milliseconds 200
    }
    $tail = @()
    foreach ($suffix in '', '.err') {
        $path = "$logPath$suffix"
        if (Test-Path $path) { $tail += Get-Content $path -Tail 40 }
    }
    throw ("${What}:`n" + ($tail -join "`n"))
}

# Claims the server as Owner with the voicebot at -Bot (further arguments in
# -BotArguments), then starts it again — a server with an owner logs its
# invite link at startup — and returns that link.
function Invoke-E2EClaimAndInvite {
    param(
        [Parameter(Mandatory)][string]$Bot,
        [string[]]$BotArguments = @()
    )
    $claimLog = Join-Path $script:E2E.Work 'claim.log'
    & $Bot --server "127.0.0.1:$($script:E2E.Port)" --claim $script:E2E.Claim `
        --name Owner --tone 0 --seconds 1 @BotArguments *> $claimLog
    if ($LASTEXITCODE -ne 0) {
        throw ("The voicebot did not claim the server:`n" + ((Get-Content $claimLog -Tail 40) -join "`n"))
    }

    $first = $script:E2E.Background[0]
    Stop-E2ETree -Id $first
    $script:E2E.Background.Remove($first) | Out-Null

    Start-E2EServerProcess -Log 'server2.log' | Out-Null
    Wait-E2ELogMatch -Log 'server2.log' -Pattern 'invite=(\S+)' -What 'The server logged no invite'
}

# Stops everything the check started and removes what it wrote.
function Stop-E2EServer {
    foreach ($id in @($script:E2E.Background)) { Stop-E2ETree -Id $id }
    $script:E2E.Background.Clear()
    if ($script:E2E.Profile) {
        Remove-E2EFolder -Path (Get-E2EProfileDir -Profile $script:E2E.Profile) | Out-Null
    }
    if ($script:E2E.Work) { Remove-E2EFolder -Path $script:E2E.Work | Out-Null }
}

# Runs a check's body against a server of its own, and cleans up whatever
# happens. Throws what the body threw, so the check's exit code is the body's.
function Invoke-E2ECheck {
    param(
        [Parameter(Mandatory)][string]$Profile,
        [Parameter(Mandatory, Position = 0)][scriptblock]$Body
    )
    try {
        Start-E2EServer -Profile $Profile
        # Cleared first: a stale code from the build above would otherwise fail
        # a body whose own last command was a cmdlet, not a program.
        $global:LASTEXITCODE = 0
        & $Body
        if ($LASTEXITCODE -ne 0) { throw "The check's body exited with $LASTEXITCODE." }
    } finally {
        Stop-E2EServer
    }
}
