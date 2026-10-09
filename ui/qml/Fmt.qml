pragma Singleton
import QtQuick

// Formatting helpers. Missing values ("never received", JSON null) show as "n/a".
QtObject {
    readonly property string none: "n/a"

    function has(v) { return v !== undefined && v !== null }

    // Fixed decimals, Unicode minus sign, optional unit.
    function num(v, decimals, unit) {
        if (!has(v)) return none
        let s = Number(v).toFixed(decimals || 0)
        if (s.charAt(0) === "-") s = "−" + s.substring(1)
        return unit ? s + " " + unit : s
    }

    function seconds(ms) {
        if (!has(ms) || ms < 0) return none
        return ms < 10000 ? (ms / 1000).toFixed(1) + " s" : Math.round(ms / 1000) + " s"
    }

    // Robot status string "<STATE>[:<CODE>]" (design §4) -> tile state.
    function statusState(text) {
        if (!has(text) || text === "") return "nodata"
        const s = String(text).split(":")[0]
        if (s === "FLT") return "crit"
        if (s === "WRN") return "warn"
        if (s === "RDY" || s === "DRV") return "ok"
        return "normal"
    }

    function overridesText(list) {
        if (!list || list.length === 0) return ""
        return list.map(o => "ch" + o.channel + " " + o.value_us).join(" · ")
    }
}
