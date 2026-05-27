"""Detect and (optionally) enable tmux mouse mode for the user.

`tu` works much better when ``set -g mouse on`` is active. On startup we
politely ask the user whether to add it to ``~/.tmux.conf`` and apply it to
the running server. The user can dismiss the prompt forever via a marker
file under ``~/.config/tu``.
"""

from __future__ import annotations

import re
from pathlib import Path

from textual import on
from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Static

from .tmux import TmuxClient

CONF_PATH = Path.home() / ".tmux.conf"
STATE_DIR = Path.home() / ".config" / "tu"
DISMISS_MARKER = STATE_DIR / "no-mouse-prompt"

HEADER_COMMENT = "# Added by tu (https://github.com/hungryZoo/tu)"
MOUSE_DIRECTIVE = "set -g mouse on"

# Match ``set [-g] mouse on|off`` and ``set-option [-g] mouse on|off``. We
# explicitly do NOT match ``setw`` / ``set-window-option`` because mouse is a
# session-level option and tmux ignores window-level forms.
_MOUSE_DIRECTIVE_RE = re.compile(
    r"^\s*set(?:-option)?\s+(?:-g\s+)?mouse\s+(on|off)\b",
    re.IGNORECASE,
)


# --------------------------------------------------------------- detection


def conf_has_mouse_directive(path: Path = CONF_PATH) -> str | None:
    """Return ``"on"`` / ``"off"`` if the conf already configures mouse.

    Returns ``None`` when the file is missing or has no relevant directive.
    Comments (anything after ``#`` on a line) are ignored.
    """

    try:
        text = path.read_text(encoding="utf-8")
    except (FileNotFoundError, IsADirectoryError, OSError):
        return None
    for raw in text.splitlines():
        body = raw.split("#", 1)[0]
        match = _MOUSE_DIRECTIVE_RE.match(body)
        if match:
            return match.group(1).lower()
    return None


def mouse_enabled_at_runtime(tmux: TmuxClient) -> bool:
    """True when ``tmux show-options -gv mouse`` is currently ``on``."""

    if not tmux.server_running():
        return False
    return (tmux.show_option("mouse") or "").lower() == "on"


def should_prompt_for_mouse(
    tmux: TmuxClient,
    *,
    conf_path: Path = CONF_PATH,
    marker: Path = DISMISS_MARKER,
) -> bool:
    """Decide whether to show the mouse-setup prompt.

    We skip the prompt when:
      - the user previously chose "never ask again",
      - the tmux server already has mouse mode on,
      - the conf already mentions ``set ... mouse ...`` (on **or** off — the
        user has expressed an intent, so don't second-guess it).
    """

    if marker.exists():
        return False
    if mouse_enabled_at_runtime(tmux):
        return False
    if conf_has_mouse_directive(conf_path) is not None:
        return False
    return True


# -------------------------------------------------------------- mutations


def prepend_mouse_setting(path: Path = CONF_PATH) -> None:
    """Insert ``set -g mouse on`` at the very top of ``~/.tmux.conf``.

    Creates the parent directory and the file itself if missing. Preserves
    whatever was already in the file.
    """

    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        existing = path.read_text(encoding="utf-8")
    except FileNotFoundError:
        existing = ""

    if existing and not existing.endswith("\n"):
        existing += "\n"

    header = f"{HEADER_COMMENT}\n{MOUSE_DIRECTIVE}\n"
    new_body = header + ("\n" + existing if existing else "")
    path.write_text(new_body, encoding="utf-8")


def apply_mouse_to_server(tmux: TmuxClient) -> bool:
    """Run ``tmux set-option -g mouse on`` against the live server."""

    if not tmux.server_running():
        return False
    return tmux.set_option("mouse", "on").ok


def mark_dismissed(marker: Path = DISMISS_MARKER) -> None:
    marker.parent.mkdir(parents=True, exist_ok=True)
    marker.touch()


# ------------------------------------------------------------- the modal


class MouseSetupModal(ModalScreen[str]):
    """Ask the user whether `tu` may enable mouse mode for them.

    Dismissed with one of ``"yes"``, ``"later"``, or ``"never"``.
    """

    DEFAULT_CSS = """
    MouseSetupModal {
        align: center middle;
    }
    #mouse-modal {
        width: 64;
        max-width: 90%;
        height: auto;
        border: thick $accent;
        background: $surface;
        padding: 1 2;
    }
    #mouse-modal .msg {
        margin-bottom: 1;
    }
    #mouse-modal Horizontal {
        height: auto;
        align: center middle;
    }
    #mouse-modal Button {
        margin: 0 1;
    }
    """

    BINDINGS = [
        Binding("y", "answer('yes')", "Yes"),
        Binding("enter", "answer('yes')", "Yes", priority=True),
        Binding("n", "answer('later')", "Later"),
        Binding("escape", "answer('later')", "Later"),
    ]

    def compose(self) -> ComposeResult:
        with Vertical(id="mouse-modal"):
            yield Static(
                "tmux 마우스 모드가 꺼져 있어요.\n"
                f"[b]~/.tmux.conf[/b] 맨 위에 [b]{MOUSE_DIRECTIVE}[/b] 을 추가할까요?\n\n"
                "추가하면 클릭/스크롤이 바로 동작합니다.",
                classes="msg",
            )
            with Horizontal():
                yield Button("예, 추가 (y)", id="m-yes", variant="success")
                yield Button("다음에 (n)", id="m-later")
                yield Button("묻지 않기", id="m-never")

    def action_answer(self, choice: str) -> None:
        self.dismiss(choice)

    @on(Button.Pressed, "#m-yes")
    def _click_yes(self) -> None:
        self.dismiss("yes")

    @on(Button.Pressed, "#m-later")
    def _click_later(self) -> None:
        self.dismiss("later")

    @on(Button.Pressed, "#m-never")
    def _click_never(self) -> None:
        self.dismiss("never")
