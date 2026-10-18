//! Application state for the ratatui port.
//!
//! Everything that the view layer needs to render a frame, and that
//! the event loop needs to advance the state machine, lives here.
//! Subprocess calls (anything that talks to `tmux`) live in
//! [`crate::app`] — keeping them out of this module makes the state
//! machine straightforward to reason about and unit-test.

use std::time::{Duration, Instant};

use ratatui::layout::{Position, Rect};

use crate::conf_setup::ConfItem;
use crate::models::Session;
use crate::update::UpdatePlan;

/// Two clicks on the *same* hit target within this window count as a
/// double-click. 450 ms matches macOS' default Finder threshold and
/// is long enough for keyboards-with-trackpads but short enough that
/// genuine separate clicks don't accidentally merge.
pub const DOUBLE_CLICK_THRESHOLD: Duration = Duration::from_millis(450);

/// Five clickable buttons across the bottom of the main view, plus
/// the Close chip in the window's top-right corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonId {
    Attach,
    New,
    Rename,
    Delete,
    Detach,
    /// Top-right `x`; not part of the bottom row.
    Close,
}

pub const BUTTON_COUNT: usize = 5;

/// Bottom row, left to right. Attach first: it is the default focus,
/// so ↑/↓ + Enter reads as "pick a session, attach".
pub const BUTTON_ORDER: [ButtonId; BUTTON_COUNT] = [
    ButtonId::Attach,
    ButtonId::New,
    ButtonId::Rename,
    ButtonId::Delete,
    ButtonId::Detach,
];

/// Tab / ← / → walk this ring: the row, then Close, then back.
const FOCUS_RING: [ButtonId; BUTTON_COUNT + 1] = [
    ButtonId::Attach,
    ButtonId::New,
    ButtonId::Rename,
    ButtonId::Delete,
    ButtonId::Detach,
    ButtonId::Close,
];

impl ButtonId {
    /// Position in the bottom row; `None` for Close.
    pub fn row_index(self) -> Option<usize> {
        BUTTON_ORDER.iter().position(|&b| b == self)
    }

    fn ring_index(self) -> usize {
        FOCUS_RING.iter().position(|&b| b == self).unwrap()
    }

    /// Widest form, `Word (key)`: the five row buttons need 74 cells.
    pub fn full_parts(self) -> (&'static str, &'static str, &'static str) {
        match self {
            ButtonId::Attach => ("Attach (", "a", ")"),
            ButtonId::New => ("New (", "n", ")"),
            ButtonId::Rename => ("Rename (", "r", ")"),
            ButtonId::Delete => ("Delete (", "Bksp", ")"),
            ButtonId::Detach => ("Detach (", "d", ")"),
            ButtonId::Close => ("Close (", "Esc", ")"),
        }
    }

    /// Word only, with the shortcut shown in place: the initial for
    /// letter keys, a short token after the word for Backspace. The
    /// row needs 56 cells this way.
    pub fn word_parts(self) -> (&'static str, &'static str, &'static str) {
        match self {
            ButtonId::Attach => ("", "A", "ttach"),
            ButtonId::New => ("", "N", "ew"),
            ButtonId::Rename => ("", "R", "ename"),
            ButtonId::Delete => ("Delete ", "Bksp", ""),
            ButtonId::Detach => ("", "D", "etach"),
            ButtonId::Close => ("Close ", "Esc", ""),
        }
    }

    /// Just the key, for the two narrowest tiers (32 / 22 cells).
    pub fn key_only(self) -> &'static str {
        match self {
            ButtonId::Attach => "A",
            ButtonId::New => "N",
            ButtonId::Rename => "R",
            ButtonId::Delete => "Bksp",
            ButtonId::Detach => "D",
            ButtonId::Close => "Esc",
        }
    }

    /// Plain text of the word tier, for tests.
    pub fn label(self) -> String {
        let (a, k, b) = self.word_parts();
        format!("{a}{k}{b}")
    }

    pub fn next(self) -> Self {
        FOCUS_RING[(self.ring_index() + 1) % FOCUS_RING.len()]
    }

    pub fn prev(self) -> Self {
        let i = self.ring_index();
        let last = FOCUS_RING.len() - 1;
        FOCUS_RING[if i == 0 { last } else { i - 1 }]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Button(ButtonId),
}

impl Default for Focus {
    /// Attach: ↑/↓ pick a session, Enter attaches.
    fn default() -> Self {
        Focus::Button(ButtonId::Attach)
    }
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
    /// Text-entry modal for `tmux rename-session`. `input` starts as
    /// the current name so a small edit is a few keystrokes.
    Rename {
        original: String,
        input: String,
    },
    /// "A newer tu exists" modal. Confirming stores the upgrade command
    /// in `post_exit_command`; `main` runs it after the TUI is torn down.
    Update {
        plan: UpdatePlan,
    },
    ConfSetup {
        items: Vec<ConfItem>,
        focus: ConfFocus,
    },
    /// Single-button acknowledge dialog shown after a successful
    /// `~/.tmux.conf` patch. Closing it exits `tu` so the user's
    /// next shell sees the freshly-applied tmux options.
    RestartNotice {
        message: String,
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
    /// The "vX.Y.Z available" note on the status row.
    UpdateNotice,
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
    pub buttons: [Option<Rect>; BUTTON_COUNT],
    pub list_inner: Option<Rect>,
    pub modal_primary: Option<Rect>,
    pub modal_secondary: Option<Rect>,
    pub update_notice: Option<Rect>,
    /// The `x` chip in the window's top border.
    pub close: Option<Rect>,
}

/// Remembers the previous click so the next one can be classified as
/// a double-click. Cleared on every double so a third click in a row
/// isn't itself doubled with the second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClickRecord {
    pub target: HitTarget,
    pub at: Instant,
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
    pub last_click: Option<ClickRecord>,

    pub geom: Geometry,

    pub status_message: String,
    pub status_kind: StatusKind,
    /// "vX.Y.Z available · how to upgrade", shown right-aligned on the
    /// status row. Filled from the update-check cache at startup.
    pub update_notice: Option<String>,
    /// The plan behind `update_notice`, for the Update modal.
    pub update_plan: Option<UpdatePlan>,

    pub should_exit: bool,
    pub post_exit_argv: Option<Vec<String>>,
    /// Shell command `main` runs after the TUI exits (the upgrade).
    pub post_exit_command: Option<String>,
}

impl AppState {
    pub fn new(inside_tmux: bool) -> Self {
        Self {
            inside_tmux,
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
        let Focus::Button(b) = self.focus;
        self.focus = Focus::Button(b.next());
    }

    pub fn focus_prev(&mut self) {
        let Focus::Button(b) = self.focus;
        self.focus = Focus::Button(b.prev());
    }

    /// Back to the default action after the selection changed.
    pub fn focus_attach(&mut self) {
        self.focus = Focus::Button(ButtonId::Attach);
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
        if let Some(r) = self.geom.update_notice {
            if r.contains(pos) {
                return Some(HitTarget::UpdateNotice);
            }
        }
        if let Some(r) = self.geom.close {
            if r.contains(pos) {
                return Some(HitTarget::Button(ButtonId::Close));
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

    /// Record a click and report whether it forms a double-click with
    /// the previous one. On a confirmed double-click we forget the
    /// history so a third click doesn't get re-paired with the second.
    pub fn detect_and_record_click(&mut self, target: HitTarget, now: Instant) -> bool {
        let is_double = self
            .last_click
            .map(|c| c.target == target && now.duration_since(c.at) < DOUBLE_CLICK_THRESHOLD)
            .unwrap_or(false);
        if is_double {
            self.last_click = None;
        } else {
            self.last_click = Some(ClickRecord { target, at: now });
        }
        is_double
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
    fn focus_starts_on_attach_and_rings_through_close() {
        let mut st = AppState::new(false);
        assert_eq!(st.focus, Focus::Button(ButtonId::Attach));
        let expected = [
            ButtonId::New,
            ButtonId::Rename,
            ButtonId::Delete,
            ButtonId::Detach,
            ButtonId::Close,
            ButtonId::Attach,
        ];
        for b in expected {
            st.focus_next();
            assert_eq!(st.focus, Focus::Button(b));
        }
    }

    #[test]
    fn focus_prev_is_inverse() {
        let mut st = AppState::new(false);
        st.focus_prev();
        assert_eq!(st.focus, Focus::Button(ButtonId::Close));
        st.focus_prev();
        assert_eq!(st.focus, Focus::Button(ButtonId::Detach));
    }

    #[test]
    fn close_chip_is_hit_tested_on_main() {
        let mut st = AppState::new(false);
        st.geom.close = Some(Rect::new(70, 0, 3, 1));
        assert_eq!(st.hit_test(71, 0), Some(HitTarget::Button(ButtonId::Close)));
        assert_eq!(st.hit_test(60, 0), None);
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
        assert_eq!(st.hit_test(2, 1), Some(HitTarget::Button(ButtonId::Attach)));
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
    fn first_click_on_a_target_is_single() {
        let mut s = AppState::new(false);
        let t = Instant::now();
        assert!(!s.detect_and_record_click(HitTarget::ListRow(0), t));
    }

    #[test]
    fn second_click_on_same_target_within_threshold_is_double() {
        let mut s = AppState::new(false);
        let t = Instant::now();
        s.detect_and_record_click(HitTarget::ListRow(0), t);
        let later = t + Duration::from_millis(200);
        assert!(s.detect_and_record_click(HitTarget::ListRow(0), later));
    }

    #[test]
    fn second_click_after_threshold_is_single() {
        let mut s = AppState::new(false);
        let t = Instant::now();
        s.detect_and_record_click(HitTarget::ListRow(0), t);
        let much_later = t + Duration::from_millis(800);
        assert!(!s.detect_and_record_click(HitTarget::ListRow(0), much_later));
    }

    #[test]
    fn second_click_on_different_target_is_single() {
        let mut s = AppState::new(false);
        let t = Instant::now();
        s.detect_and_record_click(HitTarget::ListRow(0), t);
        let later = t + Duration::from_millis(100);
        assert!(!s.detect_and_record_click(HitTarget::ListRow(1), later));
    }

    #[test]
    fn triple_click_does_not_double_again() {
        // Down x3 in quick succession should fire one double (clicks 1+2)
        // and then a single (click 3), not back-to-back doubles.
        let mut s = AppState::new(false);
        let t = Instant::now();
        s.detect_and_record_click(HitTarget::ListRow(0), t);
        let r2 = s.detect_and_record_click(HitTarget::ListRow(0), t + Duration::from_millis(120));
        let r3 = s.detect_and_record_click(HitTarget::ListRow(0), t + Duration::from_millis(240));
        assert!(r2);
        assert!(!r3);
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
