# Samples the colour of a rectangle of the running app's window
# (Windows plan W4 step 1: "its colours are right (red looks red)").
#
#   pwsh -File app/tool/windows/sample_video.ps1 -Left 320 -Top 80 -Width 640 -Height 360
#
# Flutter cannot answer this from inside a test: a `Texture` is composited by
# Windows, not drawn into Flutter's own layer tree, so a widget-level pixel
# check sees an empty box where the video is. So this takes what is actually on
# the screen — the window is brought to the front and its pixels are copied
# from the desktop — and reports the mean colour of the left and right thirds
# of the rectangle it was given.
#
# Those two thirds are the measurement. The synthetic camera's chroma bands run
# from U=96,V=160 on the left to U=152,V=104 on the right, which is reddish to
# bluish; if the red and blue channels were swapped anywhere between the
# decoder and the screen — the classic BGRA-for-RGBA mistake — the two would
# come back the other way round.
#
# -Left/-Top/-Width/-Height are in *physical* pixels inside the window's client
# area; the caller multiplies Flutter's logical rectangle by the device pixel
# ratio. -Out writes the capture to a PNG for a person to look at.

param(
    [Parameter(Mandatory)][int]$Left,
    [Parameter(Mandatory)][int]$Top,
    [Parameter(Mandatory)][int]$Width,
    [Parameter(Mandatory)][int]$Height,
    # Whose window to copy from. The caller is the app — an integration test
    # runs inside the app's own process — so it passes its own id, which is
    # exact. Under `flutter test` the window is not titled "Opencord", so
    # looking it up by name does not work.
    [int]$ProcessId,
    [string]$Title = 'Opencord',
    [string]$Out
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName System.Drawing
Add-Type -Namespace Opencord -Name Window -MemberDefinition @'
[System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Sequential)]
public struct POINT { public int X; public int Y; }

[System.Runtime.InteropServices.DllImport("user32.dll", CharSet = System.Runtime.InteropServices.CharSet.Unicode)]
public static extern System.IntPtr FindWindowW(string className, string windowName);

[System.Runtime.InteropServices.DllImport("user32.dll")]
public static extern bool SetForegroundWindow(System.IntPtr window);

[System.Runtime.InteropServices.DllImport("user32.dll")]
public static extern System.IntPtr GetForegroundWindow();

[System.Runtime.InteropServices.DllImport("user32.dll")]
public static extern bool BringWindowToTop(System.IntPtr window);

[System.Runtime.InteropServices.DllImport("user32.dll")]
public static extern uint GetWindowThreadProcessId(System.IntPtr window, System.IntPtr process);

[System.Runtime.InteropServices.DllImport("user32.dll")]
public static extern bool AttachThreadInput(uint from, uint to, bool attach);

[System.Runtime.InteropServices.DllImport("kernel32.dll")]
public static extern uint GetCurrentThreadId();

[System.Runtime.InteropServices.DllImport("user32.dll")]
public static extern bool ClientToScreen(System.IntPtr window, ref POINT point);

[System.Runtime.InteropServices.DllImport("user32.dll")]
public static extern bool IsIconic(System.IntPtr window);

[System.Runtime.InteropServices.DllImport("user32.dll")]
public static extern bool ShowWindow(System.IntPtr window, int command);
'@

$window = [IntPtr]::Zero
if ($ProcessId) {
    $owner = Get-Process -Id $ProcessId -ErrorAction Stop
    # A window can take a moment to appear, and MainWindowHandle is cached.
    foreach ($attempt in 1..20) {
        $owner.Refresh()
        if ($owner.MainWindowHandle -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 250
    }
    $window = $owner.MainWindowHandle
    if ($window -eq [IntPtr]::Zero) {
        throw "Process $ProcessId ($($owner.ProcessName)) has no main window."
    }
    Write-Host "window: $($owner.ProcessName) ($ProcessId), `"$($owner.MainWindowTitle)`""
} else {
    $window = [Opencord.Window]::FindWindowW($null, $Title)
    if ($window -eq [IntPtr]::Zero) { throw "No window called `"$Title`" is open." }
}
# A minimised window has nothing on screen to copy.
if ([Opencord.Window]::IsIconic($window)) { [void][Opencord.Window]::ShowWindow($window, 9) }

# Windows refuses `SetForegroundWindow` from a process that is not already in
# front, which is exactly this one's position. Attaching to the foreground
# thread's input queue for the moment lifts that, and is the usual way round
# it. Without this the capture quietly came back with whatever *was* in
# front — a terminal, the first time — and reported its colours as the
# video's.
$us = [Opencord.Window]::GetCurrentThreadId()
$front = [Opencord.Window]::GetWindowThreadProcessId(
    [Opencord.Window]::GetForegroundWindow(), [IntPtr]::Zero)
$attached = $front -ne 0 -and $front -ne $us -and
    [Opencord.Window]::AttachThreadInput($us, $front, $true)
try {
    [void][Opencord.Window]::ShowWindow($window, 5)
    [void][Opencord.Window]::BringWindowToTop($window)
    [void][Opencord.Window]::SetForegroundWindow($window)
} finally {
    if ($attached) { [void][Opencord.Window]::AttachThreadInput($us, $front, $false) }
}
Start-Sleep -Milliseconds 900

# And check it worked, rather than measuring whatever happens to be there.
$now = [Opencord.Window]::GetForegroundWindow()
if ($now -ne $window) {
    throw ("the app's window would not come to the front, so there is nothing " +
        'of it to copy. Nothing was measured.')
}

# The rectangle is given inside the client area; put it on the screen.
$origin = New-Object Opencord.Window+POINT
if (-not [Opencord.Window]::ClientToScreen($window, [ref]$origin)) {
    throw 'the window would not say where its client area is.'
}

$bitmap = New-Object System.Drawing.Bitmap $Width, $Height
try {
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        # From the desktop, so this is what is really being displayed —
        # PrintWindow would hand back an empty box where the texture is.
        $graphics.CopyFromScreen(
            ($origin.X + $Left), ($origin.Y + $Top),
            0, 0, $bitmap.Size,
            [System.Drawing.CopyPixelOperation]::SourceCopy)
    } finally {
        $graphics.Dispose()
    }
    if ($Out) {
        $bitmap.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
        Write-Host "capture: $Out"
    }

    # The mean of each third, sampling every fourth pixel: enough for a
    # colour, and fast enough not to hold the frame.
    function Measure-Third {
        param([int]$From, [int]$To)
        $r = 0.0; $g = 0.0; $b = 0.0; $count = 0
        for ($y = 0; $y -lt $Height; $y += 4) {
            for ($x = $From; $x -lt $To; $x += 4) {
                $pixel = $bitmap.GetPixel($x, $y)
                $r += $pixel.R; $g += $pixel.G; $b += $pixel.B
                $count++
            }
        }
        if ($count -eq 0) { throw 'the rectangle has no pixels in it.' }
        [pscustomobject]@{
            R = [math]::Round($r / $count, 1)
            G = [math]::Round($g / $count, 1)
            B = [math]::Round($b / $count, 1)
        }
    }

    $third = [math]::Max([int]($Width / 3), 1)
    $leftThird = Measure-Third -From 0 -To $third
    $rightThird = Measure-Third -From ($Width - $third) -To $Width

    # One line each, for the check script to read.
    Write-Host ("SAMPLE left  R={0} G={1} B={2}" -f $leftThird.R, $leftThird.G, $leftThird.B)
    Write-Host ("SAMPLE right R={0} G={1} B={2}" -f $rightThird.R, $rightThird.G, $rightThird.B)
    Write-Host ("SAMPLE redness left={0} right={1}" -f
        [math]::Round($leftThird.R - $leftThird.B, 1),
        [math]::Round($rightThird.R - $rightThird.B, 1))
} finally {
    $bitmap.Dispose()
}
