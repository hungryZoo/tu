"""Small reusable modal screens — text input and yes/no confirmation."""

from __future__ import annotations

from textual import on
from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Input, Label


class NameModal(ModalScreen[str | None]):
    """Prompt the user for a single line of text (e.g. a new session name)."""

    BINDINGS = [
        Binding("escape", "cancel", "Cancel", show=False),
    ]

    DEFAULT_CSS = """
    NameModal {
        align: center middle;
    }
    NameModal > Vertical {
        width: 60;
        max-width: 80%;
        height: auto;
        padding: 1 2;
        border: thick $accent;
        background: $surface;
    }
    NameModal Label {
        padding-bottom: 1;
        color: $text;
    }
    NameModal Input {
        margin-bottom: 1;
    }
    """

    def __init__(
        self,
        prompt: str,
        *,
        initial: str = "",
        placeholder: str = "",
    ) -> None:
        super().__init__()
        self.prompt = prompt
        self.initial = initial
        self.placeholder = placeholder

    def compose(self) -> ComposeResult:
        with Vertical():
            yield Label(self.prompt)
            yield Input(value=self.initial, placeholder=self.placeholder, id="name-input")

    def on_mount(self) -> None:
        self.query_one("#name-input", Input).focus()

    @on(Input.Submitted)
    def _submit(self, event: Input.Submitted) -> None:
        value = event.value.strip()
        self.dismiss(value or None)

    def action_cancel(self) -> None:
        self.dismiss(None)


class ConfirmModal(ModalScreen[bool]):
    """Yes / No confirmation dialog with Enter = Yes, Esc = No."""

    BINDINGS = [
        Binding("escape", "cancel", "No", show=False),
        Binding("enter", "confirm", "Yes", show=False),
        Binding("y", "confirm", "Yes", show=False),
        Binding("n", "cancel", "No", show=False),
    ]

    DEFAULT_CSS = """
    ConfirmModal {
        align: center middle;
    }
    ConfirmModal > Vertical {
        width: 60;
        max-width: 80%;
        height: auto;
        padding: 1 2;
        border: thick $error;
        background: $surface;
    }
    ConfirmModal Label {
        padding-bottom: 1;
        color: $text;
    }
    ConfirmModal Horizontal {
        height: auto;
        align: center middle;
    }
    ConfirmModal Button {
        margin: 0 1;
    }
    """

    def __init__(self, message: str) -> None:
        super().__init__()
        self.message = message

    def compose(self) -> ComposeResult:
        from textual.containers import Horizontal

        with Vertical():
            yield Label(self.message)
            with Horizontal():
                yield Button("Yes (y)", id="yes", variant="error")
                yield Button("No (n)", id="no")

    def on_mount(self) -> None:
        self.query_one("#yes", Button).focus()

    @on(Button.Pressed, "#yes")
    def _yes(self) -> None:
        self.dismiss(True)

    @on(Button.Pressed, "#no")
    def _no(self) -> None:
        self.dismiss(False)

    def action_confirm(self) -> None:
        self.dismiss(True)

    def action_cancel(self) -> None:
        self.dismiss(False)
