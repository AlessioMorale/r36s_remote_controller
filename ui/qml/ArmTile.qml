import QtQuick
import RcUi
import RcBackend 1.0

// Arm / deadman state as decided by the daemon (design §6). Display only:
// arming is possible only with the physical L1+R1 gesture.
Tile {
    id: armTile
    readonly property var a: Telemetry.arm
    readonly property bool hasData: Fmt.has(a.state)
    readonly property bool menu: Telemetry.menuMode

    title: "DRIVE"
    value: !hasData ? Fmt.none
         : a.state === "armed" ? "ARMED"
         : a.state === "arming" ? "ARMING…"
         : "DISARMED"
    sub1: !hasData ? ""
        : menu ? "menu open · neutral"
        : a.state === "armed" ? (a.deadman ? "R1 held" : "R1 released")
        : a.state === "arming" ? "keep L1+R1 held"
        : "L1+R1 1 s to arm"
    sub2: hasData ? "AUX1 " + (a.aux1 ? "HIGH" : "low") : ""
    stale: hasData && !Telemetry.fresh
    tileState: !hasData ? "nodata" : (a.state === "armed" || a.state === "arming") ? "armed" : "normal"

    // ARMING blinks so the 1 s hold is visible.
    property bool blinkPhase: false
    opacity: blinkPhase ? 0.55 : 1.0
    Timer {
        interval: 300
        repeat: true
        running: Telemetry.arm.state === "arming"
        onTriggered: armTile.blinkPhase = !armTile.blinkPhase
        onRunningChanged: if (!running) armTile.blinkPhase = false
    }
}
