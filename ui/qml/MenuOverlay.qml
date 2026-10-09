import QtQuick
import RcUi
import RcBackend 1.0

// Menu overlay (design §3.2 / §3.2.1 shared states). Shown while the daemon
// reports input.mode == "menu" (Select toggles it inside the daemon). The device
// has no keyboard or touch: navigation comes only from `menu_input` IPC events
// (up/down/left/right/a/b), routed here through handle(). Main.qml also maps
// arrow keys / Enter / Esc to the same handler for desktop testing.
//
// Rows are generated: the ELRS section from the module's `params` tree
// (folders open sub-pages), then Input (stick calibration) and System.
// Editable values change locally with left/right ("pending"), and A writes them
// with `param_write`; B reverts a pending value, goes back, or closes the menu.
Rectangle {
    id: menu
    color: Theme.bg

    property var stack: [{ kind: "root", title: "" }]
    property int current: 0
    property string currentKey: ""
    property var pending: ({})      // param number -> raw value not yet written
    property var busy: ({})         // request id -> { key, type, number }
    property bool calibrating: false
    property string toast: ""
    property bool toastError: false

    readonly property var page: stack[stack.length - 1]
    readonly property var rows: buildRows(page, Params.revision, pending, busy, calibrating,
                                          Net.summary, Net.robotReachable, Ipc.connected,
                                          Ipc.daemonVersion, AppConfig.theme)
    // A function, not a binding: as a property its first evaluation pulled in `rows`, whose
    // onRowsChanged handler writes `current`, which `row` depends on (a binding loop on Qt 6.4).
    function currentRow() { return current >= 0 && current < rows.length ? rows[current] : null }

    // ---------------------------------------------------------------- model
    function section(label) { return { kind: "section", key: "s:" + label, label: label } }
    function info(key, label, value) { return { kind: "info", key: key, label: label, value: value } }
    function isBusy(key) {
        for (const id in busy) if (busy[id].key === key) return true
        return false
    }

    function paramRow(p) {
        const key = "p:" + p.number
        const t = p.type
        if (t === "folder")
            return { kind: "folder", key: key, label: p.name, value: "›", number: p.number }
        if (t === "command")
            return { kind: "command", key: key, label: p.name, number: p.number,
                     value: isBusy(key) ? "sending…" : Params.displayValue(p.number) }
        if (t === "info" || t === "string")
            return info(key, p.name, Params.displayValue(p.number))
        const hasPending = pending[p.number] !== undefined
        return { kind: "param", key: key, label: p.name, number: p.number, editable: true,
                 pending: hasPending,
                 value: isBusy(key) ? "writing…" : Params.displayValue(p.number, hasPending ? pending[p.number] : undefined) }
    }

    function buildRows(pg) {
        const r = []
        if (pg.kind === "root") {
            const dev = Params.device
            r.push(section("ELRS module" + (dev && dev.name ? " · " + dev.name : "")))
            const kids = Params.childrenOf(0)
            if (kids.length === 0)
                r.push(info("i:noparams", "Module", Ipc.connected ? "No parameters (A on Reload)" : "Daemon offline"))
            for (const p of kids) r.push(paramRow(p))
            r.push({ kind: "action", key: "a:reload", label: "Reload parameters",
                     value: isBusy("a:reload") ? "reading…" : "A" })

            r.push(section("Input"))
            r.push({ kind: "action", key: "a:calibrate", label: "Calibrate sticks",
                     value: isBusy("a:calibrate") ? "…" : calibrating ? "Recording: A save, B cancel" : "Start" })

            r.push(section("System"))
            r.push(info("i:net", "WiFi / VPN", Net.summary))
            r.push(info("i:bridge", "Robot bridge", Net.robotEndpoint + (Net.robotReachable ? ", reachable" : ", unreachable")))
            r.push({ kind: "choice", key: "c:theme", label: "Theme", value: Theme.dark ? "Night" : "Paper (sunlight)" })
            r.push(info("i:daemon", "Daemon", Ipc.connected ? (Ipc.daemonVersion || "?") + " · IPC v" + Ipc.protocolVersion
                                                            : "Not connected"))
            r.push(info("i:ui", "UI", AppConfig.uiVersion + (AppConfig.webEngineAvailable ? "" : ", no WebEngine")))
        } else {
            r.push(section(pg.title))
            const kids = Params.childrenOf(pg.number)
            if (kids.length === 0) r.push(info("i:empty", "Empty folder", ""))
            for (const p of kids) r.push(paramRow(p))
        }
        return r
    }

    function focusable(i) { return i >= 0 && i < rows.length && rows[i].kind !== "section" }
    function firstFocusable() {
        for (let i = 0; i < rows.length; ++i) if (focusable(i)) return i
        return -1
    }
    function setCurrent(i) {
        current = i
        currentKey = focusable(i) ? rows[i].key : ""
    }

    // Keep focus on the same row when the tree is re-sent. Ignored until the component is complete:
    // `rows` is first evaluated from inside another binding (current row, footer text), and writing
    // `current` there is a binding loop on Qt 6.4. The initial focus is set in onCompleted.
    property bool ready: false
    Component.onCompleted: { ready = true; setCurrent(firstFocusable()) }
    onRowsChanged: {
        if (!ready) return
        for (let i = 0; i < rows.length; ++i)
            if (rows[i].key === currentKey && focusable(i)) { current = i; return }
        setCurrent(firstFocusable())
    }
    onCurrentChanged: list.positionViewAtIndex(Math.max(0, current - (current > 0 && rows[current - 1].kind === "section" ? 1 : 0)), ListView.Contain)

    function reset() {
        stack = [{ kind: "root", title: "" }]
        pending = ({})
        toast = ""
        setCurrent(firstFocusable())
    }

    // ------------------------------------------------------------ requests
    function send(type, fields, key, number) {
        const id = Ipc.request(type, fields || {})
        const b = Object.assign({}, busy)
        b[id] = { key: key, type: type, number: number }
        busy = b
        return id
    }
    function setPending(number, value) {
        const p = Object.assign({}, pending)
        if (value === undefined) delete p[number]
        else p[number] = value
        pending = p
    }
    function showToast(text, isError) {
        toast = text
        toastError = !!isError
        toastTimer.restart()
    }

    Connections {
        target: Ipc
        function onAckReceived(id, ok, error, type) {
            const b = menu.busy[id]
            if (!b) return
            const nb = Object.assign({}, menu.busy)
            delete nb[id]
            menu.busy = nb
            if (!ok) {
                if (type === "calibration_finish") menu.calibrating = false
                menu.showToast(type + ": " + (error || "failed"), true)
                return
            }
            if (type === "param_write" && b.number !== undefined) menu.setPending(b.number, undefined)
            else if (type === "calibration_start") { menu.calibrating = true; menu.showToast("Move both sticks to every corner, then press A") }
            else if (type === "calibration_finish") { menu.calibrating = false; menu.showToast(b.save ? "Calibration saved" : "Calibration discarded") }
        }
    }

    // ---------------------------------------------------------- navigation
    function move(dir) {
        for (let i = current + dir; i >= 0 && i < rows.length; i += dir)
            if (focusable(i)) { setCurrent(i); return }
    }
    function adjust(dir) {
        const r = currentRow()
        const base = pending[r.number] !== undefined ? pending[r.number] : Params.param(r.number).value
        const next = Params.stepValue(r.number, base, dir)
        setPending(r.number, next === Params.param(r.number).value ? undefined : next)
    }
    function enter(r) {
        const s = stack.slice()
        s.push({ kind: "folder", number: r.number, title: r.label })
        stack = s
        setCurrent(firstFocusable())
    }
    function back() {
        const leaving = page
        const s = stack.slice()
        s.pop()
        // Set the key first: assigning `stack` rebuilds `rows`, and onRowsChanged
        // then puts the focus back on the folder we came from.
        currentKey = leaving.kind === "folder" ? "p:" + leaving.number : ""
        stack = s
    }
    function toggleTheme() { AppConfig.theme = Theme.dark ? "light" : "dark" }

    function activate(r) {
        switch (r.kind) {
        case "param":
            if (r.pending) send("param_write", { number: r.number, value: pending[r.number] }, r.key, r.number)
            else showToast("‹ › to change, then A")
            break
        case "folder":
            enter(r)
            break
        case "command": {
            const p = Params.param(r.number)
            // CRSF command: 1 = start; when the module asks to confirm (3), 4 = confirm.
            send("param_write", { number: r.number, value: p.status === 3 ? 4 : 1 }, r.key, r.number)
            break
        }
        case "action":
            if (r.key === "a:reload") send("param_refresh", {}, r.key)
            else if (r.key === "a:calibrate") {
                if (!menu.calibrating) send("calibration_start", {}, r.key)
                else finishCalibration(true)
            }
            break
        case "choice":
            toggleTheme()
            break
        }
    }
    function finishCalibration(save) {
        const id = send("calibration_finish", { save: save }, "a:calibrate")
        busy[id].save = save
    }

    function handle(button) {
        const r = currentRow()
        switch (button) {
        case "up": move(-1); break
        case "down": move(1); break
        case "left":
            if (!r) break
            if (r.kind === "param") adjust(-1)
            else if (r.kind === "choice") toggleTheme()
            else if (stack.length > 1) back()
            break
        case "right":
            if (!r) break
            if (r.kind === "param") adjust(1)
            else if (r.kind === "choice") toggleTheme()
            else if (r.kind === "folder") enter(r)
            break
        case "a":
            if (r) activate(r)
            break
        case "b":
            if (calibrating) finishCalibration(false)
            else if (r && r.kind === "param" && r.pending) setPending(r.number, undefined)
            else if (stack.length > 1) back()
            else Ipc.request("menu_close")
            break
        default:
            break   // x, y, start: unused
        }
    }

    // --------------------------------------------------------------- view
    // While the menu is open, loud alarms show here instead of covering the
    // menu (binding the module, for example, is done with the link lost).
    readonly property var loud: Alarms.loudAlarms
    Rectangle {
        id: header
        anchors { left: parent.left; right: parent.right; top: parent.top }
        height: 40
        color: menu.loud.length > 0 ? Theme.crit : Theme.menuHeader
        border.color: Theme.border
        border.width: Theme.borderW
        // The mark keeps its clear space (1/8 of the tile, 4 px) on every side, so it is 32 px in 40.
        Image {
            id: mark
            visible: menu.loud.length === 0
            anchors { left: parent.left; leftMargin: 4; verticalCenter: parent.verticalCenter }
            width: 32; height: 32
            source: "qrc:/qt/qml/RcUi/resources/slw-mark-128.png"
            sourceSize: Qt.size(128, 128)
            smooth: true
        }
        Text {
            anchors { left: mark.visible ? mark.right : parent.left; leftMargin: 8; right: parent.right; rightMargin: 8
                      verticalCenter: parent.verticalCenter }
            elide: Text.ElideRight
            text: menu.loud.length > 0 ? String(menu.loud[0].message) + ". Robot not driven."
                                       : "Menu: sticks held neutral, robot not driven"
            color: menu.loud.length > 0 ? Theme.critText : Theme.menuHeaderText
            font.family: menu.loud.length > 0 ? Theme.display : Theme.font
            font.pixelSize: menu.loud.length > 0 ? 15 : 14
            font.weight: Font.DemiBold
        }
    }

    ListView {
        id: list
        anchors { left: parent.left; right: parent.right; top: header.bottom; bottom: footer.top; margins: 4 }
        clip: true
        model: menu.rows
        spacing: 2
        boundsBehavior: Flickable.StopAtBounds
        interactive: false
        delegate: Rectangle {
            id: rowItem
            required property var modelData
            required property int index
            readonly property bool isSection: modelData.kind === "section"
            readonly property bool focused: index === menu.current
            readonly property bool editable: modelData.kind === "param" || modelData.kind === "choice"
            width: ListView.view.width
            height: isSection ? 28 : 34
            radius: 0
            color: isSection ? "transparent" : focused ? Theme.focusFill : Theme.tile
            border.width: isSection ? 0 : focused ? 3 : Theme.borderW
            border.color: focused ? Theme.focusBorder : Theme.border

            Text {
                anchors { left: parent.left; leftMargin: rowItem.isSection ? 4 : 12; verticalCenter: parent.verticalCenter }
                width: parent.width * (rowItem.isSection ? 0.95 : 0.5)
                text: modelData.label
                color: rowItem.isSection ? Theme.textDim : rowItem.focused ? Theme.onFocus : Theme.text
                font.family: rowItem.isSection ? Theme.display : Theme.font
                font.pixelSize: rowItem.isSection ? 14 : Theme.fsMed
                font.weight: rowItem.focused ? Font.DemiBold : Font.Normal
                elide: Text.ElideRight
            }
            Text {
                visible: !rowItem.isSection
                anchors { right: parent.right; rightMargin: 12; verticalCenter: parent.verticalCenter }
                width: parent.width * 0.48
                horizontalAlignment: Text.AlignRight
                text: {
                    const v = modelData.value === undefined ? "" : String(modelData.value)
                    return rowItem.focused && rowItem.editable ? "‹ " + v + " ›" : v
                }
                color: rowItem.focused ? Theme.onFocus : modelData.pending ? Theme.accent : modelData.kind === "info" ? Theme.textDim : Theme.text
                font.family: Theme.font
                font.pixelSize: Theme.fsMed
                font.weight: rowItem.focused || modelData.pending === true ? Font.DemiBold : Font.Normal
                font.underline: modelData.pending === true
                elide: Text.ElideLeft
            }
        }
    }

    Rectangle {
        id: toastBox
        visible: menu.toast !== ""
        anchors { left: parent.left; right: parent.right; bottom: footer.top; margins: 8 }
        height: 30
        radius: 0
        color: menu.toastError ? Theme.crit : Theme.violet
        border.color: Theme.paper
        border.width: Theme.borderW
        Text {
            anchors.centerIn: parent
            width: parent.width - 16
            horizontalAlignment: Text.AlignHCenter
            elide: Text.ElideRight
            text: menu.toast
            color: Theme.paper
            font.family: Theme.font
            font.pixelSize: Theme.fsSmall
            font.weight: Font.DemiBold
        }
    }
    Timer { id: toastTimer; interval: 4000; onTriggered: menu.toast = "" }

    Rectangle {
        id: footer
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: 28
        color: Theme.bg
        border.color: Theme.border
        border.width: Theme.borderW
        Text {
            anchors.centerIn: parent
            text: {
                const r = menu.currentRow()
                if (menu.calibrating) return "A save calibration · B cancel"
                if (r && r.kind === "param")
                    return r.pending ? "A apply · B revert · L/R change" : "L/R change · A apply · B back"
                return "D-pad move · A select · B " + (menu.stack.length > 1 ? "back" : "close") + " · Select close"
            }
            color: Theme.textDim
            font.family: Theme.mono
            font.pixelSize: 12
        }
    }
}
