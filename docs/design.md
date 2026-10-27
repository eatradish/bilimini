# bili-mini 设计记录

## 1. 为什么不是一个纯 Plasma widget

面板里的 widget 只能待在面板那一小块；它的 popup 是弹窗语义（失焦关闭、不参与任务栏、
全屏/置顶行为受限）；放桌面上的 widget 又会被其他窗口盖住。WMP 迷你模式本质是一个
**无边框、置顶、可拖动的窗口**。

所以最终形态是：

- `bili-mini`：窗口本体，负责浏览 + 播放
- `plasmoid`（M5）：面板上的遥控器，通过 MPRIS 读播放状态、点一下唤起窗口

## 2. 进程架构

```
┌──────────────────────────┐   HTTP/JSON  127.0.0.1:<随机端口>
│ app: bili-mini (Qt6/QML) │ ──────────────────────────────┐
│  · 迷你 / 展开 两态窗口   │                               ▼
│  · MpvObject → libmpv    │                     ┌────────────────────┐
│  · MPRIS (M4)            │                     │ core: bili-core    │
└───────────┬──────────────┘                     │ (Rust, tokio+axum) │
            │ MPRIS                              │ · bpi-rs 封装       │
            ▼                                    │ · 推荐/搜索/取流     │
┌──────────────────────────┐   qdbus 唤起窗口    │ · cookie / 弹幕     │
│ plasmoid (纯 QML，M5)     │ ──────────────────▶ └────────────────────┘
└──────────────────────────┘
```

**握手协议**：主程序 `QProcess` 启动 `bili-core`，读它 stdout 的第一行 JSON：

```json
{"port": 38769, "token": "…", "pid": 1234}
```

之后所有请求带 `Authorization: Bearer <token>`。sidecar 只监听 `127.0.0.1`，
退出时由主程序 `terminate()` 收掉。

**为什么 sidecar 而不是 cxx-qt 编进进程**：

1. tokio 的 async 代码不用和 Qt 事件循环互相将就；
2. 接口变了可以单独重启、单独 `curl` 调、单独 `cargo test`；
3. 以后 plasmoid 或别的前端可以直接复用同一个后端。

代价是多一个进程，但由主程序全生命周期管理，用户无感。

## 3. 窗口与交互

- 迷你态 `380×214`，展开态 `420×660`（窄卡片列表），外圈留 8px 透明边画阴影
  （窗口实际尺寸 +16）
- 展开态布局：44px 标题栏（logo / 名字 / core 状态点 / 收起 / 退出）→ 40px 胶囊
  tab（推荐 / 搜索 / 历史）→ 卡片列表；播放页则是视频、控制条、信息、相关推荐
  竖排，可滚动
- 启动就是展开态；点卡片播放时自动收成迷你窗（`playIndex()` 里 `setMode("mini")`），
  展开按钮再回到展开页
- 贴右下角：`QScreen::availableGeometry()`（已扣除面板），视觉边距 16px
- 迷你态 `Qt.WindowStaysOnTopHint`；展开态取消置顶
- 展开以右下角为锚点向上长，收起同理
- 拖动：`QWindow::startSystemMove()`（X11 下由 KWin 接管，比手动搬窗口顺滑）
- 缩放：只在四个角落放手柄，四条边保持可拖动窗口
  - 迷你态：整个画面是**一个** MouseArea，按下时判断落点——角落进入缩放，
    其他地方移动超过 6px 才开始拖窗口；单一 handler 不会和别的指针处理抢手势
  - 展开态：角落同样在 QML 里算（`resizeByCorner()`）；标题区只留 logo/标题
    那一小块作为拖动区域，避免盖住顶部两个角
  - 没有用 `QWindow::startSystemResize()`：这个环境里窗口管理器会把角落的
    缩放请求当成移动，反而不可控
  - 缩放期间用 16ms 定时器直接读全局光标位置（`QCursor::pos`），不依赖
    `onPositionChanged`——窗口跟着变大时，鼠标在窗口内的相对坐标几乎不变，
    移动事件会断断续续
  - 抓住的角跟光标（按主导方向），对角固定；按下后有 3px 死区，避免"点一下角"
    就跳尺寸；缩放期间用 override cursor 锁住斜向光标
  - 缩放时用一次 `setGeometry()` 同时提交位置和尺寸（不是分开四次），
    拖动更跟手
- **宽高比**：
  - 迷你窗口可以自由缩放，画面固定 16:9 居中，多余空间是黑边；
  - 展开态窗口可以拉成任意形状，但播放区是一个恒 16:9 的盒子，按
    `min(可用宽度, 可用高度 × 16/9)` 取尺寸；
  - 真正播放时 libmpv 默认 `keepaspect`，即使源是 4:3 或竖屏，也只是加黑边，
    不会拉伸。

## 4. 数据层（M2，已接入）

`bpi-rs` 已经覆盖了需要的接口：

| 用途 | bpi-rs |
| --- | --- |
| 首页推荐 | `video.homepage_recommendations`（`x/web-interface/wbi/index/top/feed/rcmd`） |
| 取流 | `video.play_url`（`x/player/wbi/playurl`，wbi 签名已内置） |
| 视频详情 | `video.view` |
| 搜索 | `search` 模块 |
| 弹幕 | `danmaku` 模块 |
| 稍后再看 / 收藏 | `historytoview` / `fav` |

参考实现 [bilibili-tui](https://github.com/MareDevi/bilibili-tui)（MIT），它已经把
wbi 签名、推荐流、搜索、二维码登录跑通了，参数和错误码可以直接照抄。

**取流的坑**：

- B 站是 DASH，音视频分离。libmpv 侧用
  `loadfile <video_url> replace 0 "audio-file=<audio_url>"`，或者 `audio-add`；
- 媒体 URL 必须带 `Referer: https://www.bilibili.com`（否则 CDN 403）和桌面版 UA，
  统一通过 `http-header-fields` 设置；
- 位置跳转要开 `hr-seek`，否则 DASH 下跳转后短暂无声。

**画质**：未登录 720p，登录后 1080p，大会员 4K。`qn` 从 url 的 `dash` 列表里选。

**Cookie**：先做 Firefox 导入（`cookies.sqlite` 在 Linux 上是明文）+ 手动粘贴 SESSDATA；
Chromium 系的加密 cookie 放后面。存 `~/.local/share/bili-mini/auth.json`，权限 0600。

**M2 落地方式**：`bili-core` 直接用 `bpi-rs` 0.3 的 `video().homepage_recommendations()`、
`search().video()`、`video().view()`；错误统一用 `anyhow`，handler 里 `{err:#}` 带上
上下文返回给 QML。QML 侧通过 `Api.qml`（XMLHttpRequest + Bearer token）请求本地服务，
`Main.qml` 持有 `feedItems/searchResults/currentItem`，搜索输入防抖 350ms。

封面 URL 统一补成 https（搜索结果是 `//iX.hdslb.com/...`，推荐流是 `http://`），
现在直接交给 QML `Image` 加载，没有走代理。

**M3 播放**：`/play/{bvid}?cid=&qn=` 返回 DASH 的 `video_url` / `audio_url` + Referer；
Qt 侧 `MpvItem`（`QQuickFramebufferObject` + `mpv_render_context`）把画面画进 QML，
`loadfile <video> replace 0 "audio-file=<audio>"` 把两路合起来。两个坑：

- Qt 会把 `LC_NUMERIC` 切到系统 locale，libmpv 解析失败导致 `mpv_create()` 返回空 →
  `main()` 里 `setlocale(LC_NUMERIC, "C")`。
- 开 `MPV_RENDER_PARAM_ADVANCED_CONTROL` 但没实现 `report_swap()`，mpv 以为帧没被消费、
  播放时钟不动 → 先关掉 advanced control；`FLIP_Y` 用 0（QQuickFBO 会再翻一次）。

**搜索类型**：`/search?q=&type=video|user`，user 走 bpi-rs 的 `bili_user()`；搜索结果的
`cid` 是 0（搜索接口不返回），`/play` 在 cid 为 0 时先调 `view()` 补出 cid 再取流。

**UP主页**：`/user/{mid}/videos` 走 bpi-rs `user().uploaded_videos()`（wbi 签名，匿名可取第一页 30 条，
被风控时返回 `is_risk`）；QML 用 `view = "user"` + `UserView`，返回按钮把 `view` 切回 `browse`，
搜索状态保留。

**评论**：和网页同一条接口 `/x/v2/reply/wbi/main`（旧 `/x/v2/reply?sort=2` 返回的集合不一样，
会少很多热评，也没有置顶）。bpi-rs 没暴露这个端点，所以自己做了 WBI 签名：
拉 `nav` 取 `wbi_img` 的 img/sub key → 64 位 mixin 表拼出 32 位 key → 参数排序/URL 编码后
md5(query + key) 得到 `w_rid`。`mode=3` 热度 / `mode=2` 时间，`pagination_str={"offset":...}`
游标翻页，返回 `next_offset` / `is_end`；第一页带上 `top_replies`（置顶）并去重。

QML 在 `PlayerView` 里用 `CommentRow` 渲染，「查看更多」用游标拉下一页并追加；
`commentsToken` 防止切视频时旧响应写进新列表。

**弹幕**：`/danmaku/{cid}` 拉 `comment.bilibili.com/{cid}.xml`（响应是 **raw deflate**，
要自己 inflate；`reqwest` 的自动解压会按 zlib 报错，所以 `.no_deflate()` + `DeflateDecoder`），
`quick-xml` 解析 `<d p="...">`，**按 time 排序**后返回（XML 原始顺序是按 dmid，不是时间）。
QML `DanmakuLayer` 用 64 个 Text 池，由播放时钟驱动，按 `(position - startTime) * speed`
算 x，9 条泳道按 `lanesFree` 选空闲道；弹幕开关在迷你栏。

**托盘与全局快捷键**：`QSystemTrayIcon` 属于 QtWidgets，所以主程序从 `QGuiApplication` 换成了
`QApplication`（Qt Quick 不受影响）。左键 `Trigger` 切换窗口显隐，右键菜单里「显示/隐藏窗口」
的文字随 `visibleChanged` 更新；全局快捷键用 KF6 的 `KGlobalAccel` 注册一个 QAction
（默认 `Meta+Shift+B`），可在系统设置 → 快捷键里改。窗口隐藏只是 `hide()`，事件循环和
mpv 播放继续；`--screenshot` 模式不创建托盘。

**切页与返回**：`setTab()` 会把 `view` 切到 `browse`，但没有入口回播放页。现在当
`currentItem != null && view === "browse"` 时，tabs 下面会出现一条「▶ 标题 … 返回播放」的
悬浮条，点击把 `view` 切回 `player`；播放不中断（`MpvItem` 在 `view != player` 时只是隐藏）。

**展开态视频**：`MpvItem` 只有一份，放在 Loader **下面**，位置由 `win.videoRect` 决定。
MiniView 上报整窗矩形，PlayerView 上报播放框 `mapToItem(null)` 减掉 `pad`；两边都检查
`host.mode/view` 再上报，否则切换瞬间旧视图会把矩形覆盖回错的值。播放框在
`videoActive` 时是透明的（`color: transparent`、封面隐藏），视频从下面透出来；
ExpandedView 不能再有整页不透明背景。

**Cookie**：`POST /auth/import` 用 `cookie_scoop` 读 Firefox/Chrome 的 B 站 cookie，
`nav` 校验后存 cookie 头；`GET /auth/status` 返回登录态。

## 5. 播放层（M3）

- `MpvObject`：`QQuickItem` 子类，用 libmpv render API 画进 Qt Quick 的 FBO，
  `QQuickWindow::beforeRendering` 里渲染，QML 控件与弹幕叠在上面
- 必须锁 OpenGL RHI：`main()` 里 `qputenv("QSG_RHI_BACKEND", "opengl")`
  （mpv 的 render API 不认识 Vulkan RHI；软件渲染仅用于离屏截图）
- 对外暴露 `play/pause/seek/volume/speed/position/duration/ended`，
  M1 的 `MockPlayer` 状态机直接在 QML 里，替换时接口不变

## 6. 踩过的坑（都是真的）

1. **`SearchField` 撞名**：Kirigami 自带一个 `SearchField`，本地同名组件被它抢了类型解析，
   自定义信号全部变成 “non-existent property”。改名 `BiliSearchField` 后正常。
   —— 经验：本地组件别用 Kirigami 里已有的名字。
2. **QML 单例要显式声明**：`pragma Singleton` 不够，`qt_add_qml_module` 生成 qmldir 时
   不会自动加 `singleton`，必须在 CMake 里
   `set_source_files_properties(... QT_QML_SINGLETON_TYPE TRUE)`，否则运行时
   `Theme.text` 全是 `undefined`。
3. **QML 里 `Screen.availableGeometry` 拿到 undefined**：改用 C++ 的
   `QScreen::availableGeometry()`（`AppCore.placeWindow()`）算位置。
4. **KWin 会把新窗口居中**：`Component.onCompleted` 里设的坐标会被覆盖，
   要等窗口真正 map 之后（用定时器重试）再放。
5. **qmlcachegen 有时不重编译改过的 QML**：改完 QML 若行为没变，`touch` 一下再 build。
6. **`escape` 不能当函数名**：QML/JS 里是保留名，直接 “Illegal method name”。
7. **指针手势会“打架”**：窗口上同时有 DragHandler（拖动窗口）和角落 MouseArea
   （缩放）时，Qt 的 pointer handler 会抢走 press——光标显示拉伸，实际却在移动。
   解决办法是只留一个 MouseArea，在按下时决定这次手势是缩放还是移动。

## 7. 里程碑

| | 内容 | 状态 |
| --- | --- | --- |
| M0 | 骨架、sidecar 握手、空窗口 | ✅ |
| M1 | 迷你/展开两态、假数据浏览、模拟播放 | ✅ |
| M2 | 真实推荐流 + 搜索 + 封面 | ✅ |
| M3 | libmpv + playurl + DASH | |
| M4 | 历史续播、设置、MPRIS、托盘 | |
| M5 | plasmoid 遥控器、打包、自启 | |
| M6 | 弹幕、清晰度、稍后再看 | |
