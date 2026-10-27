import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root

    property var host: null

    Rectangle {
        id: header
        anchors {
            left: parent.left
            right: parent.right
            top: parent.top
        }
        height: 46
        radius: Theme.radius
        color: Theme.bg

        Rectangle {
            anchors {
                left: parent.left
                right: parent.right
                bottom: parent.bottom
            }
            height: 1
            color: Theme.border
        }

        RowLayout {
            anchors {
                fill: parent
                leftMargin: 12
                rightMargin: 6
            }
            spacing: 8

            Rectangle {
                width: 20
                height: 20
                radius: 5
                color: Theme.accent

                Text {
                    anchors.centerIn: parent
                    text: "B"
                    color: "white"
                    font.pixelSize: 12
                    font.bold: true
                }
            }

            Text {
                text: "bili-mini"
                color: Theme.textMuted
                font.pixelSize: 12
            }

            Rectangle {
                width: 6
                height: 6
                radius: 3
                color: AppCore.coreState === "ready" ? Theme.ok
                       : AppCore.coreState === "starting" ? Theme.warn : Theme.bad

                HoverHandler { id: coreHover }

                ToolTip.visible: coreHover.hovered
                ToolTip.text: "bili-core: " + AppCore.coreState
            }

            Item { Layout.fillWidth: true }

            Rectangle {
                id: authButton
                Layout.alignment: Qt.AlignVCenter
                implicitWidth: authRow.implicitWidth + 16
                implicitHeight: 22
                radius: 11
                color: authHover.hovered ? Theme.cardHover : "transparent"
                border.width: 1
                border.color: root.host && root.host.loggedIn ? Theme.ok : Theme.border

                Row {
                    id: authRow
                    anchors.centerIn: parent
                    spacing: 4

                    Image {
                        anchors.verticalCenter: parent.verticalCenter
                        width: 12
                        height: 12
                        source: "icons/user.svg"
                        opacity: root.host && root.host.loggedIn ? 1 : 0.7
                    }

                    Text {
                        text: root.host && root.host.authBusy ? "导入中…"
                              : root.host && root.host.loggedIn ? (root.host.userName || "已登录")
                              : "登录"
                        color: root.host && root.host.loggedIn ? Theme.ok : Theme.textMuted
                        font.pixelSize: 11
                        width: Math.min(implicitWidth, 56)
                        elide: Text.ElideRight
                    }
                }

                HoverHandler { id: authHover }

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: if (root.host) root.host.importCookies()
                }

                ToolTip.visible: authHover.hovered && root.host && root.host.authError !== ""
                ToolTip.text: root.host ? root.host.authError : ""
            }

            IconButton {
                icon: "icons/chevron-down.svg"
                tip: "收起 (Esc)"
                onClicked: root.host.setMode("mini")
            }

            IconButton {
                icon: "icons/close.svg"
                tip: "退出"
                onClicked: AppCore.quit()
            }
        }

        // Drag handle: the title area between the wordmark and the buttons.
        // startSystemMove() has to be called while the press is being handled,
        // not after a drag threshold, otherwise the compositor ignores it.
        Item {
            anchors {
                left: parent.left
                leftMargin: 34
                right: parent.right
                rightMargin: 130
                top: parent.top
                bottom: parent.bottom
            }

            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.OpenHandCursor
                onPressed: {
                    if (root.host && !root.host.resizing)
                        AppCore.startSystemMove(root.host);
                }
            }
        }
    }

    Rectangle {
        id: tabs
        anchors {
            left: parent.left
            right: parent.right
            top: header.bottom
        }
        height: 40
        color: Theme.bg

        RowLayout {
            anchors {
                fill: parent
                leftMargin: 12
                rightMargin: 12
            }
            spacing: 6

            PillTab {
                label: "推荐"
                active: root.host.tab === "recommend"
                onClicked: root.host.setTab("recommend")
            }

            PillTab {
                label: "搜索"
                active: root.host.tab === "search"
                onClicked: root.host.setTab("search")
            }

            PillTab {
                label: "历史"
                active: root.host.tab === "history"
                onClicked: root.host.setTab("history")
            }

            Item { Layout.fillWidth: true }

            Text {
                text: root.host && root.host.feedLoading ? "刷新中…" : "刷新"
                color: root.host && root.host.feedLoading ? Theme.textFaint : Theme.accent
                font.pixelSize: 11
                visible: root.host && root.host.tab === "recommend"
                         && root.host.view === "browse"

                HoverHandler { cursorShape: Qt.PointingHandCursor }
                TapHandler {
                    onTapped: {
                        if (root.host && !root.host.feedLoading)
                            root.host.loadFeed();
                    }
                }
            }
        }
    }

    Rectangle {
        id: nowPlaying
        visible: root.host && root.host.currentItem !== null && root.host.view !== "player"
        height: visible ? 30 : 0
        anchors {
            left: parent.left
            right: parent.right
            top: tabs.bottom
            leftMargin: 12
            rightMargin: 12
            topMargin: visible ? 8 : 0
        }
        radius: 8
        color: nowHover.hovered ? Theme.cardHover : Theme.card

        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 10
            anchors.rightMargin: 10
            spacing: 6

            Image {
                source: "icons/play.svg"
                sourceSize: Qt.size(10, 10)
                width: 10
                height: 10
                opacity: 0.8
            }

            Text {
                Layout.fillWidth: true
                text: root.host && root.host.currentItem ? root.host.currentItem.title : ""
                color: Theme.textMuted
                font.pixelSize: 11
                elide: Text.ElideRight
                maximumLineCount: 1
            }

            Text {
                text: "返回播放"
                color: Theme.accent
                font.pixelSize: 11
            }
        }

        HoverHandler { id: nowHover }

        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: if (root.host) root.host.view = "player"
        }
    }

    Loader {
        anchors {
            left: parent.left
            right: parent.right
            top: nowPlaying.visible ? nowPlaying.bottom : tabs.bottom
            bottom: parent.bottom
            leftMargin: 12
            rightMargin: 12
            topMargin: 8
            bottomMargin: 12
        }
        sourceComponent: root.host.view === "player" ? playerPage
                         : root.host.view === "user" ? userPage
                         : browsePage
    }

    Component {
        id: playerPage
        PlayerView { host: root.host }
    }

    Component {
        id: userPage
        UserView { host: root.host }
    }

    Component {
        id: browsePage
        BrowseView { host: root.host }
    }
}
