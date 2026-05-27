"""Entry point for the ``tu`` command."""

from __future__ import annotations

import argparse
import os
import sys

from . import __version__
from .app import TmuxUIApp
from .tmux import is_tmux_installed


def _parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="tu",
        description="tu — a tiny tmux session menu.",
    )
    parser.add_argument(
        "--version",
        action="version",
        version=f"tu (tmuxui) {__version__}",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    _parse_args(argv)

    if not is_tmux_installed():
        print(
            "tu: 'tmux' not found on PATH. Install tmux (e.g. brew install tmux) "
            "and try again.",
            file=sys.stderr,
        )
        return 2

    app = TmuxUIApp()
    app.run()

    # The Attach / New buttons from *outside* tmux defer the actual
    # ``tmux attach-session`` to here so it happens after Textual has
    # restored the terminal. ``execvp`` replaces the Python process; tmux
    # owns the parent shell from this point on.
    if app.post_exit_argv:
        try:
            os.execvp(app.post_exit_argv[0], app.post_exit_argv)
        except OSError as exc:  # pragma: no cover — defensive
            print(
                f"tu: failed to launch {' '.join(app.post_exit_argv)}: {exc}",
                file=sys.stderr,
            )
            return 1
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
