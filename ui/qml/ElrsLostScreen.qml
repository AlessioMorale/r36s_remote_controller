import QtQuick
import RcBackend 1.0

// Full-screen "ELRS link lost" alarm (design §3.2.1 shared states): the robot
// is in RX failsafe; the last known values are shown, marked STALE.
Rectangle {
    id: scr
    color: Theme.bg
    property bool toneOn: false

    readonly property var l: Telemetry.link
    readonly property var b: Telemetry.battery
    readonly property var s: Telemetry.status

    Rectangle {
        id: banner
        anchors { left: parent.left; right: parent.right; top: parent.top }
        height: 150
        color: Theme.crit
        border.color: Theme.critBorder
        border.width: 4
        Column {
            anchors.centerIn: parent
            spacing: 6
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: "ELRS LINK LOST"
                color: Theme.critText
                font.family: Theme.font
                font.pixelSize: 50
                font.bold: true
                font.letterSpacing: 2
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: "Robot failsafe stop · " + (Fmt.has(scr.l.age_ms) ? "last frame " + Fmt.seconds(scr.l.age_ms) + " ago"
                                                                      : "no link frame received")
                color: Theme.critText
                font.family: Theme.font
                font.pixelSize: 20
                font.bold: true
            }
        }
    }

    Row {
        id: tiles
        anchors { left: parent.left; right: parent.right; top: banner.bottom; margins: 8 }
        spacing: 8
        readonly property real tileW: (width - 2 * spacing) / 3
        Tile {
            width: tiles.tileW; height: 170
            title: "LINK"
            value: "LQ " + Fmt.none
            sub1: Fmt.num(scr.l.rssi_dbm, 0, "dBm") + " · " + Fmt.num(scr.l.snr_db, 0, "dB")
            sub2: "TX " + Fmt.num(scr.l.tx_power_mw, 0, "mW")
            stale: true
            valueSize: 34
        }
        Tile {
            width: tiles.tileW; height: 170
            title: "ROBOT BATT"
            value: Fmt.num(scr.b.voltage, 1, "V")
            sub1: (Fmt.has(scr.b.percent) ? scr.b.percent + "%" : Fmt.none)
            stale: true
            valueSize: 34
        }
        Tile {
            width: tiles.tileW; height: 170
            title: "ROBOT"
            value: Fmt.has(scr.s.text) && scr.s.text !== "" ? scr.s.text : Fmt.none
            stale: true
            valueSize: 34
        }
    }

    Rectangle {
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom; margins: 8 }
        height: 64
        color: Theme.bar
        border.color: Theme.border
        Text {
            anchors.fill: parent
            anchors.margins: 8
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            wrapMode: Text.WordWrap
            text: (scr.toneOn ? "Alarm tone on" : "Alarm tone off") +
                  " · check TX module power and antenna · move closer"
            color: Theme.text
            font.family: Theme.font
            font.pixelSize: Theme.fsMed
            font.bold: true
        }
    }
}
