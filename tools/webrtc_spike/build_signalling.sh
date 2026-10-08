set -e
export DEBIAN_FRONTEND=noninteractive CARGO_HOME=/w/cargo RUSTUP_HOME=/w/rustup CARGO_TARGET_DIR=/w/target PATH=/w/cargo/bin:$PATH
apt-get update -qq; apt-get install -y -qq build-essential pkg-config libglib2.0-dev libgstreamer1.0-dev libssl-dev >/dev/null
cd /w/src && cargo build --release -p gst-plugin-webrtc-signalling --bin gst-webrtc-signalling-server 2>&1 | tail -3
cp /w/target/release/gst-webrtc-signalling-server /out/
ls -la /out
