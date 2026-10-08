import QtQuick
import QtQuick.Window
import RcBackend 1.0

// Root window: 640x480, fullscreen on the device (eglfs).
// Z order: layout < ELRS-lost screen < status bar (always on top, design §3.2).
Window {
    id: win
    width: Theme.screenW
    height: Theme.screenH
    minimumWidth: Theme.screenW
    minimumHeight: Theme.screenH
    maximumWidth: Theme.screenW
    maximumHeight: Theme.screenH
    visible: true
    visibility: AppConfig.fullscreen ? Window.FullScreen : Window.Windowed
    color: Theme.bg
    title: "KVN remote"

    readonly property bool menuOpen: Telemetry.menuMode && Ipc.connected
    readonly property var otherLoud: Alarms.loudAlarms.filter(a => a.id !== "elrs_lost")
    readonly property bool elrsLostScreen: Alarms.elrsLost && !menuOpen
    readonly property int contentH: Theme.screenH - Theme.statusBarH

    onMenuOpenChanged: if (menuOpen && layoutLoader.item) layoutLoader.item.resetMenu()

    Item {
        id: root
        anchors.fill: parent
        focus: true

        // Desktop testing only: the device has no keyboard; the daemon sends menu_input.
        Keys.onPressed: function(event) {
            if (!win.menuOpen || !layoutLoader.item) return
            const map = {}
            map[Qt.Key_Up] = "up"; map[Qt.Key_Down] = "down"
            map[Qt.Key_Left] = "left"; map[Qt.Key_Right] = "right"
            map[Qt.Key_Return] = "a"; map[Qt.Key_Enter] = "a"; map[Qt.Key_A] = "a"
            map[Qt.Key_Escape] = "b"; map[Qt.Key_Backspace] = "b"; map[Qt.Key_B] = "b"
            const btn = map[event.key]
            if (btn) {
                layoutLoader.item.handleMenuInput(btn)
                event.accepted = true
            }
        }

        // Swappable screen layout (T0.7): LayoutB.qml today; A/C would be siblings.
        Loader {
            id: layoutLoader
            x: 0; y: 0
            width: parent.width
            height: win.contentH
            source: "Layout" + (AppConfig.layout || "B") + ".qml"
            onStatusChanged: if (status === Loader.Error) source = "LayoutB.qml"
            onLoaded: {
                item.menuOpen = Qt.binding(() => win.menuOpen)
                item.otherLoudAlarms = Qt.binding(() => win.otherLoud)
            }
        }

        ElrsLostScreen {
            x: 0; y: 0
            width: parent.width
            height: win.contentH
            visible: win.elrsLostScreen
            toneOn: AppConfig.soundEnabled
        }

        StatusBar {
            x: 0
            y: win.contentH
            width: parent.width
            height: Theme.statusBarH
            z: 100
        }
    }

    Connections {
        target: Ipc
        function onMenuInput(button) {
            if (win.menuOpen && layoutLoader.item) layoutLoader.item.handleMenuInput(button)
        }
    }

    Loader {
        active: AppConfig.soundEnabled
        source: AppConfig.soundAvailable ? "AlarmSound.qml" : ""
        onLoaded: item.active = Qt.binding(() => Alarms.loud)
    }
}
