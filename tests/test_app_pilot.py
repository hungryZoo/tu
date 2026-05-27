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
            buttons = list(app.query(Button))
            ids = [b.id for b in buttons]
            labels = [str(b.label) for b in buttons]
            assert ids == ["btn-new", "btn-attach", "btn-detach", "btn-quit"]
            assert labels == [
                "New (n)",
                "Attach (a)",
                "Detach (d)",
                "Quit (q)",
            ]
            variants = {b.id: b.variant for b in buttons}
            assert variants == {
                "btn-new": "success",
                "btn-attach": "primary",
                "btn-detach": "warning",
                "btn-quit": "error",
            }

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


def test_detach_succeeds_without_quitting_the_app(
    monkeypatch: pytest.MonkeyPatch, silence_mouse_prompt
) -> None:
    """Detach now only detaches — `tu` itself stays running. A
    successful detach must call ``tmux detach-client -s <session>`` and
    must NOT call ``self.exit()``."""

    import subprocess as subprocess_mod

    monkeypatch.setenv("TMUX", "/tmp/tmux-fake")
    monkeypatch.setenv("TMUX_PANE", "%7")

    subprocess_calls: list[list[str]] = []

    class FakeCompleted:
        def __init__(self, stdout: str = "", returncode: int = 0, stderr: str = "") -> None:
            self.stdout = stdout
            self.returncode = returncode
            self.stderr = stderr

    def fake_run(argv, *, capture_output=False, text=False, check=False):
        subprocess_calls.append(list(argv))
        if argv[:3] == ["tmux", "display-message", "-p"]:
            return FakeCompleted(stdout="work\n", returncode=0)
        if argv[:2] == ["tmux", "detach-client"]:
            return FakeCompleted(stdout="", returncode=0)
        return FakeCompleted()

    monkeypatch.setattr(subprocess_mod, "run", fake_run)
    monkeypatch.setattr(appmod.subprocess, "run", fake_run)

    async def go():
        fake = FakeTmux()
        app = TmuxUIApp(tmux=fake)
        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()
            await pilot.press("d")
            await pilot.pause()

            assert [
                "tmux",
                "display-message",
                "-p",
                "-t",
                "%7",
                "#S",
            ] in subprocess_calls
            assert ["tmux", "detach-client", "-s", "work"] in subprocess_calls
            assert not any(c[0] == "detach-client" for c in fake.calls)
            # The big behavioural change: `tu` stays alive.
            assert app._exit is False

    _run(go())


def test_detach_failure_surfaces_tmux_stderr(
    monkeypatch: pytest.MonkeyPatch, silence_mouse_prompt
) -> None:
    """When tmux returns a non-zero exit code the user must see the actual
    error message — not just a silent no-op."""

    import subprocess as subprocess_mod

    monkeypatch.setenv("TMUX", "/tmp/tmux-fake")
    monkeypatch.setenv("TMUX_PANE", "%7")

    class FakeCompleted:
        def __init__(self, stdout="", returncode=0, stderr="") -> None:
            self.stdout = stdout
            self.returncode = returncode
            self.stderr = stderr

    def fake_run(argv, *, capture_output=False, text=False, check=False):
        if argv[:3] == ["tmux", "display-message", "-p"]:
            return FakeCompleted(stdout="work\n", returncode=0)
        if argv[:2] == ["tmux", "detach-client"]:
            return FakeCompleted(returncode=1, stderr="no current client\n")
        return FakeCompleted()

    monkeypatch.setattr(subprocess_mod, "run", fake_run)
    monkeypatch.setattr(appmod.subprocess, "run", fake_run)

    async def go():
        fake = FakeTmux()
        # Make the captured fallback ALSO fail so we hit the error toast.
        original_run = fake._run

        def failing_run(args):
            result = original_run(args)
            if args and args[0] == "detach-client":
                from tmuxui.tmux import TmuxResult
                return TmuxResult(
                    argv=["tmux", *args],
                    returncode=1,
                    stdout="",
                    stderr="no current client",
                )
            return result

        fake._run = failing_run  # type: ignore[method-assign]

        app = TmuxUIApp(tmux=fake)
        notifications: list[str] = []
        original_notify = app.notify

        def capture(message, *args, **kwargs):
            notifications.append(str(message))
            return original_notify(message, *args, **kwargs)

        monkeypatch.setattr(app, "notify", capture)

        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()
            await pilot.press("d")
            await pilot.pause()

            assert any("Detach 실패" in n for n in notifications), notifications
            assert any("no current client" in n for n in notifications), notifications
            assert app._exit is False

    _run(go())


def test_detach_outside_tmux_shows_warning(
    monkeypatch: pytest.MonkeyPatch, silence_mouse_prompt
) -> None:
    monkeypatch.delenv("TMUX", raising=False)

    async def go():
        app = TmuxUIApp(tmux=FakeTmux())
        notifications: list[str] = []
        original_notify = app.notify

        def capture(message, *args, **kwargs):
            notifications.append(str(message))
            return original_notify(message, *args, **kwargs)

        monkeypatch.setattr(app, "notify", capture)

        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()
            # Press `d` directly — the button is disabled but the keybinding
            # still fires action_detach.
            await pilot.press("d")
            await pilot.pause()
            assert any("tmux 안에서" in n for n in notifications), notifications
            assert app._exit is False

    _run(go())


def test_quit_button_click_actually_exits(silence_mouse_prompt) -> None:
    """Regression: ``self.action_quit()`` is async; the click handler used
    to drop the coroutine on the floor so the button did nothing."""

    async def go():
        app = TmuxUIApp(tmux=FakeTmux())
        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()
            await pilot.click("#btn-quit")
            await pilot.pause()
            assert app._exit is True

    _run(go())


def test_quit_keypress_still_exits(silence_mouse_prompt) -> None:
    async def go():
        app = TmuxUIApp(tmux=FakeTmux())
        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()
            await pilot.press("q")
            await pilot.pause()
            assert app._exit is True

    _run(go())
