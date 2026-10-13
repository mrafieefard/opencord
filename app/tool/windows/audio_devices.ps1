# Turning a device's name into the `wasapi:` id the app knows it by, for the
# Windows checks. Dot-source it beside e2e_server.ps1.
#
# The app's ids come from cpal, which spells a device "<friendly name>
# (<device name>)" — and those two strings live in different registry values
# under the endpoint, so all three spellings are matched. Only endpoints that
# are present and enabled (DeviceState 1) count, so an unplugged headset does
# not shadow the device a check meant.
#
# The ids this builds were checked against what the app itself reports
# (crates/opencord-media/tests/devices_windows.rs): for CABLE Input, CABLE
# Output and WO Mic Device they are identical.

Set-StrictMode -Version Latest

function Get-E2EAudioDeviceId {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][ValidateSet('Render', 'Capture')][string]$Kind
    )
    $friendlyKey = '{a45c254e-df1c-4efd-8020-67d146a850e0},2'
    $deviceKey = '{b3f8fa53-0004-438e-9003-51a46e139bfc},6'
    $prefix = if ($Kind -eq 'Render') { '{0.0.0.00000000}' } else { '{0.0.1.00000000}' }
    $root = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$Kind"

    $hits = @(foreach ($endpoint in Get-ChildItem $root -ErrorAction SilentlyContinue) {
            $properties = Get-ItemProperty "$($endpoint.PSPath)\Properties" -ErrorAction SilentlyContinue
            if (-not $properties) { continue }
            $friendly = $properties.$friendlyKey
            $device = $properties.$deviceKey
            $composite = if ($friendly -and $device) { "$friendly ($device)" } else { $friendly }
            $matched = ($composite -like "*$Name*") -or
                ($friendly -like "*$Name*") -or
                ($device -like "*$Name*")
            if (-not $matched) { continue }
            if ((Get-ItemProperty $endpoint.PSPath -ErrorAction SilentlyContinue).DeviceState -ne 1) { continue }
            [pscustomobject]@{
                Id   = "wasapi:$prefix.$($endpoint.PSChildName)"
                Name = $composite
            }
        })

    if ($hits.Count -eq 0) { throw "No enabled $Kind device's name contains '$Name'." }
    if ($hits.Count -gt 1) {
        throw ("'$Name' matches $($hits.Count) $Kind devices: " +
            (($hits | ForEach-Object { $_.Name }) -join ', '))
    }
    $hits[0]
}
