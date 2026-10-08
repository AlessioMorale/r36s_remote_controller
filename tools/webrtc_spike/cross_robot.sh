export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-libav gstreamer1.0-nice gstreamer1.0-tools iproute2 >/dev/null 2>&1
export GST_PLUGIN_PATH=/out/lib/gstreamer-1.0
gst-launch-1.0 --version | head -1
/out/gst-webrtc-signalling-server --host 0.0.0.0 > /tmp/sig.log 2>&1 &
sleep 1
tc qdisc replace dev eth0 root netem loss ${LOSS:-5%} delay ${DELAY:-40ms}
STUN=stun://127.0.0.1:3478
gst-launch-1.0 webrtcsink name=ws stun-server=$STUN congestion-control=homegrown video-caps="video/x-h264" start-bitrate=1000000 min-bitrate=200000 max-bitrate=2000000 \
  videotestsrc is-live=true pattern=ball ! video/x-raw,width=640,height=480,framerate=15/1 ! ws. > /tmp/send.log 2>&1
