import QtQuick
import QtQuick.Layouts

Item {
    id: root

    property var host: null
    readonly property var item: host ? host.currentItem : null
    readonly property bool hasItem: item !== null

    readonly property int gripSize: 18

    // Controls fade out while playing and come back on mouse activity.
    property bool controlsVisible: true
    property bool controlsHovered: false
    readonly property bool controlsAutoHide: hasItem && host && host.playing

    function pokeControls() {
        root.controlsVisible = true;
        if (root.controlsAutoHide)
            hideTimer.restart();
        else
            hideTimer.stop();
    }

    Timer {
        id: hideTimer
        interval: 2400
        repeat: false
        onTriggered: {
            if (root.controlsHovered) {
                hideTimer.restart();
                return;
            }
            if (root.controlsAutoHide)
                root.controlsVisible = false;
        }
    }

    Connections {
        target: root.host
        function onPlayingChanged() { root.pokeControls(); }
    }

    // The mini view is usually created while playback already runs, so seed the
    // timer from onCompleted / autoHide changes instead of playingChanged.
    onControlsAutoHideChanged: pokeControls()

    function cornerAt(x, y) {
        const left = x <= gripSize;
        const right = x >= width - gripSize;
        const top = y <= gripSize;
        const bottom = y >= height - gripSize;
        if (left && top)
            return 1;
        if (right && top)
            return 2;
        if (left && bottom)
            return 3;
        if (right && bottom)
            return 4;
        return 0;
    }

    function signX(corner) {
        return (corner === 2 || corner === 4) ? 1 : -1;
    }

    function signY(corner) {
        return (corner === 3 || corner === 4) ? 1 : -1;
    }

    function cursorFor(corner) {
        if (corner === 1 || corner === 4)
            return Qt.SizeFDiagCursor;
        if (corner === 2 || corner === 3)
            return Qt.SizeBDiagCursor;
        return Qt.ArrowCursor;
    }

    function reportVideoRect() {
        if (root.host && root.host.mode === "mini")
            root.host.setVideoRect(0, 0, width, height);
    }

    Component.onCompleted: {
        reportVideoRect();
        pokeControls();
    }
    onWidthChanged: reportVideoRect()
    onHeightChanged: reportVideoRect()

    Rectangle {
        anchors.fill: parent
        color: "#0A0B0D"
        visible: !root.host || !root.host.videoActive
    }

    // The picture keeps 16:9; extra window space is letterboxed, never stretched.
    Item {
        id: picture
        visible: !root.host || !root.host.videoActive
        anchors.centerIn: parent
        width: Math.min(parent.width, Math.round(parent.height * 16 / 9))
        height: Math.round(width * 9 / 16)

        MockCover {
            anchors.fill: parent
            radius: Theme.radius
            seed: Theme.seedFor(root.item)
            showGlyph: false
            visible: coverImage.status !== Image.Ready
        }

        Image {
            id: coverImage
            anchors.fill: parent
            source: root.item && root.item.cover ? root.item.cover : ""
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            cache: true
        }
    }

    Item {
        anchors.centerIn: parent
        width: 58
        height: 58
        visible: root.host && (root.host.loadingStream || root.host.bufferingSpinner)
        z: 5

        Rectangle {
            anchors.fill: parent
            radius: width / 2
            color: "#A6000000"
            border.width: 1
            border.color: "#33FFFFFF"
        }

        Spinner {
            anchors.centerIn: parent
            width: 26
            height: 26
            color: "white"
        }
    }

    DanmakuLayer {
        anchors.fill: parent
        host: root.host
    }

    // One surface handles moving, corner-resizing and double-click-to-expand.
    // A single MouseArea avoids Qt pointer handlers and MouseAreas fighting
    // over the grab (which made corner drags turn into window moves).
    MouseArea {
        id: surface
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: root.cursorFor(root.cornerAt(mouseX, mouseY))

        property int corner: 0
        property bool moving: false
        property point pressPoint

        onPressed: function (mouse) {
            corner = root.cornerAt(mouse.x, mouse.y);
            pressPoint = Qt.point(mouse.x, mouse.y);
            if (corner !== 0)
                root.host.beginResize(root.signX(corner), root.signY(corner));
        }

        onEntered: root.pokeControls()

        onPositionChanged: function (mouse) {
            root.pokeControls();
            if (!pressed || corner !== 0)
                return;
            if (!moving && (Math.abs(mouse.x - pressPoint.x) + Math.abs(mouse.y - pressPoint.y) > 6)) {
                moving = true;
                AppCore.startSystemMove(root.host);
            }
        }

        onReleased: {
            if (corner !== 0)
                root.host.endResize();
            corner = 0;
            moving = false;
        }

        onCanceled: {
            if (corner !== 0)
                root.host.endResize();
            corner = 0;
            moving = false;
        }

        onDoubleClicked: root.host.setMode("expanded")

        WheelHandler {
            onWheel: function (event) {
                root.host.volume = Math.max(0, Math.min(1, root.host.volume + event.angleDelta.y / 1200));
                if (root.host.volume > 0)
                    root.host.muted = false;
            }
        }
    }

    Item {
        anchors.fill: parent
        visible: !root.hasItem

        Rectangle {
            anchors.fill: parent
            color: "#A6000000"
        }

        Column {
            anchors.centerIn: parent
            spacing: 10

            Rectangle {
                width: 54
                height: 54
                radius: 27
                color: Theme.accent
                anchors.horizontalCenter: parent.horizontalCenter

                Image {
                    anchors.centerIn: parent
                    source: "icons/play.svg"
                    sourceSize: Qt.size(20, 20)
                    width: 20
                    height: 20
                }

                HoverHandler { cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: root.host.expandBrowse() }
            }

            Text {
                text: "开始刷 B 站"
                color: "white"
                font.pixelSize: 12
                anchors.horizontalCenter: parent.horizontalCenter
            }

            Text {
                text: "双击打开推荐首页"
                color: "#99FFFFFF"
                font.pixelSize: 10
                anchors.horizontalCenter: parent.horizontalCenter
            }
        }
    }

    Rectangle {
        visible: root.hasItem && root.host && !root.host.playing
        anchors.centerIn: parent
        width: 54
        height: 54
        radius: 27
        color: "#8C000000"
        border.width: 1
        border.color: "#33FFFFFF"

        Image {
            anchors.centerIn: parent
            source: "icons/play.svg"
            sourceSize: Qt.size(20, 20)
            width: 20
            height: 20
        }

        HoverHandler { cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: root.host.togglePlay() }
    }

    Rectangle {
        id: miniGradient
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        height: 92
        radius: Theme.radius
        opacity: root.controlsVisible ? 1 : 0
        visible: opacity > 0
        enabled: root.controlsVisible
        Behavior on opacity { NumberAnimation { duration: 150 } }

        gradient: Gradient {
            GradientStop { position: 0.0; color: "#00000000" }
            GradientStop { position: 1.0; color: "#D9000000" }
        }

        HoverHandler {
            onHoveredChanged: {
                root.controlsHovered = hovered;
                if (hovered)
                    root.pokeControls();
            }
        }
    }

    RowLayout {
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
            leftMargin: 10
            rightMargin: 8
            bottomMargin: 22
        }
        spacing: 4
        opacity: root.controlsVisible ? 1 : 0
        visible: opacity > 0
        enabled: root.controlsVisible
        Behavior on opacity { NumberAnimation { duration: 150 } }

        IconButton {
            icon: root.host && root.host.playing ? "icons/pause.svg" : "icons/play.svg"
            tip: root.host && root.host.playing ? "暂停 (Space)" : "播放 (Space)"
            size: 26
            iconSize: 13
            onClicked: root.host.togglePlay()
        }

        Text {
            Layout.fillWidth: true
            text: root.item ? root.item.title : "未选择视频"
            color: "#F2FFFFFF"
            font.pixelSize: 12
            elide: Text.ElideRight
            maximumLineCount: 1
        }

        Text {
            text: root.hasItem ? Theme.fmtTime(root.host.position) : ""
            color: "#B3FFFFFF"
            font.pixelSize: 10
        }

        IconButton {
            icon: root.host && root.host.muted ? "icons/mute.svg" : "icons/volume.svg"
            tip: root.host && root.host.muted ? "取消静音 (M)" : "静音 (M)"
            size: 26
            iconSize: 13
            onClicked: if (root.host) root.host.toggleMute()
        }

        IconButton {
            icon: "icons/danmaku.svg"
            tip: root.host && root.host.danmakuEnabled ? "关闭弹幕" : "显示弹幕"
            size: 26
            iconSize: 14
            opacity: root.host && root.host.danmakuEnabled ? 1.0 : 0.4
            onClicked: if (root.host) root.host.danmakuEnabled = !root.host.danmakuEnabled
        }

        IconButton {
            icon: "icons/chevron-up.svg"
            tip: "展开"
            size: 26
            iconSize: 14
            onClicked: root.host.setMode("expanded")
        }

        IconButton {
            icon: "icons/close.svg"
            tip: "退出"
            size: 26
            iconSize: 13
            onClicked: AppCore.quit()
        }
    }

    // Interactive progress: a thin line, but with a 12px tall grab area.
    Item {
        id: miniSeek
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
            leftMargin: 10
            rightMargin: 10
            bottomMargin: 6
        }
        height: 12
        opacity: root.controlsVisible ? 1 : 0
        visible: opacity > 0
        enabled: root.controlsVisible
        Behavior on opacity { NumberAnimation { duration: 150 } }

        Rectangle {
            id: miniTrack
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width
            height: (miniSeekArea.containsMouse || miniSeekArea.pressed) ? 4 : 3
            radius: height / 2
            color: "#26FFFFFF"

            Rectangle {
                width: parent.width * (root.host && root.host.duration > 0
                                       ? Math.max(0, Math.min(1, root.host.position / root.host.duration)) : 0)
                height: parent.height
                radius: parent.radius
                color: Theme.accent
            }
        }

        HoverHandler { cursorShape: Qt.PointingHandCursor }

        MouseArea {
            id: miniSeekArea
            anchors.fill: parent
            onPressed: function (mouse) {
                if (root.host)
                    root.host.seekToRatio(Math.max(0, Math.min(1, mouse.x / width)));
            }
            onPositionChanged: function (mouse) {
                if (pressed && root.host)
                    root.host.seekToRatio(Math.max(0, Math.min(1, mouse.x / width)));
            }
        }
    }
}
