"""Shared test fixtures and stub helpers."""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass

import pytest

from tmuxui.tmux import TmuxClient, TmuxResult


@dataclass
class _RunCall:
    args: list[str]


class StubTmuxClient(TmuxClient):
    """In-memory TmuxClient that records every invocation.

    Tests can preset ``self.results`` (a mapping from the first arg to a
    :class:`TmuxResult`); anything else returns an empty-stdout success.
    """

    def __init__(self) -> None:
        super().__init__(binary="tmux")
        self.calls: list[_RunCall] = []
        self.results: dict[str, TmuxResult] = {}

    def _run(self, args: Sequence[str]) -> TmuxResult:  # type: ignore[override]
        argv = [self.binary, *args]
        self.calls.append(_RunCall(args=list(args)))
        sub = args[0] if args else ""
        canned = self.results.get(sub)
        if canned is not None:
            return canned
        return TmuxResult(argv=argv, returncode=0, stdout="", stderr="")

    @property
    def argvs(self) -> list[list[str]]:
        return [["tmux", *call.args] for call in self.calls]


@pytest.fixture
def stub() -> StubTmuxClient:
    return StubTmuxClient()
