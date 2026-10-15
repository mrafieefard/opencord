# Holds one key down for a while, wherever the focus is, and lets it go
# (Windows plan W2, step 7). `keybd_event` puts the key into the same stream a
# real keyboard uses, so a low-level hook sees it exactly as it sees a typed
# key — which is the point: the check needs push-to-talk held while another
# app has the focus, and no person at the PC.
#
#   pwsh -File app/tool/windows/hold_key.ps1 -VirtualKey 0x7C -Seconds 12
#
# -VirtualKey is a Windows virtual-key code: 0x7C is F13, a key no app uses.
# -Extended is for the keys that need the extended-key flag (the right Ctrl,
# the navigation cluster, the keypad's Enter).

param(
    [Parameter(Mandatory)][int]$VirtualKey,
    [double]$Seconds = 10,
    [switch]$Extended
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Add-Type -Namespace Opencord -Name Keyboard -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("user32.dll")]
public static extern void keybd_event(byte vk, byte scan, uint flags, System.UIntPtr extra);

[System.Runtime.InteropServices.DllImport("user32.dll")]
public static extern System.IntPtr GetForegroundWindow();

[System.Runtime.InteropServices.DllImport("user32.dll")]
public static extern uint GetWindowThreadProcessId(System.IntPtr window, out uint process);
'@

# Which app the key is really going to. The caller needs this: a check that
# wants the key pressed while something else has the focus has to be able to
# say that something else did.
$owner = 0
[void][Opencord.Keyboard]::GetWindowThreadProcessId(
    [Opencord.Keyboard]::GetForegroundWindow(), [ref]$owner)
$foreground = try {
    (Get-Process -Id $owner -ErrorAction Stop).ProcessName
} catch {
    'unknown'
}
Write-Host "foreground: $foreground"

$extendedKey = 0x1
$keyUp = 0x2
$flags = if ($Extended) { $extendedKey } else { 0 }

Write-Host ("key 0x{0:X2} down" -f $VirtualKey)
[Opencord.Keyboard]::keybd_event([byte]$VirtualKey, 0, $flags, [UIntPtr]::Zero)
try {
    Start-Sleep -Seconds $Seconds
} finally {
    # Always let the key go: a key left down would hold push-to-talk open for
    # whatever runs next, and look like a pass.
    [Opencord.Keyboard]::keybd_event([byte]$VirtualKey, 0, $flags -bor $keyUp, [UIntPtr]::Zero)
    Write-Host ("key 0x{0:X2} up" -f $VirtualKey)
}
