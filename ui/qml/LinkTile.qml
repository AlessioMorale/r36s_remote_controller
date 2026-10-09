import QtQuick
import RcUi
import RcBackend 1.0

// ELRS link: uplink LQ, RSSI, SNR, TX power (design §3.2 status fields).
Tile {
    readonly property var l: Telemetry.link
    readonly property var tx: Telemetry.tx
    readonly property bool hasData: Fmt.has(l.lq)
    readonly property bool degraded: Alarms.activeIds.indexOf("elrs_degraded") >= 0
    readonly property bool lost: Alarms.elrsLost

    title: "Link"
    value: hasData ? "LQ " + l.lq + "%" : "LQ " + Fmt.none
    sub1: Fmt.num(l.rssi_dbm, 0, "dBm") + " · " + Fmt.num(l.snr_db, 0, "dB")
    sub2: {
        let s = "TX " + Fmt.num(l.tx_power_mw, 0, "mW")
        if (Fmt.has(tx.serial_open) && !tx.serial_open) s += " · UART failed"
        else if (Fmt.has(tx.synced) && !tx.synced) s += " · no sync"
        return s
    }
    stale: hasData && (!Telemetry.fresh || l.stale === true || lost)
    tileState: !hasData ? "nodata" : (lost || l.lq === 0) ? "crit" : degraded ? "warn" : "normal"
    highlighted: degraded
}
