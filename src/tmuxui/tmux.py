"""Thin wrapper around the ``tmux`` CLI used by :class:`TmuxClient`.

Every method maps to one ``tmux ...`` invocation. List/query commands use the
``-F`` format strings defined in :mod:`tmuxui.models` so we never have to
scrape human-readable output.

The class is intentionally side-effect free outside of subprocess calls so it
plays well with unit tests — patch :meth:`TmuxClient._run` to capture the exact
argv lists that would be sent to tmux.
"""

from __future__ import annotations

import os
import shutil
import subprocess
from collections.abc import Sequence
from dataclasses import dataclass
from typing import Literal

from .models import (
    PANE_FORMAT,
    SESSION_FORMAT,
    WINDOW_FORMAT,
    Pane,
    Session,
    Window,
    parse_panes,
    parse_sessions,
    parse_windows,
)


class TmuxError(RuntimeError):
    """Raised when a tmux command exits non-zero (besides expected 'no server')."""

    def __init__(self, argv: Sequence[str], returncode: int, stderr: str):
        self.argv = list(argv)
        self.returncode = returncode
        self.stderr = stderr
        super().__init__(
            f"tmux command failed ({returncode}): {' '.join(self.argv)}\n{stderr.strip()}"
        )


@dataclass(frozen=True, slots=True)
class TmuxResult:
    argv: list[str]
    returncode: int
    stdout: str
    stderr: str

    @property
    def ok(self) -> bool:
        return self.returncode == 0


ResizeDirection = Literal["U", "D", "L", "R"]
Layout = Literal[
    "even-horizontal",
    "even-vertical",
    "main-horizontal",
    "main-vertical",
    "tiled",
]


def is_inside_tmux() -> bool:
    """True if the current process appears to be running inside a tmux client."""

    return bool(os.environ.get("TMUX"))


def is_tmux_installed() -> bool:
    """True if the ``tmux`` binary is on PATH."""

    return shutil.which("tmux") is not None


class TmuxClient:
    """Thin wrapper around the ``tmux`` CLI.

    All mutation methods return :class:`TmuxResult` so callers can inspect
    stderr without raising. Query methods (``list_*``) parse the structured
    output and return models from :mod:`tmuxui.models`.
    """

    def __init__(self, binary: str = "tmux") -> None:
        self.binary = binary

    # ------------------------------------------------------------------ infra

    def _run(self, args: Sequence[str], *, check: bool = False) -> TmuxResult:
        argv = [self.binary, *args]
        completed = subprocess.run(
            argv,
            capture_output=True,
            text=True,
            check=False,
        )
        result = TmuxResult(
            argv=argv,
            returncode=completed.returncode,
            stdout=completed.stdout or "",
            stderr=completed.stderr or "",
        )
        if check and not result.ok:
            raise TmuxError(argv, result.returncode, result.stderr)
        return result

    # ---------------------------------------------------------------- queries

    def server_running(self) -> bool:
        """Return True if a tmux server is currently up."""

        if not is_tmux_installed():
            return False
        result = self._run(["list-sessions"])
        if result.ok:
            return True
        # tmux prints "no server running on ..." or "error connecting" when
        # there is no server. We treat any non-zero exit as "not running".
        return False

    def list_sessions(self) -> list[Session]:
        result = self._run(["list-sessions", "-F", SESSION_FORMAT])
        if not result.ok:
            return []
        return parse_sessions(result.stdout)

    def list_windows(self, session: str) -> list[Window]:
        result = self._run(["list-windows", "-t", session, "-F", WINDOW_FORMAT])
        if not result.ok:
            return []
        return parse_windows(result.stdout, session)

    def list_panes(self, target: str) -> list[Pane]:
        result = self._run(["list-panes", "-t", target, "-F", PANE_FORMAT])
        if not result.ok:
            return []
        # ``target`` is "session" or "session:window". Pull the window index
        # so the returned panes know where they live.
        session, _, rest = target.partition(":")
        try:
            window_index = int(rest.split(".")[0]) if rest else 0
        except ValueError:
            window_index = 0
        return parse_panes(result.stdout, session, window_index)

    def capture_pane(self, target: str, lines: int = 200) -> str:
        """Return the contents of *target* pane as ANSI-coloured text."""

        result = self._run(
            [
                "capture-pane",
                "-p",
                "-e",
                "-J",
                "-S",
                f"-{lines}",
                "-t",
                target,
            ]
        )
        return result.stdout

    # --------------------------------------------------------------- mutation

    def new_session(self, name: str, *, detached: bool = True) -> TmuxResult:
        args = ["new-session"]
        if detached:
            args.append("-d")
        args += ["-s", name]
        return self._run(args)

    def new_window(self, session: str, name: str | None = None) -> TmuxResult:
        args = ["new-window", "-t", session]
        if name:
            args += ["-n", name]
        return self._run(args)

    def kill_session(self, name: str) -> TmuxResult:
        return self._run(["kill-session", "-t", name])

    def kill_window(self, target: str) -> TmuxResult:
        return self._run(["kill-window", "-t", target])

    def kill_pane(self, target: str) -> TmuxResult:
        return self._run(["kill-pane", "-t", target])

    def rename_session(self, target: str, name: str) -> TmuxResult:
        return self._run(["rename-session", "-t", target, name])

    def rename_window(self, target: str, name: str) -> TmuxResult:
        return self._run(["rename-window", "-t", target, name])

    def split_window(
        self,
        target: str,
        *,
        vertical: bool = False,
    ) -> TmuxResult:
        # tmux semantics: ``-h`` splits horizontally (panes side-by-side),
        # ``-v`` splits vertically (panes stacked). ``vertical=False`` means
        # "side by side" to match our ``s`` keybinding label.
        flag = "-v" if vertical else "-h"
        return self._run(["split-window", flag, "-t", target])

    def select_pane(self, target: str) -> TmuxResult:
        return self._run(["select-pane", "-t", target])

    def resize_pane(
        self,
        target: str,
        direction: ResizeDirection,
        amount: int = 5,
    ) -> TmuxResult:
        flag = {"U": "-U", "D": "-D", "L": "-L", "R": "-R"}[direction]
        return self._run(["resize-pane", flag, str(amount), "-t", target])

    def zoom_pane(self, target: str) -> TmuxResult:
        return self._run(["resize-pane", "-Z", "-t", target])

    def select_layout(self, target: str, layout: Layout) -> TmuxResult:
        return self._run(["select-layout", "-t", target, layout])

    def swap_window(self, src: str, dst: str) -> TmuxResult:
        return self._run(["swap-window", "-s", src, "-t", dst])

    def set_synchronize_panes(self, target: str, on: bool) -> TmuxResult:
        value = "on" if on else "off"
        return self._run(["set-window-option", "-t", target, "synchronize-panes", value])

    def copy_mode(self, target: str) -> TmuxResult:
        return self._run(["copy-mode", "-t", target])

    def switch_client(self, target: str) -> TmuxResult:
        return self._run(["switch-client", "-t", target])

    def switch_client_last(self) -> TmuxResult:
        return self._run(["switch-client", "-l"])

    def detach_client(self) -> TmuxResult:
        return self._run(["detach-client"])

    # ----------------------------------------------------------- key bindings

    def bind_key(self, key: str, command: str, *, no_prefix: bool = True) -> TmuxResult:
        args = ["bind-key"]
        if no_prefix:
            args.append("-n")
        args += [key, command]
        # tmux's bind-key takes the command as a single argv element if you
        # wrap with run-shell, but it actually accepts the rest of argv as the
        # command tokens. We pass the command as one shell-style string and
        # rely on ``set-buffer`` / ``display-popup`` parsing — display-popup
        # has its own argument grammar so we explicitly use it from the
        # higher-level :func:`bind_popup` helper instead.
        return self._run(args)

    def bind_popup(
        self,
        key: str,
        popup_command: str,
        *,
        width: str = "80%",
        height: str = "80%",
        title: str = " tu ",
    ) -> TmuxResult:
        """Bind *key* to open ``popup_command`` inside ``display-popup -E``."""

        return self._run(
            [
                "bind-key",
                "-n",
                key,
                "display-popup",
                "-E",
                "-h",
                height,
                "-w",
                width,
                "-T",
                title,
                popup_command,
            ]
        )

    def unbind_key(self, key: str) -> TmuxResult:
        return self._run(["unbind-key", "-n", key])

    def list_keys(self, *, table: str = "root") -> str:
        result = self._run(["list-keys", "-T", table])
        return result.stdout


def attach_argv(target: str | None = None) -> list[str]:
    """Build the argv used for the foreground ``tmux attach`` invocation."""

    argv = ["tmux", "attach-session"]
    if target is not None:
        argv += ["-t", target]
    return argv
