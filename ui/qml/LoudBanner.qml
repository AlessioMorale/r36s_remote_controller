import QtQuick
import RcUi
import RcBackend 1.0

// Loud alarms other than "ELRS link lost" (which takes the whole screen):
// elrs_degraded, serial_error, input_lost, and the UI's own ui_daemon_lost.
// A red banner across the top of the WiFi area, so the instrument column
// (which shows the failing value) stays readable.
Rectangle {
    id: banner
    property var alarms: []   // [{id, level, message}]
    visible: alarms.length > 0
    height: col.implicitHeight + 20
    radius: 0
    color: Theme.crit
    border.color: Theme.critBorder
    border.width: Theme.borderW

    function detail(a) {
        switch (a.id) {
        case "elrs_degraded": return "Link quality " + (Fmt.has(Telemetry.link.lq) ? Telemetry.link.lq + "%" : Fmt.none) + ", move closer and check the antenna"
        case "serial_error": return "TX module UART failed: check module power and wiring"
        case "input_lost": return "Gamepad lost, daemon disarmed"
        case "ui_daemon_lost": return "No telemetry, the robot will failsafe-stop"
        default: return ""
        }
    }

    Column {
        id: col
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: 10 }
        spacing: 6
        Repeater {
            model: banner.alarms
            delegate: Column {
                required property var modelData
                width: col.width
                spacing: 0
                Text {
                    width: parent.width
                    text: String(modelData.message)
                    color: Theme.critText
                    font.family: Theme.display
                    font.pixelSize: 22
                    elide: Text.ElideRight
                }
                Text {
                    width: parent.width
                    visible: text !== ""
                    text: banner.detail(modelData)
                    color: Theme.critText
                    font.family: Theme.font
                    font.pixelSize: Theme.fsSmall
                    elide: Text.ElideRight
                }
            }
        }
    }
}
