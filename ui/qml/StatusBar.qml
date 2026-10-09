import QtQuick
import RcUi
import RcBackend 1.0

// Always-on status bar, drawn natively above everything (design §3.2).
// Left: daemon link, input mode, turbo, gamepad, active overrides (design §6).
// Right: handheld battery and the WiFi/VPN indicator (quiet severity: never red).
// Violet fill, so every text on it is Paper (5.7:1).
Rectangle {
    id: bar
    color: Theme.bar

    readonly property var inp: Telemetry.input
    readonly property var ovr: Telemetry.overrides

    component Chip: Rectangle {
        property alias text: label.text
        property color fg: Theme.onBar
        property color fill: "transparent"
        property bool strong: false
        height: bar.height - 8
        width: label.implicitWidth + 14
        radius: 0
        color: fill
        border.color: Theme.paper
        border.width: 1
        Text {
            id: label
            anchors.centerIn: parent
            color: parent.fg
            font.family: Theme.mono
            font.pixelSize: Theme.fsBar - 1
            font.weight: parent.strong ? Font.DemiBold : Font.Normal
        }
    }

    Row {
        id: left
        anchors { left: parent.left; leftMargin: 4; verticalCenter: parent.verticalCenter }
        spacing: 4

        Chip {
            visible: !Ipc.connected
            text: "No daemon"
            fill: Theme.crit; fg: Theme.critText; strong: true
        }
        Chip {
            visible: Ipc.connected
            text: !Fmt.has(bar.inp.mode) ? Fmt.none : bar.inp.mode === "menu" ? "Menu" : "Drive"
            fill: bar.inp.mode === "menu" ? Theme.paper : Theme.night
            fg: bar.inp.mode === "menu" ? Theme.night : Theme.paper
            strong: true
        }
        Chip {
            visible: Ipc.connected && Fmt.has(bar.inp.turbo)
            text: "Turbo " + (bar.inp.turbo ? "on" : "off")
            fill: bar.inp.turbo ? Theme.night : "transparent"
            fg: Theme.onBar
            strong: bar.inp.turbo === true
        }
        Chip {
            visible: Ipc.connected && bar.inp.device === false
            text: "Pad missing"
            fill: Theme.crit; fg: Theme.critText; strong: true
        }
        Chip {
            visible: bar.ovr.length > 0
            text: "Override " + Fmt.overridesText(bar.ovr)
            fill: Theme.armedFill; fg: Theme.armedText; strong: true
        }
    }

    Row {
        anchors { right: parent.right; rightMargin: 8; verticalCenter: parent.verticalCenter }
        spacing: 12

        Text {
            visible: Host.percent >= 0
            anchors.verticalCenter: parent.verticalCenter
            text: "R36S " + Host.percent + "%" + (Host.charging ? " charging" : Host.percent <= 15 ? " low" : "")
            color: Theme.onBar
            font.family: Theme.mono
            font.pixelSize: Theme.fsBar - 1
            font.weight: Host.percent <= 15 ? Font.DemiBold : Font.Normal
        }
        Row {
            spacing: 6
            anchors.verticalCenter: parent.verticalCenter
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: 10; height: 10; radius: 0
                color: Net.degraded ? "transparent" : Theme.paper
                border.color: Theme.paper
                border.width: 2
            }
            Text {
                text: Net.summary
                color: Theme.onBar
                font.family: Theme.mono
                font.pixelSize: Theme.fsBar - 1
                font.weight: Net.degraded ? Font.Normal : Font.DemiBold
            }
        }
    }
}
