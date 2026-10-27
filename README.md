# bilimini

主要用途：**在屏幕右下角的小窗里看 B 站**，不挡其他窗口。Qt 6 / QML + Rust + libmpv。

![小窗](docs/screenshots/mini.png)

## 怎么用

1. 启动后先看到推荐列表（展开窗口）
2. 点任意视频，窗口收成右下角小窗并开始播放，小窗始终置顶
3. 小窗里：点画面暂停/继续，双击回到展开播放页；拖动标题条移动窗口，四角缩放
4. 播放时控制条会自动隐藏，鼠标移入再出现；`Esc` 从展开窗口收起到小窗
5. 展开播放页可以看简介、评论和相关推荐，「返回」回列表，「小窗」再回到右下角
6. 托盘图标左键显示/隐藏窗口，右键退出；全局快捷键 `Meta+Shift+B` 同样能显示/隐藏

| 展开播放页 | 推荐列表 | 搜索 UP 主 | 评论 |
| --- | --- | --- | --- |
| ![player](docs/screenshots/player.png) | ![browse](docs/screenshots/browse.png) | ![search](docs/screenshots/search-user.png) | ![comments](docs/screenshots/comments.png) |

## 功能

- **窗口**：小窗无边框、贴右下角、置顶；展开后不置顶
- **缩放**：只有四角能拉，其余区域拖动；画面保持 16:9，比例不符时加黑边而不是拉伸
- **推荐**：首页推荐流，下滑自动加载更多
- **搜索**：视频和 UP 主；UP 主主页可以查看投稿列表
- **播放**：libmpv 播放 DASH 音视频，支持暂停、进度、音量/静音、滚轮调音量、自动连播
- **弹幕**：按 cid 拉取并滚动显示，小窗控制条可以开关
- **评论**：置顶评论、热度/时间排序、查看更多
- **登录**：从 Firefox / Chrome 导入 B 站 cookie，登录后支持 1080P
- 数据服务只监听 `127.0.0.1`，每次启动生成随机 token

## 快捷键

| 键 | 作用 |
| --- | --- |
| `Space` | 播放 / 暂停 |
| `Esc` | 退出全屏；展开时收回到小窗 |
| `F` | 全屏 |
| `←` / `→` | 快退 / 快进 5 秒 |
| `↑` / `↓` | 音量 |
| `M` | 静音 |
| `Meta+Shift+B` | 全局显示 / 隐藏窗口 |

## 结构

```
app/                 Qt 6 / QML 主程序
├── Main.qml         窗口、切换、播放状态
├── MiniView.qml     小窗
├── ExpandedView.qml 展开窗口
├── BrowseView.qml   推荐 / 搜索 / 历史
├── PlayerView.qml   播放页
├── UserView.qml     UP 主页
├── DanmakuLayer.qml 弹幕层
└── src/             AppCore、MpvItem
core/                Rust sidecar
├── axum             本地 HTTP + token 鉴权
├── bpi-rs           推荐 / 搜索 / 播放地址
├── cookie-scoop     浏览器 cookie 导入
└── WBI 签名         评论
```

## 构建

需要 Qt 6.5+、CMake ≥ 3.21、Ninja、Rust 和 libmpv（0.40+，含 `client.h`、`render_gl.h`）。

Arch：

```bash
sudo pacman -S qt6-base qt6-declarative qt6-quickcontrols2 mpv rust cmake ninja pkgconf
```

AOSC OS：

```bash
sudo oma install qt-6 mpv rustc cargo cmake ninja pkgconf
```

```bash
./build.sh   # 编译 core 和 app，产物在 build/
./run.sh
```

## 登录与隐私

右上角「登录」会从浏览器读取 B 站 cookie，校验通过后存到 `~/.local/share/bili-mini/auth.json`（权限 `0600`）。cookie 只在本机使用；未登录最高 720P，登录后支持 1080P。

## 已知限制

- 只在 X11 / KWin 下验证过，Wayland 未测试
- 展开播放页的视频不做圆角裁剪
- 历史 tab 和楼中楼评论尚未实现
- 弹幕只支持滚动样式，没有屏蔽词
- 打包（AUR / Flatpak）未完成

## 开发

截图和调试参数见 `./build/bili-mini --help`；实现细节和踩过的坑记在 [docs/design.md](docs/design.md)。

## 致谢

[bpi-rs](https://crates.io/crates/bpi-rs)、[cookie-scoop](https://crates.io/crates/cookie-scoop)、[mpv](https://mpv.io/)、Qt 6 / KDE。

## License

MIT，见 [LICENSE](LICENSE)。数据来自 B 站 Web 接口，仅供个人使用。
