use gpui::*;

mod export;
mod io;
mod model;
mod parser;
mod ui;

struct RachanaApp;

impl Render for RachanaApp {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(rgb(0x1c1c1c))
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        let size = size(px(1280.0), px(800.0));
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size, cx)),
            titlebar: Some(TitlebarOptions {
                title: Some("Rachana".into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            focus: true,
            show: true,
            app_id: Some("rachana".to_string()),
            ..Default::default()
        };
        cx.open_window(options, |_window, cx| cx.new(|_cx| RachanaApp))
            .unwrap();
    });
}
