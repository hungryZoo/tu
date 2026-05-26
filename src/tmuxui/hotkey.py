"""Install / uninstall the no-prefix popup hotkey on the running tmux server.

While the Hub TUI runs we bind a single key (default ``F12``) to
``display-popup -E "tu --popup"`` so the same overlay opens from anywhere
inside tmux. The binding is removed on Hub exit. ``atexit`` is the safety net
for crashes.
"""

from __future__ import annotations

import atexit
import contextlib
import shlex
from collections.abc import Iterator

from .tmux import TmuxClient, TmuxResult, is_tmux_installed

DEFAULT_KEY = "F12"
DEFAULT_POPUP_COMMAND = "tu --popup"


class HotkeyInstaller:
    """Manage a single tmux ``bind-key -n KEY`` binding.

    Safe to use even when tmux is not installed (every operation becomes a
    no-op). Always pair :meth:`install` with :meth:`uninstall` — :meth:`use`
    is the recommended context manager which also wires up :mod:`atexit` as
    a last-resort cleanup.
    """

    def __init__(
        self,
        tmux: TmuxClient | None = None,
        *,
        key: str = DEFAULT_KEY,
        popup_command: str = DEFAULT_POPUP_COMMAND,
        width: str = "80%",
        height: str = "80%",
        title: str = " tu ",
    ) -> None:
        self.tmux = tmux or TmuxClient()
        self.key = key
        self.popup_command = popup_command
        self.width = width
        self.height = height
        self.title = title
        self._installed = False
        self._atexit_registered = False

    # ------------------------------------------------------------- inspection

    def is_bound(self) -> bool:
        """True if ``self.key`` already has a binding in the ``root`` table."""

        if not is_tmux_installed() or not self.tmux.server_running():
            return False
        output = self.tmux.list_keys(table="root")
        for line in output.splitlines():
            # Format example:
            #   bind-key  -T root  F12  display-popup -E -h 80% -w 80% ...
            tokens = shlex.split(line, posix=True)
            if len(tokens) >= 4 and tokens[0] == "bind-key" and tokens[3] == self.key:
                return True
        return False

    # ------------------------------------------------------- install / remove

    def install(self) -> TmuxResult | None:
        """Bind ``self.key`` to the tmuxui popup.

        Refuses to clobber existing bindings: if the key is already taken,
        return ``None`` and leave the server untouched. The Hub UI surfaces
        this so the user can pick a different ``--key``.
        """

        if not is_tmux_installed() or not self.tmux.server_running():
            return None
        if self.is_bound():
            return None
        result = self.tmux.bind_popup(
            self.key,
            self.popup_command,
            width=self.width,
            height=self.height,
            title=self.title,
        )
        if result.ok:
            self._installed = True
            if not self._atexit_registered:
                atexit.register(self._atexit_cleanup)
                self._atexit_registered = True
        return result

    def uninstall(self) -> TmuxResult | None:
        if not self._installed:
            return None
        if not is_tmux_installed() or not self.tmux.server_running():
            self._installed = False
            return None
        result = self.tmux.unbind_key(self.key)
        self._installed = False
        return result

    # ------------------------------------------------------------- atexit hook

    def _atexit_cleanup(self) -> None:
        # ``atexit`` runs after a normal interpreter shutdown. We protect it
        # heavily because failures here are not actionable for the user.
        try:
            self.uninstall()
        except Exception:
            pass

    # ----------------------------------------------------------------- helper

    @contextlib.contextmanager
    def use(self) -> Iterator[bool]:
        """Context manager — yields True if the binding was actually installed."""

        installed = False
        try:
            result = self.install()
            installed = result is not None and result.ok
            yield installed
        finally:
            try:
                self.uninstall()
            except Exception:
                # Best-effort cleanup; the atexit hook is the safety net.
                pass
