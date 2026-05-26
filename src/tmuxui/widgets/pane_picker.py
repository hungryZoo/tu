"""Pane focus picker — pick which pane to ``select-pane`` to."""

from __future__ import annotations

from textual import on
from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import DataTable, Label

from ..models import Pane


class PanePicker(ModalScreen[Pane | None]):
    """Show a small picker over the active window with one row per pane.

    Returns the picked :class:`Pane` (or ``None`` if cancelled). Number keys
    1..9 jump directly to the corresponding row.
    """

    BINDINGS = [
        Binding("escape", "cancel", "Cancel", show=False),
        Binding("enter", "confirm", "Pick", show=False),
        *[Binding(str(n), f"pick_index({n})", "", show=False) for n in range(1, 10)],
    ]

    DEFAULT_CSS = """
    PanePicker {
        align: center middle;
    }
    PanePicker > Vertical {
        width: 70;
        max-width: 90%;
        height: auto;
        padding: 1 2;
        border: thick $accent;
        background: $surface;
    }
    PanePicker Label {
        padding-bottom: 1;
    }
    PanePicker DataTable {
        height: auto;
        max-height: 15;
    }
    """

    def __init__(self, panes: list[Pane]) -> None:
        super().__init__()
        self.panes = list(panes)

    def compose(self) -> ComposeResult:
        with Vertical():
            yield Label("Pick a pane (number to jump, Enter to confirm, Esc to cancel):")
            table: DataTable = DataTable(cursor_type="row", zebra_stripes=False)
            yield table

    def on_mount(self) -> None:
        table = self.query_one(DataTable)
        table.add_columns("#", "Size", "Command", "Active")
        for pane in self.panes:
            active = "*" if pane.active else ""
            table.add_row(
                str(pane.index),
                f"{pane.width}x{pane.height}",
                pane.command or "",
                active,
            )
        table.focus()
        for idx, pane in enumerate(self.panes):
            if pane.active:
                table.move_cursor(row=idx)
                break

    # ----------------------------------------------------------------- actions

    def action_cancel(self) -> None:
        self.dismiss(None)

    def action_confirm(self) -> None:
        table = self.query_one(DataTable)
        row = table.cursor_row
        if 0 <= row < len(self.panes):
            self.dismiss(self.panes[row])
        else:
            self.dismiss(None)

    def action_pick_index(self, index: int) -> None:
        # Number keys: pick the Nth pane (1-indexed) when present, else jump
        # to the row whose pane index equals ``index``.
        for idx, pane in enumerate(self.panes):
            if pane.index == index:
                self.query_one(DataTable).move_cursor(row=idx)
                self.dismiss(pane)
                return

    @on(DataTable.RowSelected)
    def _row_selected(self, event: DataTable.RowSelected) -> None:
        row = event.cursor_row
        if 0 <= row < len(self.panes):
            self.dismiss(self.panes[row])
