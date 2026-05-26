"""Shared test fixtures and stub helpers."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

import pytest

from tmuxui.tmux import TmuxClient, TmuxResult


@dataclass
class _RunCall:
    args: list[str]


class StubTmuxClient(TmuxClient):
    """In-memory TmuxClient that records every invocation.

    Tests can preset ``self.results`` (a mapping from the first arg to a
    :class:`TmuxResult`) — anything else returns an empty-stdout success.
    """

    def __init__(self) -> None:
        super().__init__(binary="tmux")
        self.calls: list[_RunCall] = []
        self.results: dict[str, TmuxResult] = {}
        self.exists: bool = True
        self.running: bool = True

    def _run(self, args, *, check: bool = False) -> TmuxResult:  # type: ignore[override]
        argv = [self.binary, *args]
        self.calls.append(_RunCall(args=list(args)))
        sub = args[0] if args else ""
        canned = self.results.get(sub)
        if canned is not None:
            return canned
        return TmuxResult(argv=argv, returncode=0, stdout="", stderr="")

    # Make ``server_running`` deterministic for tests.
    def server_running(self) -> bool:  # type: ignore[override]
        return self.exists and self.running

    def list_keys(self, *, table: str = "root") -> str:  # type: ignore[override]
        canned = self.results.get("list-keys")
        return canned.stdout if canned else ""

    @property
    def argvs(self) -> list[list[str]]:
        return [["tmux", *call.args] for call in self.calls]


@pytest.fixture
def stub() -> StubTmuxClient:
    return StubTmuxClient()


@pytest.fixture
def disable_atexit(monkeypatch):
    """Stop ``HotkeyInstaller`` from registering atexit hooks during tests."""

    import atexit as _atexit

    registered: list[Any] = []

    def fake_register(func, *args, **kwargs):
        registered.append((func, args, kwargs))
        return func

    monkeypatch.setattr(_atexit, "register", fake_register)
    return registered
