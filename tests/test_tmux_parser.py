"""Unit tests for the ``tmux -F`` output parsers in :mod:`tmuxui.models`."""

from __future__ import annotations

from tmuxui.models import (
    SEP,
    parse_panes,
    parse_sessions,
    parse_windows,
)


def _row(*fields: object) -> str:
    return SEP.join(str(f) for f in fields)


def test_parse_sessions_basic() -> None:
    output = "\n".join(
        [
            _row("work", 3, 1, 1700000000),
            _row("play", 1, 0, 1699999999),
            _row("scratch", 2, 0, 1700001234),
        ]
    )

    sessions = parse_sessions(output)

    assert [s.name for s in sessions] == ["work", "play", "scratch"]
    assert sessions[0].windows == 3
    assert sessions[0].attached is True
    assert sessions[1].attached is False
    assert sessions[2].activity == 1700001234


def test_parse_sessions_skips_blank_and_short_lines() -> None:
    output = "\n".join(
        [
            "",
            "   ",
            _row("solo", 1, 0),
            "broken",  # only one field, ignored
        ]
    )

    sessions = parse_sessions(output)

    assert len(sessions) == 1
    assert sessions[0] == sessions[0].__class__(
        name="solo", windows=1, attached=False, activity=0
    )


def test_parse_windows_attaches_session_name() -> None:
    output = "\n".join(
        [
            _row(0, "edit", 1, 2),
            _row(1, "logs", 0, 1),
        ]
    )

    windows = parse_windows(output, session="work")

    assert [w.index for w in windows] == [0, 1]
    assert [w.name for w in windows] == ["edit", "logs"]
    assert windows[0].session == "work"
    assert windows[0].active is True
    assert windows[1].active is False
    assert windows[0].panes == 2
    assert windows[0].target == "work:0"


def test_parse_panes_attaches_window_index() -> None:
    output = "\n".join(
        [
            _row(0, 80, 24, 1, "vim"),
            _row(1, 80, 12, 0, "zsh"),
        ]
    )

    panes = parse_panes(output, session="work", window_index=2)

    assert panes[0].session == "work"
    assert panes[0].window_index == 2
    assert panes[0].index == 0
    assert panes[0].width == 80
    assert panes[0].height == 24
    assert panes[0].active is True
    assert panes[0].command == "vim"
    assert panes[0].target == "work:2.0"
    assert panes[1].active is False
    assert panes[1].command == "zsh"


def test_parse_panes_tolerates_missing_command() -> None:
    output = _row(0, 80, 24, 1)  # no command column

    panes = parse_panes(output, session="x", window_index=0)

    assert panes[0].command == ""


def test_parse_sessions_handles_empty_string() -> None:
    assert parse_sessions("") == []
    assert parse_windows("", "work") == []
    assert parse_panes("", "work", 0) == []
