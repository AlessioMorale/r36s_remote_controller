# CMake toolchain file for cross-building the UI on an x86_64 Linux host against a trixie
# arm64 sysroot (created by image/sysroot.sh):
#
#   cmake -S ui -B build-arm -G Ninja -DCMAKE_TOOLCHAIN_FILE=image/toolchain-aarch64.cmake \
#         -DRC_SYSROOT=$PWD/image/sysroot
#
# On Apple Silicon use image/build.sh instead (native arm64 container, no sysroot needed).
set(CMAKE_SYSTEM_NAME Linux)
set(CMAKE_SYSTEM_PROCESSOR aarch64)

if(NOT RC_SYSROOT)
  message(FATAL_ERROR "pass -DRC_SYSROOT=<trixie arm64 sysroot>")
endif()
set(CMAKE_SYSROOT ${RC_SYSROOT})
set(CMAKE_C_COMPILER aarch64-linux-gnu-gcc)
set(CMAKE_CXX_COMPILER aarch64-linux-gnu-g++)
set(CMAKE_FIND_ROOT_PATH ${RC_SYSROOT})
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE ONLY)
# Host tools (moc, rcc, qmlcachegen) must be the host's Qt of the same version
set(QT_HOST_PATH /usr CACHE PATH "host Qt for build-time tools")
