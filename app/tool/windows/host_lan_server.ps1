# A real server on the LAN, for the three-machine call in the §15 checklist
# (Windows plan W8, step 1, last item): this PC, the Linux machine and the Mac
# in one call through one server, talking both ways and watching each other's
# screens.
#
#   pwsh -File app/tool/windows/host_lan_server.ps1
#
# It is not a check and measures nothing: it removes the three things that
# otherwise go wrong when the server is not on loopback — the invite link
# carrying `localhost`, Windows Firewall dropping the gateway and the media
# port, and the data folder being a temporary one that takes the identities
# with it when the call ends.
#
# Unlike `e2e_server.ps1` this server keeps its data (`-Data`), so the three
# machines stay joined between runs, and it is a release build by default:
# a debug server's processing delays distort the node's bandwidth estimates
# (docs/decisions.md D28), which is the whole point of a real call.
#
# Ctrl-C stops it. The database stays; `-Fresh` starts over.

param(
    # Where the database and the TLS certificate live, across runs.
    [string]$Data = (Join-Path $env:LOCALAPPDATA 'Opencord-LAN'),
    # The address the other two machines reach this PC on. Found from the
    # interface that has the default gateway when this is left out.
    [string]$Address,
    [int]$Port = 7710,
    [int]$VoicePort = 7711,
    [string]$Name = 'Opencord LAN',
    # A debug build starts sooner; the call's numbers mean less (D28).
    [switch]$DebugBuild,
    # Throw the database away and claim the server again.
    [switch]$Fresh
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path

if (-not $Address) {
    # The interface the PC actually reaches the LAN through: the one with a
    # default gateway and the lowest metric. Picking by name or by "first"
    # finds Hyper-V's or a VPN's.
    $route = Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue |
        Sort-Object RouteMetric, ifMetric | Select-Object -First 1
    if (-not $route) { throw 'This PC has no default route, so it is not on a LAN.' }
    $Address = (Get-NetIPAddress -InterfaceIndex $route.ifIndex -AddressFamily IPv4 |
            Where-Object { $_.IPAddress -ne '127.0.0.1' } | Select-Object -First 1).IPAddress
    if (-not $Address) { throw 'No IPv4 address on the interface with the default route.' }
}
Write-Host "this PC on the LAN  : $Address"
Write-Host "gateway             : TCP $Port"
Write-Host "voice media         : UDP $VoicePort"
Write-Host "data folder         : $Data"

# Windows Firewall blocks both ports for a program it has never seen, and a
# blocked UDP port looks exactly like a working call with no sound. Adding a
# rule needs administrator, so this says what to run rather than failing.
# The rules are for every profile, not just Private: a laptop's Wi-Fi is often
# marked Public, and a rule on the wrong profile fails in exactly the way it
# was meant to prevent.
$elevated = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).
    IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
$wanted = @(
    @{ Name = "Opencord LAN gateway (TCP $Port)"; Protocol = 'TCP'; Port = $Port },
    @{ Name = "Opencord LAN media (UDP $VoicePort)"; Protocol = 'UDP'; Port = $VoicePort }
)
$missing = @()
foreach ($rule in $wanted) {
    if (Get-NetFirewallRule -DisplayName $rule.Name -ErrorAction SilentlyContinue) { continue }
    if ($elevated) {
        New-NetFirewallRule -DisplayName $rule.Name -Direction Inbound -Action Allow `
            -Protocol $rule.Protocol -LocalPort $rule.Port -Profile Any | Out-Null
        Write-Host "firewall            : added $($rule.Name)"
    } else {
        $missing += $rule
    }
}
if ($missing) {
    Write-Host ''
    Write-Host 'Windows Firewall has no rule for these ports. In an administrator'
    Write-Host 'PowerShell, run:'
    foreach ($rule in $missing) {
        Write-Host ("  New-NetFirewallRule -DisplayName '$($rule.Name)' -Direction Inbound " +
            "-Action Allow -Protocol $($rule.Protocol) -LocalPort $($rule.Port) -Profile Any")
    }
    Write-Host 'Without them the other machines either cannot connect at all, or join'
    Write-Host 'the call and hear nothing.'
    Write-Host ''
}

if ($Fresh -and (Test-Path $Data)) { Remove-Item $Data -Recurse -Force }
New-Item -ItemType Directory -Force -Path $Data | Out-Null

$profileName = if ($DebugBuild) { 'debug' } else { 'release' }
$build = @('build', '--manifest-path', (Join-Path $repo 'Cargo.toml'), '-p', 'opencord-server')
if (-not $DebugBuild) { $build += '--release' }
$env:CARGO_INCREMENTAL = '0'
Write-Host "building the $profileName server (a first release build takes a while)..."
& cargo @build
if ($LASTEXITCODE -ne 0) { throw 'opencord-server did not build.' }
$server = Join-Path $repo "target\$profileName\opencord-server.exe"

$env:OPENCORD_BIND = "0.0.0.0:$Port"
$env:OPENCORD_PUBLIC_HOST = $Address
$env:OPENCORD_VOICE_UDP_PORT = $VoicePort
$env:OPENCORD_DATA_DIR = $Data
$env:OPENCORD_SERVER_NAME = $Name

Write-Host ''
Write-Host 'Then, in this order:'
Write-Host "  1. On this PC, open Opencord, Add server, address $($Address):$Port, and"
Write-Host '     paste the owner claim token this prints below.'
Write-Host '  2. Copy the invite= link it prints once the server has an owner, and'
Write-Host '     add the server with it on the Mac and on the Linux machine.'
Write-Host '  3. All three join the same voice channel. Both directions, then each'
Write-Host "     machine watches another's screen share."
Write-Host ''
Write-Host 'Ctrl-C stops the server. The identities stay for the next run.'
Write-Host ''

& $server
