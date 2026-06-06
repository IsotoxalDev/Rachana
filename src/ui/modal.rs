//! Modal editing state — Vim-style modes, toggled off by default.
//! No GPUI imports; this is pure state. The DocumentView maps GPUI key events
//! to Key variants and dispatches to process_key().

/// A key event reduced to what the modal layer cares about.
/// The DocumentView translates gpui::KeyDownEvent into this before calling process_key.
#[derive(Debug, Clone, PartialEq)]
pub enum Key {
    Char(char),
    Escape,
    Enter,
    Backspace,
    Tab,
    /// Ctrl+R (redo)
    CtrlR,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EditorMode {
    Normal,
    Insert,
    /// `anchor` is the byte offset where visual selection began.
    Visual { anchor: usize },
}

/// Where to place the cursor when entering Insert mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InsertPoint {
    AtCursor,
    AtLineStart,
    AfterCursor,
    AtLineEnd,
    NewBlockBelow,
    NewBlockAbove,
}

/// Cursor motions understood by the DocumentView.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CursorMove {
    Left,
    Right,
    /// Previous block (k)
    Up,
    /// Next block (j)
    Down,
    WordForward,
    WordBack,
    WordEnd,
    LineStart,
    LineEnd,
    FirstBlock,
    LastBlock,
}

/// High-level editing intent returned by process_key.
/// The DocumentView executes these against the Document.
#[derive(Debug, Clone, PartialEq)]
pub enum ModalAction {
    None,
    MoveCursor { motion: CursorMove, count: usize },
    EnterInsert(InsertPoint),
    /// Caller must follow up with enter_visual(cursor_pos) to set the anchor.
    EnterVisual,
    EnterNormal,
    InsertChar(char),
    /// Delete char before cursor, or merge with previous block if at start.
    Backspace,
    SplitBlock,
    CycleElementType,
    DeleteCurrentBlock,
    YankCurrentBlock,
    PasteBelow,
    PasteAbove,
    ExtendSelectionLeft,
    ExtendSelectionRight,
    YankSelection,
    DeleteSelection,
    Undo,
    Redo,
    /// Placeholder — search is not yet implemented.
    SearchStub,
}

#[derive(Debug, Clone)]
pub struct ModalState {
    pub mode: EditorMode,
    /// When false the editor behaves like a normal writing app; all keys are
    /// processed as if permanently in Insert mode.
    pub enabled: bool,
    /// Accumulated numeric prefix (e.g. the 3 in 3j).
    pub pending_count: Option<usize>,
    /// Single-char operator waiting for its second key (d→dd, y→yy, g→gg).
    pub pending_operator: Option<char>,
    /// Last yanked block text, available for p/P.
    pub yank_buffer: Option<String>,
}

impl Default for ModalState {
    fn default() -> Self {
        Self {
            mode: EditorMode::Insert,
            enabled: false,
            pending_count: None,
            pending_operator: None,
            yank_buffer: None,
        }
    }
}

impl ModalState {
    pub fn new() -> Self {
        Self::default()
    }

    /// True when the editor should accept typed text directly.
    pub fn is_insert_like(&self) -> bool {
        !self.enabled || self.mode == EditorMode::Insert
    }

    pub fn enter_normal(&mut self) {
        self.mode = EditorMode::Normal;
        self.pending_count = None;
        self.pending_operator = None;
    }

    pub fn enter_insert(&mut self) {
        self.mode = EditorMode::Insert;
        self.pending_count = None;
        self.pending_operator = None;
    }

    pub fn enter_visual(&mut self, anchor: usize) {
        self.mode = EditorMode::Visual { anchor };
        self.pending_count = None;
        self.pending_operator = None;
    }

    /// Bound to Ctrl+Alt+V. Enables vim mode (resetting to Normal) or disables it.
    pub fn toggle_vim(&mut self) {
        self.enabled = !self.enabled;
        if self.enabled {
            self.enter_normal();
        } else {
            self.mode = EditorMode::Insert;
            self.pending_count = None;
            self.pending_operator = None;
        }
    }

    /// Text shown in the status bar. None when vim mode is disabled.
    pub fn status_text(&self) -> Option<&'static str> {
        if !self.enabled {
            return None;
        }
        Some(match &self.mode {
            EditorMode::Normal => "-- NORMAL --",
            EditorMode::Insert => "-- INSERT --",
            EditorMode::Visual { .. } => "-- VISUAL --",
        })
    }

    /// Dispatch a key event. Returns the high-level action the DocumentView should perform.
    pub fn process_key(&mut self, key: Key) -> ModalAction {
        if !self.enabled {
            return self.process_insert_key(key);
        }
        match self.mode {
            EditorMode::Normal => self.process_normal_key(key),
            EditorMode::Insert => self.process_insert_key(key),
            EditorMode::Visual { .. } => self.process_visual_key(key),
        }
    }

    fn process_normal_key(&mut self, key: Key) -> ModalAction {
        let c = match key {
            Key::CtrlR => return ModalAction::Redo,
            Key::Escape => return ModalAction::None,
            Key::Char(c) => c,
            _ => return ModalAction::None,
        };

        // Digit → accumulate count prefix.
        // '0' is only a count digit when a count has already started; otherwise it's LineStart.
        if let Some(d) = c.to_digit(10) {
            if c != '0' || self.pending_count.is_some() {
                self.pending_count =
                    Some(self.pending_count.unwrap_or(0) * 10 + d as usize);
                return ModalAction::None;
            }
        }

        // Second key of a two-key sequence (dd / yy / gg).
        if let Some(op) = self.pending_operator.take() {
            let count = self.pending_count.take().unwrap_or(1);
            return match (op, c) {
                ('d', 'd') => ModalAction::DeleteCurrentBlock,
                ('y', 'y') => ModalAction::YankCurrentBlock,
                ('g', 'g') => ModalAction::MoveCursor {
                    motion: CursorMove::FirstBlock,
                    count,
                },
                _ => ModalAction::None,
            };
        }

        let count = self.pending_count.take().unwrap_or(1);

        match c {
            // Motion
            'h' => ModalAction::MoveCursor { motion: CursorMove::Left, count },
            'j' => ModalAction::MoveCursor { motion: CursorMove::Down, count },
            'k' => ModalAction::MoveCursor { motion: CursorMove::Up, count },
            'l' => ModalAction::MoveCursor { motion: CursorMove::Right, count },
            'w' => ModalAction::MoveCursor { motion: CursorMove::WordForward, count },
            'b' => ModalAction::MoveCursor { motion: CursorMove::WordBack, count },
            'e' => ModalAction::MoveCursor { motion: CursorMove::WordEnd, count },
            '0' => ModalAction::MoveCursor { motion: CursorMove::LineStart, count: 1 },
            '$' => ModalAction::MoveCursor { motion: CursorMove::LineEnd, count: 1 },
            'G' => ModalAction::MoveCursor { motion: CursorMove::LastBlock, count: 1 },

            // Insert entry points
            'i' => {
                self.enter_insert();
                ModalAction::EnterInsert(InsertPoint::AtCursor)
            }
            'I' => {
                self.enter_insert();
                ModalAction::EnterInsert(InsertPoint::AtLineStart)
            }
            'a' => {
                self.enter_insert();
                ModalAction::EnterInsert(InsertPoint::AfterCursor)
            }
            'A' => {
                self.enter_insert();
                ModalAction::EnterInsert(InsertPoint::AtLineEnd)
            }
            'o' => {
                self.enter_insert();
                ModalAction::EnterInsert(InsertPoint::NewBlockBelow)
            }
            'O' => {
                self.enter_insert();
                ModalAction::EnterInsert(InsertPoint::NewBlockAbove)
            }

            // Edit
            'p' => ModalAction::PasteBelow,
            'P' => ModalAction::PasteAbove,
            'u' => ModalAction::Undo,

            // Mode transitions
            'v' => ModalAction::EnterVisual,
            '/' => ModalAction::SearchStub,

            // First key of two-key sequence
            'd' | 'y' | 'g' => {
                self.pending_operator = Some(c);
                ModalAction::None
            }

            _ => ModalAction::None,
        }
    }

    fn process_insert_key(&mut self, key: Key) -> ModalAction {
        match key {
            Key::Escape => {
                if self.enabled {
                    self.enter_normal();
                    return ModalAction::EnterNormal;
                }
                ModalAction::None
            }
            Key::Enter => ModalAction::SplitBlock,
            Key::Tab => ModalAction::CycleElementType,
            Key::Backspace => ModalAction::Backspace,
            Key::Char(c) if !c.is_control() => ModalAction::InsertChar(c),
            _ => ModalAction::None,
        }
    }

    fn process_visual_key(&mut self, key: Key) -> ModalAction {
        match key {
            Key::Char('h') => ModalAction::ExtendSelectionLeft,
            Key::Char('l') => ModalAction::ExtendSelectionRight,
            Key::Char('y') => {
                self.enter_normal();
                ModalAction::YankSelection
            }
            Key::Char('d') => {
                self.enter_normal();
                ModalAction::DeleteSelection
            }
            Key::Escape => {
                self.enter_normal();
                ModalAction::EnterNormal
            }
            _ => ModalAction::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normal_state() -> ModalState {
        let mut s = ModalState::new();
        s.enabled = true;
        s.mode = EditorMode::Normal;
        s
    }

    #[test]
    fn disabled_always_insert_like() {
        let s = ModalState::new();
        assert!(s.is_insert_like());
    }

    #[test]
    fn toggle_enables_normal_mode() {
        let mut s = ModalState::new();
        s.toggle_vim();
        assert!(s.enabled);
        assert_eq!(s.mode, EditorMode::Normal);
        assert!(!s.is_insert_like());
    }

    #[test]
    fn toggle_twice_disables() {
        let mut s = ModalState::new();
        s.toggle_vim();
        s.toggle_vim();
        assert!(!s.enabled);
    }

    #[test]
    fn hjkl_motions() {
        let mut s = normal_state();
        assert_eq!(
            s.process_key(Key::Char('j')),
            ModalAction::MoveCursor { motion: CursorMove::Down, count: 1 }
        );
        assert_eq!(
            s.process_key(Key::Char('k')),
            ModalAction::MoveCursor { motion: CursorMove::Up, count: 1 }
        );
    }

    #[test]
    fn count_prefix() {
        let mut s = normal_state();
        s.process_key(Key::Char('3'));
        let action = s.process_key(Key::Char('j'));
        assert_eq!(action, ModalAction::MoveCursor { motion: CursorMove::Down, count: 3 });
        assert!(s.pending_count.is_none());
    }

    #[test]
    fn dd_deletes_block() {
        let mut s = normal_state();
        assert_eq!(s.process_key(Key::Char('d')), ModalAction::None);
        assert_eq!(s.process_key(Key::Char('d')), ModalAction::DeleteCurrentBlock);
    }

    #[test]
    fn yy_yanks_block() {
        let mut s = normal_state();
        s.process_key(Key::Char('y'));
        assert_eq!(s.process_key(Key::Char('y')), ModalAction::YankCurrentBlock);
    }

    #[test]
    fn gg_goes_to_first_block() {
        let mut s = normal_state();
        s.process_key(Key::Char('g'));
        assert_eq!(
            s.process_key(Key::Char('g')),
            ModalAction::MoveCursor { motion: CursorMove::FirstBlock, count: 1 }
        );
    }

    #[test]
    fn i_enters_insert() {
        let mut s = normal_state();
        let action = s.process_key(Key::Char('i'));
        assert_eq!(action, ModalAction::EnterInsert(InsertPoint::AtCursor));
        assert_eq!(s.mode, EditorMode::Insert);
    }

    #[test]
    fn esc_from_insert_returns_normal() {
        let mut s = normal_state();
        s.enter_insert();
        let action = s.process_key(Key::Escape);
        assert_eq!(action, ModalAction::EnterNormal);
        assert_eq!(s.mode, EditorMode::Normal);
    }

    #[test]
    fn insert_mode_routes_chars() {
        let mut s = normal_state();
        s.enter_insert();
        assert_eq!(s.process_key(Key::Char('x')), ModalAction::InsertChar('x'));
        assert_eq!(s.process_key(Key::Tab), ModalAction::CycleElementType);
        assert_eq!(s.process_key(Key::Enter), ModalAction::SplitBlock);
        assert_eq!(s.process_key(Key::Backspace), ModalAction::Backspace);
    }

    #[test]
    fn visual_yank_returns_to_normal() {
        let mut s = normal_state();
        s.enter_visual(0);
        let action = s.process_key(Key::Char('y'));
        assert_eq!(action, ModalAction::YankSelection);
        assert_eq!(s.mode, EditorMode::Normal);
    }

    #[test]
    fn status_text_reflects_mode() {
        let mut s = normal_state();
        assert_eq!(s.status_text(), Some("-- NORMAL --"));
        s.enter_insert();
        assert_eq!(s.status_text(), Some("-- INSERT --"));
        s.enter_visual(0);
        assert_eq!(s.status_text(), Some("-- VISUAL --"));
    }

    #[test]
    fn status_text_none_when_disabled() {
        let s = ModalState::new();
        assert_eq!(s.status_text(), None);
    }

    #[test]
    fn zero_is_line_start_without_count() {
        let mut s = normal_state();
        assert_eq!(
            s.process_key(Key::Char('0')),
            ModalAction::MoveCursor { motion: CursorMove::LineStart, count: 1 }
        );
    }

    #[test]
    fn zero_extends_count_when_started() {
        let mut s = normal_state();
        s.process_key(Key::Char('1'));
        s.process_key(Key::Char('0')); // 10
        let action = s.process_key(Key::Char('j'));
        assert_eq!(action, ModalAction::MoveCursor { motion: CursorMove::Down, count: 10 });
    }
}
