#!/usr/bin/env bash
# Creates a Debian trixie arm64 sysroot with the Qt 6 development packages, for
# cross-compiling the UI from an x86_64 Linux host (see toolchain-aarch64.cmake).
#
#   sudo image/sysroot.sh [dir]       default: image/sysroot
#
# STATUS: not run (plan T1.2). Needs debootstrap and qemu-user-static.
set -euo pipefail
dir="${1:-$(dirname "${BASH_SOURCE[0]}")/sysroot}"
debootstrap --arch=arm64 --variant=minbase --foreign trixie "$dir" http://deb.debian.org/debian
cp /usr/bin/qemu-aarch64-static "$dir/usr/bin/"
chroot "$dir" /debootstrap/debootstrap --second-stage
chroot "$dir" apt-get update
chroot "$dir" apt-get install -y --no-install-recommends libc6-dev libstdc++-14-dev \
  qt6-base-dev qt6-declarative-dev qt6-multimedia-dev qt6-webengine-dev
echo "sysroot ready in $dir"
