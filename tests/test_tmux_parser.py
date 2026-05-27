"""Unit tests for :mod:`tmuxui.models` and the naming helper in
:mod:`tmuxui.tmux`."""

from __future__ import annotations

from tmuxui.models import SEP, Session, parse_sessions
from tmuxui.tmux import TmuxResult

from .conftest import StubTmuxClient


def _row(*fields: object) -> str:
    return SEP.join(str(f) for f in fields)


def test_parse_sessions_basic() -> None:
    output = "\n".join(
        [
            _row("work", 3, 1),
            _row("play", 1, 0),
            _row("scratch", 2, 0),
        ]
    )

    sessions = parse_sessions(output)

    assert [s.name for s in sessions] == ["work", "play", "scratch"]
    assert sessions[0].windows == 3
    assert sessions[0].attached is True
    assert sessions[1].attached is False
    assert sessions[2].target == "scratch"


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

    assert sessions == [Session(name="solo", windows=1, attached=False)]


def test_parse_sessions_handles_empty_string() -> None:
    assert parse_sessions("") == []


def test_next_default_name_returns_first_free_slot(stub: StubTmuxClient) -> None:
    output = "\n".join(
        [
            _row("tu-1", 1, 1),
            _row("tu-2", 1, 0),
            _row("work", 1, 0),
        ]
    )
    stub.results["list-sessions"] = TmuxResult(
        argv=["tmux", "list-sessions"],
        returncode=0,
        stdout=output,
        stderr="",
    )

    assert stub.next_default_name() == "tu-3"


def test_next_default_name_starts_at_one_when_empty(stub: StubTmuxClient) -> None:
    assert stub.next_default_name() == "tu-1"


def test_new_session_uses_detached_flag(stub: StubTmuxClient) -> None:
    stub.new_session("tu-1")

    assert stub.argvs == [["tmux", "new-session", "-d", "-s", "tu-1"]]


def test_switch_client_targets_session(stub: StubTmuxClient) -> None:
    stub.switch_client("work")

    assert stub.argvs == [["tmux", "switch-client", "-t", "work"]]


def test_detach_client_argv(stub: StubTmuxClient) -> None:
    stub.detach_client()

    assert stub.argvs == [["tmux", "detach-client"]]


def test_kill_session_argv(stub: StubTmuxClient) -> None:
    stub.kill_session("play")

    assert stub.argvs == [["tmux", "kill-session", "-t", "play"]]
