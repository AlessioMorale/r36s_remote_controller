export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq cmake build-essential pkg-config ninja-build qt6-base-dev qt6-declarative-dev qt6-multimedia-dev libqt6multimedia6 qt6-qpa-plugins libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-libav gstreamer1.0-nice gstreamer1.0-tools libgl1-mesa-dev libxkbcommon-dev >/dev/null 2>&1 || { echo APT FAILED; exit 1; }
cd /w/ui_build && cmake -S /repo/ui -B . -G Ninja -DRC_UI_WEBENGINE=OFF 2>&1 | grep -E "rc_ui:|Error|error"
cmake --build . 2>&1 | grep -E "error|Error|warning" | head -20
export GST_PLUGIN_PATH=/out/lib/gstreamer-1.0 RC_TEST_SIGNALLING=/out/gst-webrtc-signalling-server QT_QPA_PLATFORM=offscreen
ctest --output-on-failure 2>&1 | tail -40
