# WebRTC spike (webrtcsink / webrtcsrc)

Scratch scripts, run in `ubuntu:24.04` (GStreamer 1.24.2) with `~/rcspike` mounted at `/w` and `~/rcspike/out` at `/out`:

1. `build_plugins.sh`: builds `gst-plugins-rs` 0.13 (`webrtc`, `rtp`) with rustup + cargo-c (~4 min).
2. `build_signalling.sh`: builds `gst-webrtc-signalling-server`.
3. `loopback_netem.sh`: `videotestsrc` -> `webrtcsink` -> `webrtcsrc` on loopback with `tc netem` loss (needs `--cap-add NET_ADMIN`).

Results: `docs/results.md`, "WebRTC spike".
