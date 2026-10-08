set -e
export DEBIAN_FRONTEND=noninteractive CARGO_HOME=/w/r26/cargo RUSTUP_HOME=/w/r26/rustup CARGO_TARGET_DIR=/w/r26/target
apt-get update -qq
apt-get install -y -qq curl git build-essential pkg-config libssl-dev libglib2.0-dev libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev libgstreamer-plugins-bad1.0-dev libnice-dev >/dev/null
[ -x /w/r26/cargo/bin/cargo ] || curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal >/dev/null
export PATH=/w/r26/cargo/bin:$PATH
[ -x /w/r26/cargo/bin/cargo-cbuild ] || cargo install cargo-c --locked -q
[ -d /w/r26/src ] || git clone -q --depth 1 --branch 0.15 https://gitlab.freedesktop.org/gstreamer/gst-plugins-rs.git /w/r26/src
cd /w/r26/src; git log -1 --format=%h
cargo cinstall -p gst-plugin-webrtc -p gst-plugin-rtp --release --prefix=/opt/gst-rs --libdir=/opt/gst-rs/lib 2>&1 | tail -6
cargo build --release -p gst-plugin-webrtc-signalling --bin gst-webrtc-signalling-server 2>&1 | tail -2
mkdir -p /out/lib; cp -r /opt/gst-rs/lib/. /out/lib/; cp /w/r26/target/release/gst-webrtc-signalling-server /out/
find /out -name '*.so' -o -name 'gst-webrtc*'
