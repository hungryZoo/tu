"""WindowList widget — middle column of the Hub."""

from __future__ import annotations

from dataclasses import dataclass

from textual.message import Message
from textual.widgets import DataTable

from ..models import Window


class WindowList(DataTable):
    """Shows the windows that belong to the currently selected session."""

    BINDINGS = []

    DEFAULT_CSS = """
    WindowList {
        height: 1fr;
    }
    """

    @dataclass
    class Highlighted(Message):
        window: Window | None

    def __init__(self, **kwargs) -> None:
        super().__init__(
            show_cursor=True,
            cursor_type="row",
            zebra_stripes=False,
            **kwargs,
        )
        self._windows: list[Window] = []

    def on_mount(self) -> None:
        self.add_columns("#", "Name", "Panes", "Active")

    # ----------------------------------------------------------------- state

    @property
    def windows(self) -> list[Window]:
        return list(self._windows)

    @property
    def current(self) -> Window | None:
        if not self._windows:
            return None
        row = self.cursor_row
        if row < 0 or row >= len(self._windows):
            return None
        return self._windows[row]

    def set_windows(self, windows: list[Window]) -> None:
        previous_index: int | None = self.current.index if self.current else None
        self.clear()
        self._windows = list(windows)

        for window in self._windows:
            active = "*" if window.active else ""
            self.add_row(str(window.index), window.name, str(window.panes), active)

        if not self._windows:
            self.post_message(self.Highlighted(None))
            return

        new_row = 0
        if previous_index is not None:
            for idx, window in enumerate(self._windows):
                if window.index == previous_index:
                    new_row = idx
                    break
        else:
            for idx, window in enumerate(self._windows):
                if window.active:
                    new_row = idx
                    break
        self.move_cursor(row=new_row)
        self.post_message(self.Highlighted(self._windows[new_row]))

    # --------------------------------------------------------------- events

    def watch_cursor_coordinate(self, _old, _new) -> None:  # type: ignore[override]
        if not self._windows:
            self.post_message(self.Highlighted(None))
            return
        row = self.cursor_row
        if 0 <= row < len(self._windows):
            self.post_message(self.Highlighted(self._windows[row]))
