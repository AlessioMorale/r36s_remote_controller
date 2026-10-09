import QtQuick
import RcUi

// Native panel shown where WiFi-fed content (video, plots) would be.
Rectangle {
    id: ph
    property string headline
    property string detail
    property string reason
    property int headlineSize: 26

    color: Theme.novid
    border.color: Theme.border
    border.width: Theme.borderW

    Column {
        anchors.centerIn: parent
        width: parent.width - 32
        spacing: 6
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: ph.headline
            color: Theme.textMuted
            font.family: Theme.display
            font.pixelSize: ph.headlineSize
            wrapMode: Text.WordWrap
        }
        Text {
            width: parent.width
            visible: text !== ""
            horizontalAlignment: Text.AlignHCenter
            text: ph.detail
            color: Theme.textMuted
            font.family: Theme.font
            font.pixelSize: Theme.fsMed
            wrapMode: Text.WordWrap
        }
        Text {
            width: parent.width
            visible: text !== ""
            horizontalAlignment: Text.AlignHCenter
            text: ph.reason
            color: Theme.textMuted
            font.family: Theme.mono
            font.pixelSize: Theme.fsSmall
            wrapMode: Text.WordWrap
        }
    }
}
