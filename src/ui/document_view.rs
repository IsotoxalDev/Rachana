use gpui::*;

use crate::model::{Block, Document, FountainElement};
use crate::parser::parse_block;
use crate::ui::block_view::{
    char_at_visual_line_x, count_visual_lines, visual_line_and_x,
    BlockView, CursorClicked, CursorDragged, RightClicked, ShiftClicked, WordSelected,
};
use crate::ui::text_runs::element_style;
use crate::ui::file_dialog::{DialogMode, pick_file_subprocess};
use crate::ui::modal::{InsertPoint, Key, ModalAction, ModalState};

// ---------------------------------------------------------------------------
// EditHistory
// ---------------------------------------------------------------------------

pub struct EditHistory {
    undo_stack: Vec<Vec<Block>>,
    redo_stack: Vec<Vec<Block>>,
}

impl EditHistory {
    pub fn new() -> Self {
        Self { undo_stack: Vec::new(), redo_stack: Vec::new() }
    }

    /// Snapshot current blocks before a mutation.
    pub fn push(&mut self, snapshot: Vec<Block>) {
        self.undo_stack.push(snapshot);
        self.redo_stack.clear();
    }

    /// Returns the blocks to restore (caller sets them on doc), or None.
    pub fn pop_undo(&mut self, current: Vec<Block>) -> Option<Vec<Block>> {
        let prev = self.undo_stack.pop()?;
        self.redo_stack.push(current);
        Some(prev)
    }

    pub fn pop_redo(&mut self, current: Vec<Block>) -> Option<Vec<Block>> {
        let next = self.redo_stack.pop()?;
        self.undo_stack.push(current);
        Some(next)
    }
}

// ---------------------------------------------------------------------------
// DocumentView
// ---------------------------------------------------------------------------

pub struct DocumentView {
    pub document: Entity<Document>,
    pub modal: Entity<ModalState>,
    pub focused_block: usize,
    pub cursor_char: usize,
    pub yank_buffer: Option<String>,
    pub history: EditHistory,

    block_views: Vec<Entity<BlockView>>,
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
    context_menu: Option<Point<Pixels>>,
    sidebar_open: bool,
    /// Anchor point (block_index, char_index) where a mouse-drag selection started.
    selection_anchor: Option<(usize, usize)>,
    /// Guard: true while a file open/save dialog is in-flight.
    dialog_open: bool,
    _subscriptions: Vec<Subscription>,
}

impl DocumentView {
    pub fn new(
        document: Entity<Document>,
        modal: Entity<ModalState>,
        cx: &mut Context<Self>,
    ) -> Self {
        let block_count = document.read(cx).blocks.len();
        let mut block_views = Vec::with_capacity(block_count);
        for idx in 0..block_count {
            let doc = document.clone();
            let m = modal.clone();
            block_views.push(cx.new(move |cx| BlockView::new(idx, doc, m, cx)));
        }

        // Mark the first block as focused so the cursor is visible on startup.
        if let Some(first) = block_views.first() {
            first.update(cx, |v, _| v.focused = true);
        }

        // Subscribe to click, drag, and right-click events from each block view.
        let mut subs: Vec<Subscription> = Vec::new();
        for bv in &block_views {
            subs.push(cx.subscribe(bv, |this, bv, event: &CursorClicked, cx| {
                this.context_menu = None;
                this.focused_block = bv.read(cx).block_index;
                this.cursor_char = event.char_idx;
                this.selection_anchor = Some((this.focused_block, this.cursor_char));
                this.push_cursor(cx);
                cx.notify();
            }));
            subs.push(cx.subscribe(bv, |this, bv, event: &CursorDragged, cx| {
                if this.selection_anchor.is_none() { return; }
                this.focused_block = bv.read(cx).block_index;
                this.cursor_char = event.char_idx;
                this.push_cursor(cx);
                cx.notify();
            }));
            subs.push(cx.subscribe(bv, |this, bv, event: &RightClicked, cx| {
                this.context_menu = None;
                this.focused_block = bv.read(cx).block_index;
                this.cursor_char = event.char_idx;
                this.selection_anchor = None;
                this.push_cursor(cx);
                this.context_menu = Some(event.position);
                cx.notify();
            }));
            subs.push(cx.subscribe(bv, |this, bv, event: &WordSelected, cx| {
                this.context_menu = None;
                let block_idx = bv.read(cx).block_index;
                this.focused_block = block_idx;
                this.selection_anchor = Some((block_idx, event.start));
                this.cursor_char = event.end;
                this.push_cursor(cx);
                cx.notify();
            }));
            subs.push(cx.subscribe(bv, |this, bv, event: &ShiftClicked, cx| {
                this.context_menu = None;
                if this.selection_anchor.is_none() {
                    this.selection_anchor = Some((this.focused_block, this.cursor_char));
                }
                this.focused_block = bv.read(cx).block_index;
                this.cursor_char = event.char_idx;
                this.push_cursor(cx);
                cx.notify();
            }));
        }

        Self {
            document,
            modal,
            focused_block: 0,
            cursor_char: 0,
            yank_buffer: None,
            history: EditHistory::new(),
            block_views,
            focus_handle: cx.focus_handle(),
            scroll_handle: ScrollHandle::new(),
            context_menu: None,
            sidebar_open: false,
            selection_anchor: None,
            dialog_open: false,
            _subscriptions: subs,
        }
    }

    // -----------------------------------------------------------------------
    // Block view synchronisation
    // -----------------------------------------------------------------------

    fn sync_block_views(&mut self, cx: &mut Context<Self>) {
        let n = self.document.read(cx).blocks.len();
        while self.block_views.len() < n {
            let idx = self.block_views.len();
            let doc = self.document.clone();
            let modal = self.modal.clone();
            let bv = cx.new(move |cx| BlockView::new(idx, doc, modal, cx));
            self._subscriptions.push(cx.subscribe(&bv, |this, bv, event: &CursorClicked, cx| {
                this.context_menu = None;
                this.focused_block = bv.read(cx).block_index;
                this.cursor_char = event.char_idx;
                this.selection_anchor = Some((this.focused_block, this.cursor_char));
                this.push_cursor(cx);
                cx.notify();
            }));
            self._subscriptions.push(cx.subscribe(&bv, |this, bv, event: &CursorDragged, cx| {
                if this.selection_anchor.is_none() { return; }
                this.focused_block = bv.read(cx).block_index;
                this.cursor_char = event.char_idx;
                this.push_cursor(cx);
                cx.notify();
            }));
            self._subscriptions.push(cx.subscribe(&bv, |this, bv, event: &RightClicked, cx| {
                this.context_menu = None;
                this.focused_block = bv.read(cx).block_index;
                this.cursor_char = event.char_idx;
                this.selection_anchor = None;
                this.push_cursor(cx);
                this.context_menu = Some(event.position);
                cx.notify();
            }));
            self._subscriptions.push(cx.subscribe(&bv, |this, bv, event: &WordSelected, cx| {
                this.context_menu = None;
                let block_idx = bv.read(cx).block_index;
                this.focused_block = block_idx;
                this.selection_anchor = Some((block_idx, event.start));
                this.cursor_char = event.end;
                this.push_cursor(cx);
                cx.notify();
            }));
            self._subscriptions.push(cx.subscribe(&bv, |this, bv, event: &ShiftClicked, cx| {
                this.context_menu = None;
                if this.selection_anchor.is_none() {
                    this.selection_anchor = Some((this.focused_block, this.cursor_char));
                }
                this.focused_block = bv.read(cx).block_index;
                this.cursor_char = event.char_idx;
                this.push_cursor(cx);
                cx.notify();
            }));
            self.block_views.push(bv);
        }
        self.block_views.truncate(n);
        for (i, bv) in self.block_views.iter().enumerate() {
            bv.update(cx, |v, cx| { v.block_index = i; cx.notify(); });
        }
    }

    fn push_cursor(&self, cx: &mut Context<Self>) {
        for (i, bv) in self.block_views.iter().enumerate() {
            let focused = i == self.focused_block;
            let cur = if focused { self.cursor_char } else { 0 };
            let sel = self.compute_block_selection(i, cx);
            bv.update(cx, |v, cx| {
                v.focused = focused;
                v.cursor_char = cur;
                v.selection = sel;
                cx.notify();
            });
        }
    }

    /// Compute the selection range to display for block `i` given the current anchor→cursor span.
    fn compute_block_selection(&self, i: usize, cx: &Context<Self>) -> Option<std::ops::Range<usize>> {
        let (ab, ac) = self.selection_anchor?;
        let (cb, cc) = (self.focused_block, self.cursor_char);

        // Normalise to (start_block, start_char) → (end_block, end_char) in document order.
        let (sb, sc, eb, ec) = if ab < cb || (ab == cb && ac <= cc) {
            (ab, ac, cb, cc)
        } else {
            (cb, cc, ab, ac)
        };

        if i < sb || i > eb { return None; }

        let block_len = self.document.read(cx).blocks.get(i)
            .map(|b| b.text.chars().count()).unwrap_or(0);

        if sb == eb {
            if sc < ec { Some(sc..ec) } else { None }
        } else if i == sb {
            if sc < block_len { Some(sc..block_len) } else { None }
        } else if i < eb {
            if block_len > 0 { Some(0..block_len) } else { None }
        } else {
            if ec > 0 { Some(0..ec) } else { None }
        }
    }

    fn has_selection(&self) -> bool {
        match self.selection_anchor {
            None => false,
            Some((ab, ac)) => ab != self.focused_block || ac != self.cursor_char,
        }
    }

    // -----------------------------------------------------------------------
    // Read helpers
    // -----------------------------------------------------------------------

    fn block_count(&self, cx: &Context<Self>) -> usize {
        self.document.read(cx).blocks.len()
    }

    fn text_of(&self, idx: usize, cx: &Context<Self>) -> String {
        self.document.read(cx).blocks.get(idx).map(|b| b.text.clone()).unwrap_or_default()
    }

    fn char_count(&self, idx: usize, cx: &Context<Self>) -> usize {
        self.text_of(idx, cx).chars().count()
    }

    // -----------------------------------------------------------------------
    // Mutation helpers
    // -----------------------------------------------------------------------

    /// Snapshot then apply a structural change to `doc.blocks`.
    fn mutate<F>(&mut self, cx: &mut Context<Self>, f: F)
    where
        F: FnOnce(&mut Vec<Block>),
    {
        let snapshot = self.document.read(cx).blocks.clone();
        self.history.push(snapshot);
        self.document.update(cx, |doc, _| {
            f(&mut doc.blocks);
            doc.dirty = true;
        });
    }

    fn do_undo(&mut self, cx: &mut Context<Self>) {
        let current = self.document.read(cx).blocks.clone();
        if let Some(prev) = self.history.pop_undo(current) {
            self.document.update(cx, |doc, _| { doc.blocks = prev; doc.dirty = true; });
            self.sync_block_views(cx);
            self.focused_block = self.focused_block.min(self.block_count(cx).saturating_sub(1));
            self.cursor_char = self.cursor_char.min(self.char_count(self.focused_block, cx));
        }
    }

    fn do_redo(&mut self, cx: &mut Context<Self>) {
        let current = self.document.read(cx).blocks.clone();
        if let Some(next) = self.history.pop_redo(current) {
            self.document.update(cx, |doc, _| { doc.blocks = next; doc.dirty = true; });
            self.sync_block_views(cx);
            self.focused_block = self.focused_block.min(self.block_count(cx).saturating_sub(1));
            self.cursor_char = self.cursor_char.min(self.char_count(self.focused_block, cx));
        }
    }

    fn re_parse_block(&mut self, idx: usize, cx: &mut Context<Self>) {
        let text = self.text_of(idx, cx);
        let (prev_element, current_element) = {
            let doc = self.document.read(cx);
            let prev = doc.blocks.get(idx.saturating_sub(1))
                .filter(|_| idx > 0)
                .map(|b| b.element.clone());
            let cur = doc.blocks.get(idx).map(|b| b.element.clone());
            (prev, cur)
        };

        let (mut element, _) = parse_block(&text);

        if element == FountainElement::Action {
            let in_dialogue_context = matches!(
                prev_element,
                Some(FountainElement::Character) | Some(FountainElement::Parenthetical)
            );
            if in_dialogue_context {
                element = FountainElement::Dialogue;
            } else if current_element == Some(FountainElement::Dialogue) {
                // Preserve an explicitly-set Dialogue block — don't demote it to Action.
                element = FountainElement::Dialogue;
            }
        }

        self.document.update(cx, |doc, _| {
            if let Some(b) = doc.blocks.get_mut(idx) {
                b.element = element;
            }
        });
    }

    fn status_bar_text(&self, cx: &Context<Self>) -> String {
        let mode_str = self.modal.read(cx).status_text().unwrap_or("").to_string();
        let doc = self.document.read(cx);
        let name = if doc.title.is_empty() { "Untitled" } else { &doc.title };
        let dirty = if doc.dirty { " ●" } else { "" };
        if mode_str.is_empty() {
            format!("{name}{dirty}")
        } else {
            format!("{mode_str}  {name}{dirty}")
        }
    }

    fn copy_selection(&self, _window: &mut Window, cx: &Context<Self>) {
        if let Some(text) = self.get_selected_text(cx) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn get_selected_text(&self, cx: &Context<Self>) -> Option<String> {
        let (ab, ac) = self.selection_anchor?;
        let (cb, cc) = (self.focused_block, self.cursor_char);
        let (sb, sc, eb, ec) = if ab < cb || (ab == cb && ac <= cc) {
            (ab, ac, cb, cc)
        } else {
            (cb, cc, ab, ac)
        };
        if sb == eb {
            if sc >= ec { return None; }
            let text = self.text_of(sb, cx);
            let s = char_to_byte(&text, sc);
            let e = char_to_byte(&text, ec);
            return Some(text[s..e].to_string());
        }
        let mut result = String::new();
        for i in sb..=eb {
            if i > sb { result.push('\n'); }
            let text = self.text_of(i, cx);
            if i == sb {
                result.push_str(&text[char_to_byte(&text, sc)..]);
            } else if i == eb {
                result.push_str(&text[..char_to_byte(&text, ec)]);
            } else {
                result.push_str(&text);
            }
        }
        Some(result)
    }

    fn delete_selection_text(&mut self, cx: &mut Context<Self>) {
        let Some((ab, ac)) = self.selection_anchor else { return; };
        let (cb, cc) = (self.focused_block, self.cursor_char);
        let (sb, sc, eb, ec) = if ab < cb || (ab == cb && ac <= cc) {
            (ab, ac, cb, cc)
        } else {
            (cb, cc, ab, ac)
        };

        self.selection_anchor = None;

        if sb == eb {
            if sc >= ec { return; }
            let text = self.text_of(sb, cx);
            let s = char_to_byte(&text, sc);
            let e = char_to_byte(&text, ec);
            self.mutate(cx, |blocks| {
                if let Some(b) = blocks.get_mut(sb) {
                    b.text.drain(s..e);
                    b.raw.drain(s..e);
                }
            });
            self.focused_block = sb;
            self.cursor_char = sc;
            self.re_parse_block(sb, cx);
        } else {
            // Collect tail of end block, attach to truncated start block.
            let end_text = self.text_of(eb, cx);
            let end_byte = char_to_byte(&end_text, ec);
            let tail = end_text[end_byte..].to_string();

            let start_text = self.text_of(sb, cx);
            let start_byte = char_to_byte(&start_text, sc);
            let merged = format!("{}{}", &start_text[..start_byte], tail);

            self.mutate(cx, |blocks| {
                if let Some(b) = blocks.get_mut(sb) {
                    b.text = merged.clone();
                    b.raw = merged.clone();
                }
                blocks.drain(sb + 1..=eb);
            });
            self.focused_block = sb;
            self.cursor_char = sc;
            self.re_parse_block(sb, cx);
            self.sync_block_views(cx);
        }
    }

    fn do_cut(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_selection(window, cx);
        self.delete_selection_text(cx);
    }

    fn do_paste(&mut self, cx: &mut Context<Self>) {
        let text = match cx.read_from_clipboard().and_then(|i| i.text()) {
            Some(t) => t,
            None => return,
        };
        if self.has_selection() {
            self.delete_selection_text(cx);
        }
        for (i, line) in text.split('\n').enumerate() {
            if i > 0 { self.split_block(cx); }
            for ch in line.chars() {
                if !ch.is_control() { self.insert_char(ch, cx); }
            }
        }
    }

    fn do_select_all(&mut self, cx: &mut Context<Self>) {
        let last = self.block_count(cx).saturating_sub(1);
        let last_len = self.char_count(last, cx);
        self.selection_anchor = Some((0, 0));
        self.focused_block = last;
        self.cursor_char = last_len;
    }

    fn reset_to_new(&mut self, cx: &mut Context<Self>) {
        self.document.update(cx, |d, _| *d = Document::new());
        self.focused_block = 0;
        self.cursor_char = 0;
        self.history = EditHistory::new();
        self.sidebar_open = false;
        self.selection_anchor = None;
        self.sync_block_views(cx);
        self.push_cursor(cx);
        cx.notify();
    }

    fn do_new_script(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.document.read(cx).dirty {
            self.reset_to_new(cx);
            return;
        }

        let receiver = window.prompt(
            PromptLevel::Warning,
            "Unsaved Changes",
            Some("Do you want to save the current script before creating a new one?"),
            &["Save", "Don't Save", "Cancel"],
            cx,
        );

        cx.spawn_in(window, async move |weak_this, cx| {
            let Ok(choice) = receiver.await else { return; };
            match choice {
                0 => {
                    // "Save" — save synchronously if we have a path, otherwise show Save As.
                    let rx = weak_this.update(cx, |this, cx| -> Option<_> {
                        if this.document.read(cx).path.is_some() {
                            this.do_save(cx);
                            Some(None) // saved; no dialog needed
                        } else if this.dialog_open {
                            None // blocked
                        } else {
                            this.dialog_open = true;
                            let rx = cx.prompt_for_new_path(
                                &std::path::Path::new("untitled.fountain"), None
                            );
                            Some(Some(rx))
                        }
                    });

                    match rx {
                        Ok(Some(None)) => {
                            // Had a path, saved; now reset.
                            weak_this.update(cx, |this, cx| this.reset_to_new(cx)).ok();
                        }
                        Ok(Some(Some(path_rx))) => {
                            // No path — wait for Save As dialog.
                            let path = match path_rx.await {
                                Ok(Ok(Some(p))) => Some(p),
                                Ok(Ok(None)) => {
                                    weak_this.update(cx, |this, _| this.dialog_open = false).ok();
                                    return;
                                }
                                Ok(Err(_)) | Err(_) => {
                                    pick_file_subprocess(DialogMode::Save).await
                                }
                            };
                            weak_this.update(cx, |this, cx| {
                                this.dialog_open = false;
                                if let Some(p) = path {
                                    let ok = {
                                        let doc = this.document.read(cx);
                                        crate::io::save_fountain(&doc, &p).is_ok()
                                    };
                                    if ok {
                                        this.document.update(cx, |d, _| {
                                            d.path = Some(p);
                                            d.dirty = false;
                                        });
                                    }
                                    this.reset_to_new(cx);
                                }
                            }).ok();
                        }
                        _ => {} // entity gone or dialog already open
                    }
                }
                1 => {
                    // "Don't Save" — discard changes and reset.
                    weak_this.update(cx, |this, cx| this.reset_to_new(cx)).ok();
                }
                _ => {} // "Cancel" — do nothing
            }
        }).detach();
    }

    fn do_open_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog_open { return; }
        self.dialog_open = true;

        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open Fountain Script".into()),
        });
        cx.spawn_in(window, async move |weak_this, cx| {
            let path = match receiver.await {
                Ok(Ok(Some(mut paths))) => paths.pop(),
                Ok(Ok(None)) => {
                    weak_this.update(cx, |this, _| this.dialog_open = false).ok();
                    return;
                }
                Ok(Err(_)) | Err(_) => pick_file_subprocess(DialogMode::Open).await,
            };
            let loaded = path.as_deref().map(crate::io::load_fountain);
            weak_this.update(cx, |this, cx| {
                this.dialog_open = false;
                if let Some(Ok(doc)) = loaded {
                    this.document.update(cx, |d, _| *d = doc);
                    this.focused_block = 0;
                    this.cursor_char = 0;
                    this.history = EditHistory::new();
                    this.sidebar_open = false;
                    this.sync_block_views(cx);
                    this.push_cursor(cx);
                    cx.notify();
                }
            }).ok();
        }).detach();
    }

    fn do_save(&mut self, cx: &mut Context<Self>) {
        let path = self.document.read(cx).path.clone();
        if let Some(p) = path {
            let result = {
                let doc = self.document.read(cx);
                crate::io::save_fountain(&doc, &p)
            };
            match result {
                Ok(()) => { self.document.update(cx, |d, _| d.dirty = false); }
                Err(e) => eprintln!("save error: {e}"),
            }
        } else {
            // No path yet — treat Ctrl+S as Save As.
            // Can't call do_save_as here because we don't have &mut Window,
            // so just mark dirty; the caller in handle_key must use do_save_as.
        }
    }

    fn do_save_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog_open { return; }
        self.dialog_open = true;

        let receiver = cx.prompt_for_new_path(&std::path::Path::new("untitled.fountain"), None);
        cx.spawn_in(window, async move |weak_this, cx| {
            let path = match receiver.await {
                Ok(Ok(Some(p))) => Some(p),
                Ok(Ok(None)) => {
                    weak_this.update(cx, |this, _| this.dialog_open = false).ok();
                    return;
                }
                Ok(Err(_)) | Err(_) => pick_file_subprocess(DialogMode::Save).await,
            };
            weak_this.update(cx, |this, cx| {
                this.dialog_open = false;
                let Some(p) = path else { return; };
                let result = {
                    let doc = this.document.read(cx);
                    crate::io::save_fountain(&doc, &p)
                };
                match result {
                    Ok(()) => {
                        this.document.update(cx, |d, _| {
                            d.path = Some(p);
                            d.dirty = false;
                        });
                        cx.notify();
                    }
                    Err(e) => eprintln!("save-as error: {e}"),
                }
            }).ok();
        }).detach();
    }

    fn do_toggle_vim(&mut self, cx: &mut Context<Self>) {
        self.modal.update(cx, |m, _| m.toggle_vim());
        let vim_enabled = self.modal.read(cx).enabled;
        crate::settings::save(&crate::settings::Settings { vim_enabled });
    }

    fn delete_forward(&mut self, cx: &mut Context<Self>) {
        let idx = self.focused_block;
        let count = self.char_count(idx, cx);
        if self.cursor_char < count {
            let text = self.text_of(idx, cx);
            if let Some((start, ch)) = text.char_indices().nth(self.cursor_char) {
                let end = start + ch.len_utf8();
                self.mutate(cx, |blocks| {
                    if let Some(b) = blocks.get_mut(idx) {
                        b.text.drain(start..end);
                        b.raw.drain(start..end);
                    }
                });
                self.re_parse_block(idx, cx);
            }
        } else if idx + 1 < self.block_count(cx) {
            let next = self.text_of(idx + 1, cx);
            let fi = idx + 1;
            self.mutate(cx, |blocks| {
                if let Some(b) = blocks.get_mut(idx) {
                    b.text.push_str(&next);
                    b.raw.push_str(&next);
                }
                blocks.remove(fi);
            });
            self.re_parse_block(idx, cx);
            self.sync_block_views(cx);
        }
    }

    // -----------------------------------------------------------------------
    // Key dispatch
    // -----------------------------------------------------------------------

    fn handle_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.context_menu = None;

        let ks = &event.keystroke;
        let mods = &ks.modifiers;

        // Ctrl+Alt+V — toggle vim regardless of mode.
        if mods.control && mods.alt && ks.key == "v" {
            self.do_toggle_vim(cx);
            cx.notify();
            return;
        }

        // Ctrl-only shortcuts (no shift, no alt).
        if mods.control && !mods.alt {
            match ks.key.as_str() {
                "z" => {
                    self.selection_anchor = None;
                    if mods.shift { self.do_redo(cx); } else { self.do_undo(cx); }
                    self.push_cursor(cx);
                    cx.notify();
                    return;
                }
                "c" if !mods.shift => {
                    self.copy_selection(window, cx);
                    return;
                }
                "x" if !mods.shift => {
                    self.do_cut(window, cx);
                    self.push_cursor(cx);
                    cx.notify();
                    return;
                }
                "v" if !mods.shift => {
                    self.do_paste(cx);
                    self.push_cursor(cx);
                    cx.notify();
                    return;
                }
                "s" if !mods.shift => {
                    let has_path = self.document.read(cx).path.is_some();
                    if has_path {
                        self.do_save(cx);
                    } else {
                        self.do_save_as(window, cx);
                    }
                    cx.notify();
                    return;
                }
                "s" if mods.shift => {
                    self.do_save_as(window, cx);
                    cx.notify();
                    return;
                }
                "o" if !mods.shift => {
                    self.do_open_file(window, cx);
                    cx.notify();
                    return;
                }
                "a" if !mods.shift => {
                    self.do_select_all(cx);
                    self.push_cursor(cx);
                    cx.notify();
                    return;
                }
                _ => {}
            }
        }

        let key = keystroke_to_key(event);
        let action = self.modal.update(cx, |m, _| m.process_key(key));

        let is_insert = self.modal.read(cx).is_insert_like();
        if is_insert {
            self.exec_insert(action, window, cx);
        } else {
            self.exec_normal(action, window, cx);
        }

        self.push_cursor(cx);
        cx.notify();
    }

    // -----------------------------------------------------------------------
    // Insert actions
    // -----------------------------------------------------------------------

    fn exec_insert(&mut self, action: ModalAction, window: &Window, cx: &mut Context<Self>) {
        match action {
            ModalAction::InsertChar(c) => {
                if self.has_selection() { self.delete_selection_text(cx); }
                self.insert_char(c, cx);
            }
            ModalAction::Backspace => {
                if self.has_selection() {
                    self.delete_selection_text(cx);
                } else {
                    self.handle_backspace(cx);
                }
            }
            ModalAction::DeleteForward => {
                if self.has_selection() {
                    self.delete_selection_text(cx);
                } else {
                    self.delete_forward(cx);
                }
            }
            ModalAction::SelectAll => self.do_select_all(cx),
            ModalAction::SplitBlock => {
                if self.has_selection() { self.delete_selection_text(cx); }
                self.split_block(cx);
            }
            ModalAction::SplitBlockDialogue => {
                if self.has_selection() { self.delete_selection_text(cx); }
                self.split_block_as(FountainElement::Dialogue, cx);
            }
            ModalAction::CycleElementType => {
                self.selection_anchor = None;
                self.cycle_element(cx);
            }
            ModalAction::Undo => {
                self.selection_anchor = None;
                self.do_undo(cx);
            }
            ModalAction::Redo => {
                self.selection_anchor = None;
                self.do_redo(cx);
            }
            ModalAction::MoveCursor { motion, count } => {
                self.selection_anchor = None;
                self.apply_motion_insert(motion, count, window, cx);
            }
            ModalAction::SelectWordForward => {
                if self.selection_anchor.is_none() {
                    self.selection_anchor = Some((self.focused_block, self.cursor_char));
                }
                self.move_word_forward(cx);
            }
            ModalAction::SelectWordBackward => {
                if self.selection_anchor.is_none() {
                    self.selection_anchor = Some((self.focused_block, self.cursor_char));
                }
                self.move_word_back(cx);
            }
            ModalAction::EnterNormal | ModalAction::None => {
                self.selection_anchor = None;
            }
            _ => { self.selection_anchor = None; }
        }
    }

    fn apply_motion_insert(
        &mut self,
        motion: crate::ui::modal::CursorMove,
        count: usize,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        use crate::ui::modal::CursorMove::*;
        match motion {
            Up   => self.move_visual_up(count, true, window, cx),
            Down => self.move_visual_down(count, true, window, cx),
            _ => {
                for _ in 0..count {
                    match motion {
                        Left => { if self.cursor_char > 0 { self.cursor_char -= 1; } }
                        Right => {
                            let max = self.char_count(self.focused_block, cx);
                            if self.cursor_char < max { self.cursor_char += 1; }
                        }
                        LineStart => { self.cursor_char = 0; }
                        LineEnd => {
                            self.cursor_char = self.char_count(self.focused_block, cx);
                        }
                        FirstBlock => { self.focused_block = 0; self.cursor_char = 0; }
                        LastBlock => {
                            self.focused_block = self.block_count(cx).saturating_sub(1);
                            self.cursor_char = self.char_count(self.focused_block, cx);
                        }
                        WordForward => self.move_word_forward(cx),
                        WordBack => self.move_word_back(cx),
                        _ => {}
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Normal actions
    // -----------------------------------------------------------------------

    fn exec_normal(&mut self, action: ModalAction, window: &Window, cx: &mut Context<Self>) {
        self.selection_anchor = None;
        use crate::ui::modal::CursorMove::*;
        match action {
            ModalAction::MoveCursor { motion, count } => match motion {
                Up   => self.move_visual_up(count, false, window, cx),
                Down => self.move_visual_down(count, false, window, cx),
                _ => {
                    for _ in 0..count {
                        match motion {
                            Left => { if self.cursor_char > 0 { self.cursor_char -= 1; } }
                            Right => {
                                let max = self.char_count(self.focused_block, cx).saturating_sub(1);
                                if self.cursor_char < max { self.cursor_char += 1; }
                            }
                            LineStart => { self.cursor_char = 0; }
                            LineEnd => {
                                self.cursor_char =
                                    self.char_count(self.focused_block, cx).saturating_sub(1);
                            }
                            FirstBlock => { self.focused_block = 0; self.cursor_char = 0; }
                            LastBlock => {
                                self.focused_block = self.block_count(cx).saturating_sub(1);
                                self.cursor_char =
                                    self.char_count(self.focused_block, cx).saturating_sub(1);
                            }
                            WordForward => self.move_word_forward(cx),
                            WordBack => self.move_word_back(cx),
                            WordEnd => self.move_word_end(cx),
                            Up | Down => unreachable!(),
                        }
                    }
                }
            },
            ModalAction::EnterInsert(pt) => self.enter_insert_at(pt, cx),
            ModalAction::EnterVisual => {
                let anchor = self.cursor_char;
                self.modal.update(cx, |m, _| m.enter_visual(anchor));
            }
            ModalAction::DeleteCurrentBlock => self.delete_block(cx),
            ModalAction::YankCurrentBlock => {
                self.yank_buffer = Some(self.text_of(self.focused_block, cx));
            }
            ModalAction::PasteBelow => self.paste(true, cx),
            ModalAction::PasteAbove => self.paste(false, cx),
            ModalAction::Undo => self.do_undo(cx),
            ModalAction::Redo => self.do_redo(cx),
            ModalAction::SearchStub | ModalAction::None => {}
            _ => {}
        }
    }

    // -----------------------------------------------------------------------
    // Editing primitives
    // -----------------------------------------------------------------------

    fn insert_char(&mut self, c: char, cx: &mut Context<Self>) {
        let idx = self.focused_block;
        let byte_pos = {
            let text = self.text_of(idx, cx);
            text.char_indices().nth(self.cursor_char).map(|(b, _)| b).unwrap_or(text.len())
        };
        self.mutate(cx, |blocks| {
            if let Some(b) = blocks.get_mut(idx) {
                b.text.insert(byte_pos, c);
                b.raw.insert(byte_pos, c);
            }
        });
        self.cursor_char += 1;
        self.re_parse_block(idx, cx);
    }

    fn handle_backspace(&mut self, cx: &mut Context<Self>) {
        if self.cursor_char > 0 {
            let idx = self.focused_block;
            let byte_range = {
                let text = self.text_of(idx, cx);
                text.char_indices().nth(self.cursor_char - 1).map(|(b, c)| (b, b + c.len_utf8()))
            };
            if let Some((start, _end)) = byte_range {
                self.mutate(cx, |blocks| {
                    if let Some(b) = blocks.get_mut(idx) {
                        b.text.remove(start);
                        b.raw.remove(start);
                    }
                });
                self.cursor_char -= 1;
                self.re_parse_block(idx, cx);
            }
        } else if self.focused_block > 0 {
            // Merge into previous block.
            let prev = self.focused_block - 1;
            let prev_chars = self.char_count(prev, cx);
            let curr_text = self.text_of(self.focused_block, cx);
            let fi = self.focused_block;
            self.mutate(cx, |blocks| {
                if let Some(p) = blocks.get_mut(prev) {
                    p.text.push_str(&curr_text);
                    p.raw.push_str(&curr_text);
                }
                blocks.remove(fi);
            });
            self.focused_block = prev;
            self.cursor_char = prev_chars;
            self.re_parse_block(prev, cx);
            self.sync_block_views(cx);
        }
    }

    fn split_block(&mut self, cx: &mut Context<Self>) {
        let idx = self.focused_block;
        let (before, after) = {
            let text = self.text_of(idx, cx);
            let byte = text.char_indices().nth(self.cursor_char).map(|(b, _)| b).unwrap_or(text.len());
            (text[..byte].to_string(), text[byte..].to_string())
        };
        let current_element = self.document.read(cx).blocks.get(idx).map(|b| b.element.clone());
        let new_element = match current_element {
            Some(FountainElement::Character)
            | Some(FountainElement::Parenthetical) => FountainElement::Dialogue,
            _ => FountainElement::Action,
        };
        let new_block = Block::new(new_element, after.clone(), after);
        self.mutate(cx, |blocks| {
            if let Some(b) = blocks.get_mut(idx) {
                b.text = before.clone();
                b.raw = before;
            }
            blocks.insert(idx + 1, new_block);
        });
        self.focused_block = idx + 1;
        self.cursor_char = 0;
        self.re_parse_block(idx, cx);
        self.re_parse_block(idx + 1, cx);
        self.sync_block_views(cx);
    }

    fn split_block_as(&mut self, element: FountainElement, cx: &mut Context<Self>) {
        let idx = self.focused_block;
        let (before, after) = {
            let text = self.text_of(idx, cx);
            let byte = text.char_indices().nth(self.cursor_char).map(|(b, _)| b).unwrap_or(text.len());
            (text[..byte].to_string(), text[byte..].to_string())
        };
        let new_block = Block::new(element, after.clone(), after);
        self.mutate(cx, |blocks| {
            if let Some(b) = blocks.get_mut(idx) {
                b.text = before.clone();
                b.raw = before;
            }
            blocks.insert(idx + 1, new_block);
        });
        self.focused_block = idx + 1;
        self.cursor_char = 0;
        self.re_parse_block(idx, cx);
        self.sync_block_views(cx);
    }

    fn cycle_element(&mut self, cx: &mut Context<Self>) {
        let idx = self.focused_block;
        self.mutate(cx, |blocks| {
            if let Some(b) = blocks.get_mut(idx) {
                b.element = match b.element {
                    FountainElement::Action => FountainElement::Character,
                    FountainElement::Character => FountainElement::Dialogue,
                    FountainElement::Dialogue => FountainElement::Parenthetical,
                    FountainElement::Parenthetical => FountainElement::Action,
                    _ => FountainElement::Action,
                };
            }
        });
    }

    fn delete_block(&mut self, cx: &mut Context<Self>) {
        if self.block_count(cx) <= 1 {
            self.mutate(cx, |blocks| {
                if let Some(b) = blocks.get_mut(0) { b.text.clear(); b.raw.clear(); }
            });
            self.cursor_char = 0;
            return;
        }
        let idx = self.focused_block;
        self.mutate(cx, |blocks| { blocks.remove(idx); });
        self.focused_block = idx.saturating_sub(1).min(self.block_count(cx).saturating_sub(1));
        self.cursor_char = 0;
        self.sync_block_views(cx);
    }

    fn paste(&mut self, below: bool, cx: &mut Context<Self>) {
        let Some(text) = self.yank_buffer.clone() else { return; };
        let insert_at = if below { self.focused_block + 1 } else { self.focused_block };
        let new_block = Block::new(FountainElement::Action, text.clone(), text);
        self.mutate(cx, |blocks| blocks.insert(insert_at, new_block));
        self.focused_block = insert_at;
        self.cursor_char = 0;
        self.sync_block_views(cx);
    }

    fn enter_insert_at(&mut self, point: InsertPoint, cx: &mut Context<Self>) {
        match point {
            InsertPoint::AtCursor => {}
            InsertPoint::AtLineStart => { self.cursor_char = 0; }
            InsertPoint::AfterCursor => {
                let max = self.char_count(self.focused_block, cx);
                if self.cursor_char < max { self.cursor_char += 1; }
            }
            InsertPoint::AtLineEnd => {
                self.cursor_char = self.char_count(self.focused_block, cx);
            }
            InsertPoint::NewBlockBelow => {
                let idx = self.focused_block;
                let nb = Block::new(FountainElement::Action, String::new(), String::new());
                self.mutate(cx, |blocks| blocks.insert(idx + 1, nb));
                self.focused_block = idx + 1;
                self.cursor_char = 0;
                self.sync_block_views(cx);
            }
            InsertPoint::NewBlockAbove => {
                let idx = self.focused_block;
                let nb = Block::new(FountainElement::Action, String::new(), String::new());
                self.mutate(cx, |blocks| blocks.insert(idx, nb));
                self.cursor_char = 0;
                self.sync_block_views(cx);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Visual-line motions (j/k and arrow-up/down)
    // -----------------------------------------------------------------------

    fn move_visual_down(&mut self, count: usize, insert_mode: bool, window: &Window, cx: &mut Context<Self>) {
        for _ in 0..count {
            let idx = self.focused_block;
            let text = self.text_of(idx, cx);
            let Some(el) = self.document.read(cx).blocks.get(idx).map(|b| b.element.clone()) else { return; };
            let style = element_style(&el);
            let display = if style.uppercase { text.to_uppercase() } else { text };
            let wrap_w = px(700.0 - style.left_pad.as_f32() - style.right_pad.as_f32());

            let (cur_vline, cur_x) = visual_line_and_x(&display, self.cursor_char, &style, wrap_w, window);
            let total = count_visual_lines(&display, &style, wrap_w, window);

            if cur_vline + 1 < total {
                let new_char = char_at_visual_line_x(&display, &style, cur_vline + 1, cur_x, wrap_w, window);
                let max = if insert_mode { self.char_count(idx, cx) } else { self.char_count(idx, cx).saturating_sub(1) };
                self.cursor_char = new_char.min(max);
            } else if idx + 1 < self.block_count(cx) {
                self.focused_block += 1;
                let next_text = self.text_of(self.focused_block, cx);
                let Some(nel) = self.document.read(cx).blocks.get(self.focused_block).map(|b| b.element.clone()) else { return; };
                let ns = element_style(&nel);
                let next_display = if ns.uppercase { next_text.to_uppercase() } else { next_text };
                let nw = px(700.0 - ns.left_pad.as_f32() - ns.right_pad.as_f32());
                let new_char = char_at_visual_line_x(&next_display, &ns, 0, cur_x, nw, window);
                let max = if insert_mode { self.char_count(self.focused_block, cx) } else { self.char_count(self.focused_block, cx).saturating_sub(1) };
                self.cursor_char = new_char.min(max);
            }
        }
    }

    fn move_visual_up(&mut self, count: usize, insert_mode: bool, window: &Window, cx: &mut Context<Self>) {
        for _ in 0..count {
            let idx = self.focused_block;
            let text = self.text_of(idx, cx);
            let Some(el) = self.document.read(cx).blocks.get(idx).map(|b| b.element.clone()) else { return; };
            let style = element_style(&el);
            let display = if style.uppercase { text.to_uppercase() } else { text };
            let wrap_w = px(700.0 - style.left_pad.as_f32() - style.right_pad.as_f32());

            let (cur_vline, cur_x) = visual_line_and_x(&display, self.cursor_char, &style, wrap_w, window);

            if cur_vline > 0 {
                let new_char = char_at_visual_line_x(&display, &style, cur_vline - 1, cur_x, wrap_w, window);
                let max = if insert_mode { self.char_count(idx, cx) } else { self.char_count(idx, cx).saturating_sub(1) };
                self.cursor_char = new_char.min(max);
            } else if idx > 0 {
                self.focused_block -= 1;
                let prev_text = self.text_of(self.focused_block, cx);
                let Some(pel) = self.document.read(cx).blocks.get(self.focused_block).map(|b| b.element.clone()) else { return; };
                let ps = element_style(&pel);
                let prev_display = if ps.uppercase { prev_text.to_uppercase() } else { prev_text };
                let pw = px(700.0 - ps.left_pad.as_f32() - ps.right_pad.as_f32());
                let total_prev = count_visual_lines(&prev_display, &ps, pw, window);
                let new_char = char_at_visual_line_x(&prev_display, &ps, total_prev - 1, cur_x, pw, window);
                let max = if insert_mode { self.char_count(self.focused_block, cx) } else { self.char_count(self.focused_block, cx).saturating_sub(1) };
                self.cursor_char = new_char.min(max);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Word motions
    // -----------------------------------------------------------------------

    fn move_word_forward(&mut self, cx: &Context<Self>) {
        let chars: Vec<char> = self.text_of(self.focused_block, cx).chars().collect();
        let mut i = self.cursor_char;
        while i < chars.len() && !chars[i].is_whitespace() { i += 1; }
        while i < chars.len() && chars[i].is_whitespace() { i += 1; }
        self.cursor_char = i;
    }

    fn move_word_back(&mut self, cx: &Context<Self>) {
        let chars: Vec<char> = self.text_of(self.focused_block, cx).chars().collect();
        let mut i = self.cursor_char;
        if i == 0 { return; }
        i -= 1;
        while i > 0 && chars[i].is_whitespace() { i -= 1; }
        while i > 0 && !chars[i - 1].is_whitespace() { i -= 1; }
        self.cursor_char = i;
    }

    fn move_word_end(&mut self, cx: &Context<Self>) {
        let chars: Vec<char> = self.text_of(self.focused_block, cx).chars().collect();
        let mut i = self.cursor_char;
        if i + 1 >= chars.len() { return; }
        i += 1;
        while i < chars.len() && chars[i].is_whitespace() { i += 1; }
        while i + 1 < chars.len() && !chars[i + 1].is_whitespace() { i += 1; }
        self.cursor_char = i;
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for DocumentView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status: SharedString = self.status_bar_text(cx).into();
        let block_views = self.block_views.clone();
        let context_menu = self.context_menu;
        let sidebar_open = self.sidebar_open;

        let mut root = div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(0x1c1c1c))
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_key))
            .on_mouse_down(MouseButton::Right, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                this.context_menu = Some(event.position);
                this.sidebar_open = false;
                cx.notify();
            }))
            .child(
                div()
                    .id("scroll-area")
                    .flex_1()
                    .w_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll_handle)
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .justify_center()
                            .pt(px(40.0))
                            .pb(px(40.0))
                            .child(
                                {
                                    let bvs_snap = block_views.clone();
                                    div()
                                        .w(px(700.0))
                                        .min_h(px(1056.0))
                                        .bg(rgb(0x252525))
                                        .rounded(px(5.0))
                                        .shadow(vec![
                                            gpui::BoxShadow {
                                                color: hsla(0.0, 0.0, 0.0, 0.20),
                                                offset: gpui::point(px(0.0), px(6.0)),
                                                blur_radius: px(24.0),
                                                spread_radius: px(0.0),
                                                inset: false,
                                            },
                                            gpui::BoxShadow {
                                                color: hsla(0.0, 0.0, 0.0, 0.10),
                                                offset: gpui::point(px(0.0), px(1.0)),
                                                blur_radius: px(4.0),
                                                spread_radius: px(0.0),
                                                inset: false,
                                            },
                                        ])
                                        .pt(px(80.0))
                                        .pb(px(80.0))
                                        .flex()
                                        .flex_col()
                                        .on_children_prepainted(move |bounds_list, _win, cx| {
                                            for (i, bounds) in bounds_list.iter().enumerate() {
                                                if let Some(bv) = bvs_snap.get(i) {
                                                    let y = bounds.origin.y.as_f32();
                                                    bv.update(cx, |v, _| v.top_y = y);
                                                }
                                            }
                                        })
                                        .children(block_views)
                                },
                            ),
                    ),
            )
            .child(
                div()
                    .h(px(28.0))
                    .w_full()
                    .bg(rgb(0x111111))
                    .flex()
                    .items_center()
                    .px(px(12.0))
                    .child(
                        div()
                            .text_color(hsla(0.0, 0.0, 0.55, 1.0))
                            .text_size(px(11.0))
                            .child(status),
                    ),
            );

        // Hamburger button — always visible, top-left corner.
        root = root.child(
            div()
                .absolute()
                .top(px(12.0))
                .left(px(14.0))
                .w(px(32.0))
                .h(px(32.0))
                .flex()
                .flex_col()
                .justify_center()
                .items_center()
                .gap(px(5.0))
                .cursor_pointer()
                .rounded_md()
                .hover(|s| s.bg(rgb(0x3a3a3a)))
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.sidebar_open = !this.sidebar_open;
                    this.context_menu = None;
                    cx.notify();
                    cx.stop_propagation();
                }))
                .child(div().w(px(18.0)).h(px(2.0)).bg(rgb(0xaaaaaa)))
                .child(div().w(px(18.0)).h(px(2.0)).bg(rgb(0xaaaaaa)))
                .child(div().w(px(18.0)).h(px(2.0)).bg(rgb(0xaaaaaa))),
        );

        // Sidebar panel.
        if sidebar_open {
            // Dismiss overlay behind the sidebar.
            root = root.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.sidebar_open = false;
                        cx.notify();
                        cx.stop_propagation();
                    })),
            );
            root = root.child(self.render_sidebar(cx));
        }

        if let Some(pos) = context_menu {
            // Full-screen transparent overlay: any click outside menu dismisses it.
            root = root.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.context_menu = None;
                        cx.notify();
                        cx.stop_propagation();
                    }))
                    .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| {
                        this.context_menu = None;
                        cx.notify();
                        cx.stop_propagation();
                    })),
            );
            root = root.child(self.render_context_menu(pos, cx));
        }

        root
    }
}

impl DocumentView {
    fn render_sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let vim_enabled = self.modal.read(cx).enabled;
        let item_style = |d: Div| {
            d.px(px(16.0))
                .py(px(10.0))
                .text_size(px(13.0))
                .text_color(hsla(0.0, 0.0, 0.85, 1.0))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(0x383838)))
        };

        div()
            .absolute()
            .top_0()
            .left_0()
            .w(px(220.0))
            .h_full()
            .bg(rgb(0x222222))
            .border_r_1()
            .border_color(rgb(0x3a3a3a))
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| {
                cx.stop_propagation();
            }))
            // Header
            .child(
                div()
                    .h(px(52.0))
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(rgb(0x3a3a3a))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(hsla(0.0, 0.0, 0.6, 1.0))
                            .child("Menu"),
                    ),
            )
            // New Script item
            .child(
                item_style(div())
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                        this.do_new_script(window, cx);
                        cx.stop_propagation();
                    }))
                    .child(sidebar_icon("icons/new.svg"))
                    .child(div().child("New Script")),
            )
            // Open File item
            .child(
                item_style(div())
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                        this.do_open_file(window, cx);
                        cx.notify();
                        cx.stop_propagation();
                    }))
                    .child(sidebar_icon("icons/open.svg"))
                    .child(div().child("Open File…")),
            )
            // Save item
            .child(
                item_style(div())
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                        let has_path = this.document.read(cx).path.is_some();
                        if has_path { this.do_save(cx); } else { this.do_save_as(window, cx); }
                        cx.notify();
                        cx.stop_propagation();
                    }))
                    .child(sidebar_icon("icons/save.svg"))
                    .child(div().child("Save")),
            )
            // Save As item
            .child(
                item_style(div())
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                        this.do_save_as(window, cx);
                        cx.notify();
                        cx.stop_propagation();
                    }))
                    .child(sidebar_icon("icons/save.svg"))
                    .child(div().child("Save As\u{2026}")),
            )
            .child(div().h(px(1.0)).mx(px(12.0)).my(px(4.0)).bg(rgb(0x3a3a3a)))
            .child(
                item_style(div())
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.do_toggle_vim(cx);
                        this.sidebar_open = false;
                        cx.notify();
                        cx.stop_propagation();
                    }))
                    .child(
                        div()
                            .w(px(16.0))
                            .h(px(16.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(if vim_enabled {
                                hsla(0.0, 0.0, 0.85, 1.0)
                            } else {
                                hsla(0.0, 0.0, 0.0, 0.0)
                            })
                            .child("\u{2713}"),
                    )
                    .child(div().child("Vim Mode")),
            )
    }

    fn render_context_menu(
        &mut self,
        pos: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let row_style = |d: Div| {
            d.px(px(14.0))
                .py(px(5.0))
                .text_size(px(12.0))
                .text_color(hsla(0.0, 0.0, 0.85, 1.0))
                .hover(|s| s.bg(rgb(0x3a3a3a)))
                .cursor_pointer()
        };

        div()
            .absolute()
            .left(pos.x)
            .top(pos.y)
            .w(px(160.0))
            .bg(rgb(0x2a2a2a))
            .border_1()
            .border_color(rgb(0x484848))
            .rounded_md()
            .shadow_lg()
            .flex()
            .flex_col()
            .py(px(4.0))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| {
                cx.stop_propagation();
            }))
            .child(
                row_style(div())
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                        this.copy_selection(window, cx);
                        this.context_menu = None;
                        cx.notify();
                    }))
                    .child("Copy        Ctrl+C"),
            )
            .child(
                row_style(div())
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                        this.do_cut(window, cx);
                        this.context_menu = None;
                        this.push_cursor(cx);
                        cx.notify();
                    }))
                    .child("Cut         Ctrl+X"),
            )
            .child(
                row_style(div())
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.do_paste(cx);
                        this.context_menu = None;
                        this.push_cursor(cx);
                        cx.notify();
                    }))
                    .child("Paste       Ctrl+V"),
            )
            .child(div().h(px(1.0)).mx(px(8.0)).my(px(3.0)).bg(rgb(0x444444)))
            .child(
                row_style(div())
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.do_select_all(cx);
                        this.context_menu = None;
                        this.push_cursor(cx);
                        cx.notify();
                    }))
                    .child("Select All  Ctrl+A"),
            )
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn char_to_byte(text: &str, char_idx: usize) -> usize {
    text.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(text.len())
}

fn sidebar_icon(path: &'static str) -> Svg {
    svg()
        .path(path)
        .w(px(16.0))
        .h(px(16.0))
        .text_color(hsla(0.0, 0.0, 0.55, 1.0))
}

// ---------------------------------------------------------------------------
// Keystroke → Key
// ---------------------------------------------------------------------------

fn keystroke_to_key(event: &KeyDownEvent) -> Key {
    let ks = &event.keystroke;
    let mods = &ks.modifiers;

    if mods.control && !mods.shift && !mods.alt && ks.key == "r" {
        return Key::CtrlR;
    }

    match ks.key.as_str() {
        "escape" => return Key::Escape,
        "enter" | "return" => {
            if mods.shift { return Key::ShiftEnter; }
            return Key::Enter;
        }
        "backspace" => return Key::Backspace,
        "delete" => return Key::Delete,
        "home" => return Key::Home,
        "end" => return Key::End,
        "tab" => return Key::Tab,
        "left" => {
            if mods.control && mods.shift { return Key::CtrlShiftArrowLeft; }
            if mods.control { return Key::CtrlArrowLeft; }
            return Key::ArrowLeft;
        }
        "right" => {
            if mods.control && mods.shift { return Key::CtrlShiftArrowRight; }
            if mods.control { return Key::CtrlArrowRight; }
            return Key::ArrowRight;
        }
        "up" => return Key::ArrowUp,
        "down" => return Key::ArrowDown,
        _ => {}
    }

    if let Some(ch) = ks.key_char.as_deref().and_then(|s| s.chars().next()) {
        return Key::Char(ch);
    }
    if let Some(ch) = ks.key.chars().next().filter(|_| ks.key.len() == 1) {
        return Key::Char(ch);
    }

    Key::Char('\x00')
}
