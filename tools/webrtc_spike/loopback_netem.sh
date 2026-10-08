export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-libav gstreamer1.0-nice gstreamer1.0-tools iproute2 >/dev/null 2>&1
export GST_PLUGIN_PATH=/out/lib/gstreamer-1.0
/out/gst-webrtc-signalling-server > /tmp/sig.log 2>&1 &
sleep 1
STUN="stun://127.0.0.1:3478"
run() {
  echo "=== $1"
  tc qdisc replace dev lo root netem $2
  GST_DEBUG="webrtcsink*:6,*congestion*:6,*homegrown*:6" timeout 32 gst-launch-1.0 webrtcsink name=ws stun-server=$STUN congestion-control=homegrown start-bitrate=1000000 min-bitrate=200000 max-bitrate=2000000 \
    videotestsrc is-live=true pattern=ball ! video/x-raw,width=640,height=480,framerate=15/1 ! ws. > /tmp/send.log 2>&1 &
  SPID=$!
  sleep 4
  timeout 24 gst-launch-1.0 -v webrtcsrc stun-server=$STUN connect-to-first-producer=true ! decodebin ! videoconvert ! fpsdisplaysink video-sink=fakesink text-overlay=false signal-fps-measurements=true > /tmp/recv.log 2>&1 || true
  echo "recv fps samples:"; grep -o "last-message = .*" /tmp/recv.log | sed -n '5p;12p;18p;$p' | cut -c1-110
  echo "recv errors/warnings: $(grep -c -E 'ERROR|WARN' /tmp/recv.log)"
  echo "netem dropped:"; tc -s qdisc show dev lo | grep -E "Sent|dropped" | head -2
  echo "sender bitrate/loss log (sample):"
  sed 's/\x1b\[[0-9;]*m//g' /tmp/send.log | grep -i -E "bitrate|loss|rtt|delay-based|estimat" | grep -v -i "caps" | sed -n '1p;5p;$p' | cut -c1-200
  kill $SPID 2>/dev/null; wait $SPID 2>/dev/null
}
run clean "delay 0ms"
run "2pct_loss_20ms" "loss 2% delay 20ms"
run "5pct_loss_40ms" "loss 5% delay 40ms"
run "10pct_loss_60ms" "loss 10% delay 60ms"
