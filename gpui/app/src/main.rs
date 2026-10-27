//! bilimini · GPUI prototype
//!
//! Two windows share one `AppState`:
//! - expanded (browse list / player page)
//! - mini (video only, bottom-right of the primary display)
//!
//! Video comes from libmpv via `bilimini-media` (EGL + FBO + glReadPixels).

use bilimini_bili::{Bili, PlayInfo, VideoItem};
use bilimini_media::Player;
use gpui::{
    div, img, point, prelude::*, px, rgb, size, App, Application, Context, FontWeight,
    ObjectFit, Render, RenderImage, TitlebarOptions, Window, WindowBounds, WindowOptions,
};
use image::{Frame, ImageBuffer, Rgba};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

const VIDEO_WIDTH: u32 = 640;
const VIDEO_HEIGHT: u32 = 360;

const MINI_WIDTH: f32 = 380.0;
const MINI_HEIGHT: f32 = 214.0;
const EXPANDED_WIDTH: f32 = 420.0;
const EXPANDED_HEIGHT: f32 = 660.0;

const BG: u32 = 0x15161a;
const CARD: u32 = 0x1f2127;
const TEXT: u32 = 0xecedf0;
const MUTED: u32 = 0x9ba1ac;
const ACCENT: u32 = 0xfb7299;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Expanded,
    Mini,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Browse,
    Player,
}

struct AppState {
    runtime: Runtime,
    feed: Vec<VideoItem>,
    status: String,
    current: Option<VideoItem>,
    play: Option<PlayInfo>,
    player: Option<Player>,
    image: Option<Arc<RenderImage>>,
    previous: Option<Arc<RenderImage>>,
    view: View,
}

impl AppState {
    fn new(cx: &mut Context<Self>) -> Self {
        let runtime = Runtime::new().expect("tokio runtime");
        let feed = runtime
            .block_on(async { Bili::new().ok()?.feed().await.ok() })
            .unwrap_or_default();
        let status = if feed.is_empty() {
            "推荐加载失败".to_string()
        } else {
            format!("{} 条推荐", feed.len())
        };

        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(8))
                    .await;
                let _ = this.update(cx, |state, cx| {
                    if state.pump(cx) {
                        cx.notify();
                    }
                });
            }
        })
        .detach();

        Self {
            runtime,
            feed,
            status,
            current: None,
            play: None,
            player: None,
            image: None,
            previous: None,
            view: View::Browse,
        }
    }

    fn refresh(&mut self) {
        match self
            .runtime
            .block_on(async { Bili::new().ok()?.feed().await.ok() })
        {
            Some(feed) if !feed.is_empty() => {
                self.status = format!("{} 条推荐", feed.len());
                self.feed = feed;
            }
            _ => self.status = "刷新失败".to_string(),
        }
    }

    fn start(&mut self, video: VideoItem) -> bool {
        let info = self.runtime.block_on(async {
            Bili::new()
                .ok()?
                .play(&video.bvid, video.cid, 80)
                .await
                .ok()
        });

        let Some(info) = info else {
            self.status = "取流失败".to_string();
            return false;
        };

        self.status = format!("{} · qn={}", info.bvid, info.quality);
        self.player = Some(Player::open(
            &info.video_url,
            Some(&info.audio_url),
            VIDEO_WIDTH,
            VIDEO_HEIGHT,
        ));
        self.current = Some(video);
        self.play = Some(info);
        self.view = View::Player;
        self.image = None;
        self.previous = None;
        true
    }

    fn pump(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(player) = self.player.as_ref() else {
            return false;
        };

        let mut updated = false;
        while let Some(frame) = player.try_frame() {
            let Some(buffer) =
                ImageBuffer::<Rgba<u8>, _>::from_raw(frame.width, frame.height, frame.data)
            else {
                continue;
            };

            let render = Arc::new(RenderImage::new(vec![Frame::new(buffer)]));
            let old = self.image.replace(render);
            if let Some(older) = self.previous.take() {
                cx.drop_image(older, None);
            }
            self.previous = old;
            updated = true;
        }
        updated
    }
}

struct RootView {
    state: gpui::Entity<AppState>,
    role: Role,
}

impl RootView {
    fn new(state: gpui::Entity<AppState>, role: Role, cx: &mut Context<Self>) -> Self {
        // Re-render whenever AppState changes (new video frames, feed, view).
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self { state, role }
    }
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let content = match (self.role, state.view) {
            (Role::Mini, _) => mini_view(&state, self.state.clone()),
            (Role::Expanded, View::Player) => player_view(&state, self.state.clone()),
            (Role::Expanded, View::Browse) => browse_view(&state, self.state.clone()),
        };

        div()
            .size_full()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .child(content)
    }
}

fn browse_view(state: &AppState, entity: gpui::Entity<AppState>) -> gpui::AnyElement {
    let refresh_entity = entity.clone();
    let header = div()
        .flex()
        .items_center()
        .justify_between()
        .p_2()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(rgb(ACCENT))
                        .text_color(rgb(0xffffff))
                        .font_weight(FontWeight(700.0))
                        .child("B"),
                )
                .child(div().text_sm().font_weight(FontWeight(700.0)).child("bilimini"))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(state.status.clone()),
                ),
        )
        .child(action("刷新", move |_, cx| {
            refresh_entity.update(cx, |state, cx| {
                state.refresh();
                cx.notify();
            });
        }));

    let mut list = div()
        .id("feed")
        .flex()
        .flex_col()
        .flex_1()
        .overflow_y_scroll()
        .p_2()
        .gap_1();
    for (index, video) in state.feed.iter().enumerate() {
        let video = video.clone();
        let entity = entity.clone();
        let row = div()
            .id(("video", index))
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .rounded_md()
            .cursor_pointer()
            .hover(|style| style.bg(rgb(CARD)))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight(600.0))
                    .child(video.title.clone()),
            )
            .child(div().text_xs().text_color(rgb(MUTED)).child(format!(
                "{} · {} 播放 · {}",
                video.up,
                video.views,
                fmt_duration(video.duration)
            )))
            .on_click(move |_, window, cx| {
                let started = entity.update(cx, |state, _| state.start(video.clone()));
                if started {
                    open_window(cx, entity.clone(), Role::Mini);
                    window.remove_window();
                }
            });
        list = list.child(row);
    }

    div()
        .flex()
        .flex_col()
        .size_full()
        .child(header)
        .child(list)
        .into_any_element()
}

fn player_view(state: &AppState, entity: gpui::Entity<AppState>) -> gpui::AnyElement {
    let title = state
        .current
        .as_ref()
        .map(|video| video.title.clone())
        .unwrap_or_default();
    let meta = state
        .current
        .as_ref()
        .map(|video| format!("{} · {} 播放", video.up, video.views))
        .unwrap_or_default();

    let back_entity = entity.clone();
    let mini_entity = entity.clone();

    div()
        .flex()
        .flex_col()
        .size_full()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .p_2()
                .child(action("← 返回", move |_, cx| {
                    back_entity.update(cx, |state, cx| {
                        state.view = View::Browse;
                        cx.notify();
                    });
                }))
                .child(action("小窗", move |window, cx| {
                    open_window(cx, mini_entity.clone(), Role::Mini);
                    window.remove_window();
                })),
        )
        .child(video_area(state, false))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight(600.0))
                        .child(title),
                )
                .child(div().text_xs().text_color(rgb(MUTED)).child(meta))
                .child(div().text_xs().text_color(rgb(MUTED)).child(state.status.clone())),
        )
        .into_any_element()
}

fn mini_view(state: &AppState, entity: gpui::Entity<AppState>) -> gpui::AnyElement {
    div()
        .id("mini")
        .size_full()
        .bg(rgb(0x000000))
        .cursor_pointer()
        .child(video_area(state, true))
        .on_click(move |_, window, cx| {
            open_window(cx, entity.clone(), Role::Expanded);
            window.remove_window();
        })
        .into_any_element()
}

fn video_area(state: &AppState, full: bool) -> gpui::AnyElement {
    let mut area = div().w_full().bg(rgb(0x000000)).flex().items_center().justify_center();
    area = if full { area.size_full() } else { area.h(px(236.0)) };

    area.child(match &state.image {
        Some(image) => img(image.clone())
            .w_full()
            .h_full()
            .object_fit(ObjectFit::Contain)
            .into_any_element(),
        None => div().text_color(rgb(MUTED)).child("加载中…").into_any_element(),
    })
    .into_any_element()
}

fn action(
    label: &'static str,
    handler: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(label)
        .cursor_pointer()
        .px_2()
        .py_1()
        .rounded_md()
        .hover(|style| style.bg(rgb(CARD)))
        .child(div().text_xs().text_color(rgb(ACCENT)).child(label))
        .on_click(move |_, window, cx| handler(window, cx))
}

fn fmt_duration(seconds: i64) -> String {
    let minutes = seconds / 60;
    format!("{}:{:02}", minutes, seconds % 60)
}

fn window_bounds(app: &App, role: Role) -> (gpui::Bounds<gpui::Pixels>, &'static str) {
    let display = app
        .primary_display()
        .map(|display| display.bounds())
        .unwrap_or_else(|| gpui::Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(1280.0), px(720.0)),
        });

    let (width, height, title) = match role {
        Role::Mini => (MINI_WIDTH, MINI_HEIGHT, "bilimini · mini"),
        Role::Expanded => (EXPANDED_WIDTH, EXPANDED_HEIGHT, "bilimini"),
    };

    let x = f32::from(display.origin.x);
    let y = f32::from(display.origin.y);
    let display_width = f32::from(display.size.width);
    let display_height = f32::from(display.size.height);

    let origin = match role {
        Role::Mini => point(
            px(x + display_width - width - 16.0),
            px(y + display_height - height - 16.0),
        ),
        Role::Expanded => point(
            px(x + (display_width - width) / 2.0),
            px(y + (display_height - height) / 2.0),
        ),
    };

    (
        gpui::Bounds {
            origin,
            size: size(px(width), px(height)),
        },
        title,
    )
}

fn open_window(app: &mut App, state: gpui::Entity<AppState>, role: Role) {
    let (bounds, title) = window_bounds(app, role);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            ..Default::default()
        }),
        ..Default::default()
    };

    app.open_window(options, move |_window, cx| {
        cx.new(|cx| RootView::new(state.clone(), role, cx))
    })
    .expect("open window");
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let state = cx.new(|cx| AppState::new(cx));
        open_window(cx, state, Role::Expanded);
        cx.activate(true);
    });
}
