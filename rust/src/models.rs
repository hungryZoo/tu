//! Session data model + tab-delimited parser, mirroring `tmuxui.models`.

pub const SEP: char = '\t';

/// Format string for ``tmux list-sessions -F`` — must match the
/// 3-column layout `parse_sessions` expects.
pub const SESSION_FORMAT: &str = "#{session_name}\t#{session_windows}\t#{?session_attached,1,0}";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub name: String,
    pub windows: u32,
    pub attached: bool,
}

impl Session {
    pub fn target(&self) -> &str {
        &self.name
    }
}

/// Parse the output of ``tmux list-sessions -F <SESSION_FORMAT>``.
///
/// Lines that are blank or have fewer than three fields are skipped
/// silently; tmux occasionally emits trailing blanks between session
/// blocks and we don't want them to break the table.
pub fn parse_sessions(text: &str) -> Vec<Session> {
    text.lines()
        .filter_map(|raw| {
            let line = raw.trim_end();
            if line.trim().is_empty() {
                return None;
            }
            let mut parts = line.splitn(3, SEP);
            let name = parts.next()?.to_string();
            let windows: u32 = parts.next()?.parse().ok()?;
            let attached = matches!(parts.next()?, "1");
            Some(Session {
                name,
                windows,
                attached,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, windows: u32, attached: u32) -> String {
        format!("{name}{SEP}{windows}{SEP}{attached}")
    }

    #[test]
    fn parses_three_rows() {
        let output = [row("work", 3, 1), row("play", 1, 0), row("scratch", 2, 0)].join("\n");
        let sessions = parse_sessions(&output);
        assert_eq!(sessions.len(), 3);
        assert_eq!(sessions[0].name, "work");
        assert_eq!(sessions[0].windows, 3);
        assert!(sessions[0].attached);
        assert!(!sessions[1].attached);
        assert_eq!(sessions[2].target(), "scratch");
    }

    #[test]
    fn skips_blank_and_short_lines() {
        let output = ["", "   ", &row("solo", 1, 0), "broken"].join("\n");
        let sessions = parse_sessions(&output);
        assert_eq!(
            sessions,
            vec![Session {
                name: "solo".into(),
                windows: 1,
                attached: false,
            }]
        );
    }

    #[test]
    fn handles_empty_string() {
        assert!(parse_sessions("").is_empty());
    }
}
