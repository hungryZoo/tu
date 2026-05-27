"""The minimal ``tu`` TUI.

One screen: a session list and four buttons (New / Attach / Detach / Quit).
Click or press the underlined key. Arrow keys and Enter always work, no
matter which widget has focus.

Detach only detaches the tmux client — it does not close `tu`. If you
re-attach the session later, `tu` is still there waiting for you. Quit is
the only action that exits.
"""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

from textual import on
from textual.app import App, ComposeResult, SuspendNotSupported
from textual.binding import Binding
from textual.containers import Horizontal, Vertical
from textual.widgets import Button, DataTable, Footer, Header

from .models import Session
from .mouse_setup import (
    CONF_PATH,
    MouseSetupModal,
    apply_mouse_to_server,
    mark_dismissed,
    prepend_mouse_setting,
    should_prompt_for_mouse,
)
from .tmux import TmuxClient, attach_argv, is_inside_tmux

POLL_INTERVAL = 2.0  # seconds


class TmuxUIApp(App[None]):
    """Tiny session-only TUI on top of tmux."""

    CSS_PATH = "styles.tcss"

    # Enter is intentionally not bound at the app level: the focused widget
    # (DataTable row or a Button) handles it through its own default action.
    # Arrow keys (and j/k) are bound at the app level so the list stays
    # navigable even when a button currently holds focus.
    BINDINGS = [
        Binding("q", "quit", "Quit", show=True),
        Binding("ctrl+c", "quit", "Quit", show=False),
        Binding("n", "new_session", "New", show=True),
        Binding("a", "attach", "Attach", show=True),
        Binding("d", "detach", "Detach", show=True),
        Binding("r", "refresh_now", "Refresh", show=False),
        Binding("up", "move_cursor(-1)", "Up", show=False),
        Binding("k", "move_cursor(-1)", "Up", show=False),
        Binding("down", "move_cursor(1)", "Down", show=False),
        Binding("j", "move_cursor(1)", "Down", show=False),
    ]

    def __init__(self, tmux: TmuxClient | None = None) -> None:
        super().__init__()
        self.tmux = tmux or TmuxClient()
        self._sessions: list[Session] = []
        self._inside_tmux = is_inside_tmux()

    # ----------------------------------------------------------- compose

    def compose(self) -> ComposeResult:
        yield Header(show_clock=False)
        with Vertical(id="root"):
            yield DataTable(id="sessions", cursor_type="row", zebra_stripes=False)
            with Horizontal(id="buttons"):
                yield Button("New (n)", id="btn-new", variant="success")
                yield Button("Attach (a)", id="btn-attach", variant="primary")
                yield Button(
                    "Detach (d)",
                    id="btn-detach",
                    variant="warning",
                    disabled=not self._inside_tmux,
                )
                yield Button("Quit (q)", id="btn-quit", variant="error")
        yield Footer()

    def on_mount(self) -> None:
        self.title = "tu"
        self.sub_title = "in tmux" if self._inside_tmux else "outside tmux"

        table = self.query_one(DataTable)
        table.add_columns("Session", "Windows", "Attached")
        table.focus()

        self.refresh_sessions()
        self.set_interval(POLL_INTERVAL, self.refresh_sessions)

        # Offer to turn on mouse mode the first time the user lands here.
        if should_prompt_for_mouse(self.tmux):
            self.push_screen(MouseSetupModal(), self._handle_mouse_choice)

    # ----------------------------------------------------------- data

    def refresh_sessions(self) -> None:
        sessions = self.tmux.list_sessions()
        self._sessions = sessions

        table = self.query_one(DataTable)
        previous = self._selected_name(table)
        table.clear()
        for session in sessions:
            attached = "yes" if session.attached else ""
            table.add_row(session.name, str(session.windows), attached)

        # Restore cursor where possible.
        if not sessions:
            return
        target_row = 0
        if previous is not None:
            for idx, session in enumerate(sessions):
                if session.name == previous:
                    target_row = idx
                    break
        table.move_cursor(row=target_row)

    def _selected_name(self, table: DataTable | None = None) -> str | None:
        if table is None:
            table = self.query_one(DataTable)
        row = table.cursor_row
        if 0 <= row < len(self._sessions):
            return self._sessions[row].name
        return None

    # ----------------------------------------------------------- actions

    def action_attach(self) -> None:
        name = self._selected_name()
        if name is None:
            self.bell()
            return
        self._attach(name)

    def action_move_cursor(self, delta: int) -> None:
        """Move the session-list cursor by *delta*, focusing the table.

        Bound to up/down (and j/k) at the app level so the user can keep
        navigating the list while a button has focus.
        """

        table = self.query_one(DataTable)
        if table.row_count == 0:
            return
        if not table.has_focus:
            table.focus()
        new_row = max(0, min(table.row_count - 1, table.cursor_row + delta))
        if new_row != table.cursor_row:
            table.move_cursor(row=new_row)

    def action_new_session(self) -> None:
        name = self.tmux.next_default_name()
        result = self.tmux.new_session(name)
        if not result.ok:
            self.bell()
            return
        self.refresh_sessions()
        self._attach(name)

    def action_detach(self) -> None:
        """Detach the tmux client. Does NOT exit `tu` — if you re-attach
        the session later, `tu` is still here.
        """

        if not self._inside_tmux:
            self.notify(
                "Detach는 tmux 안에서 tu를 실행했을 때만 동작해요.",
                severity="warning",
                timeout=4,
            )
            self.bell()
            return

        # ``tmux detach-client`` (no args) identifies *which* client to
        # detach from the caller's controlling TTY. We're running inside a
        # tmux *pane* whose TTY is a server-allocated pty — not the user's
        # client TTY — so tmux can't resolve a target that way and silently
        # does nothing.
        #
        # Reliable approach: discover the session name via the pane id that
        # tmux puts in $TMUX_PANE, then detach every client attached to
        # that session with ``-s``. tmux doesn't need any TTY gymnastics for
        # the ``-s`` form.
        ok, detail = self._detach_session_for_current_pane()
        if ok:
            # The user is at the outer shell now and won't see this toast,
            # but if they re-attach later they'll see a quick confirmation.
            self.notify(f"Detached {detail}".strip(), timeout=3)
            return

        # Last-resort: try the captured no-arg form. Unlikely to succeed
        # via a pane pty, but it lets us surface tmux's own error message.
        fallback = self.tmux.detach_client()
        if fallback.ok:
            self.notify("Detached", timeout=3)
            return

        # All paths failed — show the user *why* so they can tell us.
        reason = (detail or fallback.stderr.strip() or "unknown error").strip()
        self.notify(
            f"Detach 실패: {reason}",
            severity="error",
            timeout=8,
        )
        self.bell()

    def _detach_session_for_current_pane(self) -> tuple[bool, str]:
        """Detach every client attached to the session that owns $TMUX_PANE.

        Returns ``(ok, detail)`` where *detail* is either the session name
        on success or a short error string on failure (suitable for showing
        to the user).
        """

        pane = os.environ.get("TMUX_PANE", "")
        if not pane:
            return False, "$TMUX_PANE is not set"

        info = subprocess.run(
            ["tmux", "display-message", "-p", "-t", pane, "#S"],
            capture_output=True,
            text=True,
            check=False,
        )
        session = info.stdout.strip()
        if info.returncode != 0 or not session:
            err = info.stderr.strip() or f"display-message exited {info.returncode}"
            return False, err

        result = subprocess.run(
            ["tmux", "detach-client", "-s", session],
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode != 0:
            err = (
                result.stderr.strip()
                or f"detach-client exited {result.returncode}"
            )
            return False, err
        return True, f"session “{session}”"

    def action_refresh_now(self) -> None:
        self.refresh_sessions()

    # ---------------------------------------------------------- mouse setup

    def _handle_mouse_choice(self, choice: str | None) -> None:
        if choice == "yes":
            try:
                prepend_mouse_setting()
            except OSError as exc:
                self.notify(
                    f"~/.tmux.conf 수정 실패: {exc}",
                    severity="error",
                    timeout=6,
                )
                return
            applied = apply_mouse_to_server(self.tmux)
            msg = f"{CONF_PATH} 에 set -g mouse on 추가 완료"
            if applied:
                msg += " · 현재 세션에도 적용됨"
            else:
                msg += " · 다음 tmux 시작부터 적용"
            self.notify(msg, severity="information", timeout=6)
        elif choice == "never":
            try:
                mark_dismissed()
            except OSError:
                # Best-effort; ignore if we can't persist the decision.
                pass
            self.notify("다음부터 묻지 않을게요.", timeout=3)
        # "later" (or dismiss): silent, ask again next launch.

    # ---------------------------------------------------------- mouse

    @on(Button.Pressed, "#btn-new")
    def _click_new(self) -> None:
        self.action_new_session()

    @on(Button.Pressed, "#btn-attach")
    def _click_attach(self) -> None:
        self.action_attach()

    @on(Button.Pressed, "#btn-detach")
    def _click_detach(self) -> None:
        self.action_detach()

    @on(Button.Pressed, "#btn-quit")
    def _click_quit(self) -> None:
        # ``App.action_quit`` is an async coroutine; calling it from a sync
        # event handler creates an un-awaited coroutine and the click ends
        # up being a no-op. The plain ``exit()`` method is sync.
        self.exit()

    @on(DataTable.RowSelected)
    def _row_selected(self, event: DataTable.RowSelected) -> None:
        row = event.cursor_row
        if 0 <= row < len(self._sessions):
            self._attach(self._sessions[row].name)

    # ---------------------------------------------------------- attach

    def _attach(self, target: str) -> None:
        """Attach (outside tmux) or switch (inside tmux) to *target*."""

        if self._inside_tmux:
            self.tmux.switch_client(target)
            # The current client moves to *target*; whatever pane was hosting
            # us is no longer visible. Exit so we don't keep state around.
            self.exit()
            return

        # Outside tmux: suspend Textual, exec ``tmux attach`` in the
        # foreground, and resume when the user detaches.
        try:
            with self.suspend():
                subprocess.run(attach_argv(target), check=False)
        except SuspendNotSupported:
            # Headless / piped environments can't hand the terminal over.
            self.bell()
            return
        self.refresh_sessions()


# Resolve the stylesheet relative to this module so the wheel ships it.
TmuxUIApp.CSS_PATH = str(Path(__file__).with_name("styles.tcss"))
