# The app's own title bar on Windows (D83, and Â§1 of
# docs/plans/check-windows.md), checked without a pair of hands.
#
#   pwsh -File app/tool/windows/check_window_chrome.ps1 `
#        -Exe C:\oc\media-viewer-player\opencord\opencord.exe
#
# It drives the real window through Win32 â the actual mouse, the actual
# keyboard â and reads the results back from the window's rect and from
# screenshots it saves beside itself. Each check prints PASS or FAIL with
# the numbers it judged on, and the screenshots stay for the report.
#
# It must run on an UNLOCKED workstation. Behind the lock screen, injected
# input reaches nothing and every capture returns the lock screen, so the
# script stops rather than reporting nonsense.
#
# What it cannot settle, and so does not claim:
#   - whether Close's hover is the app's own inverted style, and the others
#     merely highlight: it captures before/after images of each button for
#     a person to compare, since "inverted and monochrome" is a judgement;
#   - Windows 11's snap layouts on hover, which need Windows 11;
#   - anything behind a screen the app only reaches with an identity and a
#     server (Friends, a private chat, Settings), which Â§1 also asks for.

param(
    [string]$Exe = 'C:\oc\media-viewer-player\opencord\opencord.exe',
    [string]$Profile = 'chromecheck',
    [string]$ShotDir = "$PSScriptRoot\..\..\..\target\window-chrome",
    [switch]$KeepRunning
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. "$PSScriptRoot\window_ui.ps1"

if (-not (Test-OcDesktopUsable -Explain)) { exit 2 }
if (-not (Test-Path $Exe)) { throw "no build at $Exe" }

$results = New-Object System.Collections.ArrayList
function Note([string]$name, [bool]$ok, [string]$detail) {
    [void]$results.Add([pscustomobject]@{ Check = $name; Ok = $ok; Detail = $detail })
    $tag = if ($ok) { 'PASS' } else { 'FAIL' }
    Write-Host ("  {0}  {1,-42} {2}" -f $tag, $name, $detail)
}

New-Item -ItemType Directory -Force $ShotDir | Out-Null
$ShotDir = (Resolve-Path $ShotDir).Path
Write-Host "build      : $Exe"
Write-Host "screenshots: $ShotDir"

# --- start it ---------------------------------------------------------------
$env:OPENCORD_PROFILE = $Profile
$app = Start-Process -FilePath $Exe -WorkingDirectory (Split-Path $Exe) -PassThru
try {
    $window = $null
    foreach ($try in 1..40) {
        Start-Sleep -Milliseconds 750
        $window = Get-OcMainWindow -ProcessId $app.Id
        if ($window) { break }
    }
    if (-not $window) { throw 'the window never appeared' }
    $work = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea

    Write-Host ''
    Write-Host '--- 1. no system title bar, and the app draws its own ---'
    Save-OcShot -Path "$ShotDir\01-start.png" -Handle $window.Handle | Out-Null
    # A system title bar would sit in the window's top ~30px and hold the
    # window text. The app's own header is its content, so the only honest
    # machine test is that the non-client area at the top is gone: with
    # WM_NCCALCSIZE answered, the client rect fills the window but for the
    # resize border, instead of being inset by a caption's height.
    $wr = New-Object Win32Ui+RECT; [void][Win32Ui]::GetWindowRect($window.Handle, [ref]$wr)
    $cr = New-Object Win32Ui+RECT; [void][Win32Ui]::GetClientRect($window.Handle, [ref]$cr)
    $lostToFrame = ($wr.Bottom - $wr.Top) - $cr.Bottom
    $caption = [System.Windows.Forms.SystemInformation]::CaptionHeight
    Note 'no caption height lost to a system frame' ($lostToFrame -lt $caption) `
         "window $(($wr.Bottom - $wr.Top))px, client $($cr.Bottom)px, lost $lostToFrame (a caption alone is $caption)"
    Note 'window is resizable (WS_THICKFRAME)' ($window.StyleBits -match 'WS_THICKFRAME') $window.StyleBits
    Write-Host "       01-start.png is the window at start: check by eye that the top strip is the app's own."

    Write-Host ''
    Write-Host '--- 2. maximise and restore ---'
    Reset-OcWindow -Window (Get-OcMainWindow -ProcessId $app.Id)
    $before = Get-OcMainWindow -ProcessId $app.Id
    if (-not (Set-OcFocusOrStop -Window $before)) {
        Write-Host "  SKIP  the window never took the foreground, so no key below would"
        Write-Host "        prove anything about the app. Nothing here is a result."
        exit 2
    }
    Send-OcKeys -Mods LWIN -Key UP
    $max = Get-OcMainWindow -ProcessId $app.Id
    Note 'Win+Up fills the work area' `
         ($max.W -ge $work.Width -and $max.H -ge $work.Height - 8) `
         "$($max.W)x$($max.H) against a $($work.Width)x$($work.Height) work area"
    Save-OcShot -Path "$ShotDir\02-maximised.png" -Handle $max.Handle | Out-Null
    [void](Set-OcFocusOrStop -Window $max)
    Send-OcKeys -Mods LWIN -Key DOWN
    $restored = Get-OcMainWindow -ProcessId $app.Id
    Note 'Win+Down gives the size back' `
         ($restored.W -eq $before.W -and $restored.H -eq $before.H) `
         "$($restored.W)x$($restored.H), was $($before.W)x$($before.H)"
    Save-OcShot -Path "$ShotDir\03-restored.png" -Handle $restored.Handle | Out-Null

    Write-Host ''
    Write-Host '--- 3. Aero Snap to the sides ---'
    foreach ($side in 'LEFT', 'RIGHT') {
        # Win+Left and Win+Right cycle a window left half → restored → right
        # half, so sending Right straight after Left only restores it. Each
        # side starts from an unsnapped window to be a test of that side.
        Reset-OcWindow -Window (Get-OcMainWindow -ProcessId $app.Id)
        $w0 = Get-OcMainWindow -ProcessId $app.Id
        [void](Set-OcFocusOrStop -Window $w0)
        Send-OcKeys -Mods LWIN -Key $side
        Send-OcKeys -Key ESC                     # Snap Assist takes the foreground
        $snapped = Get-OcMainWindow -ProcessId $app.Id
        # A snapped window is half the work area wide and its full height.
        # Frameless windows hang their invisible resize border over the edge,
        # so allow a margin either way rather than demanding exact halves.
        $halfish = [math]::Abs($snapped.W - ($work.Width / 2)) -le 24
        $tallish = $snapped.H -ge ($work.Height - 24)
        Note "Win+$side takes half the screen" ($halfish -and $tallish) `
             "$($snapped.W)x$($snapped.H) at $($snapped.X),$($snapped.Y)"
    }
    $w0 = Get-OcMainWindow -ProcessId $app.Id
    [void](Set-OcFocusOrStop -Window $w0)
    Send-OcKeys -Mods LWIN -Key DOWN; Send-OcKeys -Key ESC

    Write-Host ''
    Write-Host '--- 4. the header drags the window, and a double click maximises ---'
    # A snapped window unsnaps under a drag instead of following it, so the
    # drag and resize checks below start from a known, restored rect.
    Reset-OcWindow -Window (Get-OcMainWindow -ProcessId $app.Id)
    $w1 = Get-OcMainWindow -ProcessId $app.Id
    # Empty header: to the right of any title, left of the caption buttons,
    # which sit in the last ~140px.
    $headerY = $w1.Y + 18
    $headerX = $w1.X + [int]($w1.W * 0.55)
    Invoke-OcDrag -FromX $headerX -FromY $headerY -ToX ($headerX + 120) -ToY ($headerY + 90)
    $moved = Get-OcMainWindow -ProcessId $app.Id
    Note 'dragging the empty header moves the window' `
         ([math]::Abs($moved.X - ($w1.X + 120)) -le 12 -and [math]::Abs($moved.Y - ($w1.Y + 90)) -le 12) `
         "moved to $($moved.X),$($moved.Y) from $($w1.X),$($w1.Y), wanted +120,+90"

    $w2 = Get-OcMainWindow -ProcessId $app.Id
    Invoke-OcClick -X ($w2.X + [int]($w2.W * 0.55)) -Y ($w2.Y + 18) -Count 2
    $dbl = Get-OcMainWindow -ProcessId $app.Id
    Note 'a double click on the header maximises' ($dbl.Maximized) "maximized=$($dbl.Maximized)"
    Invoke-OcClick -X ($dbl.X + [int]($dbl.W * 0.55)) -Y ($dbl.Y + 18) -Count 2
    $dbl2 = Get-OcMainWindow -ProcessId $app.Id
    Note 'a second double click restores' (-not $dbl2.Maximized) "maximized=$($dbl2.Maximized)"

    Write-Host ''
    Write-Host '--- 5. the edges resize ---'
    Reset-OcWindow -Window (Get-OcMainWindow -ProcessId $app.Id)
    $w3 = Get-OcMainWindow -ProcessId $app.Id
    Invoke-OcDrag -FromX ($w3.X + $w3.W - 1) -FromY ($w3.Y + [int]($w3.H / 2)) `
                  -ToX ($w3.X + $w3.W + 160) -ToY ($w3.Y + [int]($w3.H / 2))
    $wider = Get-OcMainWindow -ProcessId $app.Id
    Note 'the right edge resizes it' ($wider.W -gt $w3.W + 100) "$($w3.W) -> $($wider.W)"

    Write-Host ''
    Write-Host '--- 6. the caption buttons, for a person to judge ---'
    $w4 = Get-OcMainWindow -ProcessId $app.Id
    Save-OcShot -Path "$ShotDir\04-buttons-rest.png" -Handle $w4.Handle -Height 48 | Out-Null
    # Hover each of the three in turn. Their hover styles are a judgement
    # ("inverted, monochrome" for Close), so this only gathers the pictures.
    $spots = [ordered]@{ close = 24; maximise = 70; minimise = 116 }
    foreach ($name in $spots.Keys) {
        [void][Win32Ui]::SetCursorPos(($w4.X + $w4.W - $spots[$name]), ($w4.Y + 20))
        Start-Sleep -Milliseconds 700
        Save-OcShot -Path "$ShotDir\05-hover-$name.png" -Handle $w4.Handle -Height 48 | Out-Null
    }
    [void][Win32Ui]::SetCursorPos(($w4.X + 60), ($w4.Y + 300))
    Write-Host '       04-buttons-rest.png and 05-hover-*.png: compare by eye.'
    Write-Host "       Close should inverted and monochrome; the others merely highlight."

    Write-Host ''
    Write-Host '--- 7. scaling ---'
    Note 'the window reports a DPI' ($w4.Dpi -gt 0) "$($w4.Dpi) dpi ($([int]($w4.Dpi / 96 * 100))% scaling)"
    Write-Host '       125% and 150% need the scaling changed in Display settings and this script run again.'

    Write-Host ''
    $bad = @($results | Where-Object { -not $_.Ok })
    Write-Host ("{0} checked, {1} failed" -f $results.Count, $bad.Count)
    foreach ($b in $bad) { Write-Host "  FAILED: $($b.Check) - $($b.Detail)" }
    if ($bad.Count) { exit 1 }
    Write-Host 'Every machine-checkable part of section 1 passed.'
} finally {
    if (-not $KeepRunning -and -not $app.HasExited) {
        & taskkill.exe /PID $app.Id /T /F *> $null
    }
}
