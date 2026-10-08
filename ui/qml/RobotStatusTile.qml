import QtQuick
import RcBackend 1.0

// Robot status string over ELRS (FLIGHT_MODE), e.g. RDY, WRN:TEMP, FLT:MOTOR_L.
Tile {
    readonly property var s: Telemetry.status
    readonly property bool hasData: Fmt.has(s.text) && s.text !== ""

    title: "ROBOT"
    value: hasData ? s.text : Fmt.none
    sub1: {
        switch (tileState) {
        case "crit": return "fault"
        case "warn": return "warning"
        case "ok": return String(s.text).indexOf("DRV") === 0 ? "driving" : "ready"
        default: return ""
        }
    }
    sub2: hasData && Fmt.has(s.age_ms) && s.age_ms >= 1500 ? "last update " + Fmt.seconds(s.age_ms) + " ago" : ""
    stale: hasData && (!Telemetry.fresh || s.stale === true)
    tileState: Fmt.statusState(hasData ? s.text : undefined)
    highlighted: Alarms.activeIds.indexOf("status_stale") >= 0
}
