import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.spotix.qt 1.0

ApplicationWindow {
    id: root

    width: 1180
    height: 780
    minimumWidth: 900
    minimumHeight: 560
    visible: true
    title: "Spotix Qt"
    color: terminalBg

    readonly property SpotixApp spotix: SpotixApp {}
    property string activePane: "detail"
    property color terminalBg: "#050505"
    property color panelBg: "#0b0f10"
    property color panelAlt: "#111718"
    property color borderColor: "#2f3b3d"
    property color textColor: "#d8dee9"
    property color dimText: "#839496"
    property color accent: "#00ff87"
    property color cyan: "#00d7ff"
    property color warn: "#ffd75f"
    property color error: "#ff5f5f"
    property color selection: "#183a3a"
    property int rowHeight: 28

    Component.onCompleted: {
        root.spotix.refreshSession()
        root.spotix.loadLibrary()
        keyboardRoot.forceActiveFocus()
    }

    function formatTime(ms) {
        var total = Math.max(0, Math.floor(ms / 1000))
        var minutes = Math.floor(total / 60)
        var seconds = total % 60
        return minutes + ":" + (seconds < 10 ? "0" : "") + seconds
    }

    function parseArray(json) {
        try {
            return JSON.parse(json)
        } catch (e) {
            return []
        }
    }

    function currentTreeItem() {
        var items = root.parseArray(root.spotix.nav_tree_json)
        if (treeList.currentIndex < 0 || treeList.currentIndex >= items.length) {
            return null
        }
        return items[treeList.currentIndex]
    }

    function currentDetailRow() {
        var rows = root.parseArray(root.spotix.detail_rows_json)
        if (detailList.currentIndex < 0 || detailList.currentIndex >= rows.length) {
            return null
        }
        return rows[detailList.currentIndex]
    }

    function activateCurrent() {
        if (root.activePane === "tree") {
            var item = root.currentTreeItem()
            if (item && item.selectable) {
                root.spotix.activateTreeItem(item.id)
            }
            return
        }

        var row = root.currentDetailRow()
        if (!row) {
            return
        }
        if (row.playable || row.kind === "album" || row.kind === "artist" || row.kind === "playlist" || row.kind === "show") {
            root.spotix.activateDetailRow(row.id)
        }
    }

    function moveSelection(delta) {
        var view = root.activePane === "tree" ? treeList : detailList
        var count = view.count
        if (count <= 0) {
            return
        }
        view.currentIndex = Math.max(0, Math.min(count - 1, view.currentIndex + delta))
        view.positionViewAtIndex(view.currentIndex, ListView.Contain)
    }

    function depthPrefix(depth, expandable, playable) {
        var prefix = ""
        for (var i = 0; i < depth; i++) {
            prefix += "  "
        }
        if (playable) {
            return prefix + "> "
        }
        return prefix + (expandable ? "+ " : "- ")
    }

    Timer {
        interval: 500
        running: true
        repeat: true
        onTriggered: {
            root.spotix.refreshPlayback()
            root.spotix.refreshSession()
        }
    }

    Item {
        id: keyboardRoot
        anchors.fill: parent
        focus: true

        Keys.onPressed: function(event) {
            if (searchField.activeFocus && event.key !== Qt.Key_Escape) {
                return
            }
            if (event.key === Qt.Key_Down) {
                root.moveSelection(1)
                event.accepted = true
            } else if (event.key === Qt.Key_Up) {
                root.moveSelection(-1)
                event.accepted = true
            } else if (event.key === Qt.Key_Tab) {
                root.activePane = root.activePane === "tree" ? "detail" : "tree"
                event.accepted = true
            } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Right) {
                root.activateCurrent()
                event.accepted = true
            } else if (event.key === Qt.Key_Left || event.key === Qt.Key_Backspace) {
                root.spotix.navigateBack()
                event.accepted = true
            } else if (event.key === Qt.Key_Space) {
                root.spotix.playPause()
                event.accepted = true
            } else if (event.key === Qt.Key_Slash) {
                searchField.forceActiveFocus()
                searchField.selectAll()
                event.accepted = true
            } else if (event.key === Qt.Key_R && event.modifiers & Qt.ControlModifier) {
                root.spotix.loadLibrary()
                root.spotix.refreshSession()
                event.accepted = true
            } else if (event.key === Qt.Key_Escape) {
                keyboardRoot.forceActiveFocus()
                event.accepted = true
            }
        }

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: 10
            spacing: 8

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 42
                color: panelBg
                border.color: borderColor
                border.width: 1

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 12
                    anchors.rightMargin: 12
                    spacing: 10

                    Label {
                        text: "spotix@spotify:~$"
                        color: accent
                        font.family: "monospace"
                        font.pixelSize: 15
                        font.bold: true
                    }

                    TextField {
                        id: searchField
                        Layout.fillWidth: true
                        text: root.spotix.search_query
                        placeholderText: "search tracks, artists, albums, playlists, podcasts"
                        color: textColor
                        placeholderTextColor: dimText
                        selectionColor: selection
                        selectedTextColor: textColor
                        font.family: "monospace"
                        font.pixelSize: 15
                        background: Rectangle {
                            color: "#000000"
                            border.color: searchField.activeFocus ? accent : borderColor
                        }
                        onTextChanged: root.spotix.search_query = text
                        onAccepted: {
                            root.spotix.navigateToRoute("search")
                            root.spotix.submitSearch()
                            keyboardRoot.forceActiveFocus()
                        }
                    }

                    Button {
                        text: "run"
                        font.family: "monospace"
                        onClicked: {
                            root.spotix.navigateToRoute("search")
                            root.spotix.submitSearch()
                            keyboardRoot.forceActiveFocus()
                        }
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                spacing: 8

                Rectangle {
                    Layout.preferredWidth: 330
                    Layout.fillHeight: true
                    color: panelBg
                    border.color: root.activePane === "tree" ? accent : borderColor
                    border.width: 1

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 8
                        spacing: 6

                        Label {
                            Layout.fillWidth: true
                            text: "TREE"
                            color: cyan
                            font.family: "monospace"
                            font.pixelSize: 13
                            font.bold: true
                        }

                        ListView {
                            id: treeList
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            clip: true
                            currentIndex: 0
                            model: root.parseArray(root.spotix.nav_tree_json)
                            boundsBehavior: Flickable.StopAtBounds

                            delegate: Rectangle {
                                width: treeList.width
                                height: rowHeight
                                color: ListView.isCurrentItem && root.activePane === "tree" ? selection : "transparent"

                                RowLayout {
                                    anchors.fill: parent
                                    anchors.leftMargin: 6
                                    anchors.rightMargin: 6
                                    spacing: 8

                                    Label {
                                        Layout.fillWidth: true
                                        text: root.depthPrefix(modelData.depth, modelData.expanded, modelData.playable) + modelData.label
                                        color: ListView.isCurrentItem ? accent : textColor
                                        elide: Text.ElideRight
                                        font.family: "monospace"
                                        font.pixelSize: 14
                                    }

                                    Label {
                                        text: modelData.meta
                                        color: dimText
                                        elide: Text.ElideRight
                                        font.family: "monospace"
                                        font.pixelSize: 12
                                    }
                                }

                                MouseArea {
                                    anchors.fill: parent
                                    onClicked: {
                                        root.activePane = "tree"
                                        treeList.currentIndex = index
                                        root.spotix.activateTreeItem(modelData.id)
                                        keyboardRoot.forceActiveFocus()
                                    }
                                }
                            }
                        }
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    color: panelBg
                    border.color: root.activePane === "detail" ? accent : borderColor
                    border.width: 1

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 8
                        spacing: 6

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8

                            Label {
                                Layout.fillWidth: true
                                text: root.spotix.active_route_title
                                color: accent
                                font.family: "monospace"
                                font.pixelSize: 18
                                font.bold: true
                                elide: Text.ElideRight
                            }

                            Label {
                                text: root.spotix.authenticated ? "ONLINE" : "LOGIN"
                                color: root.spotix.authenticated ? accent : warn
                                font.family: "monospace"
                                font.pixelSize: 13
                            }
                        }

                        Label {
                            Layout.fillWidth: true
                            text: root.spotix.detail_status
                            color: dimText
                            wrapMode: Text.WordWrap
                            font.family: "monospace"
                            font.pixelSize: 13
                        }

                        Rectangle {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 1
                            color: borderColor
                        }

                        ListView {
                            id: detailList
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            clip: true
                            currentIndex: 0
                            model: root.parseArray(root.spotix.detail_rows_json)
                            boundsBehavior: Flickable.StopAtBounds

                            delegate: Rectangle {
                                width: detailList.width
                                height: rowHeight
                                color: ListView.isCurrentItem && root.activePane === "detail" ? selection : "transparent"

                                RowLayout {
                                    anchors.fill: parent
                                    anchors.leftMargin: 6
                                    anchors.rightMargin: 6
                                    spacing: 12

                                    Label {
                                        Layout.preferredWidth: 92
                                        text: "[" + modelData.kind + "]"
                                        color: modelData.playable ? accent : cyan
                                        elide: Text.ElideRight
                                        font.family: "monospace"
                                        font.pixelSize: 12
                                    }

                                    Label {
                                        Layout.fillWidth: true
                                        text: root.depthPrefix(modelData.depth, modelData.expandable, modelData.playable) + modelData.label
                                        color: ListView.isCurrentItem ? accent : textColor
                                        elide: Text.ElideRight
                                        font.family: "monospace"
                                        font.pixelSize: 14
                                    }

                                    Label {
                                        Layout.preferredWidth: 260
                                        text: modelData.meta
                                        color: dimText
                                        elide: Text.ElideRight
                                        font.family: "monospace"
                                        font.pixelSize: 12
                                    }
                                }

                                MouseArea {
                                    anchors.fill: parent
                                    onClicked: {
                                        root.activePane = "detail"
                                        detailList.currentIndex = index
                                        if (modelData.playable || modelData.kind === "album" || modelData.kind === "artist" || modelData.kind === "playlist" || modelData.kind === "show") {
                                            root.spotix.activateDetailRow(modelData.id)
                                        }
                                        keyboardRoot.forceActiveFocus()
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 118
                color: panelAlt
                border.color: borderColor
                border.width: 1

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 10
                    spacing: 6

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 12

                        Label {
                            Layout.fillWidth: true
                            text: "now-playing: " + root.spotix.now_playing_title + " | " + root.spotix.now_playing_artist
                            color: textColor
                            elide: Text.ElideRight
                            font.family: "monospace"
                            font.pixelSize: 14
                            font.bold: true
                        }

                        Button {
                            text: "prev"
                            font.family: "monospace"
                            onClicked: root.spotix.playPrevious()
                        }

                        Button {
                            text: root.spotix.playback_state === "Playing" ? "pause" : "play"
                            font.family: "monospace"
                            onClicked: root.spotix.playPause()
                        }

                        Button {
                            text: "next"
                            font.family: "monospace"
                            onClicked: root.spotix.playNext()
                        }

                        Button {
                            text: "stop"
                            font.family: "monospace"
                            onClicked: root.spotix.stopPlayback()
                        }
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 10

                        Label {
                            text: root.formatTime(root.spotix.playback_progress_ms)
                            color: dimText
                            font.family: "monospace"
                            font.pixelSize: 12
                        }

                        Slider {
                            Layout.fillWidth: true
                            from: 0
                            to: Math.max(1, root.spotix.playback_duration_ms)
                            value: root.spotix.playback_progress_ms
                            onMoved: root.spotix.seekPlayback(value / Math.max(1, root.spotix.playback_duration_ms))
                        }

                        Label {
                            text: root.formatTime(root.spotix.playback_duration_ms)
                            color: dimText
                            font.family: "monospace"
                            font.pixelSize: 12
                        }

                        Slider {
                            Layout.preferredWidth: 110
                            from: 0
                            to: 1
                            value: root.spotix.volume
                            onMoved: root.spotix.setPlaybackVolume(value)
                        }
                    }

                    Label {
                        Layout.fillWidth: true
                        text: "keys: tab pane | arrows move | enter/right open | left/backspace back | space play/pause | / search | ctrl+r reload"
                        color: dimText
                        elide: Text.ElideRight
                        font.family: "monospace"
                        font.pixelSize: 12
                    }

                    Label {
                        Layout.fillWidth: true
                        text: root.spotix.playback_status + " | " + root.spotix.queue_summary
                        color: root.spotix.playback_state === "Blocked" ? warn : dimText
                        elide: Text.ElideRight
                        font.family: "monospace"
                        font.pixelSize: 12
                    }
                }
            }
        }
    }
}
