"""Live capture-pane preview shown in the right column of the Hub."""

from __future__ import annotations

from rich.text import Text
from textual.widgets import Static


class PanePreview(Static):
    """Renders the contents of a tmux pane via ``capture-pane``."""

    DEFAULT_CSS = """
    PanePreview {
        padding: 0 1;
        height: 1fr;
        background: $surface;
        color: $text;
    }
    """

    def show_capture(self, raw: str) -> None:
        """Render *raw* ANSI-coloured text from ``tmux capture-pane -e``."""

        if not raw:
            self.update(Text.from_markup("[dim]no preview[/]"))
            return
        try:
            # Rich understands the SGR sequences tmux emits with ``-e``.
            text = Text.from_ansi(raw)
        except Exception:
            text = Text(raw)
        self.update(text)

    def clear(self) -> None:
        self.update("")
