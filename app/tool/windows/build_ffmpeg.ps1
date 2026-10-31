# Builds the FFmpeg that ships inside the Windows zip, as ../build_ffmpeg.sh
# does for Linux: the same FFmpeg with the same decoders, demuxers, parsers
# and dav1d, and Windows' hardware in place of VA-API. That means D3D11VA
# decoding, plus the NVENC, AMF, Quick Sync and Media Foundation encoders the
# codec layer opens by name (video/codec/encoder.rs). LGPL, shared, and
# linked with MSVC, so that Rust's x86_64-pc-windows-msvc links its import
# libraries. CI used to download BtbN's build instead, but BtbN deletes its
# dated builds after about two weeks.
#
#   pwsh -File app/tool/windows/build_ffmpeg.ps1 C:\ffmpeg
#
# Then point FFMPEG_DIR at that folder. CI does this, and caches the folder
# by this file and build_ffmpeg.sh.
#
# Needs:
#   - Visual Studio with the C++ tools; it enters their shell itself;
#   - CMake and Python 3 on PATH;
#   - MSYS2 with make and diffutils, at C:\msys64 or MSYS2_ROOT, because
#     FFmpeg's configure is a shell script (build_ffmpeg.sh is that half).
# NASM, Meson, Ninja and pkgconf are fetched into a scratch folder; every
# source is pinned by its hash.

param([Parameter(Mandatory)][string]$Prefix)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true

# dav1d, nv-codec-headers and FFmpeg as in ../build_ffmpeg.sh: keep them in step.
$sources = [ordered]@{
    dav1d   = @{ Url = 'https://downloads.videolan.org/pub/videolan/dav1d/1.5.4/dav1d-1.5.4.tar.xz'
                 Sha256 = '686616b7c69eb88d44459391ab25cac13b6647a3b288835c5784e71c1514a5c5'; Dir = 'dav1d-1.5.4' }
    nvcodec = @{ Url = 'https://github.com/FFmpeg/nv-codec-headers/archive/refs/tags/n13.0.19.0.tar.gz'
                 Sha256 = '86d15d1a7c0ac73a0eafdfc57bebfeba7da8264595bf531cf4d8db1c22940116'; Dir = 'nv-codec-headers-n13.0.19.0' }
    ffmpeg  = @{ Url = 'https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz'
                 Sha256 = '8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e'; Dir = 'ffmpeg-9.0.2' }
    # AMF's headers only: the runtime comes with AMD's driver.
    amf     = @{ Url = 'https://github.com/GPUOpen-LibrariesAndSDKs/AMF/releases/download/v1.5.3/AMF-headers-v1.5.3.tar.gz'
                 Sha256 = '0f521ad98041f8001147cded4a660127f18b1c27a801a5cd691bda1fa48381be'; Dir = 'amf-headers-v1.5.3' }
    # Quick Sync's dispatcher, linked into avcodec; the runtime comes with Intel's driver.
    libvpl  = @{ Url = 'https://github.com/intel/libvpl/archive/refs/tags/v2.17.0.tar.gz'
                 Sha256 = '4de3e2faf1e8307fb282e4a43f443191810f6a6b0a484fffa7995ba1c814c6ec'; Dir = 'libvpl-2.17.0' }
    nasm    = @{ Url = 'https://www.nasm.us/pub/nasm/releasebuilds/2.16.03/win64/nasm-2.16.03-win64.zip'
                 Sha256 = '3ee4782247bcb874378d02f7eab4e294a84d3d15f3f6ee2de2f47a46aa7226e6'; Dir = 'nasm-2.16.03' }
}
$pythonTools = 'meson==1.12.1', 'ninja==1.13.2', 'pkgconf==2.5.1.post2'

New-Item -ItemType Directory -Force $Prefix | Out-Null
$Prefix = (Resolve-Path $Prefix).Path
# Forward slashes throughout: cl, Meson, CMake and MSYS2 all take them, and a
# backslash in a .pc file or a shell argument is an escape somewhere.
$prefixPath = $Prefix -replace '\\', '/'
$work = Join-Path ([IO.Path]::GetTempPath()) "opencord-ffmpeg-$PID"
New-Item -ItemType Directory $work | Out-Null

try {
    # The x64 compiler, unless this is already a developer shell.
    if (-not (Get-Command cl.exe -ErrorAction SilentlyContinue)) {
        $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
        $vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if (-not $vs) { throw 'Visual Studio with the C++ tools is not installed.' }
        Import-Module (Join-Path $vs 'Common7\Tools\Microsoft.VisualStudio.DevShell.dll')
        Enter-VsDevShell -VsInstallPath $vs -SkipAutomaticLocation -DevCmdArguments '-arch=x64 -host_arch=x64' | Out-Null
    }
    $env:CC = 'cl'
    $env:CXX = 'cl'

    # Meson, Ninja and pkgconf in a venv of their own.
    python -m venv (Join-Path $work 'venv')
    $python = Join-Path $work 'venv\Scripts\python.exe'
    & $python -m pip install --quiet --disable-pip-version-check @pythonTools

    foreach ($name in $sources.Keys) {
        $source = $sources[$name]
        $file = Join-Path $work ([IO.Path]::GetFileName($source.Url))
        Invoke-WebRequest -Uri $source.Url -OutFile $file
        $got = (Get-FileHash $file -Algorithm SHA256).Hash.ToLower()
        if ($got -ne $source.Sha256) { throw "$name hashes to $got, not $($source.Sha256)." }
        if ($file.EndsWith('.zip')) {
            Expand-Archive $file -DestinationPath $work
        } else {
            # Python reads .tar.xz, which Windows' own tar may not.
            & $python -c 'import sys, tarfile; tarfile.open(sys.argv[1]).extractall(sys.argv[2], filter="data")' $file $work
        }
        $source.Path = Join-Path $work $source.Dir
    }
    # pkgconf's own exe, not the venv's `pkgconf`: that is a Python launcher
    # whose output is lost when it hands over to the exe on Windows.
    $pkgconf = Join-Path $work 'venv\Lib\site-packages\pkgconf\.bin'
    $env:PATH = "$pkgconf;$(Join-Path $work 'venv\Scripts');$($sources.nasm.Path);$env:PATH"

    # dav1d, static, so avcodec carries it. Meson names a static library
    # libdav1d.a even with MSVC, and FFmpeg's -ldav1d asks for dav1d.lib.
    meson setup (Join-Path $sources.dav1d.Path 'build') $sources.dav1d.Path `
        --prefix=$prefixPath --libdir=lib --buildtype=release --default-library=static `
        -Db_vscrt=md -Denable_tools=false -Denable_tests=false
    ninja -C (Join-Path $sources.dav1d.Path 'build') install
    Copy-Item (Join-Path $Prefix 'lib\libdav1d.a') (Join-Path $Prefix 'lib\dav1d.lib')

    # libvpl, static, with the same C runtime (/MD) as everything here; the
    # runtime's DLLs stay out of the folder (the app ships its own).
    $vplBuild = Join-Path $sources.libvpl.Path 'build'
    cmake -S $sources.libvpl.Path -B $vplBuild -G Ninja -DCMAKE_BUILD_TYPE=Release `
        "-DCMAKE_INSTALL_PREFIX=$prefixPath" -DBUILD_SHARED_LIBS=OFF -DBUILD_TESTS=OFF `
        -DBUILD_EXAMPLES=OFF -DINSTALL_EXAMPLES=OFF -DCMAKE_INSTALL_SYSTEM_RUNTIME_LIBS_SKIP=ON
    cmake --build $vplBuild
    cmake --install $vplBuild

    Copy-Item (Join-Path $sources.amf.Path 'AMF') (Join-Path $Prefix 'include') -Recurse -Force

    # FFmpeg itself, in MSYS2 with this shell's PATH: the compiler, NASM and pkgconf.
    $msys = if ($env:MSYS2_ROOT) { $env:MSYS2_ROOT } else { 'C:\msys64' }
    $env:MSYSTEM = 'MSYS'
    $env:MSYS2_PATH_TYPE = 'inherit'
    $env:CHERE_INVOKING = '1'
    $script = (Join-Path $PSScriptRoot 'build_ffmpeg.sh') -replace '\\', '/'
    & (Join-Path $msys 'usr\bin\bash.exe') -leo pipefail $script $prefixPath `
        ($sources.ffmpeg.Path -replace '\\', '/') ($sources.nvcodec.Path -replace '\\', '/')

    # LGPL §4: FFmpeg's licence goes with its DLLs, and with them the
    # licences of what is linked into them.
    $licence = @(
        'FFmpeg 9.0.2, built by app/tool/windows/build_ffmpeg.ps1 as shared',
        'libraries under the LGPL 2.1 or later. Linked into them: dav1d (BSD 2-clause)',
        'and Intel libvpl (MIT), whose licences follow FFmpeg''s.',
        '',
        (Get-Content -Raw (Join-Path $sources.ffmpeg.Path 'COPYING.LGPLv2.1')),
        '---- dav1d ----',
        (Get-Content -Raw (Join-Path $sources.dav1d.Path 'COPYING')),
        '---- Intel libvpl ----',
        (Get-Content -Raw (Join-Path $sources.libvpl.Path 'LICENSE'))
    ) -join "`n"
    Set-Content -Path (Join-Path $Prefix 'LICENSE.txt') -Value $licence -Encoding utf8NoBOM
} finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
