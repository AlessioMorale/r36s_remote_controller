import QtQuick
import RcUi
import QtQuick.Shapes

// One instrument tile: title, a large value, and up to two detail lines.
// `stale` greys it out with a dashed border and a STALE badge (last-known value);
// `highlighted` is the "visible" alarm level (amber frame).
Rectangle {
    id: tile
    property string title
    property string value: Fmt.none
    property string sub1
    property string sub2
    property string tileState: "normal"   // normal | ok | armed | warn | crit | nodata
    property bool stale: false
    property bool highlighted: false
    property int valueSize: Theme.fsBig
    property string badge: stale ? "STALE" : (tileState === "nodata" ? "NO DATA" : "")

    readonly property color fg: stale ? Theme.staleText
                              : tileState === "armed" ? Theme.armedText
                              : tileState === "ok" ? Theme.okText
                              : Theme.text
    readonly property color valueColor: stale ? Theme.staleText
                              : tileState === "crit" ? Theme.critOnTile
                              : tileState === "warn" ? Theme.warn
                              : fg

    color: stale ? Theme.staleFill
         : tileState === "armed" ? Theme.armedFill
         : tileState === "ok" ? Theme.okFill
         : tileState === "warn" ? Theme.warnFill
         : Theme.tile
    border.width: stale ? 0 : (highlighted ? 3 : 1)
    border.color: highlighted ? Theme.warn
                : tileState === "ok" ? Theme.okBorder
                : tileState === "armed" ? Theme.armedBorder
                : tileState === "crit" ? Theme.critBorder
                : tileState === "warn" ? Theme.warn
                : Theme.border

    // Dashed frame for stale values (design wireframe "ELRS link lost").
    Shape {
        anchors.fill: parent
        visible: tile.stale
        ShapePath {
            strokeColor: tile.highlighted ? Theme.warn : Theme.staleText
            strokeWidth: tile.highlighted ? 3 : 2
            strokeStyle: ShapePath.DashLine
            dashPattern: [3, 2]
            fillColor: "transparent"
            startX: 1; startY: 1
            PathLine { x: tile.width - 1; y: 1 }
            PathLine { x: tile.width - 1; y: tile.height - 1 }
            PathLine { x: 1; y: tile.height - 1 }
            PathLine { x: 1; y: 1 }
        }
    }

    Text {
        id: titleText
        x: 10; y: 7
        text: tile.title
        color: tile.stale ? Theme.staleText : (tile.tileState === "armed" ? Theme.armedText : Theme.textDim)
        font.family: Theme.font
        font.pixelSize: Theme.fsTitle
        font.bold: true
        font.letterSpacing: 1
    }

    Rectangle {
        visible: tile.badge !== ""
        anchors { right: parent.right; top: parent.top; margins: 6 }
        width: badgeText.implicitWidth + 10
        height: badgeText.implicitHeight + 2
        radius: 3
        color: tile.highlighted ? Theme.warn : Theme.staleText
        Text {
            id: badgeText
            anchors.centerIn: parent
            text: tile.badge
            color: Theme.dark ? "#0e1113" : "#ffffff"
            font.family: Theme.font
            font.pixelSize: 11
            font.bold: true
        }
    }

    Column {
        anchors { left: parent.left; right: parent.right; top: titleText.bottom; leftMargin: 10; rightMargin: 8; topMargin: 1 }
        spacing: 1
        Text {
            width: parent.width
            text: tile.value
            color: tile.valueColor
            font.family: Theme.font
            font.pixelSize: tile.valueSize
            font.bold: true
            fontSizeMode: Text.HorizontalFit
            minimumPixelSize: 16
            elide: Text.ElideRight
        }
        Text {
            width: parent.width
            visible: text !== ""
            text: tile.sub1
            color: tile.fg
            font.family: Theme.font
            font.pixelSize: Theme.fsSmall
            font.bold: true
            elide: Text.ElideRight
        }
        Text {
            width: parent.width
            visible: text !== ""
            text: tile.sub2
            color: tile.stale ? Theme.staleText : (tile.tileState === "armed" ? Theme.armedText : Theme.textDim)
            font.family: Theme.font
            font.pixelSize: Theme.fsSmall
            elide: Text.ElideRight
        }
    }
}
