"""Pilot-based smoke tests for the TmuxUIApp UI.

Verifies the layout (four buttons, Detach disabled outside tmux),
navigation behavior (arrow keys move the table cursor even when a button
holds focus), and that Attach/Detach hit the right tmux commands.
"""

from __future__ import annotations

import asyncio
from pathlib import Path

import pytest
from textual.widgets import Button, DataTable

import tmuxui.app as appmod
import tmuxui.mouse_setup as mouse_mod
from tmuxui.app import TmuxUIApp
from tmuxui.models import SEP
from tmuxui.tmux import TmuxClient, TmuxResult


class FakeTmux(TmuxClient):
    """Stub TmuxClient that returns two sessions and never spawns subprocesses."""

    def __init__(self) -> None:
        super().__init__()
        self.calls: list[list[str]] = []

    def _run(self, args):  # type: ignore[override]
        self.calls.append(list(args))
        a = args[0] if args else ""
        if a == "list-sessions":
            body = "\n".join(
                [
                    SEP.join(["work", "3", "1"]),
                    SEP.join(["play", "1", "0"]),
                ]
            )
            return TmuxResult(argv=["tmux", *args], returncode=0, stdout=body, stderr="")
        if a == "info":
            return TmuxResult(argv=["tmux", *args], returncode=0, stdout="", stderr="")
        if a == "show-options":
            return TmuxResult(argv=["tmux", *args], returncode=0, stdout="on\n", stderr="")
        return TmuxResult(argv=["tmux", *args], returncode=0, stdout="", stderr="")


def _run(coro):
    return asyncio.run(coro)


@pytest.fixture
def silence_mouse_prompt(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    """Point the mouse-setup helpers at a temp dir with mouse already 'on'.

    With ``mouse_enabled_at_runtime`` returning True the modal never opens,
    keeping these pilot tests focused on the main screen.
    """

    marker = tmp_path / "no-mouse-prompt"
    monkeypatch.setattr(mouse_mod, "CONF_PATH", tmp_path / "tmux.conf")
    monkeypatch.setattr(mouse_mod, "DISMISS_MARKER", marker)
    monkeypatch.setattr(appmod, "CONF_PATH", tmp_path / "tmux.conf")
    # Make the prompt-decision call a no-op regardless of state.
    monkeypatch.setattr(appmod, "should_prompt_for_mouse", lambda tmux: False)
    yield


def test_four_buttons_render_with_correct_labels(silence_mouse_prompt) -> None:
    async def go():
        app = TmuxUIApp(tmux=FakeTmux())
        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()
            ids = [b.id for b in app.query(Button)]
            labels = [str(b.label) for b in app.query(Button)]
            assert ids == ["btn-new", "btn-attach", "btn-detach", "btn-quit"]
            assert labels == [
                "New (n)",
                "Attach (a)",
                "Detach & Quit (d)",
                "Quit (q)",
            ]

    _run(go())


def test_detach_button_disabled_outside_tmux(
    monkeypatch: pytest.MonkeyPatch, silence_mouse_prompt
) -> None:
    monkeypatch.delenv("TMUX", raising=False)

    async def go():
        app = TmuxUIApp(tmux=FakeTmux())
        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()
            assert app.query_one("#btn-detach", Button).disabled is True
            assert app.query_one("#btn-attach", Button).disabled is False

    _run(go())


def test_detach_button_enabled_inside_tmux(
    monkeypatch: pytest.MonkeyPatch, silence_mouse_prompt
) -> None:
    monkeypatch.setenv("TMUX", "/tmp/tmux-fake")

    async def go():
        app = TmuxUIApp(tmux=FakeTmux())
        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()
            assert app.query_one("#btn-detach", Button).disabled is False

    _run(go())


def test_arrow_keys_move_cursor_when_button_focused(silence_mouse_prompt) -> None:
    """Tab to a button, press down → table cursor advances and refocuses."""

    async def go():
        app = TmuxUIApp(tmux=FakeTmux())
        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()
            table = app.query_one(DataTable)
            assert table.cursor_row == 0

            # Move focus off the table onto the New button.
            app.query_one("#btn-new", Button).focus()
            await pilot.pause()
            assert table.has_focus is False

            await pilot.press("down")
            await pilot.pause()
            assert table.cursor_row == 1
            assert table.has_focus is True  # arrow keys reclaim focus

            await pilot.press("up")
            await pilot.pause()
            assert table.cursor_row == 0

    _run(go())


def test_attach_action_runs_switch_client_when_inside_tmux(
    monkeypatch: pytest.MonkeyPatch, silence_mouse_prompt
) -> None:
    monkeypatch.setenv("TMUX", "/tmp/tmux-fake")

    async def go():
        fake = FakeTmux()
        app = TmuxUIApp(tmux=fake)
        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()
            await pilot.press("down")  # highlight "play"
            await pilot.pause()
            await pilot.press("a")
            await pilot.pause()

            switch_calls = [c for c in fake.calls if c and c[0] == "switch-client"]
            assert switch_calls == [["switch-client", "-t", "play"]]

    _run(go())


def test_clicking_attach_button_inside_tmux_switches(
    monkeypatch: pytest.MonkeyPatch, silence_mouse_prompt
) -> None:
    monkeypatch.setenv("TMUX", "/tmp/tmux-fake")

    async def go():
        fake = FakeTmux()
        app = TmuxUIApp(tmux=fake)
        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()
            await pilot.click("#btn-attach")
            await pilot.pause()

            switch_calls = [c for c in fake.calls if c and c[0] == "switch-client"]
            # Cursor starts on row 0 (work).
            assert switch_calls == [["switch-client", "-t", "work"]]

    _run(go())


def test_detach_button_quits_when_inside_tmux(
    monkeypatch: pytest.MonkeyPatch, silence_mouse_prompt
) -> None:
    """In a non-TTY pilot environment App.suspend() raises and we fall back
    to the captured ``self.tmux.detach_client()`` path. Either way the
    subprocess (or stub) must receive ``detach-client`` *and* the app must
    exit so the user is dropped back to the outer shell.
    """

    monkeypatch.setenv("TMUX", "/tmp/tmux-fake")

    async def go():
        fake = FakeTmux()
        app = TmuxUIApp(tmux=fake)
        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()
            await pilot.press("d")
            await pilot.pause()

            assert ["detach-client"] in [c for c in fake.calls]
            assert app._exit is True


    _run(go())


def test_detach_uses_app_suspend_when_supported(
    monkeypatch: pytest.MonkeyPatch, silence_mouse_prompt
) -> None:
    """When ``app.suspend()`` works, action_detach hands the TTY to a real
    ``tmux detach-client`` subprocess (not the captured TmuxClient path).
    """

    import subprocess as subprocess_mod

    monkeypatch.setenv("TMUX", "/tmp/tmux-fake")

    suspend_calls: list[bool] = []
    subprocess_calls: list[list[str]] = []

    class FakeCompleted:
        returncode = 0

    def fake_run(argv, check=False):
        subprocess_calls.append(list(argv))
        return FakeCompleted()

    monkeypatch.setattr(subprocess_mod, "run", fake_run)
    # Also patch the symbol imported into app.py.
    monkeypatch.setattr(appmod.subprocess, "run", fake_run)

    async def go():
        fake = FakeTmux()
        app = TmuxUIApp(tmux=fake)

        # Replace suspend() with a no-op context manager that records its
        # use so we can prove the TTY hand-off path is exercised.
        from contextlib import contextmanager

        @contextmanager
        def fake_suspend(self=app):
            suspend_calls.append(True)
            yield

        monkeypatch.setattr(app, "suspend", fake_suspend)

        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()
            await pilot.press("d")
            await pilot.pause()

        assert suspend_calls, "expected app.suspend() to be used"
        assert ["tmux", "detach-client"] in subprocess_calls
        # Captured path should NOT have been used.
        assert all(c[0] != "detach-client" for c in fake.calls)

    _run(go())
