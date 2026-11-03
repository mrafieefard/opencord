# Drives a real Opencord window from the command line: window styles and
# rects, the actual mouse and keyboard, and screenshots. Dot-source it; the
# checks in check_window_chrome.ps1 are its first user.
#
#   . app/tool/windows/window_ui.ps1
#   Get-OcWindows | Format-Table
#
# Everything here is Win32 through P/Invoke, so it needs no GUI tooling and
# no test harness inside the app. What it cannot do is work on a locked
# workstation: injected input never reaches an application behind the lock
# screen, and a capture returns the lock screen instead of the desktop.
# Test-OcDesktopUsable is the guard for that, and every check script should
# call it before it believes a single result.

Set-StrictMode -Version Latest

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

if (-not ('Win32Ui' -as [type])) {
Add-Type -Namespace '' -Name Win32Ui -MemberDefinition @'
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int X, int Y, int cx, int cy, uint flags);
    [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr hWnd, int nIndex);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hWnd, out RECT lpRect);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, int dwExtraInfo);
    [DllImport("user32.dll")] public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, int dwExtraInfo);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr p);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint procId);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hWnd);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, System.Text.StringBuilder s, int n);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hWnd, System.Text.StringBuilder s, int n);

    public delegate bool EnumProc(IntPtr hWnd, IntPtr p);
    public struct RECT { public int Left, Top, Right, Bottom; }
'@
}

# The style bits that say whether Windows, rather than the app, is drawing
# the frame. A frameless window usually keeps WS_CAPTION and answers
# WM_NCCALCSIZE instead, so these describe the window, they do not settle
# whether a title bar is drawn: only a screenshot does that.
$script:OcStyles = [ordered]@{
    WS_CAPTION     = 0x00C00000
    WS_THICKFRAME  = 0x00040000
    WS_SYSMENU     = 0x00080000
    WS_MINIMIZEBOX = 0x00020000
    WS_MAXIMIZEBOX = 0x00010000
    WS_BORDER      = 0x00800000
    WS_DLGFRAME    = 0x00400000
}

function Test-OcDesktopUsable {
    <#  .SYNOPSIS
        False when the workstation is locked, so nothing below can be trusted.
        LogonUI owns the desktop while the lock screen is up: clicks and keys
        go nowhere and screenshots capture the lock screen. #>
    [OutputType([bool])]
    param([switch]$Explain)
    $locked = [bool](Get-Process LogonUI -ErrorAction SilentlyContinue)
    if ($locked -and $Explain) {
        Write-Warning 'The workstation is locked. Unlock it and run this again: injected input does not reach an application behind the lock screen, and screenshots capture the lock screen instead of the desktop.'
    }
    -not $locked
}

function Get-OcWindows {
    <#  Every visible top-level window belonging to an opencord.exe, with its
        style bits, rect, DPI and owning process. #>
    $out = New-Object System.Collections.ArrayList
    $cb = [Win32Ui+EnumProc] {
        param([IntPtr]$h, [IntPtr]$p)
        if (-not [Win32Ui]::IsWindowVisible($h)) { return $true }
        $owner = 0
        [void][Win32Ui]::GetWindowThreadProcessId($h, [ref]$owner)
        $proc = Get-Process -Id $owner -ErrorAction SilentlyContinue
        if (-not $proc -or $proc.ProcessName -ne 'opencord') { return $true }

        $title = New-Object System.Text.StringBuilder 512
        [void][Win32Ui]::GetWindowText($h, $title, $title.Capacity)
        if ([string]::IsNullOrWhiteSpace($title.ToString())) { return $true }
        $class = New-Object System.Text.StringBuilder 512
        [void][Win32Ui]::GetClassName($h, $class, $class.Capacity)

        $style = [Win32Ui]::GetWindowLong($h, -16)
        $r = New-Object Win32Ui+RECT
        [void][Win32Ui]::GetWindowRect($h, [ref]$r)
        $set = foreach ($k in $script:OcStyles.Keys) {
            if (($style -band $script:OcStyles[$k]) -eq $script:OcStyles[$k]) { $k }
        }
        [void]$out.Add([pscustomobject]@{
            Handle = $h; ProcessId = $owner; Title = $title.ToString(); Class = $class.ToString()
            X = $r.Left; Y = $r.Top; W = $r.Right - $r.Left; H = $r.Bottom - $r.Top
            Maximized = [Win32Ui]::IsZoomed($h); Minimized = [Win32Ui]::IsIconic($h)
            Dpi = [Win32Ui]::GetDpiForWindow($h)
            Style = ('0x{0:X8}' -f $style); StyleBits = ($set -join ',')
        })
        return $true
    }
    [void][Win32Ui]::EnumWindows($cb, [IntPtr]::Zero)
    $out
}

function Get-OcMainWindow {
    <#  The main window, by its class; the media viewer has its own. #>
    param([int]$ProcessId = 0)
    Get-OcWindows |
        Where-Object { $_.Class -eq 'OPENCORD_MAIN_WINDOW' -and ($ProcessId -eq 0 -or $_.ProcessId -eq $ProcessId) } |
        Select-Object -First 1
}

function Save-OcShot {
    <#  A PNG of the whole virtual desktop, or of one window plus padding. #>
    param([Parameter(Mandatory)][string]$Path, [IntPtr]$Handle = [IntPtr]::Zero, [int]$Pad = 0,
          [int]$Height = 0)
    if ($Handle -ne [IntPtr]::Zero) {
        $r = New-Object Win32Ui+RECT
        [void][Win32Ui]::GetWindowRect($Handle, [ref]$r)
        $x = $r.Left - $Pad; $y = $r.Top - $Pad
        $w = ($r.Right - $r.Left) + 2 * $Pad
        $h = if ($Height -gt 0) { $Height } else { ($r.Bottom - $r.Top) + 2 * $Pad }
    } else {
        $b = [System.Windows.Forms.SystemInformation]::VirtualScreen
        $x = $b.X; $y = $b.Y; $w = $b.Width; $h = $b.Height
    }
    if ($w -le 0 -or $h -le 0) { throw "nothing to capture ($w x $h)" }
    $dir = Split-Path -Parent $Path
    if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Force $dir | Out-Null }
    $bmp = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($x, $y, 0, 0, (New-Object System.Drawing.Size $w, $h))
    $g.Dispose()
    $bmp.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    $Path
}

function Get-OcPixel {
    <#  One screen pixel, for asking whether something is drawn where it should be. #>
    param([Parameter(Mandatory)][int]$X, [Parameter(Mandatory)][int]$Y)
    $bmp = New-Object System.Drawing.Bitmap 1, 1
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($X, $Y, 0, 0, (New-Object System.Drawing.Size 1, 1))
    $g.Dispose()
    $c = $bmp.GetPixel(0, 0)
    $bmp.Dispose()
    $c
}

# --- the mouse ---------------------------------------------------------------
function Invoke-OcClick {
    param([Parameter(Mandatory)][int]$X, [Parameter(Mandatory)][int]$Y, [int]$Count = 1)
    [void][Win32Ui]::SetCursorPos($X, $Y); Start-Sleep -Milliseconds 90
    for ($i = 0; $i -lt $Count; $i++) {
        [Win32Ui]::mouse_event(0x0002, 0, 0, 0, 0)   # LEFTDOWN
        [Win32Ui]::mouse_event(0x0004, 0, 0, 0, 0)   # LEFTUP
        if ($Count -gt 1) { Start-Sleep -Milliseconds 90 }
    }
    Start-Sleep -Milliseconds 250
}

function Invoke-OcDrag {
    <#  Presses, moves in steps so the app sees real motion, releases. #>
    param([Parameter(Mandatory)][int]$FromX, [Parameter(Mandatory)][int]$FromY,
          [Parameter(Mandatory)][int]$ToX,   [Parameter(Mandatory)][int]$ToY,
          [int]$Steps = 24)
    [void][Win32Ui]::SetCursorPos($FromX, $FromY); Start-Sleep -Milliseconds 150
    [Win32Ui]::mouse_event(0x0002, 0, 0, 0, 0); Start-Sleep -Milliseconds 150
    for ($i = 1; $i -le $Steps; $i++) {
        [void][Win32Ui]::SetCursorPos(
            [int]($FromX + ($ToX - $FromX) * $i / $Steps),
            [int]($FromY + ($ToY - $FromY) * $i / $Steps))
        Start-Sleep -Milliseconds 25
    }
    Start-Sleep -Milliseconds 150
    [Win32Ui]::mouse_event(0x0004, 0, 0, 0, 0); Start-Sleep -Milliseconds 450
}

# --- the keyboard ------------------------------------------------------------
$script:OcVk = @{
    LWIN = 0x5B; UP = 0x26; DOWN = 0x28; LEFT = 0x25; RIGHT = 0x27
    ESC  = 0x1B; ALT = 0x12; F4 = 0x73; RETURN = 0x0D; CTRL = 0x11
    SHIFT = 0x10; V = 0x56
}

function Send-OcKeys {
    <#  Holds the modifiers, taps the key, releases in reverse.
        e.g. Send-OcKeys -Mods LWIN -Key UP #>
    param([string[]]$Mods = @(), [Parameter(Mandatory)][string]$Key)
    foreach ($m in $Mods) { [Win32Ui]::keybd_event($script:OcVk[$m], 0, 0, 0) }
    Start-Sleep -Milliseconds 60
    [Win32Ui]::keybd_event($script:OcVk[$Key], 0, 0, 0); Start-Sleep -Milliseconds 60
    [Win32Ui]::keybd_event($script:OcVk[$Key], 0, 0x0002, 0)
    $undo = @($Mods); [array]::Reverse($undo)
    foreach ($m in $undo) { [Win32Ui]::keybd_event($script:OcVk[$m], 0, 0x0002, 0) }
    Start-Sleep -Milliseconds 500
}

function Set-OcFocus {
    <#  SetForegroundWindow is refused to a background process often enough
        that a real click is the only reliable way in. The caller says where
        is safe to click: somewhere empty, not a button.
        Snap Assist steals the foreground after a half-screen snap, so Esc
        first clears anything like it. #>
    param([Parameter(Mandatory)]$Window, [int]$OffsetX = 80, [int]$OffsetY = 110)
    Send-OcKeys -Key ESC
    Invoke-OcClick -X ($Window.X + $OffsetX) -Y ($Window.Y + $OffsetY)
    Start-Sleep -Milliseconds 300
    [Win32Ui]::GetForegroundWindow() -eq $Window.Handle
}

function Set-OcFocusOrStop {
    <#  Set-OcFocus returns whether the foreground really changed, and a check
        that ignores that answer blames the app for the harness's own missed
        click: a keystroke sent to a window that never took the focus does
        nothing, which looks exactly like a window that ignored the key.
        So try a few times, and say plainly when the focus never arrived. #>
    param([Parameter(Mandatory)]$Window, [int]$Tries = 4,
          [int]$OffsetX = 80, [int]$OffsetY = 110)
    foreach ($try in 1..$Tries) {
        if (Set-OcFocus -Window $Window -OffsetX $OffsetX -OffsetY $OffsetY) { return $true }
        Start-Sleep -Milliseconds 400
    }
    $false
}

function Reset-OcWindow {
    <#  Puts the window back to a known, un-snapped rect.

        Aero Snap leaves a window in a state Windows remembers: dragging a
        snapped window does not move it by the drag's delta — Windows unsnaps
        it and lays the restored size under the cursor — and its edges may
        refuse to resize at all. A drag or resize check that runs straight
        after a snap check therefore measures Windows' unsnapping, not the
        app. Restoring first is what makes those two checks about the app. #>
    param([Parameter(Mandatory)]$Window, [int]$X = 200, [int]$Y = 120,
          [int]$W = 1280, [int]$H = 800)
    $SW_RESTORE = 9
    $SWP_NOZORDER = 0x0004; $SWP_NOACTIVATE = 0x0010
    [void][Win32Ui]::ShowWindow($Window.Handle, $SW_RESTORE)
    Start-Sleep -Milliseconds 250
    [void][Win32Ui]::SetWindowPos($Window.Handle, [IntPtr]::Zero, $X, $Y, $W, $H,
                                  ($SWP_NOZORDER -bor $SWP_NOACTIVATE))
    Start-Sleep -Milliseconds 350
}
