"""Data models for tmux entities and parsers for tmux -F output.

The models intentionally stay small dataclasses — they only hold the fields
``tu`` actually needs to render. Anything tmux-specific that's parsed out of
``list-sessions`` / ``list-windows`` / ``list-panes`` lives here.
"""

from __future__ import annotations

from dataclasses import dataclass, field

# Field separator used in our tmux ``-F`` format strings. We pick a literal tab
# because tmux's default substitutions never emit tab characters and it keeps
# the format string readable.
SEP = "\t"

# Format strings handed to ``tmux list-* -F`` calls. Keep these aligned with
# the corresponding ``parse_*`` functions below.
SESSION_FORMAT = SEP.join(
    [
        "#{session_name}",
        "#{session_windows}",
        "#{?session_attached,1,0}",
        "#{session_activity}",
    ]
)

WINDOW_FORMAT = SEP.join(
    [
        "#{window_index}",
        "#{window_name}",
        "#{?window_active,1,0}",
        "#{window_panes}",
    ]
)

PANE_FORMAT = SEP.join(
    [
        "#{pane_index}",
        "#{pane_width}",
        "#{pane_height}",
        "#{?pane_active,1,0}",
        "#{pane_current_command}",
    ]
)


@dataclass(frozen=True, slots=True)
class Session:
    name: str
    windows: int
    attached: bool
    activity: int = 0

    @property
    def target(self) -> str:
        return self.name


@dataclass(frozen=True, slots=True)
class Window:
    session: str
    index: int
    name: str
    active: bool
    panes: int

    @property
    def target(self) -> str:
        return f"{self.session}:{self.index}"


@dataclass(frozen=True, slots=True)
class Pane:
    session: str
    window_index: int
    index: int
    width: int
    height: int
    active: bool
    command: str = ""

    @property
    def target(self) -> str:
        return f"{self.session}:{self.window_index}.{self.index}"


def _parse_bool(value: str) -> bool:
    return value.strip() == "1"


def _parse_int(value: str, default: int = 0) -> int:
    try:
        return int(value.strip())
    except (TypeError, ValueError):
        return default


@dataclass(frozen=True, slots=True)
class TmuxSnapshot:
    """An aggregate view of the tmux server's state at a point in time."""

    sessions: list[Session] = field(default_factory=list)
    windows_by_session: dict[str, list[Window]] = field(default_factory=dict)

    def windows(self, session: str) -> list[Window]:
        return self.windows_by_session.get(session, [])


def parse_sessions(output: str) -> list[Session]:
    """Parse the output of ``tmux list-sessions -F SESSION_FORMAT``."""

    sessions: list[Session] = []
    for line in output.splitlines():
        if not line.strip():
            continue
        parts = line.split(SEP)
        if len(parts) < 3:
            continue
        name = parts[0]
        windows = _parse_int(parts[1])
        attached = _parse_bool(parts[2])
        activity = _parse_int(parts[3]) if len(parts) > 3 else 0
        sessions.append(
            Session(name=name, windows=windows, attached=attached, activity=activity)
        )
    return sessions


def parse_windows(output: str, session: str) -> list[Window]:
    """Parse the output of ``tmux list-windows -t SESSION -F WINDOW_FORMAT``."""

    windows: list[Window] = []
    for line in output.splitlines():
        if not line.strip():
            continue
        parts = line.split(SEP)
        if len(parts) < 4:
            continue
        windows.append(
            Window(
                session=session,
                index=_parse_int(parts[0]),
                name=parts[1],
                active=_parse_bool(parts[2]),
                panes=_parse_int(parts[3]),
            )
        )
    return windows


def parse_panes(output: str, session: str, window_index: int) -> list[Pane]:
    """Parse the output of ``tmux list-panes -t TARGET -F PANE_FORMAT``."""

    panes: list[Pane] = []
    for line in output.splitlines():
        if not line.strip():
            continue
        parts = line.split(SEP)
        if len(parts) < 4:
            continue
        command = parts[4] if len(parts) > 4 else ""
        panes.append(
            Pane(
                session=session,
                window_index=window_index,
                index=_parse_int(parts[0]),
                width=_parse_int(parts[1]),
                height=_parse_int(parts[2]),
                active=_parse_bool(parts[3]),
                command=command,
            )
        )
    return panes
