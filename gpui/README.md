# bilimini · GPUI spike

M0 of the GPUI port: a GPUI window playing a video through libmpv.

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
./target/debug/bilimini-gpui /path/to/video.mp4
```

System deps beyond the usual Qt/Rust toolchain: EGL (`libegl`) and a working
OpenGL/EGL driver (Mesa). No GStreamer needed.
