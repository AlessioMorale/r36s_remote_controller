export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq libqt6multimedia6 libqt6quick6 libqt6qml6 libqt6network6 libqt6shadertools6 qt6-qpa-plugins qml6-module-qtquick qml6-module-qtquick-window qml6-module-qtquick-shapes qml6-module-qtmultimedia qml6-module-qtqml qml6-module-qtqml-workerscript qml6-module-qtquick-templates qml6-module-qtquick-layouts fonts-dejavu-core python3 libgstreamer1.0-0 libgstreamer-plugins-base1.0-0 gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-libav gstreamer1.0-nice gstreamer1.0-tools libglx-mesa0 libegl1 libgl1-mesa-dri >/dev/null 2>&1
export GST_PLUGIN_PATH=/out/lib/gstreamer-1.0
/out/gst-webrtc-signalling-server > /tmp/sig.log 2>&1 &
sleep 1
gst-launch-1.0 webrtcsink name=ws stun-server=stun://127.0.0.1:3478 video-caps=video/x-h264 videotestsrc is-live=true pattern=ball ! video/x-raw,width=640,height=480,framerate=15/1 ! ws. > /tmp/send.log 2>&1 &
sleep 2
cat > /tmp/cfg.json <<JSON
{ "ipc": {"socket_path": "/tmp/rc.sock"},
  "robot": {"host": "127.0.0.1", "signaller_port": 8443, "probe_interval_ms": 500},
  "video": {"source": "webrtc", "stun_server": "none"},
  "net": {"wifi_interface": "", "zerotier_cli": "zerotier-cli-missing-for-test"},
  "ui": {"sound": false, "theme": "dark", "font_family": "DejaVu Sans"} }
JSON
python3 /repo/ui/tools/mock_daemon.py --socket /tmp/rc.sock --scenario full 2>/tmp/mock.log &
sleep 1
SRC=${SRC:-webrtc}
sed -i "s/\"source\": \"webrtc\"/\"source\": \"$SRC\"/" /tmp/cfg.json
cd /w/ui_build && cmake -S /repo/ui -B . >/dev/null 2>&1; apt-get install -y -qq cmake ninja-build build-essential pkg-config qt6-base-dev qt6-declarative-dev qt6-multimedia-dev libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev >/dev/null 2>&1
cmake -S /repo/ui -B . -G Ninja -DRC_UI_WEBENGINE=OFF 2>&1 | grep -E "rc_ui:|rror"
cmake --build . 2>&1 | grep -E "error|rror:" | head
apt-get install -y -qq xvfb xauth libxkbcommon-x11-0 libxcb-icccm4 libxcb-image0 libxcb-keysyms1 libxcb-randr0 libxcb-render-util0 libxcb-shape0 libxcb-xinerama0 libxcb-xkb1 libxcb-xfixes0 >/dev/null 2>&1
export QT_QPA_PLATFORM=xcb LIBGL_ALWAYS_SOFTWARE=1
unset QT_QUICK_BACKEND
for SRC in webrtc; do
sed -i "s/\"source\": \"[a-z]*\"/\"source\": \"$SRC\"/" /tmp/cfg.json
timeout 90 xvfb-run -a -s "-screen 0 800x600x24" /w/ui_build/rc_ui --config /tmp/cfg.json --windowed --mute --screenshot /w/shots/ui_$SRC.png --screenshot-delay 12000 > /tmp/ui.log 2>&1
echo "== $SRC exit $?"; grep -v "^$" /tmp/ui.log | grep -E "rc.webrtc" | cut -c1-300 | head -6
done
echo "--- sender:"; pgrep -fa "gst-launch" | cut -c1-80; tail -8 /tmp/send.log | cut -c1-220; echo "--- sig:"; sed "s/\x1b\[[0-9;]*m//g" /tmp/sig.log | grep -E "registered|removing|producer|listener|Received message" | cut -c1-230 | head -14
ls -la /w/shots
