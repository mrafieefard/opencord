# What a shared screen costs this PC (Windows plan W6, step 7). The plan's
# rule: start on the CPU path, and only move the convert and scale onto the
# GPU if 1080p30 takes more than 15 % of a core. This measures it so the
# decision is made from a number.
#
#   pwsh -File app/tool/windows/check_screen_cost.ps1
#
# It runs the ignored screen-capture test on its own and watches that
# process's CPU time while it captures, which is a ceiling rather than the
# capture's exact share: the process is doing nothing else, but it is still
# a test harness. Silent, and nothing is shown on screen.

param(
    [int]$Samples = 12,
    [int]$EveryMs = 500
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repo = Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $PSScriptRoot))
Set-Location $repo
$env:CARGO_INCREMENTAL = '0'
if (-not $env:LIBCLANG_PATH) { $env:LIBCLANG_PATH = 'C:\Program Files\LLVM\bin' }
if (-not $env:FFMPEG_DIR) { $env:FFMPEG_DIR = 'C:\ffmpeg' }

Write-Host 'building the test first, so the build is not in the measurement'
& cargo test --quiet -p opencord-media --test screen_windows --no-run *>&1 | Select-Object -Last 2
if ($LASTEXITCODE -ne 0) { throw 'the test did not build.' }

$exe = Get-ChildItem (Join-Path $repo 'target\debug\deps\screen_windows-*.exe') |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $exe) { throw 'the test binary is not there.' }
Write-Host "measuring: $($exe.Name)"

$cores = [Environment]::ProcessorCount
$log = Join-Path ([System.IO.Path]::GetTempPath()) 'opencord-screen-cost.log'
$process = Start-Process -FilePath $exe.FullName `
    -ArgumentList @('--ignored', '--nocapture', '--test-threads=1',
        'a_screen_is_shared_at_the_rate_it_was_asked_for') `
    -RedirectStandardOutput $log -RedirectStandardError "$log.err" `
    -PassThru -WindowStyle Hidden

# Let it get past opening and into steady capture before sampling.
Start-Sleep -Milliseconds 1200
$readings = @()
$last = $process.TotalProcessorTime
$clock = [System.Diagnostics.Stopwatch]::StartNew()
foreach ($sample in 1..$Samples) {
    Start-Sleep -Milliseconds $EveryMs
    if ($process.HasExited) { break }
    $process.Refresh()
    $now = $process.TotalProcessorTime
    $elapsed = $clock.Elapsed
    $clock.Restart()
    $busy = ($now - $last).TotalSeconds
    $last = $now
    if ($elapsed.TotalSeconds -gt 0) {
        $readings += [math]::Round(100.0 * $busy / $elapsed.TotalSeconds, 1)
    }
}
$process.WaitForExit(60000) | Out-Null
if (-not $process.HasExited) { & taskkill.exe /PID $process.Id /T /F *> $null }

Get-Content $log -ErrorAction SilentlyContinue |
    Select-String -Pattern 'sharing |started in|SCREEN |stopped in' |
    ForEach-Object { Write-Host "  $($_.Line)" }

if (-not $readings) { throw 'nothing was measured; the test ended too quickly.' }
$readings = $readings | Sort-Object
$median = $readings[[int]($readings.Count / 2)]
$worst = $readings[-1]

Write-Host '----- what it costs -----'
Write-Host "samples            : $($readings -join ', ') % of one core"
Write-Host "median             : $median % of one core"
Write-Host "worst              : $worst % of one core"
Write-Host ("of the whole CPU   : {0:N1} % ({1} cores)" -f ($median / $cores), $cores)
Write-Host 'the plan moves this to the GPU above 15 % of a core (W6 step 7)'

if ($median -gt 15) {
    Write-Host ''
    Write-Host "OVER: $median % of a core is above the 15 % the plan allows for the CPU path."
    Write-Host 'W6 step 7 says to convert and scale on the GPU with the D3D11 video'
    Write-Host 'processor instead. Put the number in windows.md section 7 either way.'
    exit 2
}
Write-Host ''
Write-Host "The CPU path is within budget at $median % of a core; no GPU work needed."
