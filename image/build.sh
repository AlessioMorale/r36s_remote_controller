#!/usr/bin/env bash
# One command: build every handheld binary (daemon, ctl, test tools, UI) for the R36S.
#
#   image/build.sh [out_dir]          default out_dir: dist/
#
# The build runs in a Debian trixie arm64 container, so the binaries are linked against the
# same glibc/Qt as the device image (trixie: glibc 2.41, Qt 6.8). On Apple Silicon this is a
# native build; on x86_64 it needs qemu-user-static binfmt (docker run --privileged
# tonistiigi/binfmt --install arm64) and is slow.
#
# STATUS: written but not yet run end to end (plan T1.2). The daemon workspace is verified on
# aarch64 Linux (Ubuntu 24.04 container); the UI is verified on macOS/Qt 6.11 only.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
out="${1:-$root/remote_controller/dist}"
mkdir -p "$out"

docker run --rm --platform linux/arm64 \
  -v "$root/src/elrs_joy:/ws/src/elrs_joy:ro" \
  -v "$root/remote_controller:/ws/remote_controller:ro" \
  -v "$out:/out" \
  --tmpfs /build:size=6g,exec \
  debian:trixie bash -euxc '
    export DEBIAN_FRONTEND=noninteractive CARGO_HOME=/build/cargo RUSTUP_HOME=/build/rustup CARGO_TARGET_DIR=/build/target
    apt-get update
    apt-get install -y --no-install-recommends build-essential cmake ninja-build pkg-config curl ca-certificates \
      qt6-base-dev qt6-declarative-dev qt6-multimedia-dev qt6-webengine-dev qt6-declarative-dev-tools
    curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
    . /build/cargo/env
    cd /ws/remote_controller
    cargo build --release --locked -p control_daemon -p test_tools
    cmake -S ui -B /build/ui -G Ninja -DCMAKE_BUILD_TYPE=Release
    cmake --build /build/ui
    install -D /build/target/release/{control_daemon,ctl,fake_tx,virtual_pad} -t /out/bin
    install -D /build/ui/rc_ui /out/bin/rc_ui
    cp -r systemd /out/systemd
    cp crates/control_daemon/config/*.toml /out/
    cp ui/config/rc_ui.json /out/
  '
echo "built into $out"
