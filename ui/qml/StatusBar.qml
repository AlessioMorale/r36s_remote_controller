import QtQuick
import RcBackend 1.0

// Always-on status bar, drawn natively above everything (design §3.2).
// Left: daemon link, input mode, turbo, gamepad, active overrides (design §6).
// Right: handheld battery and the WiFi/VPN indicator (quiet severity: never red).
Rectangle {
    id: bar
    color: Theme.bar
    border.color: Theme.border
    border.width: 1

    readonly property var inp: Telemetry.input
    readonly property var ovr: Telemetry.overrides

    component Chip: Rectangle {
        property alias text: label.text
        property color fg: Theme.text
        property color fill: "transparent"
        property bool strong: false
        height: bar.height - 8
        width: label.implicitWidth + 14
        radius: 3
        color: fill
        border.color: Qt.colorEqual(fill, "transparent") ? Theme.border : fill
        border.width: 1
        Text {
            id: label
            anchors.centerIn: parent
            color: parent.fg
            font.family: Theme.font
            font.pixelSize: Theme.fsBar
            font.bold: parent.strong
        }
    }

    Row {
        id: left
        anchors { left: parent.left; leftMargin: 4; verticalCenter: parent.verticalCenter }
        spacing: 4

        Chip {
            visible: !Ipc.connected
            text: "NO DAEMON"
            fill: Theme.crit; fg: Theme.critText; strong: true
        }
        Chip {
            visible: Ipc.connected
            text: !Fmt.has(bar.inp.mode) ? Fmt.none : bar.inp.mode === "menu" ? "MENU" : "DRIVE"
            fill: bar.inp.mode === "menu" ? Theme.menuHeader : "transparent"
            fg: bar.inp.mode === "menu" ? Theme.menuHeaderText : Theme.text
            strong: true
        }
        Chip {
            visible: Ipc.connected && Fmt.has(bar.inp.turbo)
            text: "Turbo " + (bar.inp.turbo ? "on" : "off")
            fill: bar.inp.turbo ? Theme.warnFill : "transparent"
            fg: bar.inp.turbo ? Theme.warn : Theme.textDim
            strong: bar.inp.turbo === true
        }
        Chip {
            visible: Ipc.connected && bar.inp.device === false
            text: "PAD MISSING"
            fill: Theme.crit; fg: Theme.critText; strong: true
        }
        Chip {
            visible: bar.ovr.length > 0
            text: "OVR " + Fmt.overridesText(bar.ovr)
            fill: Theme.armedFill; fg: Theme.armedText; strong: true
        }
    }

    Row {
        anchors { right: parent.right; rightMargin: 8; verticalCenter: parent.verticalCenter }
        spacing: 12

        Text {
            visible: Host.percent >= 0
            anchors.verticalCenter: parent.verticalCenter
            text: "R36S " + Host.percent + "%" + (Host.charging ? " ⚡" : "")
            color: Host.percent <= 15 ? Theme.warn : Theme.textDim
            font.family: Theme.font
            font.pixelSize: Theme.fsBar
        }
        Row {
            spacing: 6
            anchors.verticalCenter: parent.verticalCenter
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: 10; height: 10; radius: 5
                color: Net.degraded ? "transparent" : Theme.okBorder
                border.color: Net.degraded ? Theme.quietWarn : Theme.okBorder
                border.width: 2
            }
            Text {
                text: Net.summary
                color: Net.degraded ? Theme.quietWarn : Theme.text
                font.family: Theme.font
                font.pixelSize: Theme.fsBar
                font.bold: !Net.degraded
            }
        }
    }
}
