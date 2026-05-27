"""Tests for the ``tu`` launcher in :mod:`tmuxui.__main__`.

Specifically covers the post-exit ``execvp`` hand-off used by the
"outside tmux" Attach / New flows so that ``tmux attach-session`` takes
over the parent shell once Textual has restored the terminal.
"""

from __future__ import annotations

import sys

import pytest

import tmuxui.__main__ as main_mod


class _FakeApp:
    """Stand-in for :class:`tmuxui.app.TmuxUIApp` that records ``run`` calls."""

    instances: list[_FakeApp] = []

    def __init__(self, *, post_exit_argv: list[str] | None) -> None:
        self.post_exit_argv = post_exit_argv
        self.ran = False
        _FakeApp.instances.append(self)

    def run(self) -> None:
        self.ran = True


def _install_fake_app(monkeypatch: pytest.MonkeyPatch, argv: list[str] | None) -> None:
    _FakeApp.instances.clear()
    monkeypatch.setattr(main_mod, "is_tmux_installed", lambda: True)
    monkeypatch.setattr(main_mod, "TmuxUIApp", lambda: _FakeApp(post_exit_argv=argv))


def test_main_returns_zero_when_no_post_exit_argv(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _install_fake_app(monkeypatch, argv=None)

    execvp_calls: list[tuple[str, list[str]]] = []
    monkeypatch.setattr(
        main_mod.os,
        "execvp",
        lambda file, argv: execvp_calls.append((file, list(argv))),
    )

    code = main_mod.main([])

    assert code == 0
    assert execvp_calls == []
    assert _FakeApp.instances[-1].ran is True


def test_main_execvps_into_post_exit_argv(monkeypatch: pytest.MonkeyPatch) -> None:
    """When the app exits with a recorded argv the launcher must hand
    control over to it via ``os.execvp``."""

    argv = ["tmux", "attach-session", "-t", "play"]
    _install_fake_app(monkeypatch, argv=argv)

    execvp_calls: list[tuple[str, list[str]]] = []
    monkeypatch.setattr(
        main_mod.os,
        "execvp",
        lambda file, argv_: execvp_calls.append((file, list(argv_))),
    )

    code = main_mod.main([])

    assert execvp_calls == [("tmux", argv)]
    # In the real world execvp doesn't return, but our stub does — main
    # should still produce a valid return value.
    assert code == 0


def test_main_aborts_when_tmux_missing(
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    monkeypatch.setattr(main_mod, "is_tmux_installed", lambda: False)

    # If we reach the app at all, fail loudly.
    def _boom():  # pragma: no cover - sanity guard
        raise AssertionError("TmuxUIApp() should not be constructed without tmux")

    monkeypatch.setattr(main_mod, "TmuxUIApp", _boom)

    code = main_mod.main([])

    assert code == 2
    err = capsys.readouterr().err
    assert "tmux" in err


def test_main_version_flag_exits_cleanly(
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    """``argparse`` exits via SystemExit on ``--version``. Make sure that
    happens *before* we try to launch the app."""

    def _boom():  # pragma: no cover - sanity guard
        raise AssertionError("TmuxUIApp() should not be constructed for --version")

    monkeypatch.setattr(main_mod, "TmuxUIApp", _boom)

    with pytest.raises(SystemExit) as excinfo:
        main_mod.main(["--version"])

    assert excinfo.value.code == 0
    out = capsys.readouterr().out
    assert "tu" in out


def test_main_module_can_run_as_script(monkeypatch: pytest.MonkeyPatch) -> None:
    """Smoke check: importing as ``python -m tmuxui`` should still work."""

    # Just ensure the module exposes ``main`` for the script entrypoint.
    assert callable(main_mod.main)
    assert "main" in dir(sys.modules["tmuxui.__main__"])
