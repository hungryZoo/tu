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

use crate::conf_setup::{self, ConfAction};
use crate::state::{AppState, ButtonId, ConfFocus, DeleteFocus, Focus, HitTarget, Screen};
use crate::tmux;
use crate::update;
use crate::view;

const TICK_RATE: Duration = Duration::from_secs(2);
const POLL_RATE: Duration = Duration::from_millis(100);

#[derive(Default)]
pub struct AppOutcome {
    pub post_exit_argv: Option<Vec<String>>,
    /// An upgrade command to run with the terminal attached, then exit.
    pub post_exit_command: Option<String>,
}

/// `window_pct`: requested window size; 100 = the whole terminal.
/// Inside tmux the popup (or pane) already is the window, so the caller
/// passes 100 and a `label_cap` planned from the client size; on a bare
/// terminal a smaller value is re-planned every frame.
pub fn run(window_pct: u8, label_cap: crate::view::LabelTier) -> io::Result<AppOutcome> {
    let inside_tmux = tmux::is_inside_tmux();
    let mut terminal = setup_terminal()?;

    let mut state = AppState::new(inside_tmux);
    state.window_pct = window_pct.clamp(10, 100);
    state.label_cap = label_cap;
    state.set_sessions(tmux::list_sessions());
    state.update_plan = update::plan();
    state.update_notice = state.update_plan.as_ref().map(|p| p.notice());
    update::maybe_spawn_check();

    sync_tmux_conf(&mut state);

    let result = event_loop(&mut terminal, &mut state);

    restore_terminal(&mut terminal);

    result.map(|()| AppOutcome {
        post_exit_argv: state.post_exit_argv.take(),
        post_exit_command: state.post_exit_command.take(),
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
        Screen::Rename { original, input } => handle_key_rename(state, key, original, input),
        Screen::Update { .. } => handle_key_update(state, key),
        Screen::ConfSetup { focus, .. } => handle_key_conf(state, key, focus),
        Screen::RestartNotice { .. } => handle_key_notice(state, key),
    }
}

fn handle_key_main(state: &mut AppState, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        // Esc closes tu without touching anything (same as the top-right
        // Close chip). Ctrl-C is kept as the universal bail-out.
        KeyCode::Esc => {
            state.should_exit = true;
        }
        KeyCode::Char('c') if ctrl => {
            state.should_exit = true;
        }
        KeyCode::Char('n') => action_new_session(state),
        KeyCode::Char('a') => action_attach_selected(state),
        KeyCode::Char('r') => action_open_rename_modal(state),
        KeyCode::Char('u') => action_open_update_modal(state),
        KeyCode::Char('d') => action_detach(state),
        // Backspace is the advertised Delete key (it's the key labelled
        // "delete" on Mac keyboards); forward-Delete keeps working too.
        KeyCode::Backspace | KeyCode::Delete => action_open_delete_modal(state),
        KeyCode::Tab => state.focus_next(),
        KeyCode::BackTab => state.focus_prev(),
        // ↑/↓ always move the session selection and put focus back on
        // Attach, so Enter right after choosing a row attaches.
        KeyCode::Up => {
            state.move_selection(-1);
            state.focus_attach();
        }
        KeyCode::Down => {
            state.move_selection(1);
            state.focus_attach();
        }
        KeyCode::Left => state.focus_prev(),
        KeyCode::Right => state.focus_next(),
        KeyCode::Home => {
            state.move_selection(i32::MIN / 2);
            state.focus_attach();
        }
        KeyCode::End => {
            state.move_selection(i32::MAX / 2);
            state.focus_attach();
        }
        KeyCode::Enter => {
            let Focus::Button(b) = state.focus;
            activate_main_button(state, b);
        }
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

fn handle_key_rename(state: &mut AppState, key: KeyEvent, original: String, mut input: String) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Esc => dismiss_modal(state),
        KeyCode::Char('c') if ctrl => dismiss_modal(state),
        KeyCode::Enter => confirm_rename(state, &original, &input),
        KeyCode::Char('u') if ctrl => set_rename_input(state, String::new()),
        KeyCode::Backspace => {
            input.pop();
            set_rename_input(state, input);
        }
        KeyCode::Char(c) if !ctrl => {
            input.push(c);
            set_rename_input(state, input);
        }
        _ => {}
    }
}

fn handle_key_update(state: &mut AppState, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => dismiss_modal(state),
        KeyCode::Char('c') if ctrl => dismiss_modal(state),
        KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Char('u') => {
            confirm_update(state)
        }
        _ => {}
    }
}

fn set_rename_input(state: &mut AppState, new_input: String) {
    if let Screen::Rename { input, .. } = &mut state.screen {
        *input = new_input;
    }
}

fn handle_key_notice(state: &mut AppState, key: KeyEvent) {
    // The restart notice has a single OK button. Treat Enter / Esc / q
    // as confirmations; ignore everything else so a stray keystroke
    // doesn't quietly drop the user back to a stale UI.
    match key.code {
        KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char(' ') => {
            state.should_exit = true;
        }
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
        HitTarget::ListRow(_) => state.focus_attach(),
        _ => {}
    }
}

fn trigger_hit(state: &mut AppState, target: HitTarget) {
    let screen = state.screen.clone();
    match (screen, target) {
        (Screen::Main, HitTarget::Button(b)) => activate_main_button(state, b),
        (Screen::Main, HitTarget::ListRow(idx)) => {
            // First click: just select. Second click on the *same*
            // row within the double-click window: attach. This matches
            // the file-manager idiom users already expect and lets
            // them inspect a session in the list without immediately
            // diving in.
            state.selected = idx;
            let is_double = state.detect_and_record_click(HitTarget::ListRow(idx), Instant::now());
            if is_double {
                action_attach_selected(state);
            }
        }
        (Screen::Main, HitTarget::UpdateNotice) => action_open_update_modal(state),
        (Screen::Update { .. }, HitTarget::ModalPrimary) => dismiss_modal(state),
        (Screen::Update { .. }, HitTarget::ModalSecondary) => confirm_update(state),
        (Screen::Rename { .. }, HitTarget::ModalPrimary) => dismiss_modal(state),
        (Screen::Rename { original, input }, HitTarget::ModalSecondary) => {
            confirm_rename(state, &original, &input);
        }
        (Screen::ConfirmDelete { .. }, HitTarget::ModalPrimary) => dismiss_modal(state),
        (Screen::ConfirmDelete { name, .. }, HitTarget::ModalSecondary) => {
            confirm_delete(state, &name);
        }
        (Screen::ConfSetup { .. }, HitTarget::ModalPrimary) => apply_conf_modal(state),
        (Screen::ConfSetup { .. }, HitTarget::ModalSecondary) => dismiss_modal(state),
        (Screen::RestartNotice { .. }, HitTarget::ModalPrimary) => {
            state.should_exit = true;
        }
        _ => {}
    }
}

// ---------------------------------------------------------- actions

fn activate_main_button(state: &mut AppState, button: ButtonId) {
    if !button_enabled(state, button) {
        match button {
            ButtonId::Attach | ButtonId::Rename | ButtonId::Delete => {
                state.set_error("No session selected.")
            }
            ButtonId::Detach => state.set_error("Detach only works from inside tmux."),
            _ => {}
        }
        return;
    }
    match button {
        ButtonId::Attach => action_attach_selected(state),
        ButtonId::New => action_new_session(state),
        ButtonId::Rename => action_open_rename_modal(state),
        ButtonId::Delete => action_open_delete_modal(state),
        ButtonId::Detach => action_detach(state),
        ButtonId::Close => state.should_exit = true,
    }
}

fn button_enabled(state: &AppState, button: ButtonId) -> bool {
    match button {
        ButtonId::New | ButtonId::Close => true,
        ButtonId::Attach | ButtonId::Rename | ButtonId::Delete => state.has_sessions(),
        ButtonId::Detach => state.inside_tmux,
    }
}

fn action_open_update_modal(state: &mut AppState) {
    let Some(plan) = state.update_plan.clone() else {
        state.set_info(format!("tu {} is up to date.", update::CURRENT_VERSION));
        return;
    };
    state.screen = Screen::Update { plan };
}

/// Hand the upgrade command to `main`: the TUI must be gone before a
/// package manager or install.sh gets the terminal.
fn confirm_update(state: &mut AppState) {
    let Screen::Update { plan } = &state.screen else {
        return;
    };
    match plan.command() {
        Some(cmd) => {
            state.post_exit_command = Some(cmd);
            state.should_exit = true;
        }
        None => {
            // Nothing we can run: the modal already shows the URL.
            state.screen = Screen::Main;
        }
    }
}

fn action_open_rename_modal(state: &mut AppState) {
    let Some(name) = state.selected_name().map(str::to_owned) else {
        state.set_error("No session selected.");
        return;
    };
    state.screen = Screen::Rename {
        original: name.clone(),
        input: name,
    };
}

fn confirm_rename(state: &mut AppState, original: &str, input: &str) {
    if input == original {
        state.screen = Screen::Main;
        return;
    }
    if let Some(problem) = tmux::session_name_problem(input) {
        // Keep the modal open; the hint line already shows the problem.
        state.set_error(problem);
        return;
    }
    if state.sessions.iter().any(|s| s.name == input) {
        state.set_error(format!("A session named '{input}' already exists."));
        return;
    }
    let r = tmux::rename_session(original, input);
    if !r.ok() {
        state.set_error(format!("Rename failed: {}", short_err(&r.stderr)));
        return;
    }
    state.set_info(format!("Renamed '{original}' to '{input}'."));
    state.screen = Screen::Main;
    state.set_sessions(tmux::list_sessions());
    // Follow the renamed session so the highlight doesn't jump away.
    if let Some(i) = state.sessions.iter().position(|s| s.name == input) {
        state.selected = i;
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

/// Launch-time tmux config check. Silent when only our own file needs
/// rewriting; a modal when `~/.tmux.conf` itself would change.
fn sync_tmux_conf(state: &mut AppState) {
    let user = conf_setup::user_conf_path();
    let managed = conf_setup::managed_conf_path();
    let popup = conf_setup::detect_popup_support();
    match conf_setup::plan(&user, &managed, popup) {
        ConfAction::UpToDate => {}
        ConfAction::Refresh => match conf_setup::write_managed(&managed, popup) {
            Ok(()) => {
                let live = conf_setup::apply_live(&managed);
                state.set_info(if live {
                    format!("tmux config refreshed (v{}).", conf_setup::MANAGED_VERSION)
                } else {
                    format!(
                        "tmux config refreshed (v{}); applies when tmux restarts.",
                        conf_setup::MANAGED_VERSION
                    )
                });
            }
            Err(e) => state.set_error(format!("Could not write {}: {e}", managed.display())),
        },
        action @ (ConfAction::Install | ConfAction::Migrate) => {
            let lines = conf_setup::describe(&action, &user, &managed);
            state.screen = Screen::ConfSetup {
                action,
                lines,
                focus: ConfFocus::default(),
            };
        }
    }
}

fn apply_conf_modal(state: &mut AppState) {
    let action = match &state.screen {
        Screen::ConfSetup { action, .. } => action.clone(),
        _ => return,
    };
    let user = conf_setup::user_conf_path();
    let managed = conf_setup::managed_conf_path();
    let popup = conf_setup::detect_popup_support();
    let result = conf_setup::write_managed(&managed, popup).and_then(|()| match action {
        ConfAction::Migrate => conf_setup::migrate_legacy(&user, &managed),
        _ => conf_setup::install_include(&user, &managed),
    });
    match result {
        Ok(()) => {
            let live = conf_setup::apply_live(&managed);
            let server_line = if live {
                "The running tmux server has the new settings already."
            } else {
                "They take effect the next time tmux starts."
            };
            let what = match action {
                ConfAction::Migrate => {
                    "Replaced the old tu block in ~/.tmux.conf with one\nsource-file line"
                }
                _ => "Added one source-file line to ~/.tmux.conf",
            };
            let message = format!(
                "{what} and wrote {}.\n\n{server_line}\n\ntu needs a fresh start so it picks up the\nnew defaults.",
                managed.display()
            );
            state.set_info("tmux config installed.");
            state.screen = Screen::RestartNotice { message };
        }
        Err(e) => {
            state.set_error(format!("Failed to update tmux config: {e}"));
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
        assert_eq!(s.focus, Focus::Button(ButtonId::New));
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

    fn brew_plan() -> crate::update::UpdatePlan {
        crate::update::UpdatePlan {
            latest: "9.9.9".into(),
            method: crate::update::InstallMethod::Brew,
        }
    }

    #[test]
    fn u_opens_update_modal_only_when_an_update_exists() {
        let mut s = make_state(false, &["work"]);
        handle_key_main(&mut s, key(KeyCode::Char('u')));
        assert!(matches!(s.screen, Screen::Main));
        assert!(s.status_message.contains("up to date"));
        s.update_plan = Some(brew_plan());
        handle_key_main(&mut s, key(KeyCode::Char('u')));
        assert!(matches!(s.screen, Screen::Update { .. }));
    }

    #[test]
    fn clicking_the_notice_opens_the_update_modal() {
        let mut s = make_state(false, &["work"]);
        s.update_plan = Some(brew_plan());
        trigger_hit(&mut s, HitTarget::UpdateNotice);
        assert!(matches!(s.screen, Screen::Update { .. }));
    }

    #[test]
    fn confirming_update_hands_the_command_to_main_and_exits() {
        let mut s = make_state(false, &["work"]);
        s.update_plan = Some(brew_plan());
        action_open_update_modal(&mut s);
        handle_key(&mut s, key(KeyCode::Enter));
        assert!(s.should_exit);
        assert_eq!(s.post_exit_command.as_deref(), Some("brew upgrade tu"));
    }

    #[test]
    fn update_modal_esc_just_closes() {
        let mut s = make_state(false, &["work"]);
        s.update_plan = Some(brew_plan());
        action_open_update_modal(&mut s);
        handle_key(&mut s, key(KeyCode::Esc));
        assert!(matches!(s.screen, Screen::Main));
        assert!(!s.should_exit);
        assert!(s.post_exit_command.is_none());
    }

    #[test]
    fn update_modal_without_a_command_closes_on_confirm() {
        let mut s = make_state(false, &["work"]);
        s.update_plan = Some(crate::update::UpdatePlan {
            latest: "9.9.9".into(),
            method: crate::update::InstallMethod::Unknown,
        });
        action_open_update_modal(&mut s);
        trigger_hit(&mut s, HitTarget::ModalSecondary);
        assert!(matches!(s.screen, Screen::Main));
        assert!(!s.should_exit);
        assert!(s.post_exit_command.is_none());
    }

    #[test]
    fn r_opens_rename_prefilled_with_current_name() {
        let mut s = make_state(false, &["work", "play"]);
        s.selected = 1;
        handle_key_main(&mut s, key(KeyCode::Char('r')));
        match &s.screen {
            Screen::Rename { original, input } => {
                assert_eq!(original, "play");
                assert_eq!(input, "play");
            }
            other => panic!("expected Rename, got {other:?}"),
        }
    }

    #[test]
    fn rename_without_session_shows_error() {
        let mut s = make_state(false, &[]);
        handle_key_main(&mut s, key(KeyCode::Char('r')));
        assert!(matches!(s.screen, Screen::Main));
        assert!(s.status_message.contains("No session"));
    }

    #[test]
    fn rename_modal_edits_and_cancels() {
        let mut s = make_state(false, &["work"]);
        action_open_rename_modal(&mut s);
        handle_key(&mut s, key(KeyCode::Backspace));
        handle_key(&mut s, key(KeyCode::Char('x')));
        assert!(matches!(&s.screen, Screen::Rename { input, .. } if input == "worx"));
        handle_key(
            &mut s,
            KeyEvent {
                code: KeyCode::Char('u'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                state: crossterm::event::KeyEventState::empty(),
            },
        );
        assert!(matches!(&s.screen, Screen::Rename { input, .. } if input.is_empty()));
        handle_key(&mut s, key(KeyCode::Esc));
        assert!(matches!(s.screen, Screen::Main));
    }

    #[test]
    fn rename_rejects_bad_and_duplicate_names_without_calling_tmux() {
        let mut s = make_state(false, &["work", "play"]);
        confirm_rename(&mut s, "work", "a:b");
        assert!(s.status_message.contains("':'"));
        confirm_rename(&mut s, "work", "play");
        assert!(s.status_message.contains("already exists"));
        // Unchanged name is a silent no-op that just closes the modal.
        s.screen = Screen::Rename {
            original: "work".into(),
            input: "work".into(),
        };
        confirm_rename(&mut s, "work", "work");
        assert!(matches!(s.screen, Screen::Main));
    }

    #[test]
    fn arrows_pick_a_row_and_refocus_attach() {
        let mut s = make_state(false, &["work", "play"]);
        s.focus = Focus::Button(ButtonId::Delete);
        handle_key_main(&mut s, key(KeyCode::Down));
        assert_eq!(s.selected, 1);
        assert_eq!(s.focus, Focus::Button(ButtonId::Attach));
        handle_key_main(&mut s, key(KeyCode::Enter));
        assert!(s.should_exit);
        assert_eq!(
            s.post_exit_argv
                .as_ref()
                .and_then(|a| a.last())
                .map(String::as_str),
            Some("play")
        );
    }

    #[test]
    fn close_chip_exits() {
        let mut s = make_state(false, &["work"]);
        trigger_hit(&mut s, HitTarget::Button(ButtonId::Close));
        assert!(s.should_exit);
    }

    #[test]
    fn esc_cancels_and_exits() {
        let mut s = make_state(false, &[]);
        handle_key_main(
            &mut s,
            KeyEvent {
                code: KeyCode::Esc,
                modifiers: KeyModifiers::empty(),
                kind: KeyEventKind::Press,
                state: crossterm::event::KeyEventState::empty(),
            },
        );
        assert!(s.should_exit);
    }

    #[test]
    fn q_is_no_longer_a_shortcut() {
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
        assert!(!s.should_exit);
    }

    #[test]
    fn backspace_and_delete_open_delete_modal() {
        for code in [KeyCode::Backspace, KeyCode::Delete] {
            let mut s = make_state(false, &["work"]);
            handle_key_main(&mut s, key(code));
            assert!(matches!(s.screen, Screen::ConfirmDelete { .. }), "{code:?}");
        }
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
    fn cancel_button_skips_disabled_handling() {
        let mut s = make_state(false, &[]);
        activate_main_button(&mut s, ButtonId::Close);
        assert!(s.should_exit);
    }

    #[test]
    fn attach_button_is_disabled_without_sessions() {
        let mut s = make_state(false, &[]);
        activate_main_button(&mut s, ButtonId::Attach);
        assert!(!s.should_exit);
        assert!(s.status_message.contains("No session"));
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::empty(),
        }
    }

    #[test]
    fn enter_on_restart_notice_exits_tu() {
        let mut s = make_state(false, &[]);
        s.screen = Screen::RestartNotice {
            message: "test".into(),
        };
        handle_key(&mut s, key(KeyCode::Enter));
        assert!(s.should_exit);
    }

    #[test]
    fn restart_notice_ignores_unrelated_keys() {
        let mut s = make_state(false, &[]);
        s.screen = Screen::RestartNotice {
            message: "test".into(),
        };
        handle_key(&mut s, key(KeyCode::Char('x')));
        assert!(!s.should_exit);
        assert!(matches!(s.screen, Screen::RestartNotice { .. }));
    }

    #[test]
    fn clicking_ok_on_restart_notice_exits_tu() {
        let mut s = make_state(false, &[]);
        s.screen = Screen::RestartNotice {
            message: "ok".into(),
        };
        trigger_hit(&mut s, HitTarget::ModalPrimary);
        assert!(s.should_exit);
    }

    #[test]
    fn first_click_on_list_row_only_selects() {
        // Simulating the body of `trigger_hit` for ListRow so we
        // don't depend on the wall-clock inside it.
        let mut s = make_state(false, &["work", "play"]);
        let idx = 1;
        let now = Instant::now();
        s.selected = idx;
        let is_double = s.detect_and_record_click(HitTarget::ListRow(idx), now);
        assert!(!is_double);
        assert_eq!(s.selected, idx);
        assert!(!s.should_exit);
        assert!(s.post_exit_argv.is_none());
    }

    #[test]
    fn double_click_on_list_row_attaches() {
        let mut s = make_state(false, &["work", "play"]);
        let idx = 1;
        let t = Instant::now();
        s.selected = idx;
        let _ = s.detect_and_record_click(HitTarget::ListRow(idx), t);
        let is_double =
            s.detect_and_record_click(HitTarget::ListRow(idx), t + Duration::from_millis(150));
        assert!(is_double);
        if is_double {
            action_attach_selected(&mut s);
        }
        assert!(s.should_exit);
        let argv = s.post_exit_argv.as_ref().expect("expected attach argv");
        assert_eq!(argv.last().map(String::as_str), Some("play"));
    }
}
