"""SessionList widget — left column of the Hub."""

from __future__ import annotations

from dataclasses import dataclass

from textual.message import Message
from textual.widgets import DataTable

from ..models import Session


class SessionList(DataTable):
    """Shows the current tmux sessions one per row.

    Emits :class:`SessionList.Highlighted` whenever the cursor moves to a new
    session so the parent ``App`` can refresh the windows column / preview.
    """

    BINDINGS = []  # All bindings are owned by the parent App.

    DEFAULT_CSS = """
    SessionList {
        height: 1fr;
    }
    """

    @dataclass
    class Highlighted(Message):
        session: Session | None

    def __init__(self, **kwargs) -> None:
        super().__init__(
            show_cursor=True,
            cursor_type="row",
            zebra_stripes=False,
            **kwargs,
        )
        self._sessions: list[Session] = []

    def on_mount(self) -> None:
        self.add_columns("Session", "Windows", "Attached")

    # ----------------------------------------------------------------- state

    @property
    def sessions(self) -> list[Session]:
        return list(self._sessions)

    @property
    def current(self) -> Session | None:
        if not self._sessions:
            return None
        row = self.cursor_row
        if row < 0 or row >= len(self._sessions):
            return None
        return self._sessions[row]

    def set_sessions(self, sessions: list[Session]) -> None:
        """Replace the row contents in-place, preserving the cursor when possible."""

        previous_name: str | None = self.current.name if self.current else None
        self.clear()
        self._sessions = list(sessions)

        for session in self._sessions:
            attached = "yes" if session.attached else ""
            self.add_row(session.name, str(session.windows), attached)

        if not self._sessions:
            return

        # Restore the cursor to the same session if it still exists, else 0.
        new_index = 0
        if previous_name is not None:
            for idx, session in enumerate(self._sessions):
                if session.name == previous_name:
                    new_index = idx
                    break
        self.move_cursor(row=new_index)
        self.post_message(self.Highlighted(self._sessions[new_index]))

    # --------------------------------------------------------------- events

    def watch_cursor_coordinate(self, _old, _new) -> None:  # type: ignore[override]
        # DataTable already exposes cursor_row; we just translate to a message.
        if not self._sessions:
            self.post_message(self.Highlighted(None))
            return
        row = self.cursor_row
        if 0 <= row < len(self._sessions):
            self.post_message(self.Highlighted(self._sessions[row]))
