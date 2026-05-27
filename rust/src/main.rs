//! `tu` binary entry point.
//!
//! After the TUI exits, if the app left an `attach-session`-style argv
//! behind we `execvp` into it so the parent shell ends up running
//! tmux directly. That replaces the `tu` process entirely, mirroring
//! the Python implementation's `__main__.py`.

use std::os::unix::process::CommandExt;
use std::process::{Command, ExitCode};

use clap::Parser;

use tmuxui::{app, tmux};

#[derive(Parser)]
#[command(
    name = "tu",
    version,
    about = "tu — a tiny tmux session menu (list, new, attach, detach, delete)."
)]
struct Cli {}

fn main() -> ExitCode {
    Cli::parse();

    if !tmux::is_tmux_installed() {
        eprintln!(
            "tu: 'tmux' was not found on $PATH. Install tmux (e.g. `brew install tmux` or \
             `apt install tmux`) and try again."
        );
        return ExitCode::from(2);
    }

    let outcome = app::run();

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
