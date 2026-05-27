//! Cursive-based TUI mirroring `tmuxui.app::TmuxUIApp`.
//!
//! The Python side leans on Textual's reactive widgets; cursive forces
//! us to drive updates ourselves via `call_on_name`. We push all
//! cross-thread refreshes through `cb_sink` so the polling thread
//! never touches the UI directly.

use std::thread;
use std::time::Duration;

use cursive::event::{Event, Key};
use cursive::view::{Nameable, Resizable};
use cursive::views::{Button, Dialog, DummyView, LinearLayout, SelectView, TextView};
use cursive::Cursive;

use crate::conf_setup;
use crate::modals;
use crate::models::Session;
use crate::tmux;

pub const TABLE_NAME: &str = "sessions";
pub const STATUS_NAME: &str = "status";
const REFRESH_PERIOD: Duration = Duration::from_secs(2);

/// Returned after the event loop ends; if `post_exit_argv` is set, the
/// binary will `exec` into it to hand the terminal off cleanly (see
/// `main.rs`). Mirrors `TmuxUIApp.post_exit_argv` in the Python port.
#[derive(Default)]
pub struct AppOutcome {
    pub post_exit_argv: Option<Vec<String>>,
}

pub struct AppState {
    pub sessions: Vec<Session>,
    pub inside_tmux: bool,
    pub post_exit_argv: Option<Vec<String>>,
}

pub fn run() -> AppOutcome {
    let inside_tmux = tmux::is_inside_tmux();
    let mut siv = cursive::default();
    siv.set_user_data(AppState {
        sessions: Vec::new(),
        inside_tmux,
        post_exit_argv: None,
    });

    let title = if inside_tmux {
        "tu — in tmux"
    } else {
        "tu — outside tmux"
    };

    let table = SelectView::<String>::new()
        .on_submit(on_row_submit)
        .with_name(TABLE_NAME)
        .full_width()
        .min_height(6);

    let buttons = LinearLayout::horizontal()
        .child(Button::new("New (n)", action_new))
        .child(DummyView.fixed_width(2))
        .child(Button::new("Attach (a)", action_attach))
        .child(DummyView.fixed_width(2))
        .child(Button::new("Detach (d)", action_detach))
        .child(DummyView.fixed_width(2))
        .child(Button::new("Quit (q)", |s| s.quit()))
        .child(DummyView.fixed_width(2))
        .child(Button::new("Delete (del)", action_delete));

    let hints = TextView::new(
        "n: New   a: Attach   d: Detach   q: Quit   del: Delete   ↑/↓: navigate   Enter: attach",
    );

    let status = TextView::new(initial_status(inside_tmux)).with_name(STATUS_NAME);

    let content = LinearLayout::vertical()
        .child(table)
        .child(DummyView.fixed_height(1))
        .child(buttons)
        .child(DummyView.fixed_height(1))
        .child(hints)
        .child(status);

    siv.add_layer(
        Dialog::around(content)
            .title(title)
            .padding_lrtb(2, 2, 1, 1),
    );

    refresh_sessions(&mut siv);

    // ~/.tmux.conf baseline modal — runs on every launch, matching the
    // Python behaviour added in v0.7.0.
    let conf = conf_setup::conf_path();
    let missing = conf_setup::missing_directives(&conf);
    if !missing.is_empty() {
        modals::show_conf_setup(&mut siv, missing);
    }

    // Polling tick — same 2 s cadence as the Python reactive refresh.
    let cb_sink = siv.cb_sink().clone();
    thread::spawn(move || loop {
        thread::sleep(REFRESH_PERIOD);
        if cb_sink
            .send(Box::new(|s: &mut Cursive| refresh_sessions(s)))
            .is_err()
        {
            break;
        }
    });

    siv.add_global_callback('q', |s| s.quit());
    siv.add_global_callback(Event::CtrlChar('c'), |s| s.quit());
    siv.add_global_callback('n', action_new);
    siv.add_global_callback('a', action_attach);
    siv.add_global_callback('d', action_detach);
    siv.add_global_callback(Key::Del, action_delete);

    siv.run();

    let argv = siv
        .user_data::<AppState>()
        .and_then(|s| s.post_exit_argv.take());
    AppOutcome {
        post_exit_argv: argv,
    }
}

fn initial_status(inside_tmux: bool) -> &'static str {
    if inside_tmux {
        "Inside tmux — Detach (d) returns you to the parent shell."
    } else {
        "Outside tmux — Attach hands the terminal over to tmux."
    }
}

// ----------------------------------------------------- actions

fn action_new(siv: &mut Cursive) {
    let name = tmux::next_default_name();
    let result = tmux::new_session(&name);
    if !result.ok() {
        set_status(
            siv,
            format!("Failed to create session: {}", short_err(&result.stderr)),
        );
        return;
    }
    do_attach(siv, &name);
}

fn action_attach(siv: &mut Cursive) {
    match selected_name(siv) {
        Some(name) => do_attach(siv, &name),
        None => set_status(siv, "No session selected.".to_string()),
    }
}

fn do_attach(siv: &mut Cursive, target: &str) {
    let inside_tmux = siv
        .user_data::<AppState>()
        .map(|s| s.inside_tmux)
        .unwrap_or(false);
    if inside_tmux {
        let r = tmux::switch_client(target);
        if !r.ok() {
            set_status(
                siv,
                format!("Failed to switch session: {}", short_err(&r.stderr)),
            );
            return;
        }
        siv.quit();
        return;
    }
    if let Some(state) = siv.user_data::<AppState>() {
        state.post_exit_argv = Some(tmux::attach_argv(Some(target)));
    }
    siv.quit();
}

fn action_detach(siv: &mut Cursive) {
    let inside_tmux = siv
        .user_data::<AppState>()
        .map(|s| s.inside_tmux)
        .unwrap_or(false);
    if !inside_tmux {
        set_status(siv, "Detach only works from inside tmux.".to_string());
        return;
    }

    let mut detail = String::new();
    let mut ok = false;

    // Prefer the session-scoped form because the no-arg variant can
    // fail when our process owns the controlling tty.
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
        siv.quit();
    } else {
        let reason = if detail.is_empty() {
            "unknown error".to_string()
        } else {
            detail
        };
        set_status(siv, format!("Detach failed: {reason}"));
    }
}

fn action_delete(siv: &mut Cursive) {
    match selected_name(siv) {
        Some(name) => modals::show_confirm_delete(siv, name),
        None => set_status(siv, "No session selected.".to_string()),
    }
}

// Cursive's `SelectView::on_submit` hands us `&T` where `T` is the
// stored value type (`String`), so we cannot widen the parameter to
// `&str` even though clippy wants us to.
#[allow(clippy::ptr_arg)]
fn on_row_submit(siv: &mut Cursive, target: &String) {
    let target = target.clone();
    do_attach(siv, &target);
}

// ----------------------------------------------------- helpers

pub fn set_status(siv: &mut Cursive, msg: impl Into<String>) {
    let msg = msg.into();
    siv.call_on_name(STATUS_NAME, move |t: &mut TextView| {
        t.set_content(msg);
    });
}

pub fn selected_name(siv: &mut Cursive) -> Option<String> {
    siv.call_on_name(TABLE_NAME, |t: &mut SelectView<String>| {
        t.selection().map(|s| (*s).clone())
    })
    .flatten()
}

pub fn refresh_sessions(siv: &mut Cursive) {
    let sessions = tmux::list_sessions();
    let prev = selected_name(siv);

    if let Some(state) = siv.user_data::<AppState>() {
        state.sessions = sessions.clone();
    }

    let max_name = sessions
        .iter()
        .map(|s| s.name.chars().count())
        .max()
        .unwrap_or(8)
        .max(8);

    let rows: Vec<(String, String)> = sessions
        .iter()
        .map(|s| {
            let attached = if s.attached { "attached" } else { "" };
            let label = format!(
                "{:width$}  {:>3}w  {}",
                s.name,
                s.windows,
                attached,
                width = max_name
            );
            (label, s.name.clone())
        })
        .collect();

    let empty = rows.is_empty();
    let target = prev;
    let mut rows = rows;
    siv.call_on_name(TABLE_NAME, move |t: &mut SelectView<String>| {
        t.clear();
        for (label, value) in rows.drain(..) {
            t.add_item(label, value);
        }
        if let Some(p) = target {
            let mut found: Option<usize> = None;
            for (i, item) in t.iter().enumerate() {
                if *item.1 == p {
                    found = Some(i);
                    break;
                }
            }
            if let Some(i) = found {
                t.set_selection(i);
            }
        }
    });

    if empty {
        set_status(siv, "No tmux sessions yet — hit New (n) to create one.");
    }
}

fn short_err(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        "unknown error".to_string()
    } else {
        trimmed.lines().next().unwrap_or(trimmed).to_string()
    }
}
