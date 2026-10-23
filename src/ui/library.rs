use std::path::PathBuf;

use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::settings::{load_recent, RecentScript};

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

pub enum LibraryEvent {
    OpenScript(PathBuf),
    NewScript,
    OpenFileRequested,
}

// ---------------------------------------------------------------------------
// LibraryView — the "Films" box-office screen listing scripts worked on.
// ---------------------------------------------------------------------------

pub struct LibraryView {
    scripts: Vec<RecentScript>,
}

impl EventEmitter<LibraryEvent> for LibraryView {}

impl LibraryView {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self { scripts: load_recent() }
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for LibraryView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_scripts = !self.scripts.is_empty();

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1c1c1c))
            // Header / marquee
            .child(
                div()
                    .h(px(72.0))
                    .w_full()
                    .flex_shrink_0()
                    .bg(rgb(0x111111))
                    .border_b_1()
                    .border_color(rgb(0x2a2a2a))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(24.0))
                    // Title — top left, Lobster Two.
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .font_family("Lobster Two")
                                    .text_size(px(26.0))
                                    .line_height(px(28.0))
                                    .text_color(hsla(0.0, 0.0, 0.97, 1.0))
                                    .child("Rachana"),
                            )
                            .child(
                                div()
                                    .mt(px(4.0))
                                    .text_size(px(18.0))
                                    .line_height(px(28.0))
                                    .text_color(hsla(0.0, 0.0, 0.5, 1.0))
                                    .child("\u{00b7}"),
                            )
                            .child(
                                div()
                                    .mt(px(4.0))
                                    .text_size(px(11.0))
                                    .line_height(px(28.0))
                                    .text_color(hsla(0.0, 0.0, 0.5, 1.0))
                                    .child("write good films"),
                            ),
                    )
                    // Buttons — right-aligned.
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(
                                header_button(false)
                                    .id("library-open")
                                    .on_click(cx.listener(|_, _, _, cx| {
                                        cx.emit(LibraryEvent::OpenFileRequested);
                                        cx.stop_propagation();
                                    }))
                                    .child("Open File\u{2026}"),
                            )
                            .child(
                                header_button(true)
                                    .id("library-new")
                                    .on_click(cx.listener(|_, _, _, cx| {
                                        cx.emit(LibraryEvent::NewScript);
                                        cx.stop_propagation();
                                    }))
                                    .child("+ New Script"),
                            ),
                    ),
            )
            // Body
            .child(
                div()
                    .id("library-scroll")
                    .flex_1()
                    .w_full()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .p(px(28.0))
                            .flex()
                            .flex_col()
                            .when(has_scripts, |d| {
                                d.child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(hsla(0.0, 0.0, 0.5, 1.0))
                                        .mb(px(16.0))
                                        .child("NOW SHOWING"),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .flex_wrap()
                                        .gap(px(20.0))
                                        .children(
                                            self.scripts
                                                .iter()
                                                .enumerate()
                                                .map(|(ix, script)| poster_card(ix, script, cx)),
                                        ),
                                )
                            })
                            .when(!has_scripts, |d| d.child(empty_state())),
                    ),
            )
    }
}

// ---------------------------------------------------------------------------
// Pieces
// ---------------------------------------------------------------------------

fn header_button(primary: bool) -> Div {
    let base = div()
        .px(px(14.0))
        .py(px(8.0))
        .text_size(px(12.0))
        .rounded_md()
        .cursor_pointer();

    if primary {
        base.bg(rgb(0x4a90d9))
            .text_color(hsla(0.0, 0.0, 1.0, 1.0))
            .hover(|s| s.bg(rgb(0x3a7ec5)))
    } else {
        base.bg(rgb(0x2a2a2a))
            .text_color(hsla(0.0, 0.0, 0.85, 1.0))
            .border_1()
            .border_color(rgb(0x444444))
            .hover(|s| s.bg(rgb(0x383838)))
    }
}

fn empty_state() -> Div {
    div()
        .mt(px(60.0))
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(14.0))
                .text_color(hsla(0.0, 0.0, 0.6, 1.0))
                .child("No scripts in the box office yet."),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(hsla(0.0, 0.0, 0.45, 1.0))
                .child("Start a new script or open one to see it here."),
        )
}

fn poster_card(ix: usize, script: &RecentScript, cx: &mut Context<LibraryView>) -> impl IntoElement {
    let title = if script.title.trim().is_empty() {
        file_stem(&script.path)
    } else {
        script.title.clone()
    };
    let author = script.author.trim();
    let hue = poster_hue(&title);
    let path = script.path.clone();

    div()
        .id(ix)
        .w(px(168.0))
        .flex()
        .flex_col()
        .cursor_pointer()
        .on_click(cx.listener(move |_, _, _, cx| {
            cx.emit(LibraryEvent::OpenScript(path.clone()));
            cx.stop_propagation();
        }))
        // Poster face
        .child(
            div()
                .relative()
                .w_full()
                .h(px(224.0))
                .rounded_t_md()
                .bg(hsla(hue, 0.35, 0.22, 1.0))
                .border_1()
                .border_color(hsla(hue, 0.35, 0.34, 1.0))
                .hover(|s| s.border_color(hsla(hue, 0.5, 0.5, 1.0)))
                .flex()
                .items_center()
                .justify_center()
                .p(px(14.0))
                .child(
                    div()
                        .text_size(px(16.0))
                        .line_height(px(22.0))
                        .text_center()
                        .font_weight(FontWeight::BOLD)
                        .text_color(hsla(0.0, 0.0, 0.95, 1.0))
                        .child(truncate(&title.to_uppercase(), 60)),
                )
                .child(
                    div()
                        .absolute()
                        .bottom(px(8.0))
                        .right(px(10.0))
                        .text_size(px(10.0))
                        .text_color(hsla(0.0, 0.0, 0.85, 0.75))
                        .child(format!("{} sc", script.scene_count)),
                ),
        )
        // Footer card
        .child(
            div()
                .w_full()
                .bg(rgb(0x222222))
                .rounded_b_md()
                .border_1()
                .border_t_0()
                .border_color(rgb(0x333333))
                .px(px(10.0))
                .py(px(8.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(hsla(0.0, 0.0, 0.9, 1.0))
                        .child(truncate(&title, 22)),
                )
                .when(!author.is_empty(), |d| {
                    d.child(
                        div()
                            .text_size(px(10.5))
                            .text_color(hsla(0.0, 0.0, 0.55, 1.0))
                            .child(truncate(author, 24)),
                    )
                })
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(hsla(0.0, 0.0, 0.45, 1.0))
                        .child(format_last_opened(script.last_opened)),
                ),
        )
}

fn file_stem(path: &std::path::Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Untitled".to_string())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

fn poster_hue(seed: &str) -> f32 {
    let mut hash: u32 = 2166136261;
    for b in seed.bytes() {
        hash ^= b as u32;
        hash = hash.wrapping_mul(16777619);
    }
    (hash % 360) as f32 / 360.0
}

fn format_last_opened(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt| dt.format("%b %d, %Y").to_string())
        .unwrap_or_default()
}
