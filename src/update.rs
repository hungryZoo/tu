//! Lightweight "a newer tu exists" check.
//!
//! Design goals, in order: never slow down launch, never nag, never
//! touch the network more than once a day.
//!
//! * The check runs on a background thread and shells out to `curl`
//!   with a two-second cap; the TUI never waits for it. Whatever it
//!   finds is written to a small cache file and shown on the *next*
//!   launch as a one-line note on the status row. No modal, no prompt.
//! * At most one attempt per 24 h. The attempt is recorded *before*
//!   the request goes out, so an early exit or a dead network can't
//!   turn into a retry storm.
//! * `TU_NO_UPDATE_CHECK=1` disables everything, including the cache
//!   read. No `curl` on the box also disables it, silently.
//!
//! The upstream query is the `/releases/latest` redirect, which needs
//! no API quota and returns nothing but a tag name.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const LATEST_URL: &str = "https://github.com/hungryZoo/tu/releases/latest";
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const CURL_TIMEOUT_SECS: &str = "2";

/// Parsed `MAJOR.MINOR.PATCH`. Anything else (pre-release suffixes,
/// `v` prefixes are stripped first) is rejected.
pub fn parse_semver(s: &str) -> Option<(u32, u32, u32)> {
    let s = s.trim().strip_prefix('v').unwrap_or(s.trim());
    let mut it = s.split('.');
    let a = it.next()?.parse().ok()?;
    let b = it.next()?.parse().ok()?;
    let c = it.next()?.parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some((a, b, c))
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_semver(candidate), parse_semver(current)) {
        (Some(c), Some(cur)) => c > cur,
        _ => false,
    }
}

pub fn is_disabled() -> bool {
    env::var("TU_NO_UPDATE_CHECK")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false)
}

// ----------------------------------------------------------- cache

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cache {
    /// Unix seconds of the last *attempt* (successful or not).
    pub checked_at: u64,
    /// Newest version seen upstream, without the `v`.
    pub latest: Option<String>,
}

impl Cache {
    pub fn parse(text: &str) -> Cache {
        let mut c = Cache::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            match k.trim() {
                "checked_at" => c.checked_at = v.trim().parse().unwrap_or(0),
                "latest" => {
                    let v = v.trim();
                    if !v.is_empty() {
                        c.latest = Some(v.to_string());
                    }
                }
                _ => {}
            }
        }
        c
    }

    pub fn render(&self) -> String {
        format!(
            "checked_at={}\nlatest={}\n",
            self.checked_at,
            self.latest.as_deref().unwrap_or("")
        )
    }

    pub fn is_stale(&self, now: u64) -> bool {
        self.checked_at == 0 || now.saturating_sub(self.checked_at) >= CHECK_INTERVAL.as_secs()
    }
}

pub fn cache_path() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".cache")
        })
        .join("tu")
        .join("update-check")
}

pub fn read_cache(path: &Path) -> Cache {
    fs::read_to_string(path)
        .map(|t| Cache::parse(&t))
        .unwrap_or_default()
}

pub fn write_cache(path: &Path, cache: &Cache) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, cache.render())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ----------------------------------------------------------- network

/// Resolve the newest release tag by following the `/releases/latest`
/// redirect. Returns the version without its `v`.
fn fetch_latest() -> Option<String> {
    let out = Command::new("curl")
        .args([
            "-fsSLI",
            "--max-time",
            CURL_TIMEOUT_SECS,
            "-o",
            "/dev/null",
            "-w",
            "%{url_effective}",
            LATEST_URL,
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&out.stdout);
    version_from_release_url(url.trim())
}

pub fn version_from_release_url(url: &str) -> Option<String> {
    let tag = url.rsplit("/tag/").next()?;
    if tag == url {
        return None;
    }
    let v = tag.trim().trim_start_matches('v');
    parse_semver(v).map(|_| v.to_string())
}

/// Outcome of a synchronous check, for `tu --check-update`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    /// Newest release is newer than this binary.
    UpdateAvailable { latest: String, hint: &'static str },
    /// This binary is the newest release (or newer than any release).
    UpToDate { latest: String },
    /// Could not reach GitHub, or `curl` is missing.
    Unreachable,
}

/// Check right now, ignoring the 24 h interval, and refresh the cache
/// with the answer. Used by `tu --check-update`; the TUI never calls
/// this because it blocks for up to the curl timeout.
pub fn check_now() -> CheckOutcome {
    let path = cache_path();
    let mut cache = read_cache(&path);
    cache.checked_at = now_secs();
    match fetch_latest() {
        Some(latest) => {
            cache.latest = Some(latest.clone());
            let _ = write_cache(&path, &cache);
            if is_newer(&latest, CURRENT_VERSION) {
                CheckOutcome::UpdateAvailable {
                    latest,
                    hint: upgrade_hint(),
                }
            } else {
                CheckOutcome::UpToDate { latest }
            }
        }
        None => {
            let _ = write_cache(&path, &cache);
            CheckOutcome::Unreachable
        }
    }
}

/// Human-readable report for `tu --check-update`.
pub fn report(outcome: &CheckOutcome, current: &str) -> String {
    match outcome {
        CheckOutcome::UpdateAvailable { latest, hint } => {
            format!("tu {current} → {latest} available\nupgrade: {hint}\nrelease: {LATEST_URL}")
        }
        CheckOutcome::UpToDate { latest } => {
            format!("tu {current} is up to date (latest release: {latest})")
        }
        CheckOutcome::Unreachable => {
            format!("tu {current}: could not reach {LATEST_URL} (offline, or curl missing)")
        }
    }
}

/// Kick off a background check if the cache is older than a day.
/// Returns immediately. Safe to call on every launch.
pub fn maybe_spawn_check() {
    if is_disabled() {
        return;
    }
    let path = cache_path();
    let mut cache = read_cache(&path);
    let now = now_secs();
    if !cache.is_stale(now) {
        return;
    }
    // Record the attempt first so a killed process or a dead network
    // doesn't mean "try again on the very next launch".
    cache.checked_at = now;
    if write_cache(&path, &cache).is_err() {
        return;
    }
    thread::spawn(move || {
        if let Some(latest) = fetch_latest() {
            cache.latest = Some(latest);
            let _ = write_cache(&path, &cache);
        }
    });
}

// ----------------------------------------------------------- notice

/// How to upgrade, guessed from where this binary lives.
pub fn upgrade_hint_for(exe: &Path, home: Option<&Path>) -> &'static str {
    let s = exe.to_string_lossy();
    if s.contains("/Cellar/") || s.contains("/homebrew/") || s.contains("/linuxbrew/") {
        return "brew upgrade tu";
    }
    if let Some(home) = home {
        if exe.starts_with(home.join(".cargo").join("bin")) {
            return "cargo install tmux-tu";
        }
        if exe.starts_with(home.join(".local").join("bin")) || exe.starts_with(home.join("bin")) {
            return "re-run install.sh";
        }
    }
    if s.starts_with("/usr/bin/") {
        return "github.com/hungryZoo/tu/releases/latest";
    }
    "github.com/hungryZoo/tu/releases/latest"
}

pub fn upgrade_hint() -> &'static str {
    let exe = env::current_exe().unwrap_or_default();
    let home = dirs::home_dir();
    upgrade_hint_for(&exe, home.as_deref())
}

pub fn notice_text(latest: &str, hint: &str) -> String {
    format!("v{latest} available · {hint}")
}

/// The one-line note for the status row, or `None` when up to date,
/// disabled, or never checked.
pub fn notice() -> Option<String> {
    if is_disabled() {
        return None;
    }
    let cache = read_cache(&cache_path());
    let latest = cache.latest?;
    if !is_newer(&latest, CURRENT_VERSION) {
        return None;
    }
    Some(notice_text(&latest, upgrade_hint()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_parse_and_compare() {
        assert_eq!(parse_semver("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_semver("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_semver("1.2"), None);
        assert_eq!(parse_semver("1.2.3-rc1"), None);
        assert!(is_newer("1.1.2", "1.1.1"));
        assert!(is_newer("1.2.0", "1.1.9"));
        assert!(!is_newer("1.1.1", "1.1.1"));
        assert!(!is_newer("1.0.9", "1.1.0"));
        assert!(!is_newer("garbage", "1.1.0"));
    }

    #[test]
    fn version_from_redirect_url() {
        assert_eq!(
            version_from_release_url("https://github.com/hungryZoo/tu/releases/tag/v1.1.1"),
            Some("1.1.1".into())
        );
        assert_eq!(
            version_from_release_url("https://github.com/hungryZoo/tu/releases/latest"),
            None
        );
        assert_eq!(version_from_release_url(""), None);
    }

    #[test]
    fn cache_round_trip_and_staleness() {
        let c = Cache {
            checked_at: 1_700_000_000,
            latest: Some("1.2.0".into()),
        };
        assert_eq!(Cache::parse(&c.render()), c);
        assert_eq!(Cache::parse(""), Cache::default());
        assert_eq!(Cache::parse("checked_at=5\nlatest=\n").latest, None);
        assert!(!c.is_stale(1_700_000_000 + 3600));
        assert!(c.is_stale(1_700_000_000 + CHECK_INTERVAL.as_secs()));
        assert!(Cache::default().is_stale(1));
    }

    #[test]
    fn cache_file_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("nested").join("update-check");
        let c = Cache {
            checked_at: 42,
            latest: Some("9.9.9".into()),
        };
        write_cache(&p, &c).unwrap();
        assert_eq!(read_cache(&p), c);
        assert_eq!(read_cache(&dir.path().join("missing")), Cache::default());
    }

    #[test]
    fn upgrade_hint_follows_install_location() {
        let home = Path::new("/Users/me");
        let h = |p: &str| upgrade_hint_for(Path::new(p), Some(home));
        assert_eq!(h("/opt/homebrew/bin/tu"), "brew upgrade tu");
        assert_eq!(h("/usr/local/Cellar/tu/1.1.1/bin/tu"), "brew upgrade tu");
        assert_eq!(h("/home/linuxbrew/.linuxbrew/bin/tu"), "brew upgrade tu");
        assert_eq!(h("/Users/me/.cargo/bin/tu"), "cargo install tmux-tu");
        assert_eq!(h("/Users/me/.local/bin/tu"), "re-run install.sh");
        assert_eq!(h("/usr/bin/tu"), "github.com/hungryZoo/tu/releases/latest");
        assert_eq!(
            upgrade_hint_for(Path::new("/weird/tu"), None),
            "github.com/hungryZoo/tu/releases/latest"
        );
    }

    #[test]
    fn report_covers_every_outcome() {
        let up = report(
            &CheckOutcome::UpdateAvailable {
                latest: "1.2.0".into(),
                hint: "brew upgrade tu",
            },
            "1.1.2",
        );
        assert!(up.contains("1.1.2 → 1.2.0"));
        assert!(up.contains("brew upgrade tu"));
        assert!(up.contains(LATEST_URL));
        let same = report(
            &CheckOutcome::UpToDate {
                latest: "1.1.2".into(),
            },
            "1.1.2",
        );
        assert!(same.contains("up to date"));
        assert!(report(&CheckOutcome::Unreachable, "1.1.2").contains("could not reach"));
    }

    #[test]
    fn notice_text_is_one_short_line() {
        let n = notice_text("1.2.0", "brew upgrade tu");
        assert_eq!(n, "v1.2.0 available · brew upgrade tu");
        assert!(n.chars().count() < 60);
    }
}
