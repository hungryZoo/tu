"""Pilot-based smoke tests for the TmuxUIApp UI.

Verifies the layout (five buttons, Detach gated on $TMUX, Delete gated
on having any session), navigation behavior, the Attach / New / Detach
hand-offs, and the click-only Delete + confirmation modal flow.
"""

from __future__ import annotations

import asyncio
from pathlib import Path

import pytest
from textual.widgets import Button, DataTable

import tmuxui.app as appmod
import tmuxui.conf_setup as conf_mod
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
    """Stop the conf-setup modal from appearing during pilot tests.

    The pilot tests focus on the main screen — we don't want every test
    to have to dismiss the conf modal first. Patching
    ``missing_directives`` (in both the module and the bound name inside
    ``tmuxui.app``) to return an empty list keeps the modal off-screen.
    """

    monkeypatch.setattr(conf_mod, "CONF_PATH", tmp_path / "tmux.conf")
    monkeypatch.setattr(appmod, "CONF_PATH", tmp_path / "tmux.conf")
    monkeypatch.setattr(appmod, "missing_directives", lambda: [])
    yield


def test_five_buttons_render_with_correct_labels(
    monkeypatch: pytest.MonkeyPatch, silence_mouse_prompt
) -> None:
    monkeypatch.setenv("TMUX", "/tmp/tmux-fake")

    async def go():
        app = TmuxUIApp(tmux=FakeTmux())
        async with app.run_test(size=(120, 30)) as pilot:
            await pilot.pause()
            buttons = list(app.query(Button))
            ids = [b.id for b in buttons]
            labels = [str(b.label) for b in buttons]
            assert ids == [
                "btn-new",
                "btn-attach",
                "btn-delete",
                "btn-detach",
                "btn-quit",
            ]
            assert labels == [
                "New (n)",
                "Attach (a)",
                "Delete",  # NO key hint — deliberately click-only.
                "Detach (d)",
                "Quit (q)",
            ]
            variants = {b.id: b.variant for b in buttons}
            assert variants == {
                "btn-new": "success",
                "btn-attach": "primary",
                "btn-delete": "warning",
                "btn-detach": "warning",
                "btn-quit": "error",
            }

    _run(go())


def test_delete_button_has_no_app_level_keybinding(silence_mouse_prompt) -> None:
    """The Delete action MUST stay mouse-only. Verify the app exposes no
    binding pointing to a delete-style action."""

    async def go():
        app = TmuxUIApp(tmux=FakeTmux())
        async with app.run_test(size=(120, 30)) as pilot:
            await pilot.pause()
            for binding in app.BINDINGS:
                # Binding.action / .key may be on the namedtuple-like
                # Binding instance — accept both attribute forms.
                action = getattr(binding, "action", "") or ""
                assert "delete" not in action.lower(), binding
                assert "kill" not in action.lower(), binding

    _run(go())


def test_delete_button_disabled_when_no_sessions(
    monkeypatch: pytest.MonkeyPatch, silence_mouse_prompt
) -> None:
    class EmptyTmux(FakeTmux):
        def _run(self, args):  # type: ignore[override]
            self.calls.append(list(args))
            if args and args[0] == "list-sessions":
                return TmuxResult(
                    argv=["tmux", *args], returncode=0, stdout="", stderr=""
                )
            return super()._run(args)

    async def go():
        app = TmuxUIApp(tmux=EmptyTmux())
        async with app.run_test(size=(120, 30)) as pilot:
            await pilot.pause()
            assert app.query_one("#btn-delete", Button).disabled is True
            assert app.query_one("#btn-attach", Button).disabled is True

    _run(go())


def test_detach_button_disabled_outside_tmux(
    monkeypatch: pytest.MonkeyPatch, silence_mouse_prompt
) -> None:
    monkeypatch.delenv("TMUX", raising=False)

    async def go():
        app = TmuxUIApp(tmux=FakeTmux())
        async with app.run_test(size=(120, 30)) as pilot:
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
        async with app.run_test(size=(120, 30)) as pilot:
            await pilot.pause()
            assert app.query_one("#btn-detach", Button).disabled is False

    _run(go())


def test_arrow_keys_move_cursor_when_button_focused(silence_mouse_prompt) -> None:
    """Tab to a button, press down → table cursor advances and refocuses."""

    async def go():
        app = TmuxUIApp(tmux=FakeTmux())
        async with app.run_test(size=(120, 30)) as pilot:
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


def test_attach_inside_tmux_switches_client_and_exits(
    monkeypatch: pytest.MonkeyPatch, silence_mouse_prompt
) -> None:
    """Spec 2a: pressing Attach inside tmux must move the existing client
    to the highlighted session *and* close `tu`."""

    monkeypatch.setenv("TMUX", "/tmp/tmux-fake")

    async def go():
        fake = FakeTmux()
        app = TmuxUIApp(tmux=fake)
        async with app.run_test(size=(120, 30)) as pilot:
            await pilot.pause()
            await pilot.press("down")  # highlight "play"
            await pilot.pause()
            await pilot.press("a")
            await pilot.pause()

            switch_calls = [c for c in fake.calls if c and c[0] == "switch-client"]
            assert switch_calls == [["switch-client", "-t", "play"]]
            assert app._exit is True
            assert app.post_exit_argv is None  # no execvp inside tmux

    _run(go())


def test_clicking_attach_button_inside_tmux_switches(
    monkeypatch: pytest.MonkeyPatch, silence_mouse_prompt
) -> None:
    monkeypatch.setenv("TMUX", "/tmp/tmux-fake")

    async def go():
        fake = FakeTmux()
        app = TmuxUIApp(tmux=fake)
        async with app.run_test(size=(120, 30)) as pilot:
            await pilot.pause()
            await pilot.click("#btn-attach")
            await pilot.pause()

            switch_calls = [c for c in fake.calls if c and c[0] == "switch-client"]
            assert switch_calls == [["switch-client", "-t", "work"]]
            assert app._exit is True

    _run(go())


def test_attach_outside_tmux_defers_attach_to_post_exit(
    monkeypatch: pytest.MonkeyPatch, silence_mouse_prompt
) -> None:
    """Spec 1a: outside tmux, Attach must close `tu` and stash the
    ``tmux attach-session`` argv so the launcher can ``execvp`` into it."""

    monkeypatch.delenv("TMUX", raising=False)

    async def go():
        fake = FakeTmux()
        app = TmuxUIApp(tmux=fake)
        async with app.run_test(size=(120, 30)) as pilot:
            await pilot.pause()
            await pilot.press("down")  # highlight "play"
            await pilot.pause()
            await pilot.press("a")
            await pilot.pause()

            # No switch-client should be issued outside tmux.
            assert not any(c and c[0] == "switch-client" for c in fake.calls)
            assert app.post_exit_argv == [
                "tmux",
                "attach-session",
                "-t",
                "play",
            ]
            assert app._exit is True

    _run(go())


def test_new_session_outside_tmux_creates_then_defers_attach(
    monkeypatch: pytest.MonkeyPatch, silence_mouse_prompt
) -> None:
    """Spec 1b: outside tmux, ``n`` creates a new session *and* hands the
    parent shell over to it via post_exit_argv."""

    monkeypatch.delenv("TMUX", raising=False)

    async def go():
        fake = FakeTmux()
        app = TmuxUIApp(tmux=fake)
        async with app.run_test(size=(120, 30)) as pilot:
            await pilot.pause()
            await pilot.press("n")
            await pilot.pause()

            new_calls = [c for c in fake.calls if c and c[0] == "new-session"]
            assert new_calls, fake.calls
            created_name = new_calls[0][new_calls[0].index("-s") + 1]
            assert app.post_exit_argv == [
                "tmux",
                "attach-session",
                "-t",
                created_name,
            ]
            assert app._exit is True

    _run(go())


def test_detach_succeeds_and_quits_the_app(
    monkeypatch: pytest.MonkeyPatch, silence_mouse_prompt
) -> None:
    """Spec 2d: a successful detach must call
    ``tmux detach-client -s <session>`` *and* close `tu` so the user
    lands at the parent shell cleanly."""

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
        async with app.run_test(size=(120, 30)) as pilot:
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
            # We must NOT fall back to the captured no-arg detach when the
            # ``-s`` path already succeeded.
            assert not any(c[0] == "detach-client" for c in fake.calls)
            # And `tu` should be gone so the user lands at the parent shell.
            assert app._exit is True
            assert app.post_exit_argv is None  # detach doesn't execvp anything

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

        async with app.run_test(size=(120, 30)) as pilot:
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

        async with app.run_test(size=(120, 30)) as pilot:
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
        async with app.run_test(size=(120, 30)) as pilot:
            await pilot.pause()
            await pilot.click("#btn-quit")
            await pilot.pause()
            assert app._exit is True

    _run(go())


def test_quit_keypress_still_exits(silence_mouse_prompt) -> None:
    async def go():
        app = TmuxUIApp(tmux=FakeTmux())
        async with app.run_test(size=(120, 30)) as pilot:
            await pilot.pause()
            await pilot.press("q")
            await pilot.pause()
            assert app._exit is True

    _run(go())


# ----------------------------------------------------- delete flow


def test_clicking_delete_opens_confirmation_modal(silence_mouse_prompt) -> None:
    """Click on Delete must open the modal — and must NOT have killed
    the session yet."""

    async def go():
        fake = FakeTmux()
        app = TmuxUIApp(tmux=fake)
        async with app.run_test(size=(120, 30)) as pilot:
            await pilot.pause()
            await pilot.click("#btn-delete")
            await pilot.pause()

            from tmuxui.app import ConfirmDeleteModal

            assert isinstance(app.screen, ConfirmDeleteModal)
            assert not any(c and c[0] == "kill-session" for c in fake.calls)

    _run(go())


def test_delete_modal_back_button_aborts(silence_mouse_prompt) -> None:
    async def go():
        fake = FakeTmux()
        app = TmuxUIApp(tmux=fake)
        async with app.run_test(size=(120, 30)) as pilot:
            await pilot.pause()
            await pilot.click("#btn-delete")
            await pilot.pause()
            await pilot.click("#cd-back")
            await pilot.pause()

            assert not any(c and c[0] == "kill-session" for c in fake.calls)

    _run(go())


def test_delete_modal_escape_aborts(silence_mouse_prompt) -> None:
    """Escape must dismiss the modal without killing anything — escape
    is the universal cancel key."""

    async def go():
        fake = FakeTmux()
        app = TmuxUIApp(tmux=fake)
        async with app.run_test(size=(120, 30)) as pilot:
            await pilot.pause()
            await pilot.click("#btn-delete")
            await pilot.pause()
            await pilot.press("escape")
            await pilot.pause()

            assert not any(c and c[0] == "kill-session" for c in fake.calls)

    _run(go())


def test_delete_modal_delete_button_kills_selected_session(
    silence_mouse_prompt,
) -> None:
    async def go():
        fake = FakeTmux()
        app = TmuxUIApp(tmux=fake)
        async with app.run_test(size=(120, 30)) as pilot:
            await pilot.pause()
            await pilot.press("down")  # highlight "play"
            await pilot.pause()
            await pilot.click("#btn-delete")
            await pilot.pause()
            await pilot.click("#cd-delete")
            await pilot.pause()

            kill_calls = [c for c in fake.calls if c and c[0] == "kill-session"]
            assert kill_calls == [["kill-session", "-t", "play"]]

    _run(go())


def test_delete_modal_failure_shows_toast(silence_mouse_prompt) -> None:
    """A failed kill-session must surface the actual tmux stderr instead
    of silently no-op'ing."""

    async def go():
        class FailingKillTmux(FakeTmux):
            def _run(self, args):  # type: ignore[override]
                self.calls.append(list(args))
                if args and args[0] == "kill-session":
                    return TmuxResult(
                        argv=["tmux", *args],
                        returncode=1,
                        stdout="",
                        stderr="can't find session: ghost",
                    )
                # Re-use FakeTmux's canned answers for everything else.
                return TmuxClient._run(self, args)  # type: ignore[misc]

        fake = FailingKillTmux()
        app = TmuxUIApp(tmux=fake)
        notifications: list[str] = []
        original_notify = app.notify

        def capture(message, *args, **kwargs):
            notifications.append(str(message))
            return original_notify(message, *args, **kwargs)

        app.notify = capture  # type: ignore[method-assign]

        async with app.run_test(size=(120, 30)) as pilot:
            await pilot.pause()
            await pilot.click("#btn-delete")
            await pilot.pause()
            await pilot.click("#cd-delete")
            await pilot.pause()

            assert any("세션 삭제 실패" in n for n in notifications), notifications
            assert any("can't find session" in n for n in notifications), notifications

    _run(go())


def test_no_key_press_can_trigger_kill_session(silence_mouse_prompt) -> None:
    """Smoke check: pressing each ASCII key on the main screen must not
    call kill-session — Delete is *click-only*. We also assert the modal
    never appears via keyboard."""

    async def go():
        fake = FakeTmux()
        app = TmuxUIApp(tmux=fake)
        async with app.run_test(size=(120, 30)) as pilot:
            await pilot.pause()
            for key in "abcdefghijklmoprstuvwxyz0123456789":
                # Skip keys that quit / detach / new-session: those
                # actions intentionally close the modal screen and would
                # disrupt this smoke check.
                if key in {"q", "n", "a", "d"}:
                    continue
                await pilot.press(key)
                await pilot.pause()

            assert not any(c and c[0] == "kill-session" for c in fake.calls)

    _run(go())
