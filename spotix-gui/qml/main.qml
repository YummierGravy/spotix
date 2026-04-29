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
    color: "#121212"

    readonly property SpotixApp spotix: SpotixApp {}

    Component.onCompleted: {
        root.spotix.refreshSession()
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

    function parseSearch(json) {
        try {
            return JSON.parse(json)
        } catch (e) {
            return {
                tracks: [],
                albums: [],
                artists: [],
                playlists: [],
                shows: []
            }
        }
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

    RowLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            Layout.fillHeight: true
            Layout.preferredWidth: 240
            color: "#000000"

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 24
                spacing: 18

                Label {
                    text: "Spotix"
                    color: "#ffffff"
                    font.pixelSize: 28
                    font.bold: true
                }

                Button {
                    text: "Home"
                    onClicked: root.spotix.goHome()
                }

                Button {
                    text: "Library"
                    onClicked: {
                        root.spotix.route = "library"
                        root.spotix.loadLibrary()
                    }
                }

                Button {
                    text: "Search"
                    onClicked: root.spotix.route = "search"
                }

                Button {
                    text: "Playlists"
                    onClicked: root.spotix.route = "playlists"
                }

                Button {
                    text: "Albums"
                    onClicked: root.spotix.route = "albums"
                }

                Button {
                    text: "Artists"
                    onClicked: root.spotix.route = "artists"
                }

                Button {
                    text: "Lyrics"
                    onClicked: root.spotix.route = "lyrics"
                }

                Button {
                    text: "Account"
                    onClicked: root.spotix.goLogin()
                }

                Item {
                    Layout.fillHeight: true
                }

                Label {
                    Layout.fillWidth: true
                    text: root.spotix.authenticated ? "Spotify credentials found" : "Login required"
                    color: root.spotix.authenticated ? "#1ed760" : "#f0c674"
                    wrapMode: Text.WordWrap
                }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 76
                color: "#181818"

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 24
                    anchors.rightMargin: 24
                    spacing: 16

                    TextField {
                        id: searchField
                        Layout.fillWidth: true
                        placeholderText: "Search for songs, artists, albums, playlists, or podcasts"
                        text: root.spotix.search_query
                        onTextChanged: root.spotix.search_query = text
                        onAccepted: root.spotix.submitSearch()
                    }

                    Button {
                        text: "Search"
                        onClicked: root.spotix.submitSearch()
                    }

                    Button {
                        text: "Play First"
                        onClicked: root.spotix.playFirstSearchResult()
                    }
                }
            }

            StackLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                currentIndex: {
                    if (root.spotix.route === "login") return 1
                    if (root.spotix.route === "library") return 2
                    if (root.spotix.route === "search") return 3
                    if (root.spotix.route === "playlists") return 4
                    if (root.spotix.route === "albums") return 5
                    if (root.spotix.route === "artists") return 6
                    if (root.spotix.route === "lyrics") return 7
                    return 0
                }

                Rectangle {
                    color: "#121212"

                    ColumnLayout {
                        anchors.centerIn: parent
                        width: Math.min(parent.width - 96, 760)
                        spacing: 16

                        Label {
                            Layout.fillWidth: true
                            text: "Home"
                            color: "#ffffff"
                            font.pixelSize: 34
                            font.bold: true
                        }

                        Label {
                            Layout.fillWidth: true
                            text: root.spotix.status
                            color: "#b3b3b3"
                            font.pixelSize: 16
                            wrapMode: Text.WordWrap
                        }

                        Label {
                            Layout.fillWidth: true
                            text: root.spotix.search_status
                            color: "#ffffff"
                            font.pixelSize: 18
                            wrapMode: Text.WordWrap
                        }

                        Label {
                            Layout.fillWidth: true
                            text: "Qt is now the primary entrypoint. Playback, search, and shell navigation are wired through the CXX-Qt bridge; remaining screens are being filled in as QML routes."
                            color: "#b3b3b3"
                            font.pixelSize: 16
                            wrapMode: Text.WordWrap
                        }
                    }
                }

                Rectangle {
                    color: "#121212"

                    ColumnLayout {
                        anchors.centerIn: parent
                        width: Math.min(parent.width - 96, 560)
                        spacing: 16

                        Label {
                            Layout.fillWidth: true
                            text: "Account setup"
                            color: "#ffffff"
                            font.pixelSize: 34
                            font.bold: true
                        }

                        Label {
                            Layout.fillWidth: true
                            text: root.spotix.login_status
                            color: "#b3b3b3"
                            font.pixelSize: 16
                            wrapMode: Text.WordWrap
                        }

                        Label {
                            Layout.fillWidth: true
                            visible: root.spotix.login_error.length > 0
                            text: root.spotix.login_error
                            color: "#ff6b6b"
                            font.pixelSize: 14
                            wrapMode: Text.WordWrap
                        }

                        RowLayout {
                            Button {
                                text: root.spotix.login_busy ? "Waiting for browser..." : "Login with Spotify"
                                enabled: !root.spotix.login_busy
                                onClicked: root.spotix.startSpotifyLogin()
                            }

                            Button {
                                text: "Refresh"
                                onClicked: root.spotix.refreshSession()
                            }

                            Button {
                                text: "Logout"
                                enabled: root.spotix.authenticated
                                onClicked: root.spotix.logout()
                            }
                        }
                    }
                }

                Rectangle {
                    color: "#121212"

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 24
                        spacing: 12

                        RowLayout {
                            Layout.fillWidth: true

                            Label {
                                Layout.fillWidth: true
                                text: "Library"
                                color: "#ffffff"
                                font.pixelSize: 34
                                font.bold: true
                            }

                            Button {
                                text: "Load Library"
                                onClicked: root.spotix.loadLibrary()
                            }
                        }

                        Label {
                            Layout.fillWidth: true
                            text: (root.spotix.profile_name.length > 0 ? root.spotix.profile_name + " · " : "") + root.spotix.library_status
                            color: "#b3b3b3"
                            wrapMode: Text.WordWrap
                        }

                        TabBar {
                            id: libraryTabs
                            Layout.fillWidth: true
                            TabButton { text: "Tracks" }
                            TabButton { text: "Playlists" }
                            TabButton { text: "Albums" }
                            TabButton { text: "Shows" }
                        }

                        StackLayout {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            currentIndex: libraryTabs.currentIndex

                            ListView {
                                clip: true
                                model: root.parseArray(root.spotix.saved_tracks_json)
                                delegate: ItemDelegate {
                                    width: ListView.view.width
                                    text: modelData.title + " · " + modelData.artist
                                    onClicked: root.spotix.playTrack(modelData.id)
                                }
                            }

                            ListView {
                                clip: true
                                model: root.parseArray(root.spotix.playlists_json)
                                delegate: ItemDelegate {
                                    width: ListView.view.width
                                    text: modelData.title + " · " + modelData.owner
                                }
                            }

                            ListView {
                                clip: true
                                model: root.parseArray(root.spotix.saved_albums_json)
                                delegate: ItemDelegate {
                                    width: ListView.view.width
                                    text: modelData.title + " · " + modelData.artist
                                }
                            }

                            ListView {
                                clip: true
                                model: root.parseArray(root.spotix.saved_shows_json)
                                delegate: ItemDelegate {
                                    width: ListView.view.width
                                    text: modelData.title + " · " + modelData.publisher
                                }
                            }
                        }
                    }
                }

                Rectangle {
                    color: "#121212"

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 24
                        spacing: 12

                        Label {
                            Layout.fillWidth: true
                            text: "Search"
                            color: "#ffffff"
                            font.pixelSize: 34
                            font.bold: true
                        }

                        Label {
                            Layout.fillWidth: true
                            text: root.spotix.search_status
                            color: "#b3b3b3"
                            wrapMode: Text.WordWrap
                        }

                        TabBar {
                            id: searchTabs
                            Layout.fillWidth: true
                            TabButton { text: "Tracks" }
                            TabButton { text: "Albums" }
                            TabButton { text: "Artists" }
                            TabButton { text: "Playlists" }
                            TabButton { text: "Podcasts" }
                        }

                        StackLayout {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            currentIndex: searchTabs.currentIndex

                            ListView {
                                clip: true
                                model: root.parseSearch(root.spotix.search_results_json).tracks
                                delegate: ItemDelegate {
                                    width: ListView.view.width
                                    text: modelData.title + " · " + modelData.artist + " · " + modelData.album
                                    onClicked: root.spotix.playTrack(modelData.id)
                                }
                            }

                            ListView {
                                clip: true
                                model: root.parseSearch(root.spotix.search_results_json).albums
                                delegate: ItemDelegate {
                                    width: ListView.view.width
                                    text: modelData.title + " · " + modelData.artist
                                }
                            }

                            ListView {
                                clip: true
                                model: root.parseSearch(root.spotix.search_results_json).artists
                                delegate: ItemDelegate {
                                    width: ListView.view.width
                                    text: modelData.name
                                }
                            }

                            ListView {
                                clip: true
                                model: root.parseSearch(root.spotix.search_results_json).playlists
                                delegate: ItemDelegate {
                                    width: ListView.view.width
                                    text: modelData.title + " · " + modelData.owner
                                }
                            }

                            ListView {
                                clip: true
                                model: root.parseSearch(root.spotix.search_results_json).shows
                                delegate: ItemDelegate {
                                    width: ListView.view.width
                                    text: modelData.title + " · " + modelData.publisher
                                }
                            }
                        }
                    }
                }

                Rectangle {
                    color: "#121212"

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 24
                        spacing: 12

                        Label {
                            text: "Playlists"
                            color: "#ffffff"
                            font.pixelSize: 34
                            font.bold: true
                        }

                        ListView {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            clip: true
                            model: root.parseArray(root.spotix.playlists_json)
                            delegate: ItemDelegate {
                                width: ListView.view.width
                                text: modelData.title + " · " + modelData.owner
                            }
                        }
                    }
                }

                Rectangle {
                    color: "#121212"

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 24
                        spacing: 12

                        Label {
                            text: "Albums"
                            color: "#ffffff"
                            font.pixelSize: 34
                            font.bold: true
                        }

                        ListView {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            clip: true
                            model: root.parseArray(root.spotix.saved_albums_json)
                            delegate: ItemDelegate {
                                width: ListView.view.width
                                text: modelData.title + " · " + modelData.artist
                            }
                        }
                    }
                }

                Rectangle {
                    color: "#121212"

                    ColumnLayout {
                        anchors.centerIn: parent
                        width: Math.min(parent.width - 96, 760)
                        spacing: 16

                        Label {
                            Layout.fillWidth: true
                            text: "Artists"
                            color: "#ffffff"
                            font.pixelSize: 34
                            font.bold: true
                        }

                        Label {
                            Layout.fillWidth: true
                            text: "Artist profile/detail routes are next; search results already list artists from Spotify."
                            color: "#b3b3b3"
                            wrapMode: Text.WordWrap
                        }
                    }
                }

                Rectangle {
                    color: "#121212"

                    ColumnLayout {
                        anchors.centerIn: parent
                        width: Math.min(parent.width - 96, 760)
                        spacing: 16

                        Label {
                            Layout.fillWidth: true
                            text: "Lyrics"
                            color: "#ffffff"
                            font.pixelSize: 34
                            font.bold: true
                        }

                        Label {
                            Layout.fillWidth: true
                            text: "Lyrics, artwork-derived colors, credits, and fullscreen artwork remain parity work after the data-loading path stabilizes."
                            color: "#b3b3b3"
                            wrapMode: Text.WordWrap
                        }
                    }
                }
            }

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 132
                color: "#181818"

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 16
                    spacing: 8

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 16

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 2

                            Label {
                                Layout.fillWidth: true
                                text: root.spotix.now_playing_title
                                color: "#ffffff"
                                font.pixelSize: 18
                                font.bold: true
                                elide: Text.ElideRight
                            }

                            Label {
                                Layout.fillWidth: true
                                text: root.spotix.now_playing_artist + (root.spotix.now_playing_album.length > 0 ? " - " + root.spotix.now_playing_album : "")
                                color: "#b3b3b3"
                                font.pixelSize: 13
                                elide: Text.ElideRight
                            }

                            Label {
                                Layout.fillWidth: true
                                text: root.spotix.playback_status + " · " + root.spotix.queue_summary
                                color: "#7f7f7f"
                                font.pixelSize: 12
                                elide: Text.ElideRight
                            }
                        }

                        Button {
                            text: "Previous"
                            onClicked: root.spotix.playPrevious()
                        }

                        Button {
                            text: root.spotix.playback_state === "Playing" ? "Pause" : "Play"
                            onClicked: root.spotix.playPause()
                        }

                        Button {
                            text: "Next"
                            onClicked: root.spotix.playNext()
                        }

                        Button {
                            text: "Stop"
                            onClicked: root.spotix.stopPlayback()
                        }

                        Slider {
                            Layout.preferredWidth: 140
                            from: 0
                            to: 1
                            value: root.spotix.volume
                            onMoved: root.spotix.setPlaybackVolume(value)
                        }
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 12

                        Label {
                            text: root.formatTime(root.spotix.playback_progress_ms)
                            color: "#b3b3b3"
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
                            color: "#b3b3b3"
                            font.pixelSize: 12
                        }
                    }
                }
            }
        }
    }
}
