import QtQuick
import RcUi
import RcBackend 1.0

// Robot battery over ELRS (BATTERY_SENSOR).
Tile {
    readonly property var b: Telemetry.battery
    readonly property bool hasData: Fmt.has(b.voltage)

    title: "ROBOT BATT"
    value: Fmt.num(b.voltage, 1, "V")
    sub1: (Fmt.has(b.percent) ? b.percent + "%" : Fmt.none) + " · " + Fmt.num(b.current, 1, "A")
    sub2: Fmt.has(b.used_mah) ? b.used_mah + " mAh used" : ""
    stale: hasData && (!Telemetry.fresh || b.stale === true)
    tileState: !hasData ? "nodata" : (Fmt.has(b.percent) && b.percent <= 20) ? "warn" : "normal"
    highlighted: Alarms.activeIds.indexOf("battery_stale") >= 0
}
