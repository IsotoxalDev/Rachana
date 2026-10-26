use std::path::{Path, PathBuf};

use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::settings::{
    create_franchise, forget_script, list_franchises, load_recent, move_franchise, reroll_cover,
    set_script_cover, set_script_group, RecentScript,
};
use crate::ui::cover_art;
use crate::ui::file_dialog::{pick_file_subprocess, DialogMode};
use crate::ui::new_franchise_dialog::{
    NewFranchiseCancelled, NewFranchiseCreated, NewFranchiseDialog,
};
use crate::ui::new_script_dialog::{NewScriptCancelled, NewScriptCreated, NewScriptDialog};

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

pub enum LibraryEvent {
    OpenScript(PathBuf),
    CreateScript { title: String, author: String },
    OpenFileRequested,
}

// ---------------------------------------------------------------------------
// LibraryView — the "Films" box-office screen listing scripts worked on.
// ---------------------------------------------------------------------------

pub struct LibraryView {
    scripts: Vec<RecentScript>,
    franchises: Vec<String>,
    dialog: Option<Entity<NewScriptDialog>>,
    franchise_dialog: Option<Entity<NewFranchiseDialog>>,
    /// Script (index into `scripts`) a just-created franchise should be
    /// assigned to, when the New Franchise dialog was opened from a poster
    /// card's assign menu rather than the library header.
    pending_assign_ix: Option<usize>,
    /// Open assign-to-franchise menu: (script index, anchor position).
    assign_menu: Option<(usize, Point<Pixels>)>,
    /// Open right-click cover menu: (script index, anchor position).
    cover_menu: Option<(usize, Point<Pixels>)>,
    /// Guard: true while a cover-image file dialog is in-flight.
    dialog_open: bool,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<LibraryEvent> for LibraryView {}

impl Focusable for LibraryView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl LibraryView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            scripts: load_recent(),
            franchises: list_franchises(),
            dialog: None,
            franchise_dialog: None,
            pending_assign_ix: None,
            assign_menu: None,
            cover_menu: None,
            dialog_open: false,
            focus_handle: cx.focus_handle(),
            _subscriptions: Vec::new(),
        }
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.scripts = load_recent();
        self.franchises = list_franchises();
        cx.notify();
    }

    fn open_new_script_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dialog = cx.new(|cx| NewScriptDialog::new(cx));
        self._subscriptions.push(
            cx.subscribe_in(&dialog, window, |this, _, event: &NewScriptCreated, window, cx| {
                this.dialog = None;
                cx.emit(LibraryEvent::CreateScript {
                    title: event.title.clone(),
                    author: event.author.clone(),
                });
                this.focus_handle.clone().focus(window, cx);
            }),
        );
        self._subscriptions.push(
            cx.subscribe_in(&dialog, window, |this, _, _: &NewScriptCancelled, window, cx| {
                this.dialog = None;
                this.focus_handle.clone().focus(window, cx);
                cx.notify();
            }),
        );
        let handle = dialog.read(cx).first_focus_handle(cx);
        handle.focus(window, cx);
        self.dialog = Some(dialog);
        self.assign_menu = None;
        cx.notify();
    }

    fn open_new_franchise_dialog(
        &mut self, pending_assign_ix: Option<usize>, window: &mut Window, cx: &mut Context<Self>,
    ) {
        self.pending_assign_ix = pending_assign_ix;
        let dialog = cx.new(|cx| NewFranchiseDialog::new(cx));
        self._subscriptions.push(
            cx.subscribe_in(&dialog, window, |this, _, event: &NewFranchiseCreated, window, cx| {
                this.franchise_dialog = None;
                create_franchise(&event.name);
                if let Some(ix) = this.pending_assign_ix.take() {
                    if let Some(script) = this.scripts.get(ix) {
                        set_script_group(&script.path, Some(event.name.clone()));
                    }
                }
                this.refresh(cx);
                this.focus_handle.clone().focus(window, cx);
            }),
        );
        self._subscriptions.push(
            cx.subscribe_in(&dialog, window, |this, _, _: &NewFranchiseCancelled, window, cx| {
                this.franchise_dialog = None;
                this.pending_assign_ix = None;
                this.focus_handle.clone().focus(window, cx);
                cx.notify();
            }),
        );
        let handle = dialog.read(cx).first_focus_handle(cx);
        handle.focus(window, cx);
        self.franchise_dialog = Some(dialog);
        self.assign_menu = None;
        cx.notify();
    }

    fn reorder_franchise(&mut self, name: &str, delta: i32, cx: &mut Context<Self>) {
        move_franchise(name, delta);
        self.franchises = list_franchises();
        cx.notify();
    }

    fn assign_group(&mut self, ix: usize, group: Option<String>, cx: &mut Context<Self>) {
        if let Some(script) = self.scripts.get(ix) {
            set_script_group(&script.path, group);
        }
        self.assign_menu = None;
        self.refresh(cx);
    }

    fn remove_cover(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let Some(script) = self.scripts.get(ix) {
            set_script_cover(&script.path, None);
        }
        self.assign_menu = None;
        self.refresh(cx);
    }

    fn reroll_cover_art(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let Some(script) = self.scripts.get(ix) {
            reroll_cover(&script.path);
        }
        self.cover_menu = None;
        self.refresh(cx);
    }

    fn delete_script(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.cover_menu = None;
        let Some(script) = self.scripts.get(ix) else { return; };
        let name = if script.title.trim().is_empty() {
            script.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "this script".to_string())
        } else {
            script.title.clone()
        };
        let path = script.path.clone();

        let receiver = window.prompt(
            PromptLevel::Warning,
            &format!("Remove \u{201c}{name}\u{201d} from the library?"),
            Some("The file on disk won't be touched \u{2014} you can bring it back with Open File\u{2026}."),
            &["Remove", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |weak_this, cx| {
            let Ok(choice) = receiver.await else { return; };
            if choice == 0 {
                weak_this.update(cx, |this, cx| {
                    forget_script(&path);
                    this.refresh(cx);
                }).ok();
            }
        }).detach();
    }

    fn choose_cover_image(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.assign_menu = None;
        if self.dialog_open {
            return;
        }
        let Some(script_path) = self.scripts.get(ix).map(|s| s.path.clone()) else { return; };
        self.dialog_open = true;

        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose Cover Image".into()),
        });
        cx.spawn_in(window, async move |weak_this, cx| {
            let picked = match receiver.await {
                Ok(Ok(Some(mut paths))) => paths.pop(),
                Ok(Ok(None)) => {
                    weak_this.update(cx, |this, _| this.dialog_open = false).ok();
                    return;
                }
                Ok(Err(_)) | Err(_) => {
                    pick_file_subprocess(DialogMode::Open, "png", "Image Files (*.png *.jpg *.jpeg)").await
                }
            };
            weak_this.update(cx, |this, cx| {
                this.dialog_open = false;
                if let Some(src) = picked {
                    if let Some(dest_path) = copy_into_covers_cache(&script_path, &src) {
                        set_script_cover(&script_path, Some(dest_path));
                    }
                }
                this.refresh(cx);
            }).ok();
        }).detach();
    }
}

/// Copy a chosen image into the covers cache under a filename derived from
/// the script's own path, so the cover survives the source file moving away.
fn copy_into_covers_cache(script_path: &Path, src: &Path) -> Option<PathBuf> {
    let dir = crate::settings::covers_dir()?;
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("png");
    let dest = dir.join(format!("{}.{ext}", cache_key(script_path)));
    std::fs::copy(src, &dest).ok()?;
    Some(dest)
}

fn cache_key(path: &Path) -> String {
    let mut hash: u32 = 2166136261;
    for b in path.to_string_lossy().bytes() {
        hash ^= b as u32;
        hash = hash.wrapping_mul(16777619);
    }
    format!("{hash:08x}")
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for LibraryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_scripts = !self.scripts.is_empty();
        let franchises = self.franchises.clone();
        let franchise_count = franchises.len();

        let mut root = div()
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
                                header_button(false)
                                    .id("library-new-franchise")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_new_franchise_dialog(None, window, cx);
                                        cx.stop_propagation();
                                    }))
                                    .child("+ New Franchise"),
                            )
                            .child(
                                header_button(true)
                                    .id("library-new")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_new_script_dialog(window, cx);
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
                    .when(has_scripts || franchise_count > 0, |d| {
                        let ungrouped = ungrouped_scripts(&self.scripts);
                        d.child(
                            div()
                                .p(px(28.0))
                                .flex()
                                .flex_col()
                                .children(franchises.iter().enumerate().map(|(fi, name)| {
                                    let items = scripts_in_franchise(&self.scripts, name);
                                    group_section(name.clone(), items, fi, franchise_count, cx)
                                }))
                                .when(!ungrouped.is_empty(), |d| {
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
                                                ungrouped.into_iter().map(|(ix, script)| poster_card(ix, script, cx)),
                                            ),
                                    )
                                }),
                        )
                    })
                    .when(!has_scripts && franchise_count == 0, |d| {
                        d.child(
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(empty_state()),
                        )
                    }),
            );

        if let Some((ix, pos)) = self.assign_menu {
            root = root.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.assign_menu = None;
                        cx.notify();
                        cx.stop_propagation();
                    }))
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation()),
            );
            let has_cover = self.scripts.get(ix).is_some_and(|s| s.cover.is_some());
            root = root.child(render_assign_menu(ix, pos, &self.franchises, has_cover, window, cx));
        }

        if let Some((ix, pos)) = self.cover_menu {
            root = root.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.cover_menu = None;
                        cx.notify();
                        cx.stop_propagation();
                    }))
                    .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| {
                        this.cover_menu = None;
                        cx.notify();
                        cx.stop_propagation();
                    }))
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation()),
            );
            let has_cover = self.scripts.get(ix).is_some_and(|s| s.cover.is_some());
            root = root.child(render_cover_menu(ix, pos, has_cover, window, cx));
        }

        root.children(self.dialog.clone()).children(self.franchise_dialog.clone())
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

/// Scripts not filed under any franchise, most-recent-first. The `usize`
/// carried alongside each script is its index in the original list, used to
/// keep poster card element ids globally unique.
fn ungrouped_scripts(scripts: &[RecentScript]) -> Vec<(usize, &RecentScript)> {
    scripts.iter().enumerate().filter(|(_, s)| s.group.is_none()).collect()
}

/// Scripts filed under the given franchise name, most-recent-first.
fn scripts_in_franchise<'a>(scripts: &'a [RecentScript], name: &str) -> Vec<(usize, &'a RecentScript)> {
    scripts.iter().enumerate().filter(|(_, s)| s.group.as_deref() == Some(name)).collect()
}

fn folder_badge(name: &str) -> impl IntoElement {
    let hue = poster_hue(name);
    div()
        .w(px(16.0))
        .h(px(12.0))
        .rounded(px(2.0))
        .bg(hsla(hue, 0.4, 0.4, 1.0))
}

fn group_section(
    name: String, scripts: Vec<(usize, &RecentScript)>, index: usize, total: usize,
    cx: &mut Context<LibraryView>,
) -> impl IntoElement {
    let count = scripts.len();
    let name_up = name.clone();
    let name_down = name.clone();

    div()
        .flex()
        .flex_col()
        .mb(px(28.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .mb(px(12.0))
                .child(folder_badge(&name))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(hsla(0.0, 0.0, 0.85, 1.0))
                        .child(name.clone()),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(hsla(0.0, 0.0, 0.45, 1.0))
                        .child(format!("{count} script{}", if count == 1 { "" } else { "s" })),
                )
                .child(
                    div()
                        .id(("franchise-up", index))
                        .w(px(18.0))
                        .h(px(18.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(3.0))
                        .text_size(px(10.0))
                        .when(index > 0, |d| {
                            d.text_color(hsla(0.0, 0.0, 0.6, 1.0))
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(0x333333)).text_color(hsla(0.0, 0.0, 0.9, 1.0)))
                                .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                                    this.reorder_franchise(&name_up, -1, cx);
                                    cx.stop_propagation();
                                }))
                        })
                        .when(index == 0, |d| d.text_color(hsla(0.0, 0.0, 0.3, 1.0)))
                        .child("\u{25b4}"),
                )
                .child(
                    div()
                        .id(("franchise-down", index))
                        .w(px(18.0))
                        .h(px(18.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(3.0))
                        .text_size(px(10.0))
                        .when(index + 1 < total, |d| {
                            d.text_color(hsla(0.0, 0.0, 0.6, 1.0))
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(0x333333)).text_color(hsla(0.0, 0.0, 0.9, 1.0)))
                                .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                                    this.reorder_franchise(&name_down, 1, cx);
                                    cx.stop_propagation();
                                }))
                        })
                        .when(index + 1 >= total, |d| d.text_color(hsla(0.0, 0.0, 0.3, 1.0)))
                        .child("\u{25be}"),
                ),
        )
        .child(
            div()
                .p(px(14.0))
                .bg(rgb(0x1a1a1a))
                .border_1()
                .border_color(rgb(0x2a2a2a))
                .rounded_lg()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(px(20.0))
                .when(scripts.is_empty(), |d| {
                    d.child(
                        div()
                            .text_size(px(11.0))
                            .text_color(hsla(0.0, 0.0, 0.4, 1.0))
                            .child("No scripts in this franchise yet."),
                    )
                })
                .children(scripts.into_iter().map(|(ix, script)| poster_card(ix, script, cx))),
        )
}

/// Anchor a context-style menu at `pos`, flipping to grow up and/or left
/// instead of down and/or right when it would otherwise run off the window.
fn position_menu(menu: Div, pos: Point<Pixels>, width: Pixels, height_guess: Pixels, window: &Window) -> Div {
    let viewport = window.viewport_size();
    let menu = if pos.y + height_guess > viewport.height {
        menu.bottom((viewport.height - pos.y).max(px(0.0)))
    } else {
        menu.top(pos.y)
    };
    if pos.x + width > viewport.width {
        menu.right((viewport.width - pos.x).max(px(0.0)))
    } else {
        menu.left(pos.x)
    }
}

fn render_assign_menu(
    ix: usize, pos: Point<Pixels>, franchises: &[String], has_cover: bool,
    window: &Window, cx: &mut Context<LibraryView>,
) -> impl IntoElement {
    let row_style = |d: Div| {
        d.px(px(14.0))
            .py(px(6.0))
            .text_size(px(12.0))
            .text_color(hsla(0.0, 0.0, 0.85, 1.0))
            .hover(|s| s.bg(rgb(0x3a3a3a)))
            .cursor_pointer()
    };

    let width = px(200.0);
    let height_guess = px(40.0 + 28.0 * (franchises.len() + 3) as f32);
    let menu = position_menu(div(), pos, width, height_guess, window);
    let mut menu = menu
        .absolute()
        .w(width)
        .bg(rgb(0x2a2a2a))
        .border_1()
        .border_color(rgb(0x484848))
        .rounded_md()
        .shadow_lg()
        .flex()
        .flex_col()
        .py(px(4.0))
        .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| cx.stop_propagation()))
        .child(
            row_style(div())
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                    this.assign_group(ix, None, cx);
                }))
                .child("No Franchise"),
        );

    if !franchises.is_empty() {
        menu = menu.child(div().h(px(1.0)).mx(px(8.0)).my(px(3.0)).bg(rgb(0x444444)));
        for name in franchises {
            let for_click = name.clone();
            menu = menu.child(
                row_style(div())
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                        this.assign_group(ix, Some(for_click.clone()), cx);
                    }))
                    .child(name.clone()),
            );
        }
    }

    menu = menu
        .child(div().h(px(1.0)).mx(px(8.0)).my(px(3.0)).bg(rgb(0x444444)))
        .child(
            row_style(div())
                .text_color(hsla(0.0, 0.0, 0.6, 1.0))
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                    this.assign_menu = None;
                    this.open_new_franchise_dialog(Some(ix), window, cx);
                }))
                .child("+ New Franchise\u{2026}"),
        )
        .child(div().h(px(1.0)).mx(px(8.0)).my(px(3.0)).bg(rgb(0x444444)))
        .child(
            row_style(div())
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                    this.choose_cover_image(ix, window, cx);
                }))
                .child("Set Cover Image\u{2026}"),
        );

    if has_cover {
        menu = menu.child(
            row_style(div())
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                    this.remove_cover(ix, cx);
                }))
                .child("Remove Cover Image"),
        );
    }

    menu
}

fn render_cover_menu(
    ix: usize, pos: Point<Pixels>, has_cover: bool, window: &Window, cx: &mut Context<LibraryView>,
) -> impl IntoElement {
    let row_style = |d: Div| {
        d.px(px(14.0))
            .py(px(6.0))
            .text_size(px(12.0))
            .text_color(hsla(0.0, 0.0, 0.85, 1.0))
            .hover(|s| s.bg(rgb(0x3a3a3a)))
            .cursor_pointer()
    };

    let width = px(180.0);
    let height_guess = px(if has_cover { 150.0 } else { 120.0 });
    let menu = position_menu(div(), pos, width, height_guess, window);
    let mut menu = menu
        .absolute()
        .w(width)
        .bg(rgb(0x2a2a2a))
        .border_1()
        .border_color(rgb(0x484848))
        .rounded_md()
        .shadow_lg()
        .flex()
        .flex_col()
        .py(px(4.0))
        .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| cx.stop_propagation()))
        .on_mouse_down(MouseButton::Right, cx.listener(|_, _, _, cx| cx.stop_propagation()));

    menu = menu.child(
        row_style(div())
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                this.choose_cover_image(ix, window, cx);
            }))
            .child("Set Cover Image\u{2026}"),
    );

    if has_cover {
        menu = menu.child(
            row_style(div())
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                    this.remove_cover(ix, cx);
                }))
                .child("Use Generated Cover"),
        );
    }

    menu.child(
        row_style(div())
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                this.reroll_cover_art(ix, cx);
            }))
            .child("New Random Cover"),
    )
    .child(div().h(px(1.0)).mx(px(8.0)).my(px(3.0)).bg(rgb(0x444444)))
    .child(
        row_style(div())
            .text_color(hsla(0.0, 0.65, 0.6, 1.0))
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                this.delete_script(ix, window, cx);
            }))
            .child("Remove from Library"),
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
    let seed = cover_art::seed_for_path(&path, script.cover_variant);
    let current_group = script.group.clone();
    let cover = script.cover.clone();

    div()
        .id(("poster", ix))
        .w(px(168.0))
        .flex()
        .flex_col()
        .cursor_pointer()
        .on_click(cx.listener(move |_, _, _, cx| {
            cx.emit(LibraryEvent::OpenScript(path.clone()));
            cx.stop_propagation();
        }))
        .on_mouse_down(MouseButton::Right, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
            this.assign_menu = None;
            this.cover_menu = Some((ix, event.position));
            cx.notify();
            cx.stop_propagation();
        }))
        // Poster face
        .child(
            div()
                .relative()
                .w_full()
                .h(px(224.0))
                .rounded_t_md()
                .overflow_hidden()
                .border_1()
                .border_color(hsla(hue, 0.35, 0.34, 1.0))
                .hover(|s| s.border_color(hsla(hue, 0.5, 0.5, 1.0)))
                .when_some(cover.clone(), |d, cover_path| {
                    d.child(img(cover_path).size_full().object_fit(ObjectFit::Cover))
                })
                .when(cover.is_none(), |d| {
                    let plan = cover_art::plan_cover(&seed);
                    d.child(cover_art::render_cover(&plan, &title))
                })
                .child(
                    div()
                        .absolute()
                        .bottom(px(8.0))
                        .right(px(10.0))
                        .px(px(5.0))
                        .rounded(px(3.0))
                        .bg(hsla(0.0, 0.0, 0.0, 0.45))
                        .text_size(px(10.0))
                        .text_color(hsla(0.0, 0.0, 0.95, 0.9))
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
                )
                .child(
                    div()
                        .id(("poster-assign", ix))
                        .mt(px(2.0))
                        .text_size(px(10.0))
                        .cursor_pointer()
                        .text_color(if current_group.is_some() {
                            hsla(hue, 0.5, 0.65, 1.0)
                        } else {
                            hsla(0.0, 0.0, 0.4, 1.0)
                        })
                        .hover(|s| s.text_color(hsla(0.0, 0.0, 0.85, 1.0)))
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            this.assign_menu = Some((ix, event.position));
                            cx.notify();
                            cx.stop_propagation();
                        }))
                        .child(current_group.unwrap_or_else(|| "+ Franchise".to_string())),
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
