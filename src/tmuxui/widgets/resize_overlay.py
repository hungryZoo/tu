"""Modal overlay for interactive pane resizing.

While active, arrow keys (or HJKL) call ``tmux resize-pane`` repeatedly on the
selected pane. Enter/Esc dismisses.
"""

from __future__ import annotations

from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import Label

from ..actions import ActionController
from ..tmux import ResizeDirection


class ResizeOverlay(ModalScreen[None]):
    """Interactive resize mode. Returns ``None`` when dismissed."""

    BINDINGS = [
        Binding("escape", "done", "Done", show=False),
        Binding("enter", "done", "Done", show=False),
        Binding("up", "resize('U')", "Up", show=False),
        Binding("k", "resize('U')", "Up", show=False),
        Binding("down", "resize('D')", "Down", show=False),
        Binding("j", "resize('D')", "Down", show=False),
        Binding("left", "resize('L')", "Left", show=False),
        Binding("h", "resize('L')", "Left", show=False),
        Binding("right", "resize('R')", "Right", show=False),
        Binding("l", "resize('R')", "Right", show=False),
        Binding("shift+up", "resize('U', 10)", "Up 10", show=False),
        Binding("shift+down", "resize('D', 10)", "Down 10", show=False),
        Binding("shift+left", "resize('L', 10)", "Left 10", show=False),
        Binding("shift+right", "resize('R', 10)", "Right 10", show=False),
    ]

    DEFAULT_CSS = """
    ResizeOverlay {
        align: center middle;
    }
    ResizeOverlay > Vertical {
        width: 60;
        height: auto;
        padding: 1 2;
        border: thick $warning;
        background: $surface;
    }
    ResizeOverlay Label {
        padding-bottom: 1;
        text-align: center;
    }
    """

    def __init__(self, controller: ActionController, target: str, step: int = 5) -> None:
        super().__init__()
        self.controller = controller
        self.target = target
        self.step = step

    def compose(self) -> ComposeResult:
        with Vertical():
            yield Label(f"Resize pane {self.target}")
            yield Label(
                "Arrows / HJKL = ±{0}, Shift = ±10, Enter/Esc to finish".format(self.step)
            )

    # ----------------------------------------------------------------- actions

    def action_resize(self, direction: ResizeDirection, amount: int | None = None) -> None:
        self.controller.resize_pane(self.target, direction, amount or self.step)

    def action_done(self) -> None:
        self.dismiss(None)
