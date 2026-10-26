# GPUI 重写计划（gpui 分支）

目标：把 UI 从 Qt/QML + C++ 换成 GPUI，让整个项目（UI / 数据 / 播放）都是 Rust。

## 现状（main）

- `app/`：Qt 6 QML + 少量 C++（`AppCore` 单实例/窗口、`MpvItem` 的 libmpv render）
- `core/`：Rust sidecar（axum + bpi-rs + cookie-scoop + WBI），通过 loopback HTTP + token 与 QML 通信
- 播放：libmpv 渲染进 `QQuickFramebufferObject`

## GPUI 方案

单进程 Rust 应用，`gpui` 0.2 + `gpui-component` 提供列表/输入框等控件：

```
crates/
  app/       gpui 应用（窗口、两态、视图）
  bili/      数据层：bpi-rs 封装（直接复用 core/src/bili.rs 的逻辑）
  media/     libmpv 封装（SW 渲染 API → BGRA 帧）
```

去掉 sidecar、axum、HTTP 和 token：数据层直接在同一个进程里调用。

### 视频渲染

GPUI 没有暴露「外部 GL 纹理」给普通视图，最直接的路径是让 mpv 走 **software render API**：

1. `mpv_render_context_create(MPV_RENDER_API_TYPE_SW)`
2. 每帧 `mpv_render_context_render` 到一块 BGRA 缓冲（`SW_SIZE` / `SW_STRIDE` / `SW_FORMAT`）
3. 用 `RenderImage`（GPUI 的 BGRA 图像资源）上传，画在视频区域

代价是每帧一次 CPU 拷贝 + 上传。对 400×225 的小窗没问题；全屏 1080p 时按窗口尺寸
请求更低清晰度（qn），避免无谓的 1080p → 400px 缩放。

备选（暂不做）：mpv 独立子窗口、GL 纹理互操作、把视频交给合成器。

### 弹幕

GPUI 自绘文本：维护活跃弹幕表，按播放时钟推进 x，`paint` 里逐条画（或每帧构建一个
text 元素树）。需要先测 60 条同屏时的 CPU。

## 里程碑

- **M0 spike**：窗口 + 一个视频能播（mpv SW → RenderImage），测 30fps / CPU；不通过就停
- **M1**：推荐流 + 搜索（视频/UP主）+ UP 主页 + 两态窗口
- **M2**：播放页 + 进度/音量/全屏 + 弹幕
- **M3**：评论、登录（cookie-scoop）、快捷键、持久化
- **M4**：托盘/MPRIS、打包

## 风险

- GPUI 0.2.x API 变动快，Linux 后端以 Zed 自身需求为主，遇到问题只能跟上游
- 没有原生视频纹理，CPU 拷贝是硬开销；全屏播放体验需要实测
- 自绘弹幕、滚动列表的性能需要基准
- 两套 UI 并存期间维护成本高：spike 通过后再决定是否冻结 QML 版本

## 验收门槛（M0）

- 400×225 窗口播放 1080p 源，稳定 30fps
- 单核 CPU 占用 < 50%，内存无持续增长
- 暂停/恢复、seek 后画面与声音同步
