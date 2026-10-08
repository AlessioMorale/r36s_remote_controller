export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-libav gstreamer1.0-nice gstreamer1.0-tools iproute2 procps >/dev/null 2>&1
export GST_PLUGIN_PATH=/out/lib/gstreamer-1.0
/out/gst-webrtc-signalling-server > /tmp/sig.log 2>&1 &
sleep 1
tc qdisc replace dev lo root netem loss 5% delay 40ms
STUN=stun://127.0.0.1:3478
gst-launch-1.0 webrtcsink name=ws stun-server=$STUN congestion-control=homegrown video-caps="video/x-h264" start-bitrate=1000000 min-bitrate=200000 max-bitrate=2000000 \
  videotestsrc is-live=true pattern=ball ! video/x-raw,width=640,height=480,framerate=15/1 ! ws. > /tmp/send.log 2>&1 &
SPID=$!
sleep 4
gst-launch-1.0 -v webrtcsrc stun-server=$STUN connect-to-first-producer=true video-codecs="<H264>" ! decodebin ! videoconvert ! fpsdisplaysink video-sink=fakesink text-overlay=false signal-fps-measurements=true > /tmp/recv.log 2>&1 &
RPID=$!
sleep 6
echo "--- send.log"; head -c 1500 /tmp/send.log
echo "--- recv.log"; head -c 1200 /tmp/recv.log
echo "--- alive:"; sleep 0; top -b -n 2 -d 8 -p $SPID,$RPID | grep gst-lau | tail -2; ps -o pid,pcpu,rss,args -p $SPID,$RPID | cut -c1-90
echo "--- codecs:"; grep -o "avdec_[a-z0-9]*\|vp8dec\|vp9dec" /tmp/recv.log | sort | uniq -c
grep -o "last-message = .*" /tmp/recv.log | tail -1 | cut -c1-100
kill $SPID $RPID 2>/dev/null
