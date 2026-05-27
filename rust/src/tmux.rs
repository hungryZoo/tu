//! Thin wrapper over the ``tmux`` CLI — mirror of `tmuxui.tmux`.

use std::collections::HashSet;
use std::env;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::models::{parse_sessions, Session, SESSION_FORMAT};

pub const DEFAULT_NAME_PREFIX: &str = "tu";
pub const DEFAULT_NAME_MAX: u32 = 999;

#[derive(Debug, Clone)]
pub struct TmuxResult {
    pub argv: Vec<String>,
    pub returncode: i32,
    pub stdout: String,
    pub stderr: String,
}

impl TmuxResult {
    pub fn ok(&self) -> bool {
        self.returncode == 0
    }
}

fn run(args: &[&str]) -> TmuxResult {
    let argv: Vec<String> = std::iter::once("tmux".to_string())
        .chain(args.iter().map(|a| a.to_string()))
        .collect();
    match Command::new("tmux").args(args).output() {
        Ok(o) => TmuxResult {
            argv,
            returncode: o.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
        },
        Err(e) => TmuxResult {
            argv,
            returncode: -1,
            stdout: String::new(),
            stderr: e.to_string(),
        },
    }
}

pub fn is_tmux_installed() -> bool {
    Command::new("tmux")
        .arg("-V")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn is_inside_tmux() -> bool {
    env::var("TMUX").map(|v| !v.is_empty()).unwrap_or(false)
}

// ----------------------------------------------------- query

pub fn list_sessions() -> Vec<Session> {
    let r = run(&["list-sessions", "-F", SESSION_FORMAT]);
    if !r.ok() {
        return vec![];
    }
    parse_sessions(&r.stdout)
}

// ----------------------------------------------------- mutation

pub fn new_session(name: &str) -> TmuxResult {
    // `-d` keeps it detached; the UI either switches the existing
    // client to it (inside tmux) or stashes an attach-session argv for
    // ``main`` to ``execvp`` into after the TUI exits.
    run(&["new-session", "-d", "-s", name])
}

pub fn switch_client(target: &str) -> TmuxResult {
    run(&["switch-client", "-t", target])
}

pub fn detach_client() -> TmuxResult {
    run(&["detach-client"])
}

pub fn detach_client_session(session: &str) -> TmuxResult {
    run(&["detach-client", "-s", session])
}

pub fn kill_session(name: &str) -> TmuxResult {
    run(&["kill-session", "-t", name])
}

// ----------------------------------------------------- options

pub fn server_running() -> bool {
    run(&["info"]).ok()
}

pub fn show_option(name: &str) -> Option<String> {
    let r = run(&["show-options", "-g", "-v", name]);
    if !r.ok() {
        return None;
    }
    let v = r.stdout.trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

pub fn set_option(name: &str, value: &str) -> TmuxResult {
    run(&["set-option", "-g", name, value])
}

// ----------------------------------------------------- naming

pub fn next_default_name() -> String {
    let existing: HashSet<String> = list_sessions().into_iter().map(|s| s.name).collect();
    for i in 1..=DEFAULT_NAME_MAX {
        let candidate = format!("{DEFAULT_NAME_PREFIX}-{i}");
        if !existing.contains(&candidate) {
            return candidate;
        }
    }
    // Pathological case: 999 ``tu-N`` sessions exist. Fall back to a
    // monotonic timestamp.
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{DEFAULT_NAME_PREFIX}-{ts}")
}

// -------------------------------------------------- pane helpers

/// Look up the session that owns the pane referenced by ``$TMUX_PANE``.
///
/// Used by the Detach flow so we can issue
/// ``tmux detach-client -s <session>`` instead of the no-arg form,
/// which can't resolve the right client when our process is the one
/// hosting tmux's view of the controlling tty.
pub fn current_pane_session() -> Option<String> {
    let pane = env::var("TMUX_PANE").ok()?;
    if pane.is_empty() {
        return None;
    }
    let r = run(&["display-message", "-p", "-t", &pane, "#S"]);
    if !r.ok() {
        return None;
    }
    let s = r.stdout.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

pub fn attach_argv(target: Option<&str>) -> Vec<String> {
    let mut argv = vec!["tmux".to_string(), "attach-session".to_string()];
    if let Some(t) = target {
        argv.push("-t".to_string());
        argv.push(t.to_string());
    }
    argv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attach_argv_includes_target_when_provided() {
        assert_eq!(
            attach_argv(Some("work")),
            vec!["tmux", "attach-session", "-t", "work"]
        );
    }

    #[test]
    fn attach_argv_omits_target_when_none() {
        assert_eq!(attach_argv(None), vec!["tmux", "attach-session"]);
    }
}
