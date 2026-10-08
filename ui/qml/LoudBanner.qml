import QtQuick
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
    color: Theme.crit
    border.color: Theme.critBorder
    border.width: 3

    function detail(a) {
        switch (a.id) {
        case "elrs_degraded": return "LQ " + (Fmt.has(Telemetry.link.lq) ? Telemetry.link.lq + "%" : Fmt.none) + " · move closer, check antenna"
        case "serial_error": return "TX module UART failed · check module power and wiring"
        case "input_lost": return "gamepad lost · daemon disarmed"
        case "ui_daemon_lost": return "no telemetry · robot will failsafe-stop"
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
                    text: "⚠ " + String(modelData.message).toUpperCase()
                    color: Theme.critText
                    font.family: Theme.font
                    font.pixelSize: 24
                    font.bold: true
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
