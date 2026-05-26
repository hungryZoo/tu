"""High-level actions exposed to widgets and the command palette.

:class:`ActionController` is the single seam between key bindings and the
``tmux`` CLI. Widgets never call :class:`tmuxui.tmux.TmuxClient` directly —
they go through here so that mode-specific behaviour (Hub vs Popup) and the
Textual ``App.suspend()`` lifecycle are handled in one place.
"""

from __future__ import annotations

import contextlib
import subprocess
from collections.abc import Iterator
from typing import TYPE_CHECKING, Literal

from .tmux import Layout, ResizeDirection, TmuxClient, TmuxResult, attach_argv

if TYPE_CHECKING:  # pragma: no cover
    from textual.app import App


Mode = Literal["hub", "popup"]


@contextlib.contextmanager
def _maybe_suspend(app: App | None) -> Iterator[None]:
    if app is None:
        yield
    else:
        with app.suspend():
            yield


class ActionController:
    """Bridge between widget events and tmux CLI calls.

    Construct with a :class:`TmuxClient` and the current mode. Call
    :meth:`bind_app` from the Textual ``App`` once it exists so attach/detach
    can suspend the screen properly.
    """

    def __init__(self, tmux: TmuxClient, mode: Mode = "hub") -> None:
        self.tmux = tmux
        self.mode = mode
        self.app: App | None = None
        # When True, popup actions skip the automatic ``app.exit()`` so the
        # user can chain a few operations without re-opening the popup.
        self.stay_open: bool = False

    def bind_app(self, app: App) -> None:
        self.app = app

    # -------------------------------------------------------------- lifecycle

    def _exit_if_popup(self) -> None:
        if self.mode == "popup" and self.app is not None and not self.stay_open:
            self.app.exit()

    # ----------------------------------------------------------------- attach

    def attach(self, target: str | None) -> TmuxResult | None:
        """Attach to *target*, branching by mode.

        Hub: suspends Textual and execs ``tmux attach`` in the foreground.
        Popup: issues ``switch-client -t TARGET`` and closes the popup.
        """

        if self.mode == "popup":
            result = self.tmux.switch_client(target) if target else None
            self._exit_if_popup()
            return result

        with _maybe_suspend(self.app):
            argv = attach_argv(target)
            subprocess.run(argv, check=False)
        return None

    def toggle_last(self) -> TmuxResult | None:
        """Jump to the last session/client (``switch-client -l`` semantics)."""

        if self.mode == "popup":
            result = self.tmux.switch_client_last()
            self._exit_if_popup()
            return result
        with _maybe_suspend(self.app):
            subprocess.run(["tmux", "attach-session", "-t", "-"], check=False)
        return None

    def detach(self) -> TmuxResult | None:
        """Detach the current client. Only meaningful in popup mode."""

        if self.mode != "popup":
            return None
        result = self.tmux.detach_client()
        if self.app is not None:
            self.app.exit()
        return result

    # ------------------------------------------------------------- session ops

    def new_session(self, name: str) -> TmuxResult:
        return self.tmux.new_session(name)

    def kill_session(self, name: str) -> TmuxResult:
        return self.tmux.kill_session(name)

    def rename_session(self, target: str, name: str) -> TmuxResult:
        return self.tmux.rename_session(target, name)

    # -------------------------------------------------------------- window ops

    def new_window(self, session: str, name: str | None = None) -> TmuxResult:
        return self.tmux.new_window(session, name)

    def kill_window(self, target: str) -> TmuxResult:
        return self.tmux.kill_window(target)

    def rename_window(self, target: str, name: str) -> TmuxResult:
        return self.tmux.rename_window(target, name)

    def swap_window(self, src: str, dst: str) -> TmuxResult:
        return self.tmux.swap_window(src, dst)

    def toggle_synchronize_panes(self, target: str, on: bool) -> TmuxResult:
        return self.tmux.set_synchronize_panes(target, on)

    def copy_mode(self, target: str) -> TmuxResult:
        result = self.tmux.copy_mode(target)
        # In popup mode we must close so the user can interact with copy-mode
        # in the underlying pane; otherwise the popup keeps receiving keys.
        if self.mode == "popup" and self.app is not None:
            self.app.exit()
        return result

    # ---------------------------------------------------------------- pane ops

    def split(self, target: str, *, vertical: bool = False) -> TmuxResult:
        return self.tmux.split_window(target, vertical=vertical)

    def select_pane(self, target: str) -> TmuxResult:
        return self.tmux.select_pane(target)

    def zoom_pane(self, target: str) -> TmuxResult:
        return self.tmux.zoom_pane(target)

    def resize_pane(
        self,
        target: str,
        direction: ResizeDirection,
        amount: int = 5,
    ) -> TmuxResult:
        return self.tmux.resize_pane(target, direction, amount)

    def select_layout(self, target: str, layout: Layout) -> TmuxResult:
        return self.tmux.select_layout(target, layout)

    def kill_pane(self, target: str) -> TmuxResult:
        return self.tmux.kill_pane(target)
