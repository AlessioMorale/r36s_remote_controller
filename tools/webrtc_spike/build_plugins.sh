set -e
export DEBIAN_FRONTEND=noninteractive CARGO_HOME=/w/cargo RUSTUP_HOME=/w/rustup CARGO_TARGET_DIR=/w/target
apt-get update -qq
apt-get install -y -qq curl git build-essential pkg-config libssl-dev libglib2.0-dev libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev libgstreamer-plugins-bad1.0-dev libnice-dev >/dev/null
[ -x /w/cargo/bin/cargo ] || curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal >/dev/null
export PATH=/w/cargo/bin:$PATH
[ -x /w/cargo/bin/cargo-cbuild ] || cargo install cargo-c --locked -q
[ -d /w/src ] || git clone -q --depth 1 --branch 0.13 https://gitlab.freedesktop.org/gstreamer/gst-plugins-rs.git /w/src
cd /w/src; git log -1 --format=%h
cargo cinstall -p gst-plugin-webrtc -p gst-plugin-rtp --release --prefix=/opt/gst-rs --libdir=/opt/gst-rs/lib 2>&1 | tail -15
mkdir -p /out/lib; cp -r /opt/gst-rs/lib/. /out/lib/
find /out -name '*.so'
