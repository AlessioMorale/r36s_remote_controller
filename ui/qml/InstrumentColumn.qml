import QtQuick

// The fixed column of ELRS-fed instruments. It never depends on WiFi and keeps
// the same position in every mode (design §3.2.1, R6).
Column {
    id: col
    spacing: 2
    readonly property real tileH: (height - 3 * spacing) / 4

    LinkTile { width: col.width; height: col.tileH }
    BatteryTile { width: col.width; height: col.tileH }
    RobotStatusTile { width: col.width; height: col.tileH }
    ArmTile { width: col.width; height: col.tileH }
}
