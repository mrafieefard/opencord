# Does the release build still work with FFmpeg uninstalled? (Windows plan W3,
# step 5.) The DLLs are bundled next to the exe, so it should — but the only
# way to know is to take the installed copy away and look.
#
#   pwsh -File app/tool/windows/check_bundled_ffmpeg.ps1
#
# It renames FFMPEG_DIR out of the way, with nothing else on PATH that could
# satisfy the load, then:
#   1. starts the release exe and checks it is still running — if a DLL could
#      not be found, Windows fails the load of opencord_core.dll and the
#      process dies at once;
#   2. encodes and decodes real video through the *bundled* DLLs, by running
#      the V4 check with the release folder on PATH instead of FFMPEG_DIR.
#
# The folder is always put back, even on a failure or a Ctrl-C. Run nothing
# else that needs ffmpeg.exe while it goes.

param(
    [string]$Release = 'build\windows\x64\runner\Release',
    [int]$StartSeconds = 12,
    [switch]$SkipEncode
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repo = Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $PSScriptRoot))
$releaseDir = Join-Path $repo (Join-Path 'app' $Release)
$exe = Join-Path $releaseDir 'opencord.exe'
if (-not (Test-Path $exe)) {
    throw "There is no release build at $exe. Run: flutter build windows --release"
}

$installed = $env:FFMPEG_DIR
if (-not $installed) { $installed = 'C:\ffmpeg' }
if (-not (Test-Path $installed)) {
    throw "FFMPEG_DIR ($installed) is not there, so there is nothing to take away."
}
$hidden = "$installed.hidden"
if (Test-Path $hidden) {
    throw "$hidden already exists: an earlier run did not finish. Rename it back first."
}

Write-Host "release build  : $releaseDir"
Write-Host "FFmpeg to hide : $installed"
'--- the FFmpeg DLLs bundled with it ---'
$bundled = Get-ChildItem (Join-Path $releaseDir '*.dll') |
    Where-Object { $_.Name -match '^(avcodec|avformat|avutil|swresample|swscale)-\d+\.dll$' }
if (-not $bundled) { throw 'no FFmpeg DLLs are bundled: W3 step 5 is not done.' }
foreach ($dll in $bundled | Sort-Object Name) {
    Write-Host ("  {0,-20} {1,12:N0} bytes" -f $dll.Name, $dll.Length)
}
Write-Host ("  {0,-20} {1,12:N0} bytes, {2} files" -f 'all five',
    ($bundled | Measure-Object -Property Length -Sum).Sum, $bundled.Count)

$problems = @()
Rename-Item -LiteralPath $installed -NewName (Split-Path -Leaf $hidden)
try {
    Write-Host ''
    Write-Host "===== $installed is gone; the app has only what it ships ====="
    # Nothing of FFmpeg's on PATH, so a missing DLL cannot be satisfied from
    # somewhere else and quietly pass.
    $withoutFfmpeg = ($env:PATH -split ';' | Where-Object { $_ -and $_ -notmatch 'ffmpeg' }) -join ';'

    Write-Host '--- 1. does the release exe start? ---'
    $log = Join-Path ([System.IO.Path]::GetTempPath()) 'opencord-bundled.log'
    # Its own data and single-instance lock, so it does not join the user's
    # running copy and exit as a second instance would.
    $env:OPENCORD_PROFILE = 'e2e_bundled'
    $app = Start-Process -FilePath $exe -WorkingDirectory $releaseDir -PassThru `
        -RedirectStandardOutput $log -RedirectStandardError "$log.err"
    try {
        $alive = -not $app.WaitForExit($StartSeconds * 1000)
        if ($alive) {
            Write-Host "  it is still running after $StartSeconds s, so every DLL resolved"
        } else {
            Write-Host "  it exited with $($app.ExitCode) after less than $StartSeconds s"
            Get-Content $log, "$log.err" -ErrorAction SilentlyContinue | Write-Host
            $problems += "the release exe did not stay running without $installed"
        }
    } finally {
        if (-not $app.HasExited) { & taskkill.exe /PID $app.Id /T /F *> $null }
    }

    if (-not $SkipEncode) {
        Write-Host ''
        Write-Host '--- 2. do the bundled DLLs actually encode and decode? ---'
        Write-Host '  the V4 check again, loading FFmpeg from the release folder'
        # Let the GPU go: measured, a V4 run started straight after the app
        # above was force-killed saw 10 pictures instead of 152.
        Start-Sleep -Seconds 5
        # The voicebot finds its DLLs on PATH; point that at the bundle.
        $env:PATH = "$releaseDir;$withoutFfmpeg"
        # Its own process, so its exit code is its own: a script run with `&`
        # leaves $LASTEXITCODE wherever the last *native* command left it, and
        # the e2e cleanup's `taskkill` on an already-dead process sets 128.
        $pwshPath = [System.Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
        & $pwshPath -NoLogo -NoProfile -File (Join-Path $PSScriptRoot 'check_v4.ps1')
        if ($LASTEXITCODE -ne 0) {
            $problems += 'the V4 check failed with only the bundled FFmpeg'
        }
    }
} finally {
    Rename-Item -LiteralPath $hidden -NewName (Split-Path -Leaf $installed)
    Write-Host ''
    Write-Host "$installed is back"
}

if ($problems) {
    Write-Host ''
    foreach ($problem in $problems) { Write-Host "FAILED: $problem" }
    exit 1
}

Write-Host 'The release build runs and encodes with FFmpeg uninstalled.'
