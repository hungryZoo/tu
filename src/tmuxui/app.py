"""The minimal ``tu`` TUI.

One screen: a session list and three buttons (New / Detach / Quit). Click or
press the underlined key. Detach is only enabled when running inside tmux.
"""

from __future__ import annotations

import subprocess
from pathlib import Path

from textual import on
from textual.app import App, ComposeResult, SuspendNotSupported
from textual.binding import Binding
from textual.containers import Horizontal, Vertical
from textual.widgets import Button, DataTable, Footer, Header

from .models import Session
from .tmux import TmuxClient, attach_argv, is_inside_tmux

POLL_INTERVAL = 2.0  # seconds


class TmuxUIApp(App[None]):
    """Tiny session-only TUI on top of tmux."""

    CSS_PATH = "styles.tcss"

    # Enter is intentionally not bound at the app level: the focused widget
    # (DataTable row or a Button) handles it through its own default action.
    BINDINGS = [
        Binding("q", "quit", "Quit", show=True),
        Binding("ctrl+c", "quit", "Quit", show=False),
        Binding("n", "new_session", "New", show=True),
        Binding("d", "detach", "Detach", show=True),
        Binding("r", "refresh_now", "Refresh", show=False),
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
                yield Button(
                    "Detach (d)",
                    id="btn-detach",
                    variant="warning",
                    disabled=not self._inside_tmux,
                )
                yield Button("Quit (q)", id="btn-quit")
        yield Footer()

    def on_mount(self) -> None:
        self.title = "tu"
        self.sub_title = "in tmux" if self._inside_tmux else "outside tmux"

        table = self.query_one(DataTable)
        table.add_columns("Session", "Windows", "Attached")
        table.focus()

        self.refresh_sessions()
        self.set_interval(POLL_INTERVAL, self.refresh_sessions)

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

    def action_new_session(self) -> None:
        name = self.tmux.next_default_name()
        result = self.tmux.new_session(name)
        if not result.ok:
            self.bell()
            return
        self.refresh_sessions()
        self._attach(name)

    def action_detach(self) -> None:
        if not self._inside_tmux:
            self.bell()
            return
        self.tmux.detach_client()
        # ``detach-client`` returns immediately; the client is now gone so the
        # popup-or-pane that hosts us has nothing left to render. Exit cleanly.
        self.exit()

    def action_refresh_now(self) -> None:
        self.refresh_sessions()

    # ---------------------------------------------------------- mouse

    @on(Button.Pressed, "#btn-new")
    def _click_new(self) -> None:
        self.action_new_session()

    @on(Button.Pressed, "#btn-detach")
    def _click_detach(self) -> None:
        self.action_detach()

    @on(Button.Pressed, "#btn-quit")
    def _click_quit(self) -> None:
        self.action_quit()

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
