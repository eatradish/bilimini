//! bilimini · GPUI prototype
//!
//! Two windows share one `AppState`:
//! - expanded (feed list / player page, QML-like styling)
//! - mini (video + title bar, bottom-right)
//!
//! Video comes from libmpv via `bilimini-media`; covers are fetched and decoded
//! into `RenderImage`s in the background.

use bilimini_bili::{Bili, PlayInfo, VideoItem};
use bilimini_media::{PlaybackState, Player};
use gpui::{
    div, img, point, prelude::*, px, rgb, size, App, Application, ClickEvent, Context, FontWeight,
    ObjectFit, Render, RenderImage, TitlebarOptions, Window, WindowBounds, WindowOptions,
};
use image::{Frame, ImageBuffer, Rgba};
use std::collections::HashMap;
use std::sync::mpsc::Receiver;
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
const CARD_HOVER: u32 = 0x272a32;
const TEXT: u32 = 0xecedf0;
const MUTED: u32 = 0x9ba1ac;
const FAINT: u32 = 0x6c7280;
const ACCENT: u32 = 0xfb7299;
const ACCENT_SOFT: u32 = 0x3a2630;
const BORDER: u32 = 0x2a2c33;
const OK: u32 = 0x4ade80;

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Recommend,
    Search,
    History,
}

struct AppState {
    runtime: Runtime,
    feed: Vec<VideoItem>,
    covers: HashMap<String, Arc<RenderImage>>,
    cover_rx: Receiver<(String, Vec<u8>)>,
    status: String,
    tab: Tab,
    current: Option<VideoItem>,
    play: Option<PlayInfo>,
    player: Option<Player>,
    image: Option<Arc<RenderImage>>,
    previous: Option<Arc<RenderImage>>,
    playback: PlaybackState,
    muted: bool,
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

        let (cover_tx, cover_rx) = std::sync::mpsc::channel();
        let cover_items: Vec<(String, String)> = feed
            .iter()
            .filter(|video| !video.cover.is_empty())
            .map(|video| (video.bvid.clone(), video.cover.clone()))
            .collect();
        std::thread::spawn(move || {
            let Ok(runtime) = Runtime::new() else {
                return;
            };
            let client = reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36")
                .build()
                .ok();
            let Some(client) = client else { return };
            for (bvid, url) in cover_items {
                let bytes = runtime.block_on(async {
                    let response = client.get(&url).send().await.ok()?;
                    Some(response.bytes().await.ok()?.to_vec())
                });
                if let Some(bytes) = bytes {
                    let _ = cover_tx.send((bvid, bytes));
                }
            }
        });

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
            covers: HashMap::new(),
            cover_rx,
            status,
            tab: Tab::Recommend,
            current: None,
            play: None,
            player: None,
            image: None,
            previous: None,
            playback: PlaybackState {
                volume: 1.0,
                ..Default::default()
            },
            muted: false,
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
        let mut updated = false;

        while let Ok((bvid, bytes)) = self.cover_rx.try_recv() {
            if let Ok(decoded) = image::load_from_memory(&bytes) {
                let rgba = decoded.to_rgba8();
                let render = Arc::new(RenderImage::new(vec![Frame::new(rgba)]));
                self.covers.insert(bvid, render);
                updated = true;
            }
        }

        let Some(player) = self.player.as_ref() else {
            return updated;
        };

        let playback = player.state();
        if playback != self.playback {
            self.playback = playback;
            updated = true;
        }

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
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self { state, role }
    }
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let content = match (self.role, state.view) {
            (Role::Mini, _) => mini_view(&state, self.state.clone()),
            (Role::Expanded, View::Player) => {
                let width = f32::from(_window.bounds().size.width);
                player_view(&state, self.state.clone(), width)
            }
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
    let tab_entity = entity.clone();

    let header = div()
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .pt_3()
        .pb_2()
        .child(
            div()
                .w(px(20.0))
                .h(px(20.0))
                .rounded_md()
                .bg(rgb(ACCENT))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight(700.0))
                        .text_color(rgb(0xffffff))
                        .child("B"),
                ),
        )
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight(700.0))
                .child("bilimini"),
        )
        .child(div().w(px(6.0)).h(px(6.0)).rounded_full().bg(rgb(OK)))
        .child(div().flex_1())
        .child(
            div()
                .px_2()
                .py_1()
                .rounded_full()
                .border_1()
                .border_color(rgb(BORDER))
                .child(div().text_xs().text_color(rgb(MUTED)).child("登录")),
        )
        .child(action("刷新", move |_, cx| {
            refresh_entity.update(cx, |state, cx| {
                state.refresh();
                cx.notify();
            });
        }));

    let tabs = div()
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .pb_2()
        .child(tab_pill(
            "推荐",
            state.tab == Tab::Recommend,
            {
                let entity = tab_entity.clone();
                move |_, cx| {
                    entity.update(cx, |state, cx| {
                        state.tab = Tab::Recommend;
                        cx.notify();
                    });
                }
            },
        ))
        .child(tab_pill("搜索", state.tab == Tab::Search, {
            let entity = tab_entity.clone();
            move |_, cx| {
                entity.update(cx, |state, cx| {
                    state.tab = Tab::Search;
                    cx.notify();
                });
            }
        }))
        .child(tab_pill("历史", state.tab == Tab::History, {
            let entity = tab_entity.clone();
            move |_, cx| {
                entity.update(cx, |state, cx| {
                    state.tab = Tab::History;
                    cx.notify();
                });
            }
        }));

    let body = if state.tab == Tab::Recommend {
        let mut list = div().id("feed").flex().flex_col().flex_1().overflow_y_scroll().px_3().pb_3().gap_1();
        for (index, video) in state.feed.iter().enumerate() {
            let video = video.clone();
            let entity = entity.clone();
            let cover = state.covers.get(&video.bvid).cloned();

            let thumb = match cover {
                Some(image) => div()
                    .w(px(112.0))
                    .h(px(63.0))
                    .flex_shrink_0()
                    .rounded_md()
                    .overflow_hidden()
                    .child(img(image).w_full().h_full().object_fit(ObjectFit::Cover))
                    .into_any_element(),
                None => div()
                    .w(px(112.0))
                    .h(px(63.0))
                    .rounded_md()
                    .bg(rgb(CARD))
                    .into_any_element(),
            };

            let row = div()
                .id(("video", index))
                .flex()
                .gap_2()
                .p_2()
                .rounded_md()
                .cursor_pointer()
                .hover(|style| style.bg(rgb(CARD)))
                .child(thumb)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .flex_1()
                        .overflow_hidden()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight(600.0))
                                .line_clamp(2)
                                .child(video.title.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(FAINT))
                                .truncate()
                                .child(format!("{} · {} 播放", video.up, video.views)),
                        ),
                )
                .on_click(move |_, window, cx| {
                    let started = entity.update(cx, |state, _| state.start(video.clone()));
                    if started {
                        open_window(cx, entity.clone(), Role::Mini);
                        window.remove_window();
                    }
                });
            list = list.child(row);
        }
        list.into_any_element()
    } else {
        let message = match state.tab {
            Tab::Search => "搜索还没搬过来",
            _ => "还没有观看记录",
        };
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .child(div().text_color(rgb(FAINT)).child(message))
            .into_any_element()
    };

    div()
        .flex()
        .flex_col()
        .size_full()
        .child(header)
        .child(tabs)
        .child(body)
        .into_any_element()
}

fn player_view(
    state: &AppState,
    entity: gpui::Entity<AppState>,
    window_width: f32,
) -> gpui::AnyElement {
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
    let related_entity = entity.clone();

    let related: Vec<VideoItem> = state
        .feed
        .iter()
        .filter(|video| state.current.as_ref().map(|c| c.bvid != video.bvid).unwrap_or(true))
        .take(3)
        .cloned()
        .collect();

    let mut related_list = div().flex().flex_col().gap_1().px_3().pb_3();
    for (index, video) in related.into_iter().enumerate() {
        let video_for_click = video.clone();
        let entity = related_entity.clone();
        related_list = related_list.child(
            div()
                .id(("related", index))
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
                        .line_clamp(2)
                        .child(video.title.clone()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(FAINT))
                        .child(format!("{} · {} 播放", video.up, video.views)),
                )
                .on_click(move |_, window, cx| {
                    let started = entity.update(cx, |state, _| state.start(video_for_click.clone()));
                    if started {
                        open_window(cx, entity.clone(), Role::Mini);
                        window.remove_window();
                    }
                }),
        );
    }

    div()
        .flex()
        .flex_col()
        .size_full()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px_3()
                .py_2()
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
        .child(control_bar(state, entity.clone(), window_width))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .px_3()
                .py_2()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight(600.0))
                        .line_clamp(2)
                        .child(title),
                )
                .child(div().text_xs().text_color(rgb(ACCENT)).child(meta))
                .child(div().text_xs().text_color(rgb(FAINT)).child(state.status.clone())),
        )
        .child(div().h(px(1.0)).bg(rgb(BORDER)).mx_3())
        .child(div().px_3().pt_2().child(
            div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child("相关推荐"),
        ))
        .child(related_list)
        .into_any_element()
}

fn mini_view(state: &AppState, entity: gpui::Entity<AppState>) -> gpui::AnyElement {
    let title = state
        .current
        .as_ref()
        .map(|video| video.title.clone())
        .unwrap_or_default();

    div()
        .id("mini")
        .flex()
        .flex_col()
        .size_full()
        .bg(rgb(0x000000))
        .cursor_pointer()
        .child(video_area(state, true))
        .child(
            div()
                .w_full()
                .h(px(26.0))
                .px_2()
                .bg(rgb(CARD))
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .text_xs()
                        .text_color(rgb(TEXT))
                        .truncate()
                        .child(title),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(ACCENT))
                        .child("展开"),
                ),
        )
        .on_click(move |_, window, cx| {
            open_window(cx, entity.clone(), Role::Expanded);
            window.remove_window();
        })
        .into_any_element()
}

fn control_bar(
    state: &AppState,
    entity: gpui::Entity<AppState>,
    window_width: f32,
) -> gpui::AnyElement {
    let paused = state.playback.paused;
    let label = if paused { "播放" } else { "暂停" };
    let position = fmt_time(state.playback.position);
    let duration = fmt_time(state.playback.duration);
    let ratio = if state.playback.duration > 0.0 {
        (state.playback.position / state.playback.duration).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    let track = (window_width - 24.0).max(1.0);
    let muted = state.muted;

    let toggle_entity = entity.clone();
    let seek_entity = entity.clone();
    let mute_entity = entity.clone();

    div()
        .flex()
        .flex_col()
        .gap_2()
        .px_3()
        .py_2()
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(action(label, move |_, cx| {
                    toggle_entity.update(cx, |state, cx| {
                        if let Some(player) = &state.player {
                            player.set_paused(!state.playback.paused);
                        }
                        cx.notify();
                    });
                }))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(format!("{position} / {duration}")),
                )
                .child(div().flex_1())
                .child(action(if muted { "取消静音" } else { "静音" }, move |_, cx| {
                    mute_entity.update(cx, |state, cx| {
                        state.muted = !state.muted;
                        let volume = if state.muted { 0.0 } else { 1.0 };
                        if let Some(player) = &state.player {
                            player.set_volume(volume);
                        }
                        cx.notify();
                    });
                })),
        )
        .child(
            div()
                .id("seek")
                .w_full()
                .h(px(14.0))
                .flex()
                .items_center()
                .cursor_pointer()
                .child(
                    div()
                        .w_full()
                        .h(px(4.0))
                        .rounded_full()
                        .bg(rgb(BORDER))
                        .child(
                            div()
                                .w(px(track * ratio))
                                .h(px(4.0))
                                .rounded_full()
                                .bg(rgb(ACCENT)),
                        ),
                )
                .on_click(move |event, window, cx| {
                    if let ClickEvent::Mouse(mouse) = event {
                        let width = f32::from(window.bounds().size.width);
                        let track = (width - 24.0).max(1.0);
                        let x = f32::from(mouse.down.position.x) - 12.0;
                        let ratio = (x / track).clamp(0.0, 1.0);
                        seek_entity.update(cx, |state, cx| {
                            let duration = state.playback.duration;
                            if duration > 0.0 {
                                if let Some(player) = &state.player {
                                    player.seek(ratio as f64 * duration);
                                }
                            }
                            cx.notify();
                        });
                    }
                }),
        )
        .into_any_element()
}

fn fmt_time(seconds: f64) -> String {
    let total = seconds.max(0.0) as i64;
    format!("{}:{:02}", total / 60, total % 60)
}

fn video_area(state: &AppState, full: bool) -> gpui::AnyElement {
    let mut area = div()
        .w_full()
        .bg(rgb(0x000000))
        .flex()
        .items_center()
        .justify_center();
    area = if full { area.h(px(188.0)) } else { area.h(px(236.0)) };

    area.child(match &state.image {
        Some(image) => img(image.clone())
            .w_full()
            .h_full()
            .object_fit(ObjectFit::Fill)
            .into_any_element(),
        None => div().text_color(rgb(FAINT)).child("加载中…").into_any_element(),
    })
    .into_any_element()
}

fn tab_pill(
    label: &'static str,
    active: bool,
    handler: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let (background, color, border) = if active {
        (ACCENT_SOFT, ACCENT, ACCENT_SOFT)
    } else {
        (0x00000000, MUTED, BORDER)
    };

    div()
        .id(label)
        .px_3()
        .py_1()
        .rounded_full()
        .bg(rgb(background))
        .border_1()
        .border_color(rgb(border))
        .cursor_pointer()
        .child(div().text_xs().text_color(rgb(color)).child(label))
        .on_click(move |_, window, cx| handler(window, cx))
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
