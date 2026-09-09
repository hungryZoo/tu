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

/// True when we're running inside a real tmux *pane* (as opposed to
/// a popup or a `run-shell` job). tmux exports ``$TMUX_PANE`` only
/// for pane processes, so its absence while ``$TMUX`` is set means
/// we're already hosted by `display-popup` and must not open another.
pub fn is_inside_pane() -> bool {
    is_inside_tmux()
        && env::var("TMUX_PANE")
            .map(|v| !v.is_empty())
            .unwrap_or(false)
}

// ----------------------------------------------------- version

/// Parse the ``tmux -V`` banner into ``(major, minor)``.
///
/// Handles the shapes tmux actually emits: ``tmux 3.4``, ``tmux 3.3a``,
/// ``tmux next-3.5``. Anything unparsable (e.g. ``tmux master``)
/// yields `None`, which callers treat as "assume nothing".
pub fn parse_version(banner: &str) -> Option<(u32, u32)> {
    let token = banner.split_whitespace().last()?;
    let token = token.strip_prefix("next-").unwrap_or(token);
    let mut parts = token.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor_raw = parts.next()?;
    let digits: String = minor_raw
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let minor: u32 = digits.parse().ok()?;
    Some((major, minor))
}

pub fn version() -> Option<(u32, u32)> {
    let out = Command::new("tmux").arg("-V").output().ok()?;
    if !out.status.success() {
        return None;
    }
    parse_version(&String::from_utf8_lossy(&out.stdout))
}

/// ``display-popup`` arrived in tmux 3.2.
pub fn supports_popup() -> bool {
    matches!(version(), Some(v) if v >= (3, 2))
}

/// ``display-popup -B`` (no border) arrived in tmux 3.3.
pub fn supports_borderless_popup() -> bool {
    matches!(version(), Some(v) if v >= (3, 3))
}

// ----------------------------------------------------- popup

/// Build the ``display-popup`` argv that re-launches *exe* full-screen
/// over the current client. The popup closes when the command exits
/// (`-E`); the border is dropped when the server is new enough.
pub fn popup_argv(exe: &str, borderless: bool) -> Vec<String> {
    let mut argv: Vec<String> = vec!["display-popup".into(), "-E".into()];
    if borderless {
        argv.push("-B".into());
    }
    argv.extend(["-w", "100%", "-h", "100%"].iter().map(|s| s.to_string()));
    argv.push(shell_quote(exe));
    argv
}

/// Re-exec ourselves inside a full-screen tmux popup. Returns the
/// tmux result; the popup itself runs asynchronously on the server,
/// so this call comes back as soon as tmux has accepted the request.
pub fn open_self_in_popup() -> TmuxResult {
    let exe = env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "tu".to_string());
    let argv = popup_argv(&exe, supports_borderless_popup());
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run(&refs)
}

/// Single-quote *s* for ``sh -c`` — tmux runs popup commands through
/// the user's shell, so a path with spaces must be protected.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
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

/// ``tmux set-option -ga <name> <value>`` — append to a string option.
pub fn append_option(name: &str, value: &str) -> TmuxResult {
    run(&["set-option", "-ga", name, value])
}

/// Bind a left click on the status line in the root table:
/// ``bind -T root MouseDown1Status if -F <cond> <then> <else>``.
pub fn bind_root_mouse_status(condition: &str, then: &str, otherwise: &str) -> TmuxResult {
    run(&[
        "bind-key",
        "-T",
        "root",
        "MouseDown1Status",
        "if-shell",
        "-F",
        condition,
        then,
        otherwise,
    ])
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

    #[test]
    fn parse_version_handles_release_banners() {
        assert_eq!(parse_version("tmux 3.4"), Some((3, 4)));
        assert_eq!(parse_version("tmux 3.3a"), Some((3, 3)));
        assert_eq!(parse_version("tmux 2.9a"), Some((2, 9)));
        assert_eq!(parse_version("tmux next-3.5"), Some((3, 5)));
        assert_eq!(parse_version("tmux 3.4\n"), Some((3, 4)));
    }

    #[test]
    fn parse_version_rejects_unknown_shapes() {
        assert_eq!(parse_version("tmux master"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn popup_argv_is_fullscreen_and_closes_on_exit() {
        assert_eq!(
            popup_argv("/usr/bin/tu", true),
            vec![
                "display-popup",
                "-E",
                "-B",
                "-w",
                "100%",
                "-h",
                "100%",
                "'/usr/bin/tu'"
            ]
        );
        assert_eq!(
            popup_argv("/usr/bin/tu", false),
            vec![
                "display-popup",
                "-E",
                "-w",
                "100%",
                "-h",
                "100%",
                "'/usr/bin/tu'"
            ]
        );
    }

    #[test]
    fn popup_argv_quotes_paths_with_spaces() {
        let argv = popup_argv("/Users/me/my bin/tu", false);
        assert_eq!(
            argv.last().map(String::as_str),
            Some("'/Users/me/my bin/tu'")
        );
        let argv = popup_argv("/it's/tu", false);
        assert_eq!(argv.last().map(String::as_str), Some("'/it'\\''s/tu'"));
    }
}
