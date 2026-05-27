//! Application state for the ratatui port.
//!
//! Everything that the view layer needs to render a frame, and that
//! the event loop needs to advance the state machine, lives here.
//! Subprocess calls (anything that talks to `tmux`) live in
//! [`crate::app`] — keeping them out of this module makes the state
//! machine straightforward to reason about and unit-test.

use ratatui::layout::{Position, Rect};

use crate::conf_setup::Directive;
use crate::models::Session;

/// Five clickable buttons across the bottom of the main view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonId {
    New,
    Attach,
    Detach,
    Quit,
    Delete,
}

pub const BUTTON_ORDER: [ButtonId; 5] = [
    ButtonId::New,
    ButtonId::Attach,
    ButtonId::Detach,
    ButtonId::Quit,
    ButtonId::Delete,
];

impl ButtonId {
    pub fn index(self) -> usize {
        BUTTON_ORDER.iter().position(|&b| b == self).unwrap()
    }

    pub fn label(self) -> &'static str {
        match self {
            ButtonId::New => "New (n)",
            ButtonId::Attach => "Attach (a)",
            ButtonId::Detach => "Detach (d)",
            ButtonId::Quit => "Quit (q)",
            ButtonId::Delete => "Delete (del)",
        }
    }

    pub fn next(self) -> Self {
        BUTTON_ORDER[(self.index() + 1) % BUTTON_ORDER.len()]
    }

    pub fn prev(self) -> Self {
        let i = self.index();
        let last = BUTTON_ORDER.len() - 1;
        BUTTON_ORDER[if i == 0 { last } else { i - 1 }]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    #[default]
    List,
    Button(ButtonId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeleteFocus {
    #[default]
    Cancel,
    Confirm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConfFocus {
    #[default]
    Yes,
    Later,
}

/// What top-level screen the user is currently looking at. Modals are
/// modeled as their own variants so they own the data (directive list
/// or target session name) they need.
#[derive(Debug, Clone, Default)]
pub enum Screen {
    #[default]
    Main,
    ConfirmDelete {
        name: String,
        focus: DeleteFocus,
    },
    ConfSetup {
        directives: Vec<Directive>,
        focus: ConfFocus,
    },
}

/// Anything the mouse can land on. Stored under `state.hover` and
/// `state.pressed` so the view can highlight the right widget on the
/// next render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitTarget {
    Button(ButtonId),
    ListRow(usize),
    ModalPrimary,
    ModalSecondary,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StatusKind {
    #[default]
    Info,
    Error,
}

/// Geometry computed during the most recent render — we read this
/// back when the user clicks or moves the mouse so the event handler
/// knows where every widget lives.
#[derive(Debug, Default)]
pub struct Geometry {
    pub buttons: [Option<Rect>; 5],
    pub list_inner: Option<Rect>,
    pub modal_primary: Option<Rect>,
    pub modal_secondary: Option<Rect>,
}

#[derive(Debug, Default)]
pub struct AppState {
    pub inside_tmux: bool,
    pub sessions: Vec<Session>,
    pub selected: usize,
    pub list_offset: usize,

    pub screen: Screen,
    pub focus: Focus,
    pub hover: Option<HitTarget>,
    pub pressed: Option<HitTarget>,

    pub geom: Geometry,

    pub status_message: String,
    pub status_kind: StatusKind,

    pub should_exit: bool,
    pub post_exit_argv: Option<Vec<String>>,
}

impl AppState {
    pub fn new(inside_tmux: bool) -> Self {
        Self {
            inside_tmux,
            status_message: initial_status(inside_tmux).to_string(),
            ..Default::default()
        }
    }

    /// Refresh from a freshly-pulled session list, preserving the
    /// selection by name (not by index) so a session disappearing in
    /// the middle doesn't make the cursor jump.
    pub fn set_sessions(&mut self, sessions: Vec<Session>) {
        let previously_selected = self.selected_name().map(str::to_owned);
        self.sessions = sessions;
        self.selected = match previously_selected {
            Some(name) => self
                .sessions
                .iter()
                .position(|s| s.name == name)
                .unwrap_or(0),
            None => 0,
        };
        if self.selected >= self.sessions.len() {
            self.selected = self.sessions.len().saturating_sub(1);
        }
    }

    pub fn selected_name(&self) -> Option<&str> {
        self.sessions.get(self.selected).map(|s| s.name.as_str())
    }

    pub fn move_selection(&mut self, delta: i32) {
        if self.sessions.is_empty() {
            return;
        }
        let len = self.sessions.len() as i32;
        let mut idx = self.selected as i32 + delta;
        if idx < 0 {
            idx = len - 1;
        } else if idx >= len {
            idx = 0;
        }
        self.selected = idx as usize;
    }

    pub fn set_status(&mut self, message: impl Into<String>, kind: StatusKind) {
        self.status_message = message.into();
        self.status_kind = kind;
    }

    pub fn set_info(&mut self, message: impl Into<String>) {
        self.set_status(message, StatusKind::Info);
    }

    pub fn set_error(&mut self, message: impl Into<String>) {
        self.set_status(message, StatusKind::Error);
    }

    pub fn focus_next(&mut self) {
        self.focus = match self.focus {
            Focus::List => Focus::Button(ButtonId::New),
            Focus::Button(b) => {
                let nxt = b.next();
                if nxt == BUTTON_ORDER[0] {
                    Focus::List
                } else {
                    Focus::Button(nxt)
                }
            }
        };
    }

    pub fn focus_prev(&mut self) {
        self.focus = match self.focus {
            Focus::List => Focus::Button(*BUTTON_ORDER.last().unwrap()),
            Focus::Button(b) => {
                if b == BUTTON_ORDER[0] {
                    Focus::List
                } else {
                    Focus::Button(b.prev())
                }
            }
        };
    }

    /// True iff at least one session is available — used to grey out
    /// Attach / Detach / Delete in the view.
    pub fn has_sessions(&self) -> bool {
        !self.sessions.is_empty()
    }

    /// Resolve a screen-space position into the widget under it.
    /// Buttons take precedence over the list (they sit "in front").
    pub fn hit_test(&self, col: u16, row: u16) -> Option<HitTarget> {
        let pos = Position::new(col, row);

        // When a modal is up only its buttons accept hits.
        if !matches!(self.screen, Screen::Main) {
            if let Some(r) = self.geom.modal_primary {
                if r.contains(pos) {
                    return Some(HitTarget::ModalPrimary);
                }
            }
            if let Some(r) = self.geom.modal_secondary {
                if r.contains(pos) {
                    return Some(HitTarget::ModalSecondary);
                }
            }
            return None;
        }

        for (i, rect) in self.geom.buttons.iter().enumerate() {
            if let Some(r) = rect {
                if r.contains(pos) {
                    return Some(HitTarget::Button(BUTTON_ORDER[i]));
                }
            }
        }

        if let Some(inner) = self.geom.list_inner {
            if inner.contains(pos) && row >= inner.y {
                let visible_index = (row - inner.y) as usize;
                let absolute = visible_index + self.list_offset;
                if absolute < self.sessions.len() {
                    return Some(HitTarget::ListRow(absolute));
                }
            }
        }

        None
    }

    pub fn clear_pressed(&mut self) {
        self.pressed = None;
    }

    pub fn ensure_list_offset(&mut self, visible_rows: u16) {
        if visible_rows == 0 || self.sessions.is_empty() {
            self.list_offset = 0;
            return;
        }
        let visible = visible_rows as usize;
        if self.selected < self.list_offset {
            self.list_offset = self.selected;
        } else if self.selected >= self.list_offset + visible {
            self.list_offset = self.selected + 1 - visible;
        }
        let max_offset = self.sessions.len().saturating_sub(visible);
        if self.list_offset > max_offset {
            self.list_offset = max_offset;
        }
    }
}

pub fn initial_status(inside_tmux: bool) -> &'static str {
    if inside_tmux {
        "Inside tmux · Detach (d) returns to the parent shell"
    } else {
        "Outside tmux · selecting a session hands the terminal over to tmux"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(name: &str) -> Session {
        Session {
            name: name.into(),
            windows: 1,
            attached: false,
        }
    }

    #[test]
    fn focus_cycles_through_buttons_and_list() {
        let mut st = AppState::new(false);
        assert_eq!(st.focus, Focus::List);
        st.focus_next();
        assert_eq!(st.focus, Focus::Button(ButtonId::New));
        st.focus_next();
        assert_eq!(st.focus, Focus::Button(ButtonId::Attach));
        for _ in 0..3 {
            st.focus_next();
        }
        assert_eq!(st.focus, Focus::Button(ButtonId::Delete));
        st.focus_next();
        assert_eq!(st.focus, Focus::List);
    }

    #[test]
    fn focus_prev_is_inverse() {
        let mut st = AppState::new(false);
        st.focus_prev();
        assert_eq!(st.focus, Focus::Button(ButtonId::Delete));
        st.focus_prev();
        assert_eq!(st.focus, Focus::Button(ButtonId::Quit));
    }

    #[test]
    fn set_sessions_keeps_selection_by_name() {
        let mut st = AppState::new(false);
        st.set_sessions(vec![s("a"), s("b"), s("c")]);
        st.selected = 1; // "b"
        st.set_sessions(vec![s("a"), s("c"), s("b")]); // "b" moved
        assert_eq!(st.selected_name(), Some("b"));
    }

    #[test]
    fn set_sessions_clamps_when_selection_disappears() {
        let mut st = AppState::new(false);
        st.set_sessions(vec![s("a"), s("b"), s("c")]);
        st.selected = 2;
        st.set_sessions(vec![s("a")]);
        assert_eq!(st.selected, 0);
    }

    #[test]
    fn move_selection_wraps() {
        let mut st = AppState::new(false);
        st.set_sessions(vec![s("a"), s("b"), s("c")]);
        st.move_selection(-1);
        assert_eq!(st.selected, 2);
        st.move_selection(1);
        assert_eq!(st.selected, 0);
    }

    #[test]
    fn hit_test_prefers_button_over_list_when_overlapping() {
        let mut st = AppState::new(false);
        st.set_sessions(vec![s("a")]);
        st.geom.buttons[0] = Some(Rect::new(0, 0, 10, 3));
        st.geom.list_inner = Some(Rect::new(0, 0, 10, 3));
        assert_eq!(st.hit_test(2, 1), Some(HitTarget::Button(ButtonId::New)));
    }

    #[test]
    fn hit_test_on_list_row_uses_offset() {
        let mut st = AppState::new(false);
        st.set_sessions(vec![s("a"), s("b"), s("c"), s("d")]);
        st.list_offset = 1; // "b" is the first visible row
        st.geom.list_inner = Some(Rect::new(0, 5, 20, 3));
        assert_eq!(st.hit_test(2, 5), Some(HitTarget::ListRow(1)));
        assert_eq!(st.hit_test(2, 6), Some(HitTarget::ListRow(2)));
        assert_eq!(st.hit_test(2, 99), None);
    }

    #[test]
    fn hit_test_returns_only_modal_buttons_when_modal_is_open() {
        let mut st = AppState::new(false);
        st.screen = Screen::ConfirmDelete {
            name: "x".into(),
            focus: DeleteFocus::default(),
        };
        st.geom.buttons[0] = Some(Rect::new(0, 0, 10, 3));
        st.geom.modal_primary = Some(Rect::new(20, 10, 8, 3));
        st.geom.modal_secondary = Some(Rect::new(30, 10, 10, 3));

        // Even though the New button is technically at (0,0), it
        // shouldn't catch hits while a modal is open.
        assert_eq!(st.hit_test(2, 1), None);
        assert_eq!(st.hit_test(22, 11), Some(HitTarget::ModalPrimary));
        assert_eq!(st.hit_test(32, 11), Some(HitTarget::ModalSecondary));
    }

    #[test]
    fn ensure_list_offset_scrolls_into_view() {
        let mut st = AppState::new(false);
        st.set_sessions((0..20).map(|i| s(&format!("s{i}"))).collect());
        st.selected = 15;
        st.ensure_list_offset(5);
        // selected (15) must be the last visible row -> offset 11
        assert_eq!(st.list_offset, 11);

        st.selected = 2;
        st.ensure_list_offset(5);
        assert_eq!(st.list_offset, 2);
    }
}
