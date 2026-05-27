"""Minimal tmux CLI wrapper used by :mod:`tmuxui.app`.

Only the calls the simplified ``tu`` UI needs: list sessions, create a new
session, attach/switch to a session, and detach the current client.
"""

from __future__ import annotations

import os
import shutil
import subprocess
from collections.abc import Sequence
from dataclasses import dataclass

from .models import SESSION_FORMAT, Session, parse_sessions

DEFAULT_NAME_PREFIX = "tu"
DEFAULT_NAME_MAX = 999


@dataclass(frozen=True, slots=True)
class TmuxResult:
    argv: list[str]
    returncode: int
    stdout: str
    stderr: str

    @property
    def ok(self) -> bool:
        return self.returncode == 0


def is_tmux_installed() -> bool:
    """True if the ``tmux`` binary is on PATH."""

    return shutil.which("tmux") is not None


def is_inside_tmux() -> bool:
    """True if the current process is running inside a tmux client."""

    return bool(os.environ.get("TMUX"))


class TmuxClient:
    """Tiny wrapper around the ``tmux`` CLI."""

    def __init__(self, binary: str = "tmux") -> None:
        self.binary = binary

    def _run(self, args: Sequence[str]) -> TmuxResult:
        argv = [self.binary, *args]
        completed = subprocess.run(
            argv,
            capture_output=True,
            text=True,
            check=False,
        )
        return TmuxResult(
            argv=argv,
            returncode=completed.returncode,
            stdout=completed.stdout or "",
            stderr=completed.stderr or "",
        )

    # -------------------------------------------------------------- query

    def list_sessions(self) -> list[Session]:
        result = self._run(["list-sessions", "-F", SESSION_FORMAT])
        if not result.ok:
            return []
        return parse_sessions(result.stdout)

    # ----------------------------------------------------------- mutation

    def new_session(self, name: str) -> TmuxResult:
        # ``-d`` so the session is created detached; the UI handles attaching
        # afterwards either via App.suspend()+attach or via switch-client.
        return self._run(["new-session", "-d", "-s", name])

    def switch_client(self, target: str) -> TmuxResult:
        return self._run(["switch-client", "-t", target])

    def detach_client(self) -> TmuxResult:
        return self._run(["detach-client"])

    # ----------------------------------------------------- naming helpers

    def next_default_name(self) -> str:
        """Return the smallest free ``tu-N`` name (falls back to a timestamp)."""

        existing = {s.name for s in self.list_sessions()}
        for i in range(1, DEFAULT_NAME_MAX + 1):
            candidate = f"{DEFAULT_NAME_PREFIX}-{i}"
            if candidate not in existing:
                return candidate
        # Pathological case: 999 ``tu-N`` sessions already exist.
        import time

        return f"{DEFAULT_NAME_PREFIX}-{int(time.time())}"


def attach_argv(target: str | None = None) -> list[str]:
    """Build the argv used for the foreground ``tmux attach`` invocation."""

    argv = ["tmux", "attach-session"]
    if target is not None:
        argv += ["-t", target]
    return argv
