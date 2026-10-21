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

use tmux_tu::{app, tmux, update, view};

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

    /// Ask GitHub for the newest release right now, print the result,
    /// and exit. Exit code 0 when up to date, 10 when an update is
    /// available, 11 when GitHub could not be reached. Also refreshes
    /// the cache the TUI's status-row note reads from.
    #[arg(long)]
    check_update: bool,

    /// Check for a newer release and, if there is one, upgrade in place
    /// using the method this binary was installed with (Homebrew, cargo,
    /// install.sh, .deb or .rpm). Exit code 0 when done or already up to
    /// date, 11 when GitHub could not be reached, 1 when the upgrade
    /// command failed.
    #[arg(long)]
    update: bool,

    /// Window size as a percentage of the terminal (10-100). Inside tmux
    /// this is the popup size, drawn over the live panes; outside tmux
    /// it is a centred window on a blank screen. TU_SIZE sets the same.
    #[arg(long, value_name = "PCT")]
    size: Option<u8>,
}

fn window_size(cli: &Cli) -> u8 {
    cli.size
        .or_else(|| std::env::var("TU_SIZE").ok().and_then(|v| v.parse().ok()))
        .unwrap_or(view::DEFAULT_WINDOW_PCT)
        .clamp(10, 100)
}

fn env_no_popup() -> bool {
    std::env::var("TU_NO_POPUP")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false)
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if cli.update {
        let outcome = update::check_now();
        println!("{}", update::report(&outcome, update::CURRENT_VERSION));
        return match outcome {
            update::CheckOutcome::UpdateAvailable { .. } => {
                let method = update::detect_install_method();
                match method.upgrade_command() {
                    Some(cmd) => run_upgrade_and_report(&cmd),
                    None => {
                        eprintln!(
                            "tu: can't tell how this binary was installed; update it by hand."
                        );
                        ExitCode::from(1)
                    }
                }
            }
            update::CheckOutcome::UpToDate { .. } => ExitCode::SUCCESS,
            update::CheckOutcome::Unreachable => ExitCode::from(11),
        };
    }

    if cli.check_update {
        let outcome = update::check_now();
        println!("{}", update::report(&outcome, update::CURRENT_VERSION));
        return match outcome {
            update::CheckOutcome::UpToDate { .. } => ExitCode::SUCCESS,
            update::CheckOutcome::UpdateAvailable { .. } => ExitCode::from(10),
            update::CheckOutcome::Unreachable => ExitCode::from(11),
        };
    }

    if !tmux::is_tmux_installed() {
        eprintln!(
            "tu: 'tmux' was not found on $PATH. Install tmux (e.g. `brew install tmux` or \
             `apt install tmux`) and try again."
        );
        return ExitCode::from(2);
    }

    let size = window_size(&cli);

    if !cli.no_popup && !env_no_popup() && tmux::is_inside_pane() && tmux::supports_popup() {
        // A popup's size is fixed when it opens, so decide now: on a
        // small client (e.g. an editor's terminal panel) use all of it.
        let popup_pct = tmux::client_size()
            .map(|(w, h)| view::popup_pct(w, h, size))
            .unwrap_or(size);
        let r = tmux::open_self_in_popup(popup_pct, size);
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

    // Inside a popup the popup *is* the window, and inline in a pane
    // (--no-popup) the pane is; only a bare terminal gets the centred
    // window. A popup still needs the label cap its size implies: plan
    // it from the client with the requested size, exactly as the pane
    // instance (or the status-bar binding) did when opening it.
    let (window_pct, label_cap) = if tmux::is_inside_pane() {
        (100, view::LabelTier::Full)
    } else if tmux::is_inside_tmux() {
        let cap = tmux::client_size()
            .map(|(w, h)| view::plan_layout(w, h, size).label_cap)
            .unwrap_or(view::LabelTier::Full);
        (100, cap)
    } else {
        (size, view::LabelTier::Full)
    };
    let outcome = match app::run(window_pct, label_cap) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("tu: terminal i/o error: {e}");
            return ExitCode::from(1);
        }
    };

    if let Some(cmd) = outcome.post_exit_command {
        return run_upgrade_and_report(&cmd);
    }

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

/// Run the upgrade with the terminal attached (sudo may prompt) and
/// say how it went. The binary on disk is replaced, so we exit.
fn run_upgrade_and_report(cmd: &str) -> ExitCode {
    println!("tu: running: {cmd}");
    match update::run_upgrade(cmd) {
        Ok(status) if status.success() => {
            println!("tu: updated. Start `tu` again to use the new version.");
            ExitCode::SUCCESS
        }
        Ok(status) => {
            eprintln!("tu: upgrade command exited with {status}");
            ExitCode::from(1)
        }
        Err(e) => {
            eprintln!("tu: could not run the upgrade command: {e}");
            ExitCode::from(1)
        }
    }
}
