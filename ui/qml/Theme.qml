pragma Singleton
import QtQuick
import RcBackend 1.0

// Colors and sizes. Dark is the default; "light" is the high-contrast sunlight
// palette of design §3.2.1 option C, switchable from the menu.
QtObject {
    readonly property bool dark: AppConfig.theme !== "light"

    // Surfaces
    readonly property color bg:          dark ? "#07090a" : "#e9edef"
    readonly property color bar:         dark ? "#0e1113" : "#f4f6f7"
    readonly property color tile:        dark ? "#171c20" : "#ffffff"
    readonly property color border:      dark ? "#2a3238" : "#c9d1d6"
    readonly property color video:       dark ? "#1a2126" : "#dfe5e8"
    readonly property color novid:       dark ? "#161c20" : "#eef1f2"

    // Text
    readonly property color text:        dark ? "#e8edf0" : "#101418"
    readonly property color textDim:     dark ? "#9aa6ae" : "#4a555c"
    readonly property color textMuted:   dark ? "#8c98a0" : "#5b666d"

    // States
    readonly property color okFill:      dark ? "#1f3b2e" : "#e3f4ec"
    readonly property color okBorder:    dark ? "#3ddc97" : "#0b7a4b"
    readonly property color okText:      dark ? "#e8edf0" : "#0b3d27"
    readonly property color armedFill:   dark ? "#f2b13b" : "#fff1d6"
    readonly property color armedBorder: dark ? "#f2b13b" : "#a05e00"
    readonly property color armedText:   dark ? "#1b1306" : "#3d2400"
    readonly property color warn:        dark ? "#f2b13b" : "#a05e00"
    readonly property color warnFill:    dark ? "#3a2c10" : "#fff1d6"
    readonly property color crit:        "#c42b1c"
    readonly property color critBorder:  "#ff5a4e"
    readonly property color critText:    "#ffffff"
    readonly property color critOnTile:  dark ? "#ff6b5e" : "#b3261e"
    readonly property color staleFill:   dark ? "#22292e" : "#e4e8ea"
    readonly property color staleText:   dark ? "#8c98a0" : "#5b666d"
    readonly property color quiet:       dark ? "#9aa6ae" : "#4a555c"
    readonly property color quietWarn:   dark ? "#c9a25a" : "#7a5a1c"

    // Menu
    readonly property color menuHeader:     "#f2b13b"
    readonly property color menuHeaderText: "#1b1306"
    readonly property color focusFill:   dark ? "#1d3a44" : "#d6eef5"
    readonly property color focusBorder: dark ? "#7fd4e8" : "#0a6f88"
    readonly property color accent:      dark ? "#7fd4e8" : "#0a6f88"

    // Typography (pixel sizes; 640x480 at ~3.5": 1 px ≈ 0.11 mm)
    readonly property string font: AppConfig.fontFamily
    readonly property int fsTitle: 13
    readonly property int fsBig: 30
    readonly property int fsMed: 17
    readonly property int fsSmall: 14
    readonly property int fsBar: 15

    // Option B geometry (design §3.2.1): 448x336 video, 192 px instrument column.
    readonly property int screenW: 640
    readonly property int screenH: 480
    readonly property int statusBarH: 32
    readonly property int instrumentW: 192
    readonly property int videoW: 448
    readonly property int videoH: 336
}
