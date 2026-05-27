"""Data models for tmux sessions and the ``tmux -F`` parser."""

from __future__ import annotations

from dataclasses import dataclass

# Field separator used in our tmux ``-F`` format string. A literal tab is
# safe because tmux's default substitutions never emit tab characters.
SEP = "\t"

SESSION_FORMAT = SEP.join(
    [
        "#{session_name}",
        "#{session_windows}",
        "#{?session_attached,1,0}",
    ]
)


@dataclass(frozen=True, slots=True)
class Session:
    name: str
    windows: int
    attached: bool

    @property
    def target(self) -> str:
        return self.name


def parse_sessions(output: str) -> list[Session]:
    """Parse the output of ``tmux list-sessions -F SESSION_FORMAT``."""

    sessions: list[Session] = []
    for line in output.splitlines():
        if not line.strip():
            continue
        parts = line.split(SEP)
        if len(parts) < 3:
            continue
        sessions.append(
            Session(
                name=parts[0],
                windows=_int(parts[1]),
                attached=parts[2].strip() == "1",
            )
        )
    return sessions


def _int(value: str, default: int = 0) -> int:
    try:
        return int(value.strip())
    except (TypeError, ValueError):
        return default
