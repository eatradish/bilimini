import QtQuick
import QtQuick.Controls

Item {
    id: root

    property var host: null
    property var filtered: []

    readonly property bool loading: host !== null && (host.feedLoading || host.searchLoading)
    readonly property string errorText: host === null ? ""
                                        : (host.tab === "search" ? host.searchError : host.feedError)
    readonly property bool isUser: host !== null && host.tab === "search" && host.searchType === "user"

    function refilter() {
        if (!host)
            return;
        if (host.tab === "search") {
            filtered = host.searchResults || [];
        } else if (host.tab === "history") {
            filtered = [];
        } else {
            filtered = host.feedItems || [];
        }
    }

    Connections {
        target: root.host
        function onTabChanged() { root.refilter() }
        function onQueryChanged() { root.refilter() }
        function onFeedItemsChanged() { root.refilter() }
        function onSearchResultsChanged() { root.refilter() }
    }

    Component.onCompleted: refilter()

    Column {
        id: header
        visible: root.host && root.host.tab === "search"
        height: visible ? implicitHeight : 0
        anchors {
            left: parent.left
            right: parent.right
            top: parent.top
        }
        spacing: 8

        Item {
            width: parent.width
            height: 32

            BiliSearchField {
                anchors.fill: parent
                placeholderText: "搜索视频、UP 主"
                text: root.host ? root.host.query : ""
                onSearchEdited: function (text) {
                    if (root.host && root.host.query !== text)
                        root.host.query = text;
                }
                onActiveFocusChanged: if (root.host) root.host.typing = activeFocus
            }
        }

        Row {
            width: parent.width
            height: 26
            spacing: 6

            Repeater {
                model: [{ key: "video", label: "视频" }, { key: "user", label: "UP主" }]

                delegate: Rectangle {
                    readonly property bool selected: root.host !== null && root.host.searchType === modelData.key
                    width: 54
                    height: 26
                    radius: 13
                    color: selected ? Theme.accentSoft : "transparent"
                    border.width: 1
                    border.color: selected ? Theme.accent : Theme.border

                    Text {
                        anchors.centerIn: parent
                        text: modelData.label
                        font.pixelSize: 11
                        color: selected ? Theme.accent : Theme.textMuted
                    }

                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: if (root.host) root.host.searchType = modelData.key
                    }
                }
            }
        }
    }

    ListView {
        id: list
        anchors {
            left: parent.left
            right: parent.right
            top: header.bottom
            bottom: parent.bottom
            topMargin: header.visible ? 10 : 0
        }
        clip: true
        spacing: 4
        model: root.filtered
        boundsBehavior: Flickable.StopAtBounds

        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

        // Infinite scroll on the recommendation tab.
        onContentYChanged: {
            if (root.host && root.host.tab === "recommend"
                    && contentY + height >= contentHeight - 240)
                root.host.loadMoreFeed();
        }
        onAtYEndChanged: {
            if (atYEnd && root.host && root.host.tab === "recommend")
                root.host.loadMoreFeed();
        }

        footer: Item {
            width: list.width
            height: root.host && root.host.tab === "recommend" ? 44 : 0

            Row {
                anchors.centerIn: parent
                spacing: 6
                visible: root.host && (root.host.feedLoadingMore || root.host.feedEnd)

                Spinner {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 14
                    height: 14
                    visible: root.host && root.host.feedLoadingMore
                    color: Theme.textMuted
                }

                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.host && root.host.feedEnd ? "没有更多了" : "加载中…"
                    color: Theme.textMuted
                    font.pixelSize: 11
                }
            }
        }

        delegate: Item {
            width: ListView.view.width
            height: root.isUser ? userRow.height : videoRow.height

            VideoRow {
                id: videoRow
                width: parent.width
                visible: !root.isUser
                item: modelData
                onClicked: root.host.playItem(modelData, root.filtered, index)
            }

            UserRow {
                id: userRow
                width: parent.width
                visible: root.isUser
                user: modelData
                onClicked: root.host.openUser(modelData)
            }
        }
    }

    Column {
        visible: root.filtered.length === 0
        anchors.centerIn: list
        spacing: 8

        Image {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: !root.loading && root.errorText === ""
            source: root.host && root.host.tab === "search" ? "icons/search.svg" : "icons/history.svg"
            sourceSize: Qt.size(24, 24)
            width: 24
            height: 24
            opacity: 0.3
        }

        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            width: Math.min(implicitWidth, list.width - 24)
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            text: {
                if (root.loading)
                    return "加载中…";
                if (root.errorText !== "")
                    return "加载失败：" + root.errorText;
                if (!root.host)
                    return "";
                if (root.host.tab === "search")
                    return root.host.query.trim() === "" ? "输入关键词，搜索 B 站"
                           : (root.isUser ? "没有找到相关 UP 主" : "没有找到相关视频");
                if (root.host.tab === "history")
                    return "还没有观看记录";
                return "暂时没有内容";
            }
            color: root.errorText !== "" ? Theme.bad : Theme.textFaint
            font.pixelSize: 12
        }
    }
}
