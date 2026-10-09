import QtQuick
import QtMultimedia
import RcBackend 1.0

// Native video from the robot's WebRTC stream (WebrtcVideo). Loaded by VideoArea only when the
// build has WebRTC support; shows nothing but the picture, so placeholders stay in VideoArea.
Item {
    id: view
    readonly property bool live: Webrtc.state === "live"

    VideoOutput {
        id: output
        anchors.fill: parent
        fillMode: VideoOutput.PreserveAspectFit
        visible: view.live
    }
    Component.onCompleted: Webrtc.videoSink = output.videoSink
}
