import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import BiliMini

Window {
    id: win

    visible: true
    title: "bili-mini"
    color: "transparent"
    flags: Qt.Window | Qt.FramelessWindowHint
           | (win.mode === "mini" ? Qt.WindowStaysOnTopHint : 0)

    property int pad: 8
    property string mode: AppCore.initialMode === "mini" ? "mini" : "expanded"
    property string view: AppCore.initialView === "player" ? "player" : "browse"
    property string tab: "recommend"
    property string query: ""
    property int currentIndex: -1
    property bool playing: false
    property real position: 0
    property real duration: 0
    property real volume: 0.8
    property bool muted: false
    property bool typing: false
    property bool initialPlacementDone: false
    property bool videoActive: false
    property string playError: ""
    property bool loadingStream: false
    property bool buffering: false
    property bool bufferingSpinner: false
    property bool danmakuEnabled: true
    property var danmakuItems: []
    property var currentUser: null
    property var userVideos: []
    property bool userLoading: false
    property bool userLoadingMore: false
    property bool userEnd: false
    property int userPage: 1
    property string userError: ""
    property var comments: []
    property int commentCount: 0
    property bool commentsLoading: false
    property bool commentsMoreLoading: false
    property string commentsError: ""
    property string commentsOffset: ""
    property bool commentsEnd: false
    property string commentsSort: "hot"
    property int commentsToken: 0
    property rect videoRect: Qt.rect(0, 0, 0, 0)

    function setVideoRect(x, y, w, h) {
        videoRect = Qt.rect(x, y, w, h);
    }
    property bool loggedIn: false
    property string userName: ""
    property bool authBusy: false
    property string authError: ""

    // Per-mode window size, so resizing one mode leaves the other alone.
    // Persisted across restarts in M4.
    property size miniSize: Qt.size(380, 214)
    property size expandedSize: Qt.size(420, 660)
    property bool geometryLocked: false

    // Corner-resize session state.
    property bool resizing: false
    property int resizeSignX: 1
    property int resizeSignY: 1
    property point resizePressCursor
    property rect resizeStartRect

    // Data from bili-core.
    property var feedItems: []
    property bool feedLoading: false
    property bool feedLoadingMore: false
    property bool feedEnd: false
    property int feedFreshIdx: 1
    property string feedError: ""
    property var searchResults: []
    property string searchType: "video"
    property bool searchLoading: false
    property string searchError: ""

    // Playback context: the item, the list it came from, and its index there.
    property var currentItem: null
    property var currentList: []

    readonly property var relatedItems: {
        const items = feedItems;
        if (items.length === 0)
            return [];
        const out = [];
        for (let i = 0; i < items.length && out.length < 4; ++i) {
            if (!currentItem || items[i].bvid !== currentItem.bvid)
                out.push(items[i]);
        }
        return out;
    }


    function contentWidth() {
        return (mode === "mini" ? miniSize : expandedSize).width;
    }

    function contentHeight() {
        return (mode === "mini" ? miniSize : expandedSize).height;
    }

    function applyGeometry() {
        if (win.visibility === Window.FullScreen)
            return;
        win.minimumWidth = (mode === "mini" ? 280 : 380) + 2 * pad;
        win.minimumHeight = (mode === "mini" ? 160 : 520) + 2 * pad;
        geometryLocked = true;
        win.width = contentWidth() + 2 * pad;
        win.height = contentHeight() + 2 * pad;
        geometryLocked = false;
        placeWindow();
    }

    function saveModeSize() {
        const w = win.width - 2 * pad;
        const h = win.height - 2 * pad;
        if (mode === "mini")
            miniSize = Qt.size(w, h);
        else
            expandedSize = Qt.size(w, h);
    }

    onWidthChanged: {
        if (geometryLocked || win.visibility === Window.FullScreen)
            return;
        sizeSaveTimer.restart();
    }

    onHeightChanged: {
        if (geometryLocked || win.visibility === Window.FullScreen)
            return;
        sizeSaveTimer.restart();
    }

    Timer {
        id: sizeSaveTimer
        interval: 400
        onTriggered: win.saveModeSize()
    }

    function placeWindow() {
        if (AppCore.placeWindow(win, win.pad))
            initialPlacementDone = true;
    }

    function beginResize(xSign, ySign) {
        if (win.visibility === Window.FullScreen || resizing)
            return;
        // Hover-based cursors get lost as the window edge moves under the
        // pointer, so lock the shape for the duration of the drag.
        AppCore.pushCursor(xSign === ySign ? Qt.SizeFDiagCursor : Qt.SizeBDiagCursor);
        resizeSignX = xSign;
        resizeSignY = ySign;
        resizeStartRect = Qt.rect(win.x, win.y, win.width, win.height);
        resizePressCursor = AppCore.cursorPos();
        resizing = true;
    }

    function endResize() {
        if (!resizing)
            return;
        resizing = false;
        AppCore.popCursor();
        sizeSaveTimer.restart();
    }

    // Corner resize: the grabbed corner is pinned to the cursor, the opposite
    // corner stays put. The window is free-form; the picture inside keeps 16:9
    // and letterboxes itself, so it is never stretched.
    function resizeByCorner(xSign, ySign, startRect, cursor) {
        if (win.visibility === Window.FullScreen)
            return;

        const left = startRect.x;
        const top = startRect.y;
        const right = startRect.x + startRect.width;
        const bottom = startRect.y + startRect.height;

        const w = Math.max(win.minimumWidth,
                           Math.min(4000, Math.round(xSign < 0 ? right - cursor.x : cursor.x - left)));
        const h = Math.max(win.minimumHeight,
                           Math.min(4000, Math.round(ySign < 0 ? bottom - cursor.y : cursor.y - top)));

        geometryLocked = true;
        AppCore.setWindowGeometry(win,
                                  xSign < 0 ? Math.round(right - w) : Math.round(left),
                                  ySign < 0 ? Math.round(bottom - h) : Math.round(top),
                                  w, h);
        geometryLocked = false;
        sizeSaveTimer.restart();
    }

    // Resizing changes the local mouse coordinates, so onPositionChanged can
    // stall while the corner follows the cursor. Poll the global cursor.
    Timer {
        interval: 16
        repeat: true
        running: win.resizing
        onTriggered: {
            const cursor = AppCore.cursorPos();
            // Small dead zone so merely clicking a corner does not snap it.
            if (Math.abs(cursor.x - win.resizePressCursor.x)
                    + Math.abs(cursor.y - win.resizePressCursor.y) < 3)
                return;
            win.resizeByCorner(win.resizeSignX, win.resizeSignY, win.resizeStartRect, cursor);
        }
    }

    function setMode(m) {
        if (mode === m)
            return;
        mode = m; // onModeChanged re-applies geometry and placement
    }

    function openUser(user) {
        if (!user || !user.mid)
            return;
        currentUser = user;
        userVideos = [];
        userLoading = true;
        userLoadingMore = false;
        userEnd = false;
        userPage = 1;
        userError = "";
        view = "user";
        Api.request("/user/" + user.mid + "/videos?page=1", function (data) {
            win.userLoading = false;
            win.userVideos = data.items || [];
            win.userError = data.error || "";
            win.userEnd = win.userVideos.length === 0
                          || (data.count && win.userVideos.length >= data.count);
        }, function (error) {
            win.userLoading = false;
            win.userError = error;
        });
    }

    function loadMoreUserVideos() {
        if (!currentUser || userLoading || userLoadingMore || userEnd)
            return;
        const next = userPage + 1;
        userLoadingMore = true;
        Api.request("/user/" + currentUser.mid + "/videos?page=" + next, function (data) {
            if (data.error) {
                win.userLoadingMore = false;
                win.userError = data.error;
                return;
            }
            const items = data.items || [];
            const known = ({});
            for (let i = 0; i < win.userVideos.length; ++i)
                known[win.userVideos[i].bvid] = true;
            const merged = win.userVideos.slice();
            for (let i = 0; i < items.length; ++i) {
                if (!known[items[i].bvid]) {
                    known[items[i].bvid] = true;
                    merged.push(items[i]);
                }
            }
            win.userVideos = merged;
            win.userPage = next;
            if (items.length === 0 || (data.count && merged.length >= data.count))
                win.userEnd = true;
            win.userLoadingMore = false;
        }, function (error) {
            win.userLoadingMore = false;
            win.userError = error;
        });
    }

    function closeUser() {
        currentUser = null;
        view = "browse";
    }

    function loadComments(reset) {
        if (!currentItem)
            return;
        if (reset) {
            comments = [];
            commentCount = 0;
            commentsLoading = true;
            commentsMoreLoading = false;
            commentsError = "";
            commentsOffset = "";
            commentsEnd = false;
        } else {
            commentsMoreLoading = true;
        }

        const token = ++commentsToken;
        const bvid = currentItem.bvid;
        const offset = reset ? "" : commentsOffset;
        Api.request("/comments/" + bvid + "?offset=" + encodeURIComponent(offset) + "&sort=" + commentsSort, function (result) {
            if (token !== win.commentsToken)
                return;
            win.commentsLoading = false;
            win.commentsMoreLoading = false;
            const items = result.items || [];
            win.comments = reset ? items : win.comments.concat(items);
            win.commentCount = result.count || 0;
            win.commentsOffset = result.next || "";
            win.commentsEnd = result.is_end === true;
            if (!reset && items.length === 0)
                win.commentsEnd = true;
            win.commentsError = result.error || "";
        }, function (error) {
            if (token !== win.commentsToken)
                return;
            win.commentsLoading = false;
            win.commentsMoreLoading = false;
            win.commentsError = error;
        });
    }

    function loadMoreComments() {
        if (!currentItem || commentsLoading || commentsMoreLoading || commentsEnd)
            return;
        loadComments(false);
    }

    function setCommentSort(sort) {
        if (commentsSort === sort)
            return;
        commentsSort = sort;
        loadComments(true);
    }

    function setTab(t) {
        tab = t;
        view = "browse";
    }

    function expandBrowse() {
        tab = "recommend";
        view = "browse";
        setMode("expanded");
    }

    function selectAt(item, list, index) {
        if (!item)
            return;
        currentItem = item;
        currentList = (list && list.length) ? list : [item];
        currentIndex = (index === undefined || index < 0) ? currentList.indexOf(item) : index;
        duration = item.duration || 0;
        position = 0;
    }

    function playItem(item, list, index) {
        selectAt(item, list, index);
        playing = true;
        view = "player";
        // Browsing happens in the expanded page; playback shrinks to the
        // corner player (unless --stay-expanded).
        if (!AppCore.stayExpanded)
            setMode("mini");
        startPlayback(item);
        if (AppCore.browseAfterPlay)
            view = "browse";
    }

    function playByBvid(bvid) {
        Api.request("/video/" + bvid, function (data) {
            const video = data.video;
            if (!video) {
                console.warn("view failed:", data.error);
                return;
            }
            const item = {
                bvid: video.bvid,
                cid: video.cid,
                title: video.title,
                cover: video.pic,
                up: video.owner ? video.owner.name : "",
                duration: (video.pages && video.pages.length > 0) ? video.pages[0].duration : 0,
                views: video.stat ? video.stat.view : 0,
                pubdate: 0,
                desc: ""
            };
            win.playItem(item, [item], 0);
        }, function (error) {
            console.warn("view request failed:", error);
        });
    }

    function startPlayback(item) {
        if (!item || item.bvid === undefined)
            return;
        playError = "";
        loadingStream = true;
        danmakuItems = [];
        loadComments(true);
        const cid = item.cid || 0;
        Api.request("/play/" + item.bvid + "?cid=" + cid + "&qn=80", function (data) {
            if (data.error) {
                win.playError = data.error;
                win.loadingStream = false;
                console.warn("play failed:", data.error);
                return;
            }
            win.videoActive = true;
            mpv.play(data.video_url, data.audio_url);

            const danmakuCid = data.cid || cid;
            if (danmakuCid > 0) {
                Api.request("/danmaku/" + danmakuCid, function (result) {
                    win.danmakuItems = result.items || [];
                }, function (error) {
                    console.warn("danmaku failed:", error);
                });
            }
        }, function (error) {
            win.playError = error;
            win.loadingStream = false;
            console.warn("play request failed:", error);
        });
    }

    function playIndex(index) {
        if (index < 0 || index >= feedItems.length)
            return;
        playItem(feedItems[index], feedItems, index);
    }

    function togglePlay() {
        if (!currentItem) {
            const list = (tab === "search" && searchResults.length) ? searchResults : feedItems;
            if (list.length > 0)
                playItem(list[0], list, 0);
            return;
        }
        mpv.togglePause();
    }

    function nextVideo() {
        if (!currentList || currentList.length === 0)
            return;
        const next = (currentIndex + 1) % currentList.length;
        selectAt(currentList[next], currentList, next);
        playing = true;
        startPlayback(currentItem);
    }

    function prevVideo() {
        if (!currentList || currentList.length === 0)
            return;
        const prev = (currentIndex - 1 + currentList.length) % currentList.length;
        selectAt(currentList[prev], currentList, prev);
        playing = true;
        startPlayback(currentItem);
    }

    function refreshAuth() {
        if (AppCore.coreState !== "ready")
            return;
        Api.request("/auth/status", function (data) {
            win.loggedIn = data.logged_in === true;
            win.userName = data.uname || "";
        }, function (error) {
            console.warn("auth status:", error);
        });
    }

    function importCookies() {
        if (authBusy)
            return;
        authBusy = true;
        authError = "";
        Api.post("/auth/import", function (data) {
            win.authBusy = false;
            win.loggedIn = data.ok === true;
            win.userName = data.uname || "";
            if (data.ok) {
                // Re-fetch with the logged-in client so the feed matches.
                win.loadFeed();
            }
        }, function (error) {
            win.authBusy = false;
            win.authError = error;
            console.warn("cookie import:", error);
        });
    }

    function loadFeed() {
        if (AppCore.coreState !== "ready" || feedLoading)
            return;
        feedLoading = true;
        feedLoadingMore = false;
        feedEnd = false;
        feedFreshIdx = 1;
        feedError = "";
        Api.request("/feed?fresh_idx=1", function (data) {
            win.feedLoading = false;
            win.feedError = data.error || "";
            win.feedItems = data.items || [];
            win.feedEnd = win.feedItems.length === 0;
            if (AppCore.initialSearch !== "") {
                win.searchType = AppCore.initialSearchType !== "" ? AppCore.initialSearchType : "video";
                win.tab = "search";
                win.query = AppCore.initialSearch;
            }
            if (AppCore.initialPlayBvid !== "" && win.currentItem === null) {
                win.playByBvid(AppCore.initialPlayBvid);
            } else if (AppCore.initialAutoplay && win.currentItem === null && win.feedItems.length > 0) {
                win.playItem(win.feedItems[0], win.feedItems, 0);
            }
            if (win.currentItem === null && win.feedItems.length > 0) {
                let index = -1;
                if (AppCore.initialItem >= 0 && AppCore.initialItem < win.feedItems.length)
                    index = AppCore.initialItem;
                else if (AppCore.initialView === "player")
                    index = 0;
                if (index >= 0) {
                    win.selectAt(win.feedItems[index], win.feedItems, index);
                    win.playing = false;
                }
            }
        }, function (error) {
            win.feedLoading = false;
            win.feedError = error;
        });
    }

    // Infinite scroll: ask bili-core for the next fresh_idx batch and append
    // the unseen videos. The web UI does the same with its refresh counter.
    function loadMoreFeed() {
        if (AppCore.coreState !== "ready" || feedLoading || feedLoadingMore || feedEnd)
            return;
        if (tab !== "recommend")
            return;
        const next = feedFreshIdx + 1;
        feedLoadingMore = true;
        Api.request("/feed?fresh_idx=" + next, function (data) {
            if (data.error) {
                win.feedLoadingMore = false;
                win.feedError = data.error;
                return;
            }
            const items = data.items || [];
            const known = ({});
            for (let i = 0; i < win.feedItems.length; ++i)
                known[win.feedItems[i].bvid] = true;
            const merged = win.feedItems.slice();
            for (let i = 0; i < items.length; ++i) {
                if (!known[items[i].bvid]) {
                    known[items[i].bvid] = true;
                    merged.push(items[i]);
                }
            }
            win.feedItems = merged;
            win.feedFreshIdx = next;
            if (items.length === 0)
                win.feedEnd = true;
            win.feedLoadingMore = false;
        }, function (error) {
            win.feedLoadingMore = false;
            win.feedError = error;
        });
    }

    function runSearch() {
        const keyword = (query || "").trim();
        if (keyword === "") {
            searchResults = [];
            searchError = "";
            return;
        }
        searchResults = [];
        searchLoading = true;
        searchError = "";
        Api.request("/search?q=" + encodeURIComponent(keyword) + "&type=" + searchType, function (data) {
            win.searchLoading = false;
            win.searchError = data.error || "";
            win.searchResults = data.items || [];
        }, function (error) {
            win.searchLoading = false;
            win.searchError = error;
        });
    }

    Timer {
        id: searchDebounce
        interval: 350
        onTriggered: win.runSearch()
    }

    onQueryChanged: if (tab === "search") searchDebounce.restart()

    onSearchTypeChanged: {
        if (tab !== "search")
            return;
        searchResults = [];
        if ((query || "").trim() !== "")
            searchDebounce.restart();
    }

    function seekToRatio(ratio) {
        position = Math.max(0, Math.min(1, ratio)) * duration;
        mpv.seek(position);
    }

    function seekBy(delta) {
        position = Math.max(0, Math.min(duration, position + delta));
    }

    function toggleMute() {
        muted = !muted;
    }

    function toggleFullscreen() {
        if (win.visibility === Window.FullScreen) {
            win.visibility = Window.Windowed;
            win.pad = 8;
            applyGeometry();
        } else {
            win.pad = 0;
            win.visibility = Window.FullScreen;
        }
    }

    function handleEscape() {
        if (win.visibility === Window.FullScreen) {
            toggleFullscreen();
        } else if (mode === "expanded") {
            setMode("mini");
        }
    }

    onModeChanged: applyGeometry()

    // `screen` can still be null while the component completes; place the
    // window as soon as the platform tells us which output we are on.
    onScreenChanged: if (!initialPlacementDone) placeWindow()

    Shortcut {
        sequence: "Space"
        enabled: !win.typing
        onActivated: win.togglePlay()
    }
    Shortcut {
        sequence: "Escape"
        onActivated: win.handleEscape()
    }
    Shortcut {
        sequence: "F"
        enabled: !win.typing
        onActivated: win.toggleFullscreen()
    }
    Shortcut {
        sequence: "Left"
        enabled: !win.typing
        onActivated: win.seekBy(-5)
    }
    Shortcut {
        sequence: "Right"
        enabled: !win.typing
        onActivated: win.seekBy(5)
    }
    Shortcut {
        sequence: "Up"
        enabled: !win.typing
        onActivated: win.volume = Math.min(1, win.volume + 0.05)
    }
    Shortcut {
        sequence: "Down"
        enabled: !win.typing
        onActivated: win.volume = Math.max(0, win.volume - 0.05)
    }
    Shortcut {
        sequence: "M"
        enabled: !win.typing
        onActivated: win.toggleMute()
    }

    Connections {
        target: mpv
        function onPositionChanged() {
            win.position = mpv.position;
            if (mpv.position > 0.05)
                win.loadingStream = false;
        }
        function onDurationChanged() { if (mpv.duration > 0) win.duration = mpv.duration; }
        function onPlayingChanged() { win.playing = mpv.playing; }
        function onBufferingChanged() { win.buffering = mpv.buffering; }
        function onEnded() { win.nextVideo(); }
        function onErrorChanged() { if (mpv.error !== "") win.playError = mpv.error; }
    }

    // Only show the buffering spinner when the stall lasts a moment, so quick
    // seeks do not flash it.
    Timer {
        id: bufferingDelay
        interval: 250
        running: win.buffering && !win.bufferingSpinner
        onTriggered: win.bufferingSpinner = true
    }

    onBufferingChanged: {
        if (!win.buffering)
            win.bufferingSpinner = false;
    }

    // Never leave the spinner up forever if the stream stalls.
    Timer {
        interval: 12000
        running: win.loadingStream
        onTriggered: win.loadingStream = false
    }

    // The window manager may centre a freshly mapped frameless window right
    // after Component.onCompleted, so re-apply the corner placement while it
    // settles.
    Timer {
        interval: 100
        repeat: true
        running: win.visible && !win.initialPlacementDone
                 && win.visibility !== Window.FullScreen
        property int attempts: 0
        onTriggered: {
            win.placeWindow();
            if (++attempts >= 40)
                running = false;
        }
    }

    Connections {
        target: AppCore
        function onCoreChanged() {
            if (AppCore.coreState === "ready") {
                win.loadFeed();
                win.refreshAuth();
            }
        }
        function onShowRequested() {
            win.show();
            win.raise();
            win.requestActivate();
        }
    }

    Component.onCompleted: {
        applyGeometry();
        if (AppCore.coreState === "ready") {
            loadFeed();
            refreshAuth();
        }
    }

    Item {
        id: rootItem
        anchors.fill: parent

        Repeater {
            model: win.pad > 0 ? 5 : 0
            Rectangle {
                anchors.fill: frame
                anchors.margins: -(index + 1)
                radius: win.mode === "mini" ? 0 : Theme.radius + index + 1
                color: "transparent"
                border.width: 1
                border.color: Qt.rgba(0, 0, 0, 0.13 - index * 0.024)
            }
        }

        Rectangle {
            id: frame
            anchors.fill: parent
            anchors.margins: win.pad
            radius: win.mode === "mini" ? 0 : (win.pad > 0 ? Theme.radius : 0)
            color: Theme.bg
            clip: true

            MpvItem {
                id: mpv
                x: win.videoRect.x
                y: win.videoRect.y
                width: win.videoRect.width
                height: win.videoRect.height
                visible: win.videoActive && win.videoRect.width > 1
                         && (win.mode === "mini" || win.view === "player")
                volume: win.muted ? 0 : win.volume
            }

            Loader {
                anchors.fill: parent
                sourceComponent: win.mode === "mini" ? miniComponent : expandedComponent
            }

            // Drawn above the content so the 1px outline stays visible.
            Rectangle {
                anchors.fill: parent
                radius: frame.radius
                color: "transparent"
                border.width: 1
                border.color: Theme.border
                visible: win.visibility !== Window.FullScreen
            }

            ResizeHandles {
                anchors.fill: parent
                host: win
                // The mini player runs its own corner logic on its surface.
                visible: win.mode === "expanded" && win.visibility !== Window.FullScreen
            }
        }
    }

    Component {
        id: miniComponent
        MiniView {
            host: win
        }
    }

    Component {
        id: expandedComponent
        ExpandedView {
            host: win
        }
    }
}
