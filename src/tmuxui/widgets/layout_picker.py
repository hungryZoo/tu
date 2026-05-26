"""Named layout picker with live preview."""

from __future__ import annotations

from textual import on
from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import DataTable, Label

from ..actions import ActionController
from ..tmux import Layout

LAYOUTS: tuple[Layout, ...] = (
    "even-horizontal",
    "even-vertical",
    "main-horizontal",
    "main-vertical",
    "tiled",
)


class LayoutPicker(ModalScreen[Layout | None]):
    """Pick a named layout for the target window.

    Highlighting a row applies the layout immediately so the user sees the
    effect live. Pressing ``Enter`` keeps it, ``Esc`` reverts to whatever the
    layout was when the picker was opened (caller supplies that string).
    """

    BINDINGS = [
        Binding("escape", "cancel", "Cancel", show=False),
        Binding("enter", "confirm", "Apply", show=False),
    ]

    DEFAULT_CSS = """
    LayoutPicker {
        align: center middle;
    }
    LayoutPicker > Vertical {
        width: 50;
        height: auto;
        padding: 1 2;
        border: thick $accent;
        background: $surface;
    }
    LayoutPicker Label {
        padding-bottom: 1;
    }
    LayoutPicker DataTable {
        height: auto;
        max-height: 10;
    }
    """

    def __init__(
        self,
        controller: ActionController,
        target: str,
        previous_layout: str | None = None,
    ) -> None:
        super().__init__()
        self.controller = controller
        self.target = target
        self.previous_layout = previous_layout

    def compose(self) -> ComposeResult:
        with Vertical():
            yield Label("Layout (highlight = preview, Enter = apply, Esc = revert)")
            yield DataTable(cursor_type="row", zebra_stripes=False)

    def on_mount(self) -> None:
        table = self.query_one(DataTable)
        table.add_columns("Layout")
        for layout in LAYOUTS:
            table.add_row(layout)
        table.focus()
        table.move_cursor(row=0)

    @on(DataTable.RowHighlighted)
    def _highlighted(self, event: DataTable.RowHighlighted) -> None:
        row = event.cursor_row
        if 0 <= row < len(LAYOUTS):
            self.controller.select_layout(self.target, LAYOUTS[row])

    def action_confirm(self) -> None:
        table = self.query_one(DataTable)
        row = table.cursor_row
        if 0 <= row < len(LAYOUTS):
            self.dismiss(LAYOUTS[row])
        else:
            self.dismiss(None)

    def action_cancel(self) -> None:
        if self.previous_layout:
            # ``select-layout`` accepts both names and raw layout strings.
            self.controller.tmux.select_layout(self.target, self.previous_layout)  # type: ignore[arg-type]
        self.dismiss(None)
