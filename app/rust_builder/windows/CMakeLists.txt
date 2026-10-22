# The Flutter tooling requires that developers have a version of Visual Studio
# installed that includes CMake 3.14 or later. You should not increase this
# version, as doing so will cause the plugin to fail to compile for some
# customers of the plugin.
cmake_minimum_required(VERSION 3.14)

# Project-level configuration.
set(PROJECT_NAME "opencord_core")
project(${PROJECT_NAME} LANGUAGES CXX)

include("../cargokit/cmake/cargokit.cmake")
apply_cargokit(${PROJECT_NAME} ../../../../../../../crates/opencord-core opencord_core "")

# FFmpeg's shared libraries, which the codec layer links against
# (docs/plans/windows.md W3 step 5). Linux ships its own build inside the
# bundle; on Windows they come from the LGPL build FFMPEG_DIR points at, the
# same one the bindings were generated against.
#
# Only the five opencord_core.dll actually needs: avcodec, avformat and
# swscale are linked directly, avutil and swresample come in behind them
# (measured with dumpbin /dependents). The names carry a version
# (avcodec-63.dll), so they are matched by prefix rather than written out;
# avfilter, avdevice and postproc are left out deliberately, nothing here
# calls them.
set(opencord_core_ffmpeg_libraries "")
if(DEFINED ENV{FFMPEG_DIR})
  # FFMPEG_DIR is a Windows path with backslashes. They have to go: the paths
  # end up written into the generated cmake_install.cmake, where `C:\ffmpeg`
  # is read back as the escape `\f` and the install step dies parsing it.
  file(TO_CMAKE_PATH "$ENV{FFMPEG_DIR}" opencord_core_ffmpeg_dir)
  foreach(library avcodec avformat avutil swresample swscale)
    file(GLOB found "${opencord_core_ffmpeg_dir}/bin/${library}-*.dll")
    if(NOT found)
      message(FATAL_ERROR
        "FFmpeg's ${library} DLL is not in ${opencord_core_ffmpeg_dir}/bin. "
        "FFMPEG_DIR has to point at the shared LGPL build (windows.md §1).")
    endif()
    list(APPEND opencord_core_ffmpeg_libraries ${found})
  endforeach()
else()
  message(FATAL_ERROR
    "FFMPEG_DIR is not set, so FFmpeg cannot be bundled and the app would "
    "not start away from this machine (windows.md §1, W3 step 5).")
endif()

# List of absolute paths to libraries that should be bundled with the plugin.
# This list could contain prebuilt libraries, or libraries created by an
# external build triggered from this build file.
set(opencord_core_bundled_libraries
  "${${PROJECT_NAME}_cargokit_lib}"
  ${opencord_core_ffmpeg_libraries}
  PARENT_SCOPE
)
