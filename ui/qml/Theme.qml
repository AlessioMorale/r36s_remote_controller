pragma Singleton
import QtQuick
import RcBackend 1.0

// Slamming Works "Night shift" palette: flat fills, no gradients,
// radius 0, 2px borders. Dark is the default; "light" is the sunlight theme (Paper surfaces,
// Night text), switchable from the menu.
//
// Contrast rules the properties below follow:
//  - Slam Violet is a fill, never text on Night (3.0:1). Violet text on dark is Ultraviolet.
//  - Text on a Violet fill is Paper (5.7:1), never Ultraviolet (2.4:1).
//  - Secondary text is Ultraviolet (dark) or Violet (light), never grey.
//  - Acid Lime is the one accent per screen: the armed tile on the drive screen, the focus ring
//    in the menu.
// The state colors (ok green, warn amber, crit red) are not brand colors: they are safety
// signals, kept distinct from the palette on purpose.
QtObject {
    readonly property bool dark: AppConfig.theme !== "light"

    // Brand palette
    readonly property color violet:      "#9400D3"   // Slam Violet: fills only
    readonly property color ultraviolet: "#C77DFF"   // violet text and icons on dark
    readonly property color night:       "#0E0B14"
    readonly property color paper:       "#F2EFE8"
    readonly property color lime:        "#C6FF3D"   // Acid Lime: one moment per screen

    // Surfaces
    readonly property color bg:          dark ? night : paper
    readonly property color bar:         violet
    readonly property color tile:        dark ? night : paper
    readonly property color border:      dark ? paper : night
    readonly property color video:       dark ? night : paper
    readonly property color novid:       dark ? night : paper

    // Text
    readonly property color text:        dark ? paper : night
    readonly property color textDim:     dark ? ultraviolet : violet
    readonly property color textMuted:   dark ? ultraviolet : violet
    readonly property color onBar:       paper       // text on the Violet status bar
    readonly property color onFocus:     paper       // text on a Violet focused row

    // States
    readonly property color okFill:      tile
    readonly property color okBorder:    dark ? "#3ddc97" : "#0b7a4b"
    readonly property color okText:      text
    readonly property color armedFill:   lime
    readonly property color armedBorder: dark ? lime : night
    readonly property color armedText:   night
    readonly property color warn:        dark ? "#f2b13b" : "#8a4f00"
    readonly property color warnFill:    tile
    readonly property color crit:        "#c42b1c"
    readonly property color critBorder:  paper
    readonly property color critText:    paper
    readonly property color critOnTile:  dark ? "#ff6b5e" : "#b3261e"
    readonly property color staleFill:   tile
    readonly property color staleText:   dark ? ultraviolet : violet

    // Menu
    readonly property color menuHeader:     night
    readonly property color menuHeaderText: paper
    readonly property color focusFill:   violet
    readonly property color focusBorder: dark ? lime : night
    readonly property color accent:      dark ? ultraviolet : violet

    // Geometry (radius 0, 2px borders)
    readonly property int borderW: 2

    // Typography (pixel sizes; 640x480 at ~3.5": 1 px ≈ 0.11 mm).
    // Headings and big values: Archivo Black. Body: IBM Plex Sans. Small labels: IBM Plex Mono.
    // All three are bundled (resources/fonts, SIL OFL). ui.font_family overrides the body face.
    readonly property string display: "Archivo Black"
    readonly property string font: AppConfig.fontFamily !== "" ? AppConfig.fontFamily : "IBM Plex Sans"
    readonly property string mono: "IBM Plex Mono"
    readonly property int fsTitle: 13
    readonly property int fsBig: 28
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
