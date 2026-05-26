"""Static command registry shared by the command palette and the help overlay.

Each :class:`CommandSpec` is pure metadata — the actual handler lives on the
running ``App`` and is resolved by ``id``. Keeping things this way means
adding a new action is a single dataclass entry plus a matching method on
the app, no plumbing through three files.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Literal

Category = Literal["session", "window", "pane", "navigation", "meta"]


@dataclass(frozen=True, slots=True)
class CommandSpec:
    id: str
    en: str
    ko: str
    key: str
    category: Category
    description: str
    aliases: tuple[str, ...] = field(default_factory=tuple)

    def search_haystack(self) -> str:
        # Join all searchable text so fuzzy matching has Korean + English
        # + the key itself in one comparable string.
        parts = [self.en, self.ko, self.key, *self.aliases]
        return " ".join(parts).lower()


COMMANDS: tuple[CommandSpec, ...] = (
    # ---------------------------------------------------------------- session
    CommandSpec(
        "session.new",
        "New session",
        "새 세션",
        "n",
        "session",
        "Create a new tmux session and prompt for a name.",
        aliases=("create session", "세션 생성"),
    ),
    CommandSpec(
        "session.rename",
        "Rename session",
        "세션 이름 변경",
        "r",
        "session",
        "Rename the highlighted session.",
        aliases=("rename", "이름",),
    ),
    CommandSpec(
        "session.kill",
        "Kill session",
        "세션 삭제",
        "x",
        "session",
        "Kill the highlighted session.",
        aliases=("delete session", "삭제"),
    ),
    CommandSpec(
        "session.attach",
        "Attach / switch",
        "접속",
        "Enter",
        "session",
        "Attach (Hub) or switch-client (Popup) to the highlighted session.",
        aliases=("attach", "switch", "접속"),
    ),
    CommandSpec(
        "session.last",
        "Last session",
        "최근 세션",
        "L",
        "session",
        "Toggle to the previously used session.",
        aliases=("toggle last", "이전 세션", "토글"),
    ),
    # ----------------------------------------------------------------- window
    CommandSpec(
        "window.new",
        "New window",
        "새 윈도우",
        "N",
        "window",
        "Create a new window in the current session.",
        aliases=("create window", "윈도우 생성"),
    ),
    CommandSpec(
        "window.rename",
        "Rename window",
        "윈도우 이름 변경",
        "r",
        "window",
        "Rename the highlighted window.",
    ),
    CommandSpec(
        "window.kill",
        "Kill window",
        "윈도우 삭제",
        "x",
        "window",
        "Kill the highlighted window.",
    ),
    CommandSpec(
        "window.move",
        "Move window",
        "윈도우 이동",
        "m",
        "window",
        "Enter move mode: arrows reorder the window, Enter confirms.",
        aliases=("reorder", "재정렬"),
    ),
    CommandSpec(
        "window.sync",
        "Toggle synchronize-panes",
        "입력 동기화",
        "S",
        "window",
        "Broadcast typed input to every pane in the window.",
        aliases=("synchronize", "동기화"),
    ),
    CommandSpec(
        "window.copy_mode",
        "Copy mode",
        "카피 모드",
        "y",
        "window",
        "Enter tmux copy mode in the active pane (popup closes).",
        aliases=("scrollback", "스크롤백"),
    ),
    # ------------------------------------------------------------------- pane
    CommandSpec(
        "pane.split_h",
        "Split horizontally",
        "가로 분할",
        "s",
        "pane",
        "Split the active pane left/right.",
        aliases=("split", "분할"),
    ),
    CommandSpec(
        "pane.split_v",
        "Split vertically",
        "세로 분할",
        "v",
        "pane",
        "Split the active pane top/bottom.",
        aliases=("split vertical", "분할"),
    ),
    CommandSpec(
        "pane.pick",
        "Pick pane focus",
        "페인 선택",
        "o",
        "pane",
        "Open the pane picker for the current window.",
        aliases=("focus pane", "포커스"),
    ),
    CommandSpec(
        "pane.zoom",
        "Toggle zoom",
        "줌 토글",
        "z",
        "pane",
        "Zoom / unzoom the active pane.",
        aliases=("zoom", "최대화"),
    ),
    CommandSpec(
        "pane.resize",
        "Resize mode",
        "리사이즈 모드",
        "R",
        "pane",
        "Enter resize mode — arrows/HJKL grow or shrink the active pane.",
        aliases=("resize", "크기"),
    ),
    CommandSpec(
        "pane.layout",
        "Layout picker",
        "레이아웃 선택",
        "[",
        "pane",
        "Pick a named layout (live preview).",
        aliases=("layout", "배치"),
    ),
    CommandSpec(
        "pane.kill",
        "Kill pane",
        "페인 삭제",
        "x",
        "pane",
        "Kill the active pane.",
    ),
    # ------------------------------------------------------------ navigation
    CommandSpec(
        "nav.search",
        "Incremental search",
        "검색",
        "/",
        "navigation",
        "Filter sessions and windows by name.",
        aliases=("filter", "find"),
    ),
    CommandSpec(
        "nav.focus_next",
        "Focus next column",
        "다음 컬럼",
        "Tab",
        "navigation",
        "Cycle column focus.",
    ),
    CommandSpec(
        "nav.detach",
        "Detach client",
        "나가기",
        "d",
        "navigation",
        "Detach the current tmux client (popup only).",
        aliases=("detach", "나가기"),
    ),
    # ------------------------------------------------------------------- meta
    CommandSpec(
        "meta.palette",
        "Command palette",
        "명령 팔레트",
        ":",
        "meta",
        "Open this palette.",
    ),
    CommandSpec(
        "meta.help",
        "Help / cheat sheet",
        "도움말",
        "?",
        "meta",
        "Show the help overlay.",
    ),
    CommandSpec(
        "meta.quit",
        "Quit",
        "종료",
        "q",
        "meta",
        "Quit tu (Hub mode).",
    ),
)


def by_id(command_id: str) -> CommandSpec | None:
    for command in COMMANDS:
        if command.id == command_id:
            return command
    return None


def filter_commands(query: str, mode: Literal["hub", "popup"] = "hub") -> list[CommandSpec]:
    """Subsequence match (very small, no external fuzzy dep).

    A command matches if every character of ``query`` appears in
    :meth:`CommandSpec.search_haystack` in order. Both Korean and English
    text is in the haystack so the user can type either.
    """

    q = query.strip().lower()
    if not q:
        result = list(COMMANDS)
    else:
        result = [c for c in COMMANDS if _subsequence_match(q, c.search_haystack())]

    if mode == "hub":
        # ``detach`` doesn't apply outside a tmux client; hide it.
        result = [c for c in result if c.id != "nav.detach"]
    return result


def _subsequence_match(query: str, haystack: str) -> bool:
    it = iter(haystack)
    return all(ch in it for ch in query)
