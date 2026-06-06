use gpui::*;

use crate::model::{Block, Document, FountainElement};
use crate::parser::parse_block;
use crate::ui::block_view::BlockView;
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
            self.block_views.push(cx.new(move |cx| BlockView::new(idx, doc, modal, cx)));
        }
        self.block_views.truncate(n);
        for (i, bv) in self.block_views.iter().enumerate() {
            bv.update(cx, |v, _| v.block_index = i);
        }
    }

    fn push_cursor(&self, cx: &mut Context<Self>) {
        for (i, bv) in self.block_views.iter().enumerate() {
            let focused = i == self.focused_block;
            let cur = if focused { self.cursor_char } else { 0 };
            bv.update(cx, |v, _| { v.focused = focused; v.cursor_char = cur; });
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
        let prev_element = self.document.read(cx).blocks
            .get(idx.saturating_sub(1))
            .filter(|_| idx > 0)
            .map(|b| b.element.clone());

        let (mut element, _) = parse_block(&text);

        // Context rule: a plain line following CHARACTER, DIALOGUE, or PARENTHETICAL
        // is DIALOGUE unless it was explicitly forced to something else.
        if element == FountainElement::Action {
            let in_dialogue_context = matches!(
                prev_element,
                Some(FountainElement::Character)
                    | Some(FountainElement::Dialogue)
                    | Some(FountainElement::Parenthetical)
            );
            if in_dialogue_context {
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

    // -----------------------------------------------------------------------
    // Key dispatch
    // -----------------------------------------------------------------------

    fn handle_key(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let ks = &event.keystroke;
        let mods = &ks.modifiers;

        // Ctrl+Alt+V — toggle vim regardless of mode.
        if mods.control && mods.alt && ks.key == "v" {
            self.modal.update(cx, |m, _| m.toggle_vim());
            cx.notify();
            return;
        }

        // Ctrl+Z / Ctrl+Shift+Z — undo/redo (insert mode shortcut, bypasses modal).
        if mods.control && !mods.alt && ks.key == "z" {
            if mods.shift { self.do_redo(cx); } else { self.do_undo(cx); }
            self.push_cursor(cx);
            cx.notify();
            return;
        }

        let key = keystroke_to_key(event);
        let action = self.modal.update(cx, |m, _| m.process_key(key));

        let is_insert = self.modal.read(cx).is_insert_like();
        if is_insert {
            self.exec_insert(action, cx);
        } else {
            self.exec_normal(action, cx);
        }

        self.push_cursor(cx);
        cx.notify();
    }

    // -----------------------------------------------------------------------
    // Insert actions
    // -----------------------------------------------------------------------

    fn exec_insert(&mut self, action: ModalAction, cx: &mut Context<Self>) {
        match action {
            ModalAction::InsertChar(c) => self.insert_char(c, cx),
            ModalAction::Backspace => self.handle_backspace(cx),
            ModalAction::SplitBlock => self.split_block(cx),
            ModalAction::CycleElementType => self.cycle_element(cx),
            ModalAction::Undo => self.do_undo(cx),
            ModalAction::Redo => self.do_redo(cx),
            ModalAction::MoveCursor { motion, count } => {
                self.apply_motion_insert(motion, count, cx);
            }
            ModalAction::EnterNormal | ModalAction::None => {}
            _ => {}
        }
    }

    fn apply_motion_insert(
        &mut self,
        motion: crate::ui::modal::CursorMove,
        count: usize,
        cx: &mut Context<Self>,
    ) {
        use crate::ui::modal::CursorMove::*;
        for _ in 0..count {
            match motion {
                Left => { if self.cursor_char > 0 { self.cursor_char -= 1; } }
                Right => {
                    let max = self.char_count(self.focused_block, cx);
                    if self.cursor_char < max { self.cursor_char += 1; }
                }
                Up => {
                    if self.focused_block > 0 {
                        self.focused_block -= 1;
                        self.cursor_char =
                            self.cursor_char.min(self.char_count(self.focused_block, cx));
                    }
                }
                Down => {
                    if self.focused_block + 1 < self.block_count(cx) {
                        self.focused_block += 1;
                        self.cursor_char =
                            self.cursor_char.min(self.char_count(self.focused_block, cx));
                    }
                }
                _ => {}
            }
        }
    }

    // -----------------------------------------------------------------------
    // Normal actions
    // -----------------------------------------------------------------------

    fn exec_normal(&mut self, action: ModalAction, cx: &mut Context<Self>) {
        use crate::ui::modal::CursorMove::*;
        match action {
            ModalAction::MoveCursor { motion, count } => {
                for _ in 0..count {
                    match motion {
                        Left => { if self.cursor_char > 0 { self.cursor_char -= 1; } }
                        Right => {
                            let max = self.char_count(self.focused_block, cx).saturating_sub(1);
                            if self.cursor_char < max { self.cursor_char += 1; }
                        }
                        Up => {
                            if self.focused_block > 0 {
                                self.focused_block -= 1;
                                self.cursor_char = self.cursor_char.min(
                                    self.char_count(self.focused_block, cx).saturating_sub(1),
                                );
                            }
                        }
                        Down => {
                            if self.focused_block + 1 < self.block_count(cx) {
                                self.focused_block += 1;
                                self.cursor_char = self.cursor_char.min(
                                    self.char_count(self.focused_block, cx).saturating_sub(1),
                                );
                            }
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
                    }
                }
            }
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
            | Some(FountainElement::Dialogue)
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

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1c1c1c))
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_key))
            .child(
                div()
                    .id("scroll-area")
                    .flex_1()
                    .w_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll_handle)
                    // Centering row: pushes the fixed-width page to the horizontal centre
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .justify_center()
                            .pt(px(40.0))
                            .pb(px(40.0))
                            .child(
                                // Fixed-width page — 700px so block padding reliably clips text
                                div()
                                    .w(px(700.0))
                                    .min_h(px(1056.0))
                                    .bg(rgb(0x252525))
                                    .pt(px(80.0))
                                    .pb(px(80.0))
                                    .flex()
                                    .flex_col()
                                    .children(block_views),
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
            )
    }
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
        "enter" | "return" => return Key::Enter,
        "backspace" => return Key::Backspace,
        "tab" => return Key::Tab,
        "left" => return Key::Char('h'),
        "right" => return Key::Char('l'),
        "up" => return Key::Char('k'),
        "down" => return Key::Char('j'),
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
