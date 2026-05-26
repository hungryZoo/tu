"""Footer key hint bar.

Always visible at the bottom of the screen. The bar updates as focus moves
between widgets so the user only ever sees the keys that are meaningful for
the currently focused column or modal.
"""

from __future__ import annotations

from typing import Literal

from textual.widgets import Static

# Keys are intentionally short. The Help overlay (``?``) shows the full,
# longer-form description. Korean labels stick to vocabulary a tmux beginner
# would recognise even without prior tmux exposure.
HUB_HINTS = (
    "Enter:접속  Tab:포커스  n:새세션  N:새윈도우  r:이름변경  x:삭제  "
    "m:이동  s/v:분할  z:줌  o:페인  R:리사이즈  [:레이아웃  S:동기화  "
    "/:검색  ::명령  ?:도움말  q:종료"
)

POPUP_HINTS = (
    "Enter:전환  Tab:포커스  d:나가기  n:새세션  N:새윈도우  r:이름변경  "
    "x:삭제  s/v:분할  z:줌  o:페인  R:리사이즈  [:레이아웃  S:동기화  "
    "L:최근  ::명령  ?:도움말  Esc:닫기"
)


Mode = Literal["hub", "popup"]


class KeyHintBar(Static):
    """A one-line static widget pinned to the bottom of the app."""

    DEFAULT_CSS = """
    KeyHintBar {
        dock: bottom;
        height: 1;
        background: $boost;
        color: $text;
        padding: 0 1;
    }
    """

    def __init__(self, mode: Mode = "hub", **kwargs) -> None:
        super().__init__("", **kwargs)
        self.mode = mode
        self._refresh_text()

    def set_mode(self, mode: Mode) -> None:
        self.mode = mode
        self._refresh_text()

    def show_context(self, hints: str) -> None:
        """Override the bar with a context-specific hint string (e.g. a modal)."""

        self.update(hints)

    def restore(self) -> None:
        self._refresh_text()

    def _refresh_text(self) -> None:
        self.update(HUB_HINTS if self.mode == "hub" else POPUP_HINTS)
