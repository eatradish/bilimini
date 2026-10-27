# bilimini · GPUI prototype

GPUI port of the player: an expanded window (feed list / player page) and a
mini window (video only, bottom-right), sharing one `AppState` and one libmpv
player. Clicking a feed row starts playback and swaps to the mini window;
clicking the mini window opens the expanded player; `小窗` goes back.

## What works

- feed cards with real covers (fetched + decoded in the background), title,
  up/views; header with tabs and login pill (visual only for now)
- click a row → resolve cid + qn=80 DASH urls → play → swap to the mini window
- mini window (380×214) at the bottom-right of the primary display, with a
  title bar + 展开 hint; click it to open the expanded player
- expanded player: video, title, up/views, `返回` to the feed, `小窗` back to mini
- frames are shared by both windows through one `RenderImage`

Not there yet: drag-to-seek, volume slider, danmaku, comments, search, login
UI, always-on-top. The player video area is a fixed 236px box.

## How it works

- `media/` owns a libmpv handle on a worker thread.
- mpv renders through `MPV_RENDER_API_TYPE_OPENGL` into an offscreen EGL/FBO
  target (pbuffer + FBO, Mesa/GL).
- Every new frame is read back with `glReadPixels(GL_BGRA)` and sent over a
  bounded channel to the UI thread.
- `app/` wraps each frame in a `gpui::RenderImage` and paints it with the
  built-in `img` element, dropping the image from two frames ago so GPUI's
  sprite atlas does not accumulate textures.

Notes:

- `MPV_RENDER_API_TYPE_SW` (software renderer) crashes inside this mpv 0.41 +
  zimg build, even from a minimal C program, so the GL path is required.
- `MPV_RENDER_PARAM_FLIP_Y` must be 0 with a bottom-up `glReadPixels` readback.
- After `mpv_render_context_render`, re-bind the FBO before reading pixels:
  mpv may leave its own framebuffer bound.
- `xattr 0.2.3` (via `zed-async-tar` → `gpui_http_client`) refers to
  `libc::ENOATTR`, which modern libc no longer exports on Linux. It is vendored
  under `vendor/xattr` and patched to `ENODATA`.

## Build / run

```bash
cargo build
./target/debug/bilimini-gpui /path/to/video.mp4            # local file
./target/debug/bilimini-gpui BV1jhhR6bELW                  # resolve + play a bvid
./target/debug/bilimini-gpui "<video-url>" "<audio-url>"   # bilibili DASH pair
```

Bilibili DASH audio is a separate URL; it is added with `audio-add ... select`
once the video reports `FILE_LOADED`. Both requests carry Referer/User-Agent.

Passing a `BV...` id uses the in-process `bilimini-bili` data layer (bpi-rs) to
resolve the cid and fetch a qn=80 DASH pair; cookies from
`~/.local/share/bili-mini/auth.json` are used when present.

System deps beyond the usual Qt/Rust toolchain: EGL (`libegl`) and a working
OpenGL/EGL driver (Mesa). No GStreamer needed.
