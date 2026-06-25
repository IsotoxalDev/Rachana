use std::borrow::Cow;

use gpui::*;

mod export;
mod io;
mod model;
mod parser;
mod settings;
mod ui;

use model::Document;
use ui::document_view::DocumentView;
use ui::modal::ModalState;
use ui::text_runs::{
    COURIER_PRIME_BOLD, COURIER_PRIME_BOLD_ITALIC, COURIER_PRIME_ITALIC, COURIER_PRIME_REGULAR,
};

struct EmbeddedAssets;

impl AssetSource for EmbeddedAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let bytes: &'static [u8] = match path {
            "icons/new.svg"  => include_bytes!("../assets/icons/new.svg"),
            "icons/open.svg" => include_bytes!("../assets/icons/open.svg"),
            "icons/save.svg" => include_bytes!("../assets/icons/save.svg"),
            _ => return Ok(None),
        };
        Ok(Some(Cow::Borrowed(bytes)))
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(vec![])
    }
}

fn main() {
    gpui_platform::application()
        .with_assets(EmbeddedAssets)
        .run(|cx: &mut App| {
            cx.text_system()
                .add_fonts(vec![
                    Cow::Borrowed(COURIER_PRIME_REGULAR),
                    Cow::Borrowed(COURIER_PRIME_BOLD),
                    Cow::Borrowed(COURIER_PRIME_ITALIC),
                    Cow::Borrowed(COURIER_PRIME_BOLD_ITALIC),
                ])
                .ok();

            cx.set_prompt_builder(ui::prompt::build_prompt);

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

            let app_settings = settings::load();

            cx.open_window(options, |_window, cx| {
                let initial_doc = if std::env::var("TESTING").as_deref() == Ok("1") {
                    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("example/Big-Fish.fountain");
                    crate::io::load_fountain(&path).unwrap_or_else(|e| {
                        eprintln!("TESTING: failed to load Big-Fish.fountain: {e}");
                        Document::new()
                    })
                } else if let Some(path_arg) = std::env::args().nth(1) {
                    let path = std::path::PathBuf::from(path_arg);
                    crate::io::load_fountain(&path).unwrap_or_else(|e| {
                        eprintln!("error opening file: {e}");
                        Document::new()
                    })
                } else {
                    Document::new()
                };

                let document = cx.new(|_| initial_doc);
                let modal = cx.new(|_| {
                    let mut m = ModalState::new();
                    if app_settings.vim_enabled {
                        m.toggle_vim();
                    }
                    m
                });
                cx.new(|cx| DocumentView::new(document, modal, cx))
            })
            .unwrap();
        });
}
