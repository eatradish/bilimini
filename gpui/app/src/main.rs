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
    fn new(target: String, cx: &mut Context<Self>) -> Self {
        let player = Player::open(&target, VIDEO_WIDTH, VIDEO_HEIGHT);

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
    let target = std::env::args()
        .nth(1)
        .expect("usage: bilimini-gpui <video-url-or-path>");

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
        cx.open_window(options, move |_window, cx| {
            cx.new(|cx| VideoView::new(target.clone(), cx))
        })
        .expect("failed to open window");

        cx.activate(true);
    });
}
