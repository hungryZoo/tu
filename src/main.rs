//! `tu` binary entry point.
//!
//! Two launch paths:
//!
//! * Inside a tmux pane on tmux ≥ 3.2 we don't draw at all — we ask
//!   the server to re-run this same binary in a full-screen
//!   `display-popup` and exit. The popup instance sees ``$TMUX`` but
//!   no ``$TMUX_PANE``, so it renders inline and never recurses.
//! * Everywhere else (outside tmux, inside a popup, old tmux, or
//!   `--no-popup`) we run the TUI in the current terminal.
//!
//! After the TUI exits, if the app left an `attach-session`-style argv
//! behind we `execvp` into it so the parent shell ends up running
//! tmux directly. That replaces the `tu` process entirely, mirroring
//! the Python implementation's `__main__.py`.

use std::os::unix::process::CommandExt;
use std::process::{Command, ExitCode};

use clap::Parser;

use tmux_tu::{app, tmux};

#[derive(Parser)]
#[command(
    name = "tu",
    version,
    about = "tu — a tiny tmux session menu (list, new, attach, detach, delete)."
)]
struct Cli {
    /// Run inline in the current pane instead of opening a full-screen
    /// tmux popup (the default when launched from inside tmux ≥ 3.2).
    /// Setting TU_NO_POPUP=1 has the same effect.
    #[arg(long)]
    no_popup: bool,
}

fn env_no_popup() -> bool {
    std::env::var("TU_NO_POPUP")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false)
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if !tmux::is_tmux_installed() {
        eprintln!(
            "tu: 'tmux' was not found on $PATH. Install tmux (e.g. `brew install tmux` or \
             `apt install tmux`) and try again."
        );
        return ExitCode::from(2);
    }

    if !cli.no_popup && !env_no_popup() && tmux::is_inside_pane() && tmux::supports_popup() {
        let r = tmux::open_self_in_popup();
        if r.ok() {
            return ExitCode::SUCCESS;
        }
        // Popup refused (e.g. no attached client). Fall through and
        // draw inline rather than leaving the user with nothing.
        eprintln!(
            "tu: popup unavailable ({}), running inline.",
            r.stderr.trim()
        );
    }

    let outcome = match app::run() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("tu: terminal i/o error: {e}");
            return ExitCode::from(1);
        }
    };

    let Some(argv) = outcome.post_exit_argv else {
        return ExitCode::SUCCESS;
    };
    if argv.is_empty() {
        return ExitCode::SUCCESS;
    }

    // `exec` only returns on failure — on success we never reach here.
    let err = Command::new(&argv[0]).args(&argv[1..]).exec();
    eprintln!("tu: failed to launch `{}`: {err}", argv.join(" "));
    ExitCode::from(1)
}
