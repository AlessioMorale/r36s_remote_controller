import QtQuick
import RcUi
import QtWebEngine

// The Lichtblick page (design §3.3, plan T4.5). Only compiled in with
// RC_UI_WEBENGINE. It runs in Chromium's renderer process; the host (VideoArea)
// destroys and recreates this item when `failed` is emitted.
//
// Failure detection:
//  * renderProcessTerminated (crash, OOM kill, kill -9 of the renderer)
//  * the main frame fails to load, or does not finish within loadTimeoutMs
//  * hang: a trivial runJavaScript() round trip gets no answer for hangTimeoutMs
Item {
    id: root
    property url url
    property int loadTimeoutMs: 30000
    property int pingIntervalMs: 3000
    property int hangTimeoutMs: 10000

    property bool ready: false
    property bool dead: false
    property double lastPong: 0

    signal failed(string reason)

    function fail(reason) {
        if (dead) return
        dead = true
        ready = false
        console.warn("Lichtblick view failed:", reason)
        failed(reason)
    }

    WebEngineProfile {
        id: profile
        // Nothing persisted: every start loads the layout baked into index.html.
        offTheRecord: true
        httpCacheType: WebEngineProfile.MemoryHttpCache
        persistentCookiesPolicy: WebEngineProfile.NoPersistentCookies
    }

    WebEngineView {
        id: view
        anchors.fill: parent
        profile: profile
        url: root.url
        backgroundColor: Theme.novid
        activeFocusOnPress: false
        settings.localContentCanAccessRemoteUrls: true   // file:// page -> ws://robot:8765
        settings.localContentCanAccessFileUrls: true
        settings.showScrollBars: false
        settings.errorPageEnabled: false
        settings.playbackRequiresUserGesture: false
        settings.focusOnNavigationEnabled: false

        onLoadingChanged: function(info) {
            if (info.status === WebEngineView.LoadSucceededStatus) {
                root.lastPong = Date.now()
                root.ready = true
            } else if (info.status === WebEngineView.LoadFailedStatus) {
                root.fail("load failed: " + info.errorString)
            }
        }
        onRenderProcessTerminated: function(status, exitCode) {
            root.fail("renderer terminated (status " + status + ", code " + exitCode + ")")
        }
    }

    Timer {
        interval: root.loadTimeoutMs
        running: !root.ready && !root.dead
        onTriggered: root.fail("load timeout")
    }

    Timer {
        interval: root.pingIntervalMs
        repeat: true
        running: root.ready && !root.dead
        onTriggered: {
            if (Date.now() - root.lastPong > root.hangTimeoutMs) {
                root.fail("renderer hung (no JS reply for " + (Date.now() - root.lastPong) + " ms)")
                return
            }
            view.runJavaScript("1", function(result) { root.lastPong = Date.now() })
        }
    }
}
