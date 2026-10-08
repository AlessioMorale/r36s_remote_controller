export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-libav gstreamer1.0-nice gstreamer1.0-tools iproute2 >/dev/null 2>&1
export GST_PLUGIN_PATH=/out/lib/gstreamer-1.0
gst-launch-1.0 --version | head -1
tc qdisc replace dev eth0 root netem loss ${LOSS:-5%} delay ${DELAY:-40ms}
STUN=stun://127.0.0.1:3478
until timeout 2 bash -c 'echo > /dev/tcp/robot/8443' 2>/dev/null; do sleep 2; done
sleep 4
timeout 30 gst-launch-1.0 -v webrtcsrc signaller::uri=ws://robot:8443 stun-server=$STUN connect-to-first-producer=true video-codecs="<H264>" ! decodebin ! videoconvert ! fpsdisplaysink video-sink=fakesink text-overlay=false signal-fps-measurements=true > /tmp/recv.log 2>&1
grep -o "avdec_[a-z0-9]*" /tmp/recv.log | sort -u
grep -o "last-message = .*" /tmp/recv.log | sed -n '5p;12p;$p' | cut -c1-100
echo "errors: $(grep -c -E 'ERROR|WARN' /tmp/recv.log)"
tc -s qdisc show dev eth0 | grep -E "Sent" | head -1
