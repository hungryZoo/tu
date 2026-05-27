//! Detect and (optionally) patch baseline directives in ``~/.tmux.conf``.
//!
//! Mirror of `tmuxui.conf_setup`. We currently manage two directives:
//!
//! * ``set -g mouse on``                 — clicks and scrolling Just Work
//! * ``set -g history-limit 10000000``   — a roomy scrollback buffer
//!
//! On every launch we re-read the conf and prompt if anything we
//! manage is still missing. If the user has already expressed an
//! intent (even via ``set -g mouse off``) we leave their file alone.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use regex::Regex;

pub const HEADER_COMMENT: &str = "# Added by tu (https://github.com/hungryZoo/tu)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directive {
    pub option: String,
    pub value: String,
    pub label: String,
}

impl Directive {
    pub fn line(&self) -> String {
        format!("set -g {} {}", self.option, self.value)
    }
}

pub fn mouse() -> Directive {
    Directive {
        option: "mouse".into(),
        value: "on".into(),
        label: "mouse support".into(),
    }
}

pub fn history_limit() -> Directive {
    Directive {
        option: "history-limit".into(),
        value: "10000000".into(),
        label: "scrollback size".into(),
    }
}

/// The directives `tu` knows how to enforce, in the order they should
/// appear to the user (and in the file).
pub fn managed() -> Vec<Directive> {
    vec![mouse(), history_limit()]
}

pub fn conf_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".tmux.conf")
}

// ----------------------------------------------------- detection

fn option_matcher(option: &str) -> Regex {
    // `set` (and the long form `set-option`) with an optional `-g`,
    // followed by the option name as a whole word. We deliberately do
    // NOT accept `setw` / `set-window-option` because the directives
    // we manage are session-scoped.
    Regex::new(&format!(
        r"(?i)^\s*set(-option)?\s+(-g\s+)?{}\b",
        regex::escape(option),
    ))
    .expect("regex compile")
}

fn option_present(text: &str, option: &str) -> bool {
    let matcher = option_matcher(option);
    for line in text.lines() {
        // Strip the inline-comment tail before matching so a `# set -g
        // mouse on` example doesn't count as a real directive.
        let body = line.split('#').next().unwrap_or("");
        if matcher.is_match(body) {
            return true;
        }
    }
    false
}

pub fn missing_directives(path: &Path) -> Vec<Directive> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return managed(),
    };
    managed()
        .into_iter()
        .filter(|d| !option_present(&text, &d.option))
        .collect()
}

pub fn should_prompt(path: &Path) -> bool {
    !missing_directives(path).is_empty()
}

/// Return the value of any ``set [-g] mouse on|off`` line, or `None`
/// if the conf doesn't configure mouse mode at all. Kept around so
/// the existing Python test suite's expectations can be reused.
pub fn conf_has_mouse_directive(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let matcher = option_matcher("mouse");
    for line in text.lines() {
        let body = line.split('#').next().unwrap_or("");
        if let Some(m) = matcher.find(body) {
            let tail = body[m.end()..].trim();
            if let Some(first) = tail.split_whitespace().next() {
                let val = first.to_lowercase();
                if val == "on" || val == "off" {
                    return Some(val);
                }
            }
        }
    }
    None
}

// ------------------------------------------------------ mutation

/// Append *directives* to the end of *path* under a ``tu`` header.
///
/// Appending — not prepending — lets tmux's last-line-wins rule keep
/// our values authoritative even if an older conflicting directive
/// lives higher up. Creates the file (and any missing parent dirs).
pub fn append_directives(directives: &[Directive], path: &Path) -> io::Result<()> {
    if directives.is_empty() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut existing = fs::read_to_string(path).unwrap_or_default();
    if !existing.is_empty() && !existing.ends_with('\n') {
        existing.push('\n');
    }
    let mut block = String::new();
    if !existing.is_empty() {
        block.push('\n'); // visual break between user content and our block
    }
    block.push_str(HEADER_COMMENT);
    block.push('\n');
    for d in directives {
        block.push_str(&d.line());
        block.push('\n');
    }
    fs::write(path, existing + &block)
}

/// Run ``tmux set-option -g <opt> <val>`` for each directive against
/// the *live* server. Returns `false` if there's no server or any
/// individual `set-option` failed.
pub fn apply_directives_to_server(directives: &[Directive]) -> bool {
    if directives.is_empty() {
        return true;
    }
    if !crate::tmux::server_running() {
        return false;
    }
    let mut ok = true;
    for d in directives {
        if !crate::tmux::set_option(&d.option, &d.value).ok() {
            ok = false;
        }
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_conf(dir: &tempfile::TempDir, contents: &str) -> PathBuf {
        let p = dir.path().join("tmux.conf");
        let mut f = fs::File::create(&p).expect("create");
        f.write_all(contents.as_bytes()).expect("write");
        p
    }

    #[test]
    fn missing_returns_all_when_file_absent() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("nope.conf");
        assert_eq!(missing_directives(&p), managed());
    }

    #[test]
    fn detects_partial_config() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_conf(&dir, "set -g mouse on\n");
        assert_eq!(missing_directives(&p), vec![history_limit()]);
    }

    #[test]
    fn treats_explicit_off_as_configured() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_conf(&dir, "set -g mouse off\nset -g history-limit 5000\n");
        assert!(missing_directives(&p).is_empty());
    }

    #[test]
    fn ignores_setw_and_inline_comments() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_conf(
            &dir,
            "# set -g mouse on\nsetw -g mouse on\nset-option -g history-limit 42\n",
        );
        // mouse still missing; history-limit configured via set-option.
        assert_eq!(missing_directives(&p), vec![mouse()]);
    }

    #[test]
    fn append_writes_block_at_end() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_conf(&dir, "set -g status on\nset -g history-limit 1000\n");
        append_directives(&[history_limit()], &p).unwrap();
        let body = fs::read_to_string(&p).unwrap();
        assert!(body.starts_with("set -g status on\nset -g history-limit 1000\n"));
        assert!(body.contains(HEADER_COMMENT));
        assert!(body.trim_end().ends_with(&history_limit().line()));
    }

    #[test]
    fn append_creates_missing_file_and_parents() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("nested").join("tmux.conf");
        append_directives(&managed(), &p).unwrap();
        let body = fs::read_to_string(&p).unwrap();
        assert!(body.contains(&mouse().line()));
        assert!(body.contains(&history_limit().line()));
    }

    #[test]
    fn append_is_a_noop_for_empty_input() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_conf(&dir, "set -g status on\n");
        append_directives(&[], &p).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "set -g status on\n");
    }

    #[test]
    fn conf_has_mouse_directive_handles_on_off_and_missing() {
        // Each tempdir must be bound to a local so it outlives the
        // call below — unbound temporaries get dropped before the
        // assertion runs, which deletes the conf file we just wrote.
        let on_dir = tempfile::tempdir().unwrap();
        let on = write_conf(&on_dir, "set -g mouse on\n");
        let off_dir = tempfile::tempdir().unwrap();
        let off = write_conf(&off_dir, "set-option -g mouse off\n");
        let missing_dir = tempfile::tempdir().unwrap();
        let missing = write_conf(&missing_dir, "# set -g mouse on\nset -g status on\n");
        assert_eq!(conf_has_mouse_directive(&on), Some("on".into()));
        assert_eq!(conf_has_mouse_directive(&off), Some("off".into()));
        assert_eq!(conf_has_mouse_directive(&missing), None);
    }
}
