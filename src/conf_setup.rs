//! Detect and (optionally) patch baseline directives in ``~/.tmux.conf``.
//!
//! Mirror of `tmuxui.conf_setup`. We manage three things:
//!
//! * ``set -g mouse on``                 — clicks and scrolling Just Work
//! * ``set -g history-limit 10000000``   — a roomy scrollback buffer
//! * a clickable ``tu`` button on the right of the status bar that
//!   opens `tu` in a full-screen popup (tmux ≥ 3.2 only)
//!
//! On every launch we re-read the conf and prompt if anything we
//! manage is still missing. If the user has already expressed an
//! intent (even via ``set -g mouse off``) we leave their file alone.
//! The status-bar button is recognised by its ``range=user|tu``
//! marker, so a hand-edited copy still counts as present.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use regex::Regex;

pub const HEADER_COMMENT: &str = "# Added by tu (https://github.com/hungryZoo/tu)";
pub const FOOTER_COMMENT: &str = "# End of tu";

/// The format marker that identifies our status-bar button. Anything
/// containing it counts as "button already configured".
pub const BUTTON_RANGE_MARKER: &str = "range=user|tu";

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

/// One unit of configuration we may offer to add to ``~/.tmux.conf``.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfItem {
    /// A single ``set -g <option> <value>`` line.
    Option(Directive),
    /// The clickable status-bar button: a ``status-right`` fragment
    /// plus a root-table mouse binding that opens `tu` in a popup.
    StatusButton {
        /// Pass ``-B`` to ``display-popup`` (tmux ≥ 3.3 drops the border).
        borderless: bool,
    },
}

impl ConfItem {
    /// Short name used in status / notice text.
    pub fn name(&self) -> String {
        match self {
            ConfItem::Option(d) => d.option.clone(),
            ConfItem::StatusButton { .. } => "status-bar button".into(),
        }
    }

    /// The exact lines written to the conf file for this item.
    pub fn lines(&self) -> Vec<String> {
        match self {
            ConfItem::Option(d) => vec![d.line()],
            ConfItem::StatusButton { borderless } => vec![
                "set -g status-right-length 60".into(),
                format!("set -ag status-right \"{}\"", button_fragment()),
                "bind -T root MouseDown1Status \\".into(),
                "  if -F '#{==:#{mouse_status_range},tu}' \\".into(),
                format!("    '{}' \\", popup_command(*borderless)),
                "    'select-window -t ='".into(),
            ],
        }
    }
}

/// The ``status-right`` fragment that draws the button. ``range=user|tu``
/// names the clickable region so the mouse binding can tell it apart
/// from the rest of the bar.
pub fn button_fragment() -> String {
    format!("#[{BUTTON_RANGE_MARKER}]#[fg=#1e1e2e,bg=#cba6f7,bold] tu #[norange]#[default]")
}

/// The tmux command the button runs: `tu` full-screen in a popup.
pub fn popup_command(borderless: bool) -> String {
    if borderless {
        "display-popup -E -B -w 100% -h 100% tu".into()
    } else {
        "display-popup -E -w 100% -h 100% tu".into()
    }
}

/// The if-shell condition matching a click on our button.
pub const BUTTON_CLICK_CONDITION: &str = "#{==:#{mouse_status_range},tu}";
/// What a click elsewhere on the status bar does (tmux's default).
pub const STATUS_CLICK_DEFAULT: &str = "select-window -t =";

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

fn button_present(text: &str) -> bool {
    // `#[range=...]` lives *inside* a quoted string, so the inline
    // comment stripping used for `set` lines would eat it. Only skip
    // whole-line comments here.
    text.lines()
        .any(|raw| !raw.trim_start().starts_with('#') && raw.contains(BUTTON_RANGE_MARKER))
}

/// Everything from the managed set that *path* doesn't configure yet.
///
/// `popup` says whether the running tmux can host popups at all; when
/// it can't we never offer the button. `borderless` picks the
/// ``display-popup`` flavour to write.
pub fn missing_items(path: &Path, popup: Option<PopupSupport>) -> Vec<ConfItem> {
    let text = fs::read_to_string(path).unwrap_or_default();
    let mut items: Vec<ConfItem> = managed()
        .into_iter()
        .filter(|d| !option_present(&text, &d.option))
        .map(ConfItem::Option)
        .collect();
    if let Some(support) = popup {
        if !button_present(&text) {
            items.push(ConfItem::StatusButton {
                borderless: support.borderless,
            });
        }
    }
    items
}

/// What the running tmux can do with popups, as far as the conf
/// writer cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PopupSupport {
    pub borderless: bool,
}

/// Probe the installed tmux: `None` below 3.2 (no popups at all).
pub fn detect_popup_support() -> Option<PopupSupport> {
    if !crate::tmux::supports_popup() {
        return None;
    }
    Some(PopupSupport {
        borderless: crate::tmux::supports_borderless_popup(),
    })
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
pub fn append_directives(directives: &[Directive], path: &Path) -> io::Result<()> {
    let items: Vec<ConfItem> = directives.iter().cloned().map(ConfItem::Option).collect();
    append_items(&items, path)
}

/// Append *items* to the end of *path* between the ``tu`` header and
/// footer comments.
///
/// Appending — not prepending — lets tmux's last-line-wins rule keep
/// our values authoritative even if an older conflicting directive
/// lives higher up. Creates the file (and any missing parent dirs).
pub fn append_items(items: &[ConfItem], path: &Path) -> io::Result<()> {
    if items.is_empty() {
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
    for item in items {
        for line in item.lines() {
            block.push_str(&line);
            block.push('\n');
        }
    }
    block.push_str(FOOTER_COMMENT);
    block.push('\n');
    fs::write(path, existing + &block)
}

/// Run ``tmux set-option -g <opt> <val>`` for each directive against
/// the *live* server. Returns `false` if there's no server or any
/// individual `set-option` failed.
pub fn apply_directives_to_server(directives: &[Directive]) -> bool {
    let items: Vec<ConfItem> = directives.iter().cloned().map(ConfItem::Option).collect();
    apply_items_to_server(&items)
}

/// Push *items* to the *live* server so they work without a restart.
/// Returns `false` if there's no server or any command failed.
pub fn apply_items_to_server(items: &[ConfItem]) -> bool {
    if items.is_empty() {
        return true;
    }
    if !crate::tmux::server_running() {
        return false;
    }
    let mut ok = true;
    for item in items {
        let applied = match item {
            ConfItem::Option(d) => crate::tmux::set_option(&d.option, &d.value).ok(),
            ConfItem::StatusButton { borderless } => {
                let fragment = button_fragment();
                let popup = popup_command(*borderless);
                crate::tmux::set_option("status-right-length", "60").ok()
                    && crate::tmux::append_option("status-right", &fragment).ok()
                    && crate::tmux::bind_root_mouse_status(
                        BUTTON_CLICK_CONDITION,
                        &popup,
                        STATUS_CLICK_DEFAULT,
                    )
                    .ok()
            }
        };
        if !applied {
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

    fn button() -> ConfItem {
        ConfItem::StatusButton { borderless: true }
    }

    #[test]
    fn missing_items_offers_button_only_with_popup_support() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_conf(&dir, "set -g mouse on\nset -g history-limit 5000\n");
        assert_eq!(missing_items(&p, None), vec![]);
        assert_eq!(
            missing_items(&p, Some(PopupSupport { borderless: true })),
            vec![button()]
        );
        assert_eq!(
            missing_items(&p, Some(PopupSupport { borderless: false })),
            vec![ConfItem::StatusButton { borderless: false }]
        );
    }

    #[test]
    fn button_is_detected_by_range_marker_even_when_hand_edited() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_conf(
            &dir,
            "set -g mouse on\nset -g history-limit 1\nset -g status-right '#[range=user|tu] TU #[norange]'\n",
        );
        assert!(missing_items(&p, Some(PopupSupport { borderless: true })).is_empty());
    }

    #[test]
    fn commented_out_button_does_not_count() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_conf(
            &dir,
            "set -g mouse on\nset -g history-limit 1\n# set -ag status-right '#[range=user|tu] tu #[norange]'\n",
        );
        assert_eq!(
            missing_items(&p, Some(PopupSupport { borderless: true })),
            vec![button()]
        );
    }

    #[test]
    fn append_items_writes_full_block_with_footer() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("tmux.conf");
        let items = vec![
            ConfItem::Option(mouse()),
            ConfItem::Option(history_limit()),
            button(),
        ];
        append_items(&items, &p).unwrap();
        let body = fs::read_to_string(&p).unwrap();
        let expected = "\
# Added by tu (https://github.com/hungryZoo/tu)
set -g mouse on
set -g history-limit 10000000
set -g status-right-length 60
set -ag status-right \"#[range=user|tu]#[fg=#1e1e2e,bg=#cba6f7,bold] tu #[norange]#[default]\"
bind -T root MouseDown1Status \\
  if -F '#{==:#{mouse_status_range},tu}' \\
    'display-popup -E -B -w 100% -h 100% tu' \\
    'select-window -t ='
# End of tu
";
        assert_eq!(body, expected);
        // And a second launch sees nothing left to add.
        assert!(missing_items(&p, Some(PopupSupport { borderless: true })).is_empty());
    }

    #[test]
    fn button_without_borderless_omits_dash_b() {
        let lines = ConfItem::StatusButton { borderless: false }.lines();
        assert!(lines
            .iter()
            .any(|l| l.contains("display-popup -E -w 100% -h 100% tu")));
        assert!(!lines.iter().any(|l| l.contains(" -B ")));
    }

    #[test]
    fn append_writes_block_at_end() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_conf(&dir, "set -g status on\nset -g history-limit 1000\n");
        append_directives(&[history_limit()], &p).unwrap();
        let body = fs::read_to_string(&p).unwrap();
        assert!(body.starts_with("set -g status on\nset -g history-limit 1000\n"));
        assert!(body.contains(HEADER_COMMENT));
        assert!(body.contains(&format!("{}\n{}", history_limit().line(), FOOTER_COMMENT)));
        assert!(body.trim_end().ends_with(FOOTER_COMMENT));
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
