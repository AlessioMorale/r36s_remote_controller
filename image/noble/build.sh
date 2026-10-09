#!/usr/bin/env bash
# Build everything the handheld needs, for Ubuntu 24.04 (noble) arm64, in a container.
#
#   image/noble/build.sh [out_dir]          default out_dir: dist/noble
#
# Output (consumed by image/noble/provision.sh):
#   bin/{control_daemon,ctl,rc_ui}            handheld binaries (UI: WebRTC video, no WebEngine)
#   gst-rs/lib/gstreamer-1.0/*.so             gst-plugins-rs 0.13 webrtc + rtp (webrtcsrc)
#   systemd/, config/                         units, udev rule, daemon/mapping/ui config
#
# The container is arm64 noble, the same glibc (2.39), GStreamer (1.24) and Qt (6.4) as the
# device, so on Apple Silicon this is a native build (x86_64 hosts need qemu binfmt and are slow).
# Cargo and rustup caches live in the docker volume rc-noble-cache; delete it to start clean.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
out="${1:-$root/dist/noble}"
mkdir -p "$out"

docker run --rm --platform linux/arm64 \
  -v "$root:/ws:ro" \
  -v "$out:/out" \
  -v rc-noble-cache:/cache \
  ubuntu:24.04 bash -euxc '
    export DEBIAN_FRONTEND=noninteractive CARGO_HOME=/cache/cargo RUSTUP_HOME=/cache/rustup CARGO_TARGET_DIR=/cache/target
    apt-get update -qq
    apt-get install -y -qq --no-install-recommends build-essential cmake ninja-build pkg-config curl git ca-certificates \
      libssl-dev libglib2.0-dev libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev libgstreamer-plugins-bad1.0-dev libnice-dev \
      qt6-base-dev qt6-declarative-dev qt6-multimedia-dev qt6-declarative-dev-tools libqt6opengl6-dev >/dev/null
    [ -x /cache/cargo/bin/cargo ] || curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
    export PATH=/cache/cargo/bin:$PATH

    # --- daemon + ctl (the cxx bindings need the elrs_joy submodule: deps/elrs_joy)
    cd /ws
    cargo build --release --locked -p control_daemon

    # --- UI: WebRTC video, no WebEngine (the handheld image has no Qt WebEngine)
    cmake -S ui -B /cache/ui -G Ninja -DCMAKE_BUILD_TYPE=Release -DRC_UI_WEBENGINE=OFF -DRC_UI_TESTS=OFF -DRC_UI_WEBRTC=ON
    cmake --build /cache/ui

    # --- gst-plugins-rs (webrtcsrc): not packaged for noble
    [ -x /cache/cargo/bin/cargo-cbuild ] || cargo install cargo-c --locked
    [ -d /cache/gst-plugins-rs ] || git clone --depth 1 --branch 0.13 https://gitlab.freedesktop.org/gstreamer/gst-plugins-rs.git /cache/gst-plugins-rs
    (cd /cache/gst-plugins-rs && cargo cinstall -p gst-plugin-webrtc -p gst-plugin-rtp --release --prefix=/opt/gst-rs --libdir=/opt/gst-rs/lib)

    rm -rf /out/*
    install -D /cache/target/release/control_daemon /cache/target/release/ctl -t /out/bin
    install -D /cache/ui/rc_ui -t /out/bin
    install -D /opt/gst-rs/lib/gstreamer-1.0/libgstrswebrtc.so /opt/gst-rs/lib/gstreamer-1.0/libgstrsrtp.so -t /out/gst-rs/lib/gstreamer-1.0
    mkdir -p /out/systemd /out/config
    cp systemd/*.service systemd/*.rules /out/systemd/
    cp crates/control_daemon/config/*.toml ui/config/rc_ui.json /out/config/
    cp image/noble/provision.sh /out/provision.sh
    find /out -type f | sort
  '
echo "built into $out"
