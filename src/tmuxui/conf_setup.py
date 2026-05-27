"""Ensure ``~/.tmux.conf`` has the baseline tmux options ``tu`` relies on.

`tu` is a much friendlier place when these directives are active:

* ``set -g mouse on``                — clicking and scrolling Just Work
* ``set -g history-limit 10000000``  — a roomy scrollback buffer

This module owns the detection, prompting, and patching logic for both.

The check is run on **every** launch (there is no permanent
"don't ask again" path) — but once the user has expressed an intent for
a directive (even by writing ``set -g mouse off``) we leave their conf
alone. New directives are appended at the **end** of the file so they
override any earlier conflicting line tmux might have already parsed.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path

from textual import on
from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Static

from .tmux import TmuxClient

CONF_PATH = Path.home() / ".tmux.conf"
HEADER_COMMENT = "# Added by tu (https://github.com/hungryZoo/tu)"


@dataclass(frozen=True, slots=True)
class Directive:
    """A single ``set -g <option> <value>`` line ``tu`` knows how to enforce.

    *label* is a short Korean phrase shown to the user inside the prompt.
    """

    option: str
    value: str
    label: str

    @property
    def line(self) -> str:
        return f"set -g {self.option} {self.value}"


# The full list of directives `tu` cares about. Order matters: directives
# are presented to the user (and written into the file) in this order.
MOUSE = Directive(option="mouse", value="on", label="마우스 모드")
HISTORY_LIMIT = Directive(
    option="history-limit",
    value="10000000",
    label="스크롤백 라인 수",
)

MANAGED_DIRECTIVES: tuple[Directive, ...] = (MOUSE, HISTORY_LIMIT)


# Backwards-compatible aliases so other modules / older imports keep working.
MOUSE_DIRECTIVE = MOUSE.line


# --------------------------------------------------------------- detection


def _option_matcher(option: str) -> re.Pattern[str]:
    """Compile a regex that matches ``set [-g] <option> ...`` lines.

    We deliberately accept the bare ``set`` and ``set-option`` forms, and
    tolerate the ``-g`` flag being missing — both are valid ways to set a
    session-level option in tmux. We do NOT match ``setw`` /
    ``set-window-option`` because the directives we care about are
    session-scoped.
    """

    return re.compile(
        rf"^\s*set(?:-option)?\s+(?:-g\s+)?{re.escape(option)}\b",
        re.IGNORECASE,
    )


def _option_present(text: str, option: str) -> bool:
    """True if *text* already contains any ``set [-g] <option> ...`` line.

    Comments (everything after ``#`` on a line) are ignored so a commented-
    out example doesn't count as "configured".
    """

    matcher = _option_matcher(option)
    for raw in text.splitlines():
        body = raw.split("#", 1)[0]
        if matcher.match(body):
            return True
    return False


def conf_has_mouse_directive(path: Path = CONF_PATH) -> str | None:
    """Return ``"on"`` / ``"off"`` if the conf already configures mouse mode.

    Kept for the existing test suite — internally we use
    :func:`missing_directives` for prompt decisions.
    """

    try:
        text = path.read_text(encoding="utf-8")
    except (FileNotFoundError, IsADirectoryError, OSError):
        return None
    matcher = _option_matcher("mouse")
    for raw in text.splitlines():
        body = raw.split("#", 1)[0]
        match = matcher.match(body)
        if match:
            # The pattern only matches the prefix; grab the *value* by
            # scanning the rest of the line manually.
            tail = body[match.end():].strip().split()
            if tail and tail[0].lower() in {"on", "off"}:
                return tail[0].lower()
    return None


def missing_directives(path: Path = CONF_PATH) -> list[Directive]:
    """Return the managed directives the user's conf does *not* already set."""

    try:
        text = path.read_text(encoding="utf-8")
    except (FileNotFoundError, IsADirectoryError, OSError):
        # No conf at all — every directive is missing.
        return list(MANAGED_DIRECTIVES)

    return [d for d in MANAGED_DIRECTIVES if not _option_present(text, d.option)]


def should_prompt(path: Path = CONF_PATH) -> bool:
    """True when at least one managed directive is missing from the conf."""

    return bool(missing_directives(path))


# -------------------------------------------------------------- mutations


def append_directives(
    directives: list[Directive] | tuple[Directive, ...],
    path: Path = CONF_PATH,
) -> None:
    """Append *directives* to the end of ``path`` under a ``tu`` header.

    Appending (rather than prepending) means tmux's last-line-wins rule
    keeps our values authoritative even if the file already has an older
    conflicting directive higher up.

    Creates the file and any missing parent directories. A no-op when
    *directives* is empty.
    """

    if not directives:
        return

    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        existing = path.read_text(encoding="utf-8")
    except FileNotFoundError:
        existing = ""

    if existing and not existing.endswith("\n"):
        existing += "\n"

    block_parts: list[str] = []
    if existing:
        # Blank line between the user's content and our block.
        block_parts.append("")
    block_parts.append(HEADER_COMMENT)
    block_parts.extend(d.line for d in directives)
    block_parts.append("")  # trailing newline

    path.write_text(existing + "\n".join(block_parts), encoding="utf-8")


def apply_directives_to_server(
    directives: list[Directive] | tuple[Directive, ...],
    tmux: TmuxClient,
) -> bool:
    """Run ``tmux set-option -g <opt> <val>`` for each directive.

    Returns False if there is no running server or any individual
    ``set-option`` failed.
    """

    if not directives:
        return True
    if not tmux.server_running():
        return False
    ok = True
    for d in directives:
        if not tmux.set_option(d.option, d.value).ok:
            ok = False
    return ok


# ------------------------------------------------------------- the modal


class ConfSetupModal(ModalScreen[str]):
    """Ask the user whether ``tu`` may add the missing directives.

    Dismissed with one of ``"yes"`` or ``"later"``.
    """

    DEFAULT_CSS = """
    ConfSetupModal {
        align: center middle;
    }
    #conf-modal {
        width: 72;
        max-width: 95%;
        height: auto;
        border: thick $accent;
        background: $surface;
        padding: 1 2;
    }
    #conf-modal .msg {
        margin-bottom: 1;
    }
    #conf-modal .body {
        margin-bottom: 1;
    }
    #conf-modal Horizontal {
        height: auto;
        align: center middle;
    }
    #conf-modal Button {
        margin: 0 1;
    }
    """

    BINDINGS = [
        Binding("y", "answer('yes')", "Yes"),
        Binding("enter", "answer('yes')", "Yes", priority=True),
        Binding("n", "answer('later')", "Later"),
        Binding("escape", "answer('later')", "Later"),
    ]

    def __init__(self, missing: list[Directive]) -> None:
        super().__init__()
        # Defensive copy so the caller can keep mutating their list.
        self._missing = list(missing)

    def compose(self) -> ComposeResult:
        lines = ["[b]~/.tmux.conf[/b] 에 다음 항목을 추가할까요?", ""]
        for d in self._missing:
            lines.append(f"  [b]{d.line}[/b]   ({d.label})")
        lines.append("")
        lines.append("추가하면 현재 tmux 서버와 다음 실행부터 모두 적용됩니다.")
        text = "\n".join(lines)
        with Vertical(id="conf-modal"):
            yield Static(text, classes="body")
            with Horizontal():
                yield Button("예, 추가 (y)", id="m-yes", variant="success")
                yield Button("다음에 (n)", id="m-later")

    def action_answer(self, choice: str) -> None:
        self.dismiss(choice)

    @on(Button.Pressed, "#m-yes")
    def _click_yes(self) -> None:
        self.dismiss("yes")

    @on(Button.Pressed, "#m-later")
    def _click_later(self) -> None:
        self.dismiss("later")
