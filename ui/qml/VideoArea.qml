import QtQuick
import RcUi
import RcBackend 1.0

// The WiFi-fed part of the screen (design §3.2.1 option B): video on top
// (videoH) and the plots strip below. video.source picks the video: "webrtc"
// (native view, WebrtcView, the robot's webrtcsink stream; no plots) or
// "lichtblick" (below). With the robot's bridge reachable it
// hosts Lichtblick (one view whose fixed layout holds both the video panel and
// the plot). Otherwise, or while Lichtblick is (re)starting, native placeholders
// are shown. Watchdog (plan T4.5): on failure the view is destroyed and
// recreated after an exponential backoff (2 s, 4 s, ... 30 s).
Item {
    id: area
    property int videoH: Theme.videoH

    readonly property bool useWebrtc: AppConfig.videoSource === "webrtc" && Webrtc.available
    readonly property bool wantedRtc: useWebrtc && Net.robotReachable
    readonly property bool canHost: AppConfig.videoSource === "lichtblick" && AppConfig.webEngineAvailable
                                    && AppConfig.lichtblickEnabled
    readonly property bool wanted: canHost && Net.robotReachable
    property bool cooling: false        // waiting out the backoff after a failure
    property int failures: 0
    property string lastFailure: ""
    property int backoffMs: 2000
    readonly property bool viewReady: lbLoader.item !== null && lbLoader.item.ready === true

    onWantedRtcChanged: syncWebrtc()
    Component.onCompleted: syncWebrtc()
    function syncWebrtc() {
        if (wantedRtc)
            Webrtc.start(AppConfig.signallerUri, AppConfig.webrtcStunServer,
                         AppConfig.webrtcStallMs, AppConfig.webrtcConnectTimeoutMs)
        else
            Webrtc.stop()
    }

    function headline() {
        if (useWebrtc && wantedRtc) {
            if (Webrtc.state === "retrying") return "VIDEO RESTARTING"
            return "CONNECTING VIDEO…"
        }
        return wanted ? (cooling ? "VIDEO VIEW RESTARTING" : "CONNECTING VIDEO…") : "NO VIDEO LINK"
    }
    function detail() {
        if (useWebrtc && wantedRtc)
            return Webrtc.state === "retrying" ? "retry in " + Math.round(Webrtc.retryInMs / 1000) + " s" : "waiting for the robot's stream"
        return wanted ? (cooling ? "retry in " + Math.round(backoffMs / 1000) + " s" : "loading Lichtblick") : "line-of-sight only"
    }
    function reasonText() {
        if (AppConfig.videoSource === "none") return "video disabled in config"
        if (AppConfig.videoSource === "webrtc" && !Webrtc.available) return "WebRTC not available in this build"
        if (!AppConfig.lichtblickEnabled && !useWebrtc) return "Lichtblick disabled in config"
        if (!AppConfig.webEngineAvailable) return "built without QtWebEngine"
        if (Net.wifi === "down") return "WiFi down"
        if (Net.vpn === "down") return "VPN down"
        if (!Net.robotReachable) return "robot bridge " + Net.robotEndpoint + " unreachable"
        return ""
    }

    // --- native placeholders (always underneath) ---------------------------
    Placeholder {
        id: videoPh
        x: 0; y: 0
        width: area.width
        height: area.videoH
        headline: area.headline()
        detail: area.detail()
        reason: area.wantedRtc ? (Webrtc.state === "retrying" ? Webrtc.reason : Net.robotEndpoint)
                               : area.wanted ? (area.cooling ? area.lastFailure : Net.robotEndpoint) : area.reasonText()
    }
    Placeholder {
        x: 0
        y: area.videoH + 2
        width: area.width
        height: area.height - area.videoH - 2
        headline: area.useWebrtc ? "Video only" : "Plots need WiFi"
        headlineSize: 18
        detail: ""
    }

    // --- native WebRTC video --------------------------------------------------
    Loader {
        x: 0; y: 0
        width: area.width
        height: area.videoH
        active: area.useWebrtc
        source: area.useWebrtc ? "WebrtcView.qml" : ""
        visible: Webrtc.state === "live"
    }

    // --- Lichtblick --------------------------------------------------------
    Loader {
        id: lbLoader
        anchors.fill: parent
        active: area.wanted && !area.cooling
        visible: area.viewReady
        asynchronous: false
        source: area.canHost ? "LichtblickWeb.qml" : ""
        onLoaded: {
            item.url = AppConfig.lichtblickUrl
            item.loadTimeoutMs = AppConfig.lichtblickLoadTimeoutMs
            item.pingIntervalMs = AppConfig.lichtblickPingIntervalMs
            item.hangTimeoutMs = AppConfig.lichtblickHangTimeoutMs
        }
    }
    Connections {
        target: lbLoader.item
        ignoreUnknownSignals: true
        function onFailed(reason) {
            area.failures += 1
            area.lastFailure = reason
            area.backoffMs = Math.min(30000, 2000 * Math.pow(2, Math.min(area.failures - 1, 4)))
            // Unload outside the view's own signal handler: destroys the WebEngineView.
            Qt.callLater(function() { area.cooling = true; backoff.restart() })
        }
    }
    Timer {
        id: backoff
        interval: area.backoffMs
        onTriggered: area.cooling = false
    }
    // A view that stays healthy for a minute resets the backoff.
    Timer {
        interval: 60000
        running: area.viewReady
        onTriggered: { area.failures = 0; area.backoffMs = 2000 }
    }
}
