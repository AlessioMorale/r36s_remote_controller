import QtQuick
import RcUi
import RcBackend 1.0

// Screen layout option B — "Instrument Split" (design §3.2.1, recommended).
// Left: the WiFi-fed area (448x336 video + plots strip); right: the fixed ELRS
// instrument column. Only the left area changes between full and degraded mode.
//
// Layout contract (shared by any LayoutX.qml loaded by Main.qml):
//   property bool menuOpen         - set by Main
//   function handleMenuInput(btn)  - menu navigation (up/down/left/right/a/b)
//   function resetMenu()           - called when the menu opens
// Full-screen states (ELRS lost) and the status bar are drawn by Main.qml.
Item {
    id: layout
    property bool menuOpen: false
    property var otherLoudAlarms: []

    function handleMenuInput(btn) { menuOverlay.handle(btn) }
    function resetMenu() { menuOverlay.reset() }

    VideoArea {
        id: videoArea
        x: 0; y: 0
        width: Theme.videoW
        height: parent.height
        videoH: Theme.videoH
    }

    InstrumentColumn {
        x: Theme.videoW + 2
        y: 0
        width: parent.width - x
        height: parent.height
    }

    MenuOverlay {
        id: menuOverlay
        x: 0; y: 0
        width: Theme.videoW
        height: parent.height
        visible: layout.menuOpen
    }

    LoudBanner {
        x: 0; y: 0
        width: Theme.videoW
        alarms: layout.menuOpen ? [] : layout.otherLoudAlarms
    }
}
