"""Help overlay — a categorised cheat sheet for every keybinding."""

from __future__ import annotations

from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import VerticalScroll
from textual.screen import ModalScreen
from textual.widgets import DataTable, Label

from ..commands import COMMANDS, Category

CATEGORY_ORDER: tuple[Category, ...] = (
    "session",
    "window",
    "pane",
    "navigation",
    "meta",
)

CATEGORY_LABEL: dict[Category, str] = {
    "session": "Session · 세션",
    "window": "Window · 윈도우",
    "pane": "Pane · 페인",
    "navigation": "Navigation · 이동",
    "meta": "Meta · 메타",
}


class HelpOverlay(ModalScreen[None]):
    """A scrollable cheat sheet. Press any key (Esc, q, ?) to dismiss."""

    BINDINGS = [
        Binding("escape", "dismiss_screen", "Close", show=False),
        Binding("q", "dismiss_screen", "Close", show=False),
        Binding("question_mark", "dismiss_screen", "Close", show=False),
    ]

    DEFAULT_CSS = """
    HelpOverlay {
        align: center middle;
    }
    HelpOverlay > VerticalScroll {
        width: 90;
        max-width: 95%;
        height: 80%;
        padding: 1 2;
        border: thick $accent;
        background: $surface;
    }
    HelpOverlay Label.section {
        padding-top: 1;
        text-style: bold;
        color: $accent;
    }
    HelpOverlay DataTable {
        height: auto;
        margin-bottom: 1;
    }
    """

    def compose(self) -> ComposeResult:
        with VerticalScroll():
            yield Label("tu — Help (Esc/q to close)", classes="title")
            for category in CATEGORY_ORDER:
                yield Label(CATEGORY_LABEL[category], classes="section")
                table = DataTable(
                    cursor_type="none",
                    zebra_stripes=False,
                    show_header=True,
                )
                yield table

    def on_mount(self) -> None:
        tables = list(self.query(DataTable))
        for table, category in zip(tables, CATEGORY_ORDER):
            table.add_columns("Key", "Action", "한국어", "Description")
            for command in COMMANDS:
                if command.category != category:
                    continue
                table.add_row(command.key, command.en, command.ko, command.description)

    def action_dismiss_screen(self) -> None:
        self.dismiss(None)
