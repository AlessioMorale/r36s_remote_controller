export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-libav gstreamer1.0-nice gstreamer1.0-tools procps >/dev/null 2>&1
export GST_PLUGIN_PATH=/out/lib/gstreamer-1.0
/out/gst-webrtc-signalling-server > /tmp/sig.log 2>&1 &
sleep 1
STUN=stun://127.0.0.1:3478
gst-launch-1.0 webrtcsink name=ws stun-server=$STUN video-caps="video/x-h264" videotestsrc is-live=true ! video/x-raw,width=640,height=480,framerate=15/1 ! ws. > /tmp/send.log 2>&1 &
SPID=$!
sleep 5
echo "idle (no consumer):"; top -b -n 2 -d 6 -p $SPID | grep gst-lau | tail -1 | awk '{print $9"%cpu"}'
gst-launch-1.0 webrtcsrc stun-server=$STUN connect-to-first-producer=true video-codecs="<H264>" ! decodebin ! fakesink sync=true > /dev/null 2>&1 &
RPID=$!
sleep 6
echo "with consumer:"; top -b -n 2 -d 6 -p $SPID | grep gst-lau | tail -1 | awk '{print $9"%cpu"}'
kill $RPID; sleep 3
echo "after consumer left:"; top -b -n 2 -d 6 -p $SPID | grep gst-lau | tail -1 | awk '{print $9"%cpu"}'
kill $SPID
