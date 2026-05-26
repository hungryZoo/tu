"""Entry point for the ``tu`` command.

Branching rules (see plan):

1. ``--popup`` flag or ``TMUXUI_POPUP=1`` env → run the popup-mode app
   directly (we're already inside ``tmux display-popup -E``).
2. ``$TMUX`` is set but we *weren't* spawned from display-popup → re-exec
   ourselves via ``tmux display-popup -E tu --popup`` so the user sees a
   floating overlay instead of taking over the current pane.
3. Otherwise → Hub mode, full-screen TUI. While Hub runs we register a tmux
   key binding (default ``F12``) that opens the same popup from anywhere.
"""

from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path
from typing import NoReturn

from . import __version__
from .app import TmuxUIApp
from .hotkey import DEFAULT_KEY, DEFAULT_POPUP_COMMAND, HotkeyInstaller
from .tmux import TmuxClient, is_inside_tmux, is_tmux_installed


def _parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="tu",
        description=(
            "tu — a friendly TUI on top of tmux. Outside tmux you get a "
            "fullscreen hub; inside tmux you get a floating popup. F12 "
            "opens the popup from anywhere while the hub is running."
        ),
    )
    parser.add_argument(
        "--popup",
        action="store_true",
        help="Run popup mode (used by the F12 binding; rarely needed manually).",
    )
    parser.add_argument(
        "--key",
        default=DEFAULT_KEY,
        help=f"Hotkey to open the popup from inside tmux (default: {DEFAULT_KEY}).",
    )
    parser.add_argument(
        "--stay",
        action="store_true",
        help="Popup mode: keep the popup open after each action.",
    )
    parser.add_argument(
        "--version",
        action="version",
        version=f"tu (tmuxui) {__version__}",
    )
    return parser.parse_args(argv)


def _eprint(*parts: object) -> None:
    print(*parts, file=sys.stderr)


def _exec_popup(popup_command: str) -> NoReturn:
    """Replace this process with ``tmux display-popup -E POPUP_COMMAND``."""

    # Set the re-entry guard so the popup-side ``tu`` short-circuits to popup
    # mode rather than recursing into another display-popup.
    os.environ["TMUXUI_POPUP"] = "1"
    argv = [
        "tmux",
        "display-popup",
        "-E",
        "-h",
        "80%",
        "-w",
        "80%",
        "-T",
        " tu ",
        popup_command,
    ]
    os.execvp("tmux", argv)  # noqa: S606  # intentional: replaces process


def main(argv: list[str] | None = None) -> int:
    args = _parse_args(argv)

    if not is_tmux_installed():
        _eprint(
            "tu: 'tmux' not found on PATH. Install tmux (e.g. brew install tmux) and try again."
        )
        return 2

    explicit_popup = args.popup or os.environ.get("TMUXUI_POPUP") == "1"

    # 1) Already in popup mode — run the compact UI directly.
    if explicit_popup:
        app = TmuxUIApp(mode="popup", stay_open=args.stay)
        app.run()
        return 0

    # 2) Inside tmux but not yet in a popup: re-exec through display-popup so
    # the user sees an overlay instead of replacing their current pane.
    if is_inside_tmux():
        popup_command = DEFAULT_POPUP_COMMAND
        # ``--stay`` passes through so users can opt into the chainable popup.
        if args.stay:
            popup_command = f"{DEFAULT_POPUP_COMMAND} --stay"
        _exec_popup(popup_command)

    # 3) Hub mode — fullscreen TUI plus a temporary F12 binding.
    tmux = TmuxClient()
    installer = HotkeyInstaller(tmux, key=args.key)
    with installer.use() as bound:
        if not bound and not installer.is_bound():
            # The binding silently failed (no server yet, etc.). Hub still
            # works; the user just won't get F12 until a session is created.
            pass
        app = TmuxUIApp(tmux=tmux, mode="hub")
        app.run()
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())


__all__ = ["main", "Path"]
