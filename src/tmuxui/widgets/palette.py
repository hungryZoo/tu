"""Command palette modal — fuzzy search over the static command registry."""

from __future__ import annotations

from textual import on
from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import DataTable, Input

from ..commands import CommandSpec, filter_commands


class CommandPalette(ModalScreen[CommandSpec | None]):
    """VS-Code-style command palette. Returns the picked command spec."""

    BINDINGS = [
        Binding("escape", "cancel", "Cancel", show=False),
        Binding("enter", "confirm", "Pick", show=False),
        Binding("down", "cursor_down", "Down", show=False),
        Binding("up", "cursor_up", "Up", show=False),
    ]

    DEFAULT_CSS = """
    CommandPalette {
        align: center top;
    }
    CommandPalette > Vertical {
        width: 80;
        max-width: 95%;
        height: auto;
        margin-top: 4;
        padding: 1 2;
        border: thick $accent;
        background: $surface;
    }
    CommandPalette Input {
        margin-bottom: 1;
    }
    CommandPalette DataTable {
        height: auto;
        max-height: 18;
    }
    """

    def __init__(self, mode: str = "hub") -> None:
        super().__init__()
        self.mode = mode
        self._results: list[CommandSpec] = []

    def compose(self) -> ComposeResult:
        with Vertical():
            yield Input(placeholder="명령 검색 / search…", id="palette-input")
            yield DataTable(cursor_type="row", zebra_stripes=False, show_header=True)

    def on_mount(self) -> None:
        table = self.query_one(DataTable)
        table.add_columns("Key", "Action", "한국어", "Category")
        self._set_results(filter_commands("", mode=self.mode))  # type: ignore[arg-type]
        self.query_one("#palette-input", Input).focus()

    # ------------------------------------------------------------------ events

    @on(Input.Changed, "#palette-input")
    def _filter(self, event: Input.Changed) -> None:
        self._set_results(filter_commands(event.value, mode=self.mode))  # type: ignore[arg-type]

    @on(Input.Submitted, "#palette-input")
    def _submit(self) -> None:
        self.action_confirm()

    @on(DataTable.RowSelected)
    def _row_selected(self, event: DataTable.RowSelected) -> None:
        row = event.cursor_row
        if 0 <= row < len(self._results):
            self.dismiss(self._results[row])

    # ----------------------------------------------------------------- actions

    def action_cancel(self) -> None:
        self.dismiss(None)

    def action_confirm(self) -> None:
        if not self._results:
            self.dismiss(None)
            return
        table = self.query_one(DataTable)
        row = max(0, min(table.cursor_row, len(self._results) - 1))
        self.dismiss(self._results[row])

    def action_cursor_down(self) -> None:
        table = self.query_one(DataTable)
        table.move_cursor(row=min(table.cursor_row + 1, len(self._results) - 1))

    def action_cursor_up(self) -> None:
        table = self.query_one(DataTable)
        table.move_cursor(row=max(table.cursor_row - 1, 0))

    # ----------------------------------------------------------------- helper

    def _set_results(self, results: list[CommandSpec]) -> None:
        self._results = results
        table = self.query_one(DataTable)
        table.clear()
        for command in results:
            table.add_row(command.key, command.en, command.ko, command.category)
        if results:
            table.move_cursor(row=0)
