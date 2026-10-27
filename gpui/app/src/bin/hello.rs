use gpui::{div, prelude::*, rgb, App, Application, Window, WindowOptions};

struct Hello;

impl gpui::Render for Hello {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div().size_full().bg(rgb(0x223344)).child("hello gpui")
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.open_window(WindowOptions::default(), |_window, cx| cx.new(|_| Hello))
            .unwrap();
        cx.activate(true);
    });
}
