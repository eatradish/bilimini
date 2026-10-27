# bilimini

> 缩在屏幕右下角的 B 站迷你播放器 · Qt 6 / QML + Rust + libmpv

启动是一个窄卡片推荐页，点视频后收成右下角小窗；展开是完整的播放页，带弹幕和评论。
冲着 Windows Media Player 迷你模式的手感做的。

![播放页](docs/screenshots/player.png)

| 迷你窗 | 推荐列表 | 搜索 UP 主 | 评论 |
| --- | --- | --- | --- |
| ![mini](docs/screenshots/mini.png) | ![browse](docs/screenshots/browse.png) | ![search](docs/screenshots/search-user.png) | ![comments](docs/screenshots/comments.png) |

## 功能

- **两态窗口**：无边框、贴右下角（自动避开底部面板）；迷你态置顶，展开态不置顶，以右下角为锚点向上长
- **四角缩放**：只有四个角能拉，四边留给拖动；画面永远保持 16:9，窗口比例不对时加黑边，不拉伸
- 拖动移动、双击画面展开、`Esc` 收起
- **推荐流**：B 站首页推荐（真实接口），缩略图 + 标题 + UP/播放量
- **搜索**：视频 / UP主 两种类型，UP主结果可在 app 内打开 TA 的投稿列表
- **播放**：libmpv 内嵌（DASH 音视频两路），迷你窗和展开播放页都能看；暂停、进度、音量/静音、自动连播、缓冲转圈
- **弹幕**：按 cid 拉取、按时间排序、9 条泳道滚动，迷你栏可开关
- **评论**：与网页同源的接口，置顶评论 + 热度/时间排序 + 「查看更多」游标翻页
- **系统托盘**：左键点图标在显示/隐藏之间切换，右键菜单可退出；全局快捷键 `Meta+Shift+B`（可在 KDE 系统设置里改）
- **登录**：从 Firefox / Chrome 导入 B 站 cookie（`cookie_scoop`），登录后 1080P
- **本地服务**：Rust sidecar 只监听 `127.0.0.1`，每次启动生成随机 token，QML 请求带 Bearer 头；无遥测

## 快捷键

| 键 | 作用 |
| --- | --- |
| `Space` | 播放 / 暂停 |
| `Esc` | 退出全屏；展开态下收起到迷你 |
| `F` | 全屏 |
| `←` / `→` | 快退 / 快进 5 秒 |
| `↑` / `↓` | 音量 |
| `M` | 静音 |
| `Meta+Shift+B` | 全局显示 / 隐藏窗口（KGlobalAccel，窗口隐藏时也生效） |
| 双击迷你画面 | 展开 |
| 滚轮（迷你态） | 音量 |

## 架构

```
app/                 Qt 6 / QML 主程序
├── Main.qml         窗口、两态、播放状态机
├── MiniView.qml     迷你窗（控制条、弹幕层）
├── ExpandedView.qml 展开态（header + tabs + 内容 Loader）
├── BrowseView.qml   推荐 / 搜索 / 历史
├── PlayerView.qml   播放页（视频、简介、评论、相关推荐）
├── UserView.qml     UP 主页（投稿列表）
├── DanmakuLayer.qml 弹幕层（Text 池 + 播放时钟驱动）
└── src/
    ├── appcore.*    单实例、窗口放置、sidecar 生命周期
    └── mpvitem.*    QQuickFramebufferObject + libmpv render API
core/                Rust sidecar（bili-core）
├── axum            本地 HTTP + token 鉴权
├── bpi-rs          推荐 / 搜索 / 视频详情 / 播放地址
├── cookie-scoop    浏览器 cookie 导入
└── WBI 签名        评论（网页同款 /x/v2/reply/wbi/main）
```

启动时 sidecar 在 stdout 打印 `{"port":..,"token":..}` 握手，主程序退出时一并结束。

## 构建

依赖：

- Qt 6.5+（Quick / QuickControls2 / Network / OpenGL）
- CMake ≥ 3.21、Ninja、pkg-config
- Rust 工具链
- libmpv（mpv 0.40+，需要 `mpv/client.h` 和 `mpv/render_gl.h`）

Arch / AOSC 系：

```bash
sudo pacman -S qt6-base qt6-declarative qt6-quickcontrols2 mpv rust cmake ninja pkgconf
```

构建与运行：

```bash
./build.sh     # cargo build（core）+ cmake/ninja（app），并把 bili-core 拷到 build/
./run.sh       # 启动，默认展开推荐列表
```

产物：

```
build/bili-mini    # Qt 主程序
build/bili-core    # Rust 数据服务（主程序自动拉起）
```

首次构建需要联网拉 crates.io 依赖。

## 登录与隐私

右上角「登录」会用 `cookie_scoop` 读 Firefox / Chrome 里的 B 站 cookie，`nav` 校验通过后
存到 `~/.local/share/bili-mini/auth.json`（`0600`），之后启动自动带上。

- 匿名可以播到 720P 以下，登录后请求 1080P
- cookie 只在本机使用，不会上传到任何第三方；`.gitignore` 不包含任何凭据
- 数据服务只监听 loopback，token 每次启动随机生成

## 调试参数

```bash
./build/bili-mini --screenshot /tmp/shot.png --delay 8000 \
                  --mode expanded --view player --play-bvid BV1xx411c7mD
```

| 参数 | 作用 |
| --- | --- |
| `--screenshot <path>` | 渲染窗口到图片后退出 |
| `--delay <ms>` | 截图前等待时间 |
| `--mode mini\|expanded` | 初始形态 |
| `--view browse\|player` | 初始页面 |
| `--search <q>` / `--search-type video\|user` | 打开搜索页并搜索 |
| `--play-bvid <BV...>` | 直接播放指定视频 |
| `--stay-expanded` | 播放时不收成小窗 |
| `--no-core` | 不启动数据服务 |
| `--dump-window` | 打印窗口几何/置顶状态 |

## 已知限制

- 只在 X11 / KWin 下验证过，Wayland 未测试
- 展开播放页的视频不做圆角裁剪（方形画面叠在圆角框里）
- 历史 tab 还是占位
- 弹幕只按滚动渲染（顶部/底部弹幕也当滚动处理），暂无屏蔽词/透明度设置
- 评论未做楼中楼展开
- 打包（AUR / Flatpak / Plasma 小部件）未完成

## 路线图

- 观看历史与续播、MPRIS / 媒体键、托盘
- Plasma 面板遥控小部件、打包、开机自启
- 清晰度切换、弹幕样式与屏蔽词

## 设计记录

实现中踩过的坑都记在 [docs/design.md](docs/design.md)：QML 单例注册、C++ 定位窗口、
指针处理器与 MouseArea 冲突、`LC_NUMERIC` 导致 `mpv_create()` 失败、mpv
`ADVANCED_CONTROL` 与 `report_swap()`、弹幕接口的 raw deflate、评论的 WBI 签名等。

## 致谢

- [bpi-rs](https://crates.io/crates/bpi-rs)：B 站接口封装
- [cookie-scoop](https://crates.io/crates/cookie-scoop)：浏览器 cookie 读取
- [mpv / libmpv](https://mpv.io/)：播放与渲染
- Qt 6 / KDE

## License

MIT，见 [LICENSE](LICENSE)。

数据来自 B 站 Web 接口，仅供个人使用；接口随时可能变化。
