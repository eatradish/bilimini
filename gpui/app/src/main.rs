use bilimini_media::Player;
use gpui::{
    div, img, point, prelude::*, px, size, App, Application, AsyncApp, Bounds, Context,
    RenderImage, Render, TitlebarOptions, Window, WindowBounds, WindowOptions,
};
use image::{Frame, ImageBuffer, Rgba};
use std::sync::Arc;
use std::time::Duration;

const VIDEO_WIDTH: u32 = 640;
const VIDEO_HEIGHT: u32 = 360;

struct VideoView {
    player: Player,
    image: Option<Arc<RenderImage>>,
    previous: Option<Arc<RenderImage>>,
    status: String,
}

impl VideoView {
    fn new(target: String, audio: Option<String>, cx: &mut Context<Self>) -> Self {
        let player = Player::open(&target, audio.as_deref(), VIDEO_WIDTH, VIDEO_HEIGHT);

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(8))
                    .await;

                let alive = this.update(cx, |view, cx| {
                    if view.pump(cx) {
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();

        Self {
            player,
            image: None,
            previous: None,
            status: target,
        }
    }

    fn pump(&mut self, cx: &mut Context<Self>) -> bool {
        let mut updated = false;

        while let Some(frame) = self.player.try_frame() {
            let Some(buffer) =
                ImageBuffer::<Rgba<u8>, _>::from_raw(frame.width, frame.height, frame.data)
            else {
                continue;
            };

            let render = Arc::new(RenderImage::new(vec![Frame::new(buffer)]));
            let old = self.image.replace(render);

            // GPUI caches every RenderImage id in the sprite atlas; dropping the
            // frame from two frames ago keeps GPU memory bounded.
            if let Some(older) = self.previous.take() {
                cx.drop_image(older, None);
            }
            self.previous = old;

            updated = true;
        }

        updated
    }
}

impl Render for VideoView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let content = match &self.image {
            Some(image) => img(image.clone()).w_full().h_full().into_any_element(),
            None => div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .child("bilimini · gpui spike")
                .child(div().text_sm().child(self.status.clone()))
                .into_any_element(),
        };

        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgb(0x000000))
            .text_color(gpui::rgb(0xffffff))
            .child(content)
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let target = args
        .next()
        .expect("usage: bilimini-gpui <bvid | video-url-or-path> [audio-url]");
    let mut audio = args.next();

    // A bare BV id is resolved through the data layer: cid + DASH urls.
    let target = if target.starts_with("BV") && audio.is_none() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let play = runtime
            .block_on(async {
                let bili = bilimini_bili::Bili::new()?;
                bili.play(&target, 0, 80).await
            })
            .expect("resolve bvid");
        eprintln!(
            "[app] {} cid={} qn={} {}x{:?}",
            play.bvid, play.cid, play.quality, play.width.unwrap_or(0), play.height
        );
        if !play.audio_url.is_empty() {
            audio = Some(play.audio_url);
        }
        play.video_url
    } else {
        target
    };

    Application::new().run(move |cx: &mut App| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(120.0), px(120.0)),
                size: size(px(VIDEO_WIDTH as f32), px(VIDEO_HEIGHT as f32)),
            })),
            titlebar: Some(TitlebarOptions {
                title: Some("bilimini (gpui)".into()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let target = target.clone();
        let audio = audio.clone();
        cx.open_window(options, move |_window, cx| {
            cx.new(|cx| VideoView::new(target.clone(), audio.clone(), cx))
        })
        .expect("failed to open window");

        cx.activate(true);
    });
}
