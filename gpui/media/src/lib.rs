//! libmpv backend for the GPUI port.
//!
//! mpv renders through its OpenGL render API into an offscreen EGL/FBO target;
//! each new frame is read back as BGRA and handed to the UI thread as a GPUI
//! image. The software render API (MPV_RENDER_API_TYPE_SW) crashes on this
//! mpv/zimg build, so the GL path is the only workable one.

use libmpv_sys as mpv;
use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
                          (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const REFERER: &str = "https://www.bilibili.com/";

const RENDER_UPDATE_FRAME: u64 = 1;

// ---- EGL ------------------------------------------------------------------

type EGLDisplay = *mut c_void;
type EGLConfig = *mut c_void;
type EGLSurface = *mut c_void;
type EGLContext = *mut c_void;

const EGL_SURFACE_TYPE: c_int = 0x3033;
const EGL_PBUFFER_BIT: c_int = 0x0001;
const EGL_RENDERABLE_TYPE: c_int = 0x3040;
const EGL_OPENGL_BIT: c_int = 0x0008;
const EGL_RED_SIZE: c_int = 0x3024;
const EGL_GREEN_SIZE: c_int = 0x3023;
const EGL_BLUE_SIZE: c_int = 0x3022;
const EGL_ALPHA_SIZE: c_int = 0x3021;
const EGL_DEPTH_SIZE: c_int = 0x3025;
const EGL_NONE: c_int = 0x3038;
const EGL_WIDTH: c_int = 0x3057;
const EGL_HEIGHT: c_int = 0x3056;
const EGL_OPENGL_API: u32 = 0x30A2;

#[link(name = "EGL")]
extern "C" {
    fn eglGetDisplay(display_id: *mut c_void) -> EGLDisplay;
    fn eglInitialize(dpy: EGLDisplay, major: *mut c_int, minor: *mut c_int) -> c_int;
    fn eglChooseConfig(
        dpy: EGLDisplay,
        attribs: *const c_int,
        configs: *mut EGLConfig,
        config_size: c_int,
        num: *mut c_int,
    ) -> c_int;
    fn eglCreatePbufferSurface(
        dpy: EGLDisplay,
        config: EGLConfig,
        attribs: *const c_int,
    ) -> EGLSurface;
    fn eglBindAPI(api: u32) -> c_int;
    fn eglCreateContext(
        dpy: EGLDisplay,
        config: EGLConfig,
        share: EGLContext,
        attribs: *const c_int,
    ) -> EGLContext;
    fn eglMakeCurrent(
        dpy: EGLDisplay,
        draw: EGLSurface,
        read: EGLSurface,
        ctx: EGLContext,
    ) -> c_int;
    fn eglGetProcAddress(name: *const c_char) -> *mut c_void;
    fn eglDestroyContext(dpy: EGLDisplay, ctx: EGLContext) -> c_int;
    fn eglDestroySurface(dpy: EGLDisplay, surface: EGLSurface) -> c_int;
    fn eglTerminate(dpy: EGLDisplay) -> c_int;
    fn eglReleaseThread() -> c_int;
}

unsafe fn egl_proc(name: &str) -> *mut c_void {
    let name = CString::new(name).unwrap();
    eglGetProcAddress(name.as_ptr())
}

struct Gl {
    gen_textures: unsafe extern "C" fn(c_int, *mut u32),
    bind_texture: unsafe extern "C" fn(u32, u32),
    tex_image_2d: unsafe extern "C" fn(u32, i32, i32, i32, i32, i32, u32, u32, *const c_void),
    gen_framebuffers: unsafe extern "C" fn(c_int, *mut u32),
    bind_framebuffer: unsafe extern "C" fn(u32, u32),
    framebuffer_texture_2d: unsafe extern "C" fn(u32, u32, u32, u32, i32),
    check_framebuffer_status: unsafe extern "C" fn(u32) -> u32,
    read_pixels: unsafe extern "C" fn(i32, i32, i32, i32, u32, u32, *mut c_void),
    pixel_store_i: unsafe extern "C" fn(u32, i32),
    delete_framebuffers: unsafe extern "C" fn(c_int, *const u32),
    delete_textures: unsafe extern "C" fn(c_int, *const u32),
}

impl Gl {
    unsafe fn load() -> Self {
        unsafe fn sym<T>(name: &str) -> T {
            let ptr = egl_proc(name);
            assert!(!ptr.is_null(), "missing GL symbol {name}");
            std::mem::transmute_copy(&ptr)
        }
        Self {
            gen_textures: sym("glGenTextures"),
            bind_texture: sym("glBindTexture"),
            tex_image_2d: sym("glTexImage2D"),
            gen_framebuffers: sym("glGenFramebuffers"),
            bind_framebuffer: sym("glBindFramebuffer"),
            framebuffer_texture_2d: sym("glFramebufferTexture2D"),
            check_framebuffer_status: sym("glCheckFramebufferStatus"),
            read_pixels: sym("glReadPixels"),
            pixel_store_i: sym("glPixelStorei"),
            delete_framebuffers: sym("glDeleteFramebuffers"),
            delete_textures: sym("glDeleteTextures"),
        }
    }
}

const GL_TEXTURE_2D: u32 = 0x0DE1;
const GL_RGBA8: i32 = 0x8058;
const GL_RGBA: u32 = 0x1908;
const GL_UNSIGNED_BYTE: u32 = 0x1401;
const GL_FRAMEBUFFER: u32 = 0x8D40;
const GL_COLOR_ATTACHMENT0: u32 = 0x8CE0;
const GL_BGRA: u32 = 0x80E1;
const GL_PACK_ALIGNMENT: u32 = 0x0D05;
const GL_FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;

// ---- mpv ------------------------------------------------------------------

#[derive(Clone)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub seq: u64,
}

pub struct Player {
    frames: Receiver<VideoFrame>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Player {
    pub fn open(target: &str, width: u32, height: u32) -> Self {
        let (tx, rx) = sync_channel(2);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let target = target.to_string();

        let thread = std::thread::spawn(move || run(&target, width, height, tx, stop_flag));
        Self {
            frames: rx,
            stop,
            thread: Some(thread),
        }
    }

    pub fn try_frame(&self) -> Option<VideoFrame> {
        self.frames.try_recv().ok()
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn param(type_: u32, data: *mut c_void) -> mpv::mpv_render_param {
    mpv::mpv_render_param { type_, data }
}

unsafe fn set_option(handle: *mut mpv::mpv_handle, name: &str, value: &str) {
    let name = CString::new(name).unwrap();
    let value = CString::new(value).unwrap();
    mpv::mpv_set_option_string(handle, name.as_ptr(), value.as_ptr());
}

unsafe extern "C" fn gl_proc(_ctx: *mut c_void, name: *const c_char) -> *mut c_void {
    eglGetProcAddress(name)
}

struct Egl {
    display: EGLDisplay,
    surface: EGLSurface,
    context: EGLContext,
}

impl Egl {
    unsafe fn new() -> Option<Self> {
        let display = eglGetDisplay(std::ptr::null_mut());
        if display.is_null() {
            return None;
        }
        let mut major = 0;
        let mut minor = 0;
        if eglInitialize(display, &mut major, &mut minor) != 1 {
            return None;
        }

        let config_attribs = [
            EGL_SURFACE_TYPE,
            EGL_PBUFFER_BIT,
            EGL_RENDERABLE_TYPE,
            EGL_OPENGL_BIT,
            EGL_RED_SIZE,
            8,
            EGL_GREEN_SIZE,
            8,
            EGL_BLUE_SIZE,
            8,
            EGL_ALPHA_SIZE,
            8,
            EGL_DEPTH_SIZE,
            24,
            EGL_NONE,
        ];
        let mut config: EGLConfig = std::ptr::null_mut();
        let mut num_configs = 0;
        eglChooseConfig(
            display,
            config_attribs.as_ptr(),
            &mut config,
            1,
            &mut num_configs,
        );
        if num_configs == 0 {
            return None;
        }

        let pbuffer_attribs = [EGL_WIDTH, 16, EGL_HEIGHT, 16, EGL_NONE];
        let surface = eglCreatePbufferSurface(display, config, pbuffer_attribs.as_ptr());
        if surface.is_null() {
            return None;
        }

        eglBindAPI(EGL_OPENGL_API);
        let context = eglCreateContext(display, config, std::ptr::null_mut(), std::ptr::null());
        if context.is_null() {
            return None;
        }
        if eglMakeCurrent(display, surface, surface, context) != 1 {
            return None;
        }

        Some(Self {
            display,
            surface,
            context,
        })
    }
}

impl Drop for Egl {
    fn drop(&mut self) {
        unsafe {
            eglMakeCurrent(
                self.display,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            eglDestroyContext(self.display, self.context);
            eglDestroySurface(self.display, self.surface);
            eglTerminate(self.display);
            eglReleaseThread();
        }
    }
}

fn run(target: &str, width: u32, height: u32, tx: SyncSender<VideoFrame>, stop: Arc<AtomicBool>) {
    unsafe {
        let Some(egl) = Egl::new() else {
            eprintln!("[media] EGL init failed");
            return;
        };
        let gl = Gl::load();

        let mut texture = 0u32;
        let mut fbo = 0u32;
        (gl.gen_textures)(1, &mut texture);
        (gl.bind_texture)(GL_TEXTURE_2D, texture);
        (gl.tex_image_2d)(
            GL_TEXTURE_2D,
            0,
            GL_RGBA8,
            width as i32,
            height as i32,
            0,
            GL_RGBA,
            GL_UNSIGNED_BYTE,
            std::ptr::null(),
        );
        (gl.gen_framebuffers)(1, &mut fbo);
        (gl.bind_framebuffer)(GL_FRAMEBUFFER, fbo);
        (gl.framebuffer_texture_2d)(
            GL_FRAMEBUFFER,
            GL_COLOR_ATTACHMENT0,
            GL_TEXTURE_2D,
            texture,
            0,
        );
        if (gl.check_framebuffer_status)(GL_FRAMEBUFFER) != GL_FRAMEBUFFER_COMPLETE {
            eprintln!("[media] framebuffer incomplete");
            return;
        }

        let handle = mpv::mpv_create();
        if handle.is_null() {
            return;
        }
        set_option(handle, "vo", "libmpv");
        set_option(handle, "hwdec", "no");
        set_option(handle, "idle", "yes");
        set_option(handle, "keep-open", "no");
        set_option(handle, "osc", "no");
        set_option(handle, "input-default-bindings", "no");
        set_option(handle, "cache", "yes");
        set_option(handle, "user-agent", USER_AGENT);
        set_option(handle, "referrer", REFERER);
        if mpv::mpv_initialize(handle) < 0 {
            mpv::mpv_terminate_destroy(handle);
            return;
        }

        let gl_init = mpv::mpv_opengl_init_params {
            get_proc_address: Some(gl_proc),
            get_proc_address_ctx: std::ptr::null_mut(),
            extra_exts: std::ptr::null(),
        };
        let mut create_params = [
            param(
                mpv::mpv_render_param_type_MPV_RENDER_PARAM_API_TYPE,
                b"opengl\0".as_ptr() as *mut c_void,
            ),
            param(
                mpv::mpv_render_param_type_MPV_RENDER_PARAM_OPENGL_INIT_PARAMS,
                &gl_init as *const _ as *mut c_void,
            ),
            param(
                mpv::mpv_render_param_type_MPV_RENDER_PARAM_INVALID,
                std::ptr::null_mut(),
            ),
        ];
        let mut context: *mut mpv::mpv_render_context = std::ptr::null_mut();
        if mpv::mpv_render_context_create(&mut context, handle, create_params.as_mut_ptr()) < 0 {
            mpv::mpv_terminate_destroy(handle);
            return;
        }

        let command = CString::new("loadfile").unwrap();
        let target = CString::new(target).unwrap();
        let replace = CString::new("replace").unwrap();
        let mut args = [
            command.as_ptr(),
            target.as_ptr(),
            replace.as_ptr(),
            std::ptr::null(),
        ];
        mpv::mpv_command(handle, args.as_mut_ptr());

        let mut buffer = vec![0u8; (width as usize) * (height as usize) * 4];
        let mut seq = 0u64;
        let mut gl_fbo = mpv::mpv_opengl_fbo {
            fbo: fbo as i32,
            w: width as i32,
            h: height as i32,
            internal_format: 0,
        };

        while !stop.load(Ordering::Relaxed) {
            loop {
                let event = mpv::mpv_wait_event(handle, 0.0);
                if event.is_null() {
                    break;
                }
                if (*event).event_id == mpv::mpv_event_id_MPV_EVENT_NONE {
                    break;
                }
            }

            if mpv::mpv_render_context_update(context) & RENDER_UPDATE_FRAME != 0 {
                (gl.bind_framebuffer)(GL_FRAMEBUFFER, fbo);
                let mut flip: c_int = 0;
                let mut render_params = [
                    param(
                        mpv::mpv_render_param_type_MPV_RENDER_PARAM_OPENGL_FBO,
                        &mut gl_fbo as *mut _ as *mut c_void,
                    ),
                    param(
                        mpv::mpv_render_param_type_MPV_RENDER_PARAM_FLIP_Y,
                        &mut flip as *mut _ as *mut c_void,
                    ),
                    param(
                        mpv::mpv_render_param_type_MPV_RENDER_PARAM_INVALID,
                        std::ptr::null_mut(),
                    ),
                ];

                let render = mpv::mpv_render_context_render(context, render_params.as_mut_ptr());
                if render >= 0 {
                    // mpv may leave its own framebuffer bound; read from ours.
                    (gl.bind_framebuffer)(GL_FRAMEBUFFER, fbo);
                    (gl.pixel_store_i)(GL_PACK_ALIGNMENT, 1);
                    (gl.read_pixels)(
                        0,
                        0,
                        width as i32,
                        height as i32,
                        GL_BGRA,
                        GL_UNSIGNED_BYTE,
                        buffer.as_mut_ptr() as *mut c_void,
                    );
                    seq += 1;
                    let _ = tx.try_send(VideoFrame {
                        width,
                        height,
                        data: buffer.clone(),
                        seq,
                    });
                }
            }

            std::thread::sleep(Duration::from_millis(4));
        }

        mpv::mpv_render_context_free(context);
        mpv::mpv_terminate_destroy(handle);
        (gl.delete_framebuffers)(1, &fbo);
        (gl.delete_textures)(1, &texture);
        drop(egl);
    }
}
