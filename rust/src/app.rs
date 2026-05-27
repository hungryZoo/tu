//! Event loop driving the ratatui view + the cross-thread plumbing
//! around the `tmux` CLI.
//!
//! The event loop is intentionally single-threaded: we poll crossterm
//! with a 100 ms timeout, refresh the session list every two seconds,
//! and redraw after every input or tick. Subprocess calls are
//! synchronous — `tmux list-sessions` lands in the ballpark of 5–30 ms
//! on a normal machine, well below a frame, so blocking the main loop
//! is fine.

use std::io;
use std::panic;
use std::time::{Duration, Instant};

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::conf_setup::{self, Directive};
use crate::state::{AppState, ButtonId, ConfFocus, DeleteFocus, Focus, HitTarget, Screen};
use crate::tmux;
use crate::view;

const TICK_RATE: Duration = Duration::from_secs(2);
const POLL_RATE: Duration = Duration::from_millis(100);

#[derive(Default)]
pub struct AppOutcome {
    pub post_exit_argv: Option<Vec<String>>,
}

pub fn run() -> io::Result<AppOutcome> {
    let inside_tmux = tmux::is_inside_tmux();
    let mut terminal = setup_terminal()?;

    let mut state = AppState::new(inside_tmux);
    state.set_sessions(tmux::list_sessions());

    let missing = conf_setup::missing_directives(&conf_setup::conf_path());
    if !missing.is_empty() {
        state.screen = Screen::ConfSetup {
            directives: missing,
            focus: ConfFocus::default(),
        };
    }

    let result = event_loop(&mut terminal, &mut state);

    restore_terminal(&mut terminal);

    result.map(|()| AppOutcome {
        post_exit_argv: state.post_exit_argv.take(),
    })
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
) -> io::Result<()> {
    let mut last_tick = Instant::now();
    loop {
        terminal.draw(|f| view::render(f, state))?;

        let timeout = TICK_RATE.saturating_sub(last_tick.elapsed()).min(POLL_RATE);
        if event::poll(timeout)? {
            handle_event(state, event::read()?);
        }
        if last_tick.elapsed() >= TICK_RATE {
            state.set_sessions(tmux::list_sessions());
            last_tick = Instant::now();
        }
        if state.should_exit {
            break Ok(());
        }
    }
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    // Make sure a panic mid-render still leaves the terminal usable.
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        original_hook(info);
    }));
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) {
    let _ = disable_raw_mode();
    let _ = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    );
    let _ = terminal.show_cursor();
}

// ----------------------------------------------------- dispatch

fn handle_event(state: &mut AppState, ev: Event) {
    match ev {
        Event::Key(k) if k.kind == KeyEventKind::Press => handle_key(state, k),
        Event::Mouse(m) => handle_mouse(state, m),
        _ => {}
    }
}

fn handle_key(state: &mut AppState, key: KeyEvent) {
    // Snapshot the screen so we can mutate `state` in handlers below.
    match state.screen.clone() {
        Screen::Main => handle_key_main(state, key),
        Screen::ConfirmDelete { name, focus } => handle_key_delete(state, key, name, focus),
        Screen::ConfSetup { focus, .. } => handle_key_conf(state, key, focus),
    }
}

fn handle_key_main(state: &mut AppState, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            state.should_exit = true;
        }
        KeyCode::Char('c') if ctrl => {
            state.should_exit = true;
        }
        KeyCode::Char('n') => action_new_session(state),
        KeyCode::Char('a') => action_attach_selected(state),
        KeyCode::Char('d') => action_detach(state),
        KeyCode::Delete => action_open_delete_modal(state),
        KeyCode::Tab => state.focus_next(),
        KeyCode::BackTab => state.focus_prev(),
        KeyCode::Up => match state.focus {
            Focus::List => state.move_selection(-1),
            _ => state.focus = Focus::List,
        },
        KeyCode::Down => match state.focus {
            Focus::List => state.move_selection(1),
            _ => state.focus = Focus::List,
        },
        KeyCode::Left => {
            if let Focus::Button(b) = state.focus {
                state.focus = Focus::Button(b.prev());
            }
        }
        KeyCode::Right => {
            if let Focus::Button(b) = state.focus {
                state.focus = Focus::Button(b.next());
            }
        }
        KeyCode::Home => state.move_selection(i32::MIN / 2),
        KeyCode::End => state.move_selection(i32::MAX / 2),
        KeyCode::Enter => match state.focus {
            Focus::List => action_attach_selected(state),
            Focus::Button(b) => activate_main_button(state, b),
        },
        _ => {}
    }
}

fn handle_key_delete(state: &mut AppState, key: KeyEvent, name: String, focus: DeleteFocus) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => dismiss_modal(state),
        KeyCode::Tab | KeyCode::BackTab | KeyCode::Left | KeyCode::Right => {
            toggle_delete_focus(state, focus);
        }
        KeyCode::Enter => match focus {
            DeleteFocus::Cancel => dismiss_modal(state),
            DeleteFocus::Confirm => confirm_delete(state, &name),
        },
        _ => {}
    }
}

fn handle_key_conf(state: &mut AppState, key: KeyEvent, focus: ConfFocus) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => apply_conf_modal(state),
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => dismiss_modal(state),
        KeyCode::Tab | KeyCode::BackTab | KeyCode::Left | KeyCode::Right => {
            toggle_conf_focus(state, focus);
        }
        KeyCode::Enter => match focus {
            ConfFocus::Yes => apply_conf_modal(state),
            ConfFocus::Later => dismiss_modal(state),
        },
        _ => {}
    }
}

fn toggle_delete_focus(state: &mut AppState, current: DeleteFocus) {
    let next = match current {
        DeleteFocus::Cancel => DeleteFocus::Confirm,
        DeleteFocus::Confirm => DeleteFocus::Cancel,
    };
    if let Screen::ConfirmDelete { focus, .. } = &mut state.screen {
        *focus = next;
    }
}

fn toggle_conf_focus(state: &mut AppState, current: ConfFocus) {
    let next = match current {
        ConfFocus::Yes => ConfFocus::Later,
        ConfFocus::Later => ConfFocus::Yes,
    };
    if let Screen::ConfSetup { focus, .. } = &mut state.screen {
        *focus = next;
    }
}

// ---------------------------------------------------------- mouse

fn handle_mouse(state: &mut AppState, m: MouseEvent) {
    match m.kind {
        MouseEventKind::Moved | MouseEventKind::Drag(_) => {
            state.hover = state.hit_test(m.column, m.row);
        }
        MouseEventKind::Down(MouseButton::Left) => {
            let target = state.hit_test(m.column, m.row);
            state.pressed = target;
            state.hover = target;
            if let Some(t) = target {
                update_focus_from_hit(state, t);
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            let release_target = state.hit_test(m.column, m.row);
            let pressed = state.pressed.take();
            if let (Some(p), Some(r)) = (pressed, release_target) {
                if p == r {
                    trigger_hit(state, p);
                }
            }
            state.hover = state.hit_test(m.column, m.row);
        }
        MouseEventKind::ScrollDown => {
            if matches!(state.screen, Screen::Main) {
                state.move_selection(1);
            }
        }
        MouseEventKind::ScrollUp => {
            if matches!(state.screen, Screen::Main) {
                state.move_selection(-1);
            }
        }
        _ => {}
    }
}

fn update_focus_from_hit(state: &mut AppState, target: HitTarget) {
    match target {
        HitTarget::Button(b) => state.focus = Focus::Button(b),
        HitTarget::ListRow(_) => state.focus = Focus::List,
        _ => {}
    }
}

fn trigger_hit(state: &mut AppState, target: HitTarget) {
    let screen = state.screen.clone();
    match (screen, target) {
        (Screen::Main, HitTarget::Button(b)) => activate_main_button(state, b),
        (Screen::Main, HitTarget::ListRow(idx)) => {
            state.selected = idx;
            action_attach_selected(state);
        }
        (Screen::ConfirmDelete { .. }, HitTarget::ModalPrimary) => dismiss_modal(state),
        (Screen::ConfirmDelete { name, .. }, HitTarget::ModalSecondary) => {
            confirm_delete(state, &name);
        }
        (Screen::ConfSetup { .. }, HitTarget::ModalPrimary) => apply_conf_modal(state),
        (Screen::ConfSetup { .. }, HitTarget::ModalSecondary) => dismiss_modal(state),
        _ => {}
    }
}

// ---------------------------------------------------------- actions

fn activate_main_button(state: &mut AppState, button: ButtonId) {
    if !button_enabled(state, button) {
        match button {
            ButtonId::Attach | ButtonId::Delete => state.set_error("No session selected."),
            ButtonId::Detach => state.set_error("Detach only works from inside tmux."),
            _ => {}
        }
        return;
    }
    match button {
        ButtonId::New => action_new_session(state),
        ButtonId::Attach => action_attach_selected(state),
        ButtonId::Detach => action_detach(state),
        ButtonId::Quit => state.should_exit = true,
        ButtonId::Delete => action_open_delete_modal(state),
    }
}

fn button_enabled(state: &AppState, button: ButtonId) -> bool {
    match button {
        ButtonId::New | ButtonId::Quit => true,
        ButtonId::Attach | ButtonId::Delete => state.has_sessions(),
        ButtonId::Detach => state.inside_tmux,
    }
}

fn action_new_session(state: &mut AppState) {
    let name = tmux::next_default_name();
    let result = tmux::new_session(&name);
    if !result.ok() {
        state.set_error(format!(
            "Failed to create session: {}",
            short_err(&result.stderr)
        ));
        return;
    }
    do_attach(state, &name);
}

fn action_attach_selected(state: &mut AppState) {
    let Some(name) = state.selected_name().map(str::to_owned) else {
        state.set_error("No session selected.");
        return;
    };
    do_attach(state, &name);
}

fn do_attach(state: &mut AppState, target: &str) {
    if state.inside_tmux {
        let r = tmux::switch_client(target);
        if !r.ok() {
            state.set_error(format!(
                "Failed to switch session: {}",
                short_err(&r.stderr)
            ));
            return;
        }
        state.should_exit = true;
    } else {
        state.post_exit_argv = Some(tmux::attach_argv(Some(target)));
        state.should_exit = true;
    }
}

fn action_detach(state: &mut AppState) {
    if !state.inside_tmux {
        state.set_error("Detach only works from inside tmux.");
        return;
    }
    let mut ok = false;
    let mut detail = String::new();

    if let Some(session) = tmux::current_pane_session() {
        let r = tmux::detach_client_session(&session);
        if r.ok() {
            ok = true;
        } else {
            detail = r.stderr.trim().to_string();
        }
    }
    if !ok {
        let r = tmux::detach_client();
        if r.ok() {
            ok = true;
        } else if detail.is_empty() {
            detail = r.stderr.trim().to_string();
        }
    }
    if ok {
        state.should_exit = true;
    } else {
        let reason = if detail.is_empty() {
            "unknown error".to_string()
        } else {
            detail
        };
        state.set_error(format!("Detach failed: {reason}"));
    }
}

fn action_open_delete_modal(state: &mut AppState) {
    let Some(name) = state.selected_name().map(str::to_owned) else {
        state.set_error("No session selected.");
        return;
    };
    state.screen = Screen::ConfirmDelete {
        name,
        focus: DeleteFocus::default(),
    };
}

fn confirm_delete(state: &mut AppState, name: &str) {
    let r = tmux::kill_session(name);
    if !r.ok() {
        state.set_error(format!(
            "Failed to delete session '{name}': {}",
            short_err(&r.stderr)
        ));
        // Leave the modal up so the user can decide whether to retry
        // — same behaviour as the cursive build.
        return;
    }
    state.set_info(format!("Session '{name}' deleted."));
    state.screen = Screen::Main;
    state.set_sessions(tmux::list_sessions());
}

fn dismiss_modal(state: &mut AppState) {
    state.screen = Screen::Main;
}

fn apply_conf_modal(state: &mut AppState) {
    // Grab the directives we need from the modal state before we
    // tear it down.
    let directives: Vec<Directive> = match &state.screen {
        Screen::ConfSetup { directives, .. } => directives.clone(),
        _ => return,
    };
    let conf = conf_setup::conf_path();
    match conf_setup::append_directives(&directives, &conf) {
        Ok(()) => {
            let applied = conf_setup::apply_directives_to_server(&directives);
            let names: Vec<&str> = directives.iter().map(|d| d.option.as_str()).collect();
            let tail = if applied {
                " · live tmux server updated"
            } else {
                " · will take effect on next tmux start"
            };
            state.set_info(format!(
                "Appended {} to {}{tail}",
                names.join(", "),
                conf.display()
            ));
            state.screen = Screen::Main;
        }
        Err(e) => {
            state.set_error(format!("Failed to update ~/.tmux.conf: {e}"));
            state.screen = Screen::Main;
        }
    }
}

fn short_err(stderr: &str) -> String {
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        "unknown error".to_string()
    } else {
        trimmed.lines().next().unwrap_or(trimmed).to_string()
    }
}

// ----------------------------------------------------- a tiny test surface

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Session;

    fn make_state(inside_tmux: bool, sessions: &[&str]) -> AppState {
        let mut s = AppState::new(inside_tmux);
        s.set_sessions(
            sessions
                .iter()
                .map(|n| Session {
                    name: (*n).into(),
                    windows: 1,
                    attached: false,
                })
                .collect(),
        );
        s
    }

    #[test]
    fn attach_outside_tmux_sets_post_exit_argv() {
        let mut s = make_state(false, &["work", "play"]);
        s.selected = 1;
        action_attach_selected(&mut s);
        assert!(s.should_exit);
        assert_eq!(
            s.post_exit_argv,
            Some(vec![
                "tmux".to_string(),
                "attach-session".to_string(),
                "-t".to_string(),
                "play".to_string()
            ])
        );
    }

    #[test]
    fn attach_with_no_sessions_shows_error() {
        let mut s = make_state(false, &[]);
        action_attach_selected(&mut s);
        assert!(!s.should_exit);
        assert!(s.status_message.contains("No session"));
    }

    #[test]
    fn detach_outside_tmux_shows_error() {
        let mut s = make_state(false, &["work"]);
        action_detach(&mut s);
        assert!(!s.should_exit);
        assert!(s.status_message.to_lowercase().contains("inside tmux"));
    }

    #[test]
    fn delete_button_opens_modal_when_session_selected() {
        let mut s = make_state(false, &["work"]);
        action_open_delete_modal(&mut s);
        assert!(matches!(s.screen, Screen::ConfirmDelete { .. }));
    }

    #[test]
    fn delete_button_is_noop_without_session() {
        let mut s = make_state(false, &[]);
        action_open_delete_modal(&mut s);
        assert!(matches!(s.screen, Screen::Main));
        assert!(s.status_message.contains("No session"));
    }

    #[test]
    fn focus_left_right_walks_button_row() {
        let mut s = make_state(false, &["work"]);
        s.focus = Focus::Button(ButtonId::Attach);
        handle_key_main(
            &mut s,
            KeyEvent {
                code: KeyCode::Right,
                modifiers: KeyModifiers::empty(),
                kind: KeyEventKind::Press,
                state: crossterm::event::KeyEventState::empty(),
            },
        );
        assert_eq!(s.focus, Focus::Button(ButtonId::Detach));
        handle_key_main(
            &mut s,
            KeyEvent {
                code: KeyCode::Left,
                modifiers: KeyModifiers::empty(),
                kind: KeyEventKind::Press,
                state: crossterm::event::KeyEventState::empty(),
            },
        );
        assert_eq!(s.focus, Focus::Button(ButtonId::Attach));
    }

    #[test]
    fn quit_button_exits_via_keyboard() {
        let mut s = make_state(false, &[]);
        handle_key_main(
            &mut s,
            KeyEvent {
                code: KeyCode::Char('q'),
                modifiers: KeyModifiers::empty(),
                kind: KeyEventKind::Press,
                state: crossterm::event::KeyEventState::empty(),
            },
        );
        assert!(s.should_exit);
    }

    #[test]
    fn ctrl_c_exits() {
        let mut s = make_state(false, &[]);
        handle_key_main(
            &mut s,
            KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                state: crossterm::event::KeyEventState::empty(),
            },
        );
        assert!(s.should_exit);
    }

    #[test]
    fn quit_button_skips_disabled_handling() {
        let mut s = make_state(false, &[]);
        activate_main_button(&mut s, ButtonId::Quit);
        assert!(s.should_exit);
    }

    #[test]
    fn attach_button_is_disabled_without_sessions() {
        let mut s = make_state(false, &[]);
        activate_main_button(&mut s, ButtonId::Attach);
        assert!(!s.should_exit);
        assert!(s.status_message.contains("No session"));
    }
}
