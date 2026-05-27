"""The minimal ``tu`` TUI.

One screen: a session list and four buttons (New / Attach / Detach / Quit).
Click or press the underlined key. Arrow keys and Enter always work, no
matter which widget has focus.

Every action ultimately *closes* `tu`; what differs is what they do to tmux
first:

* **Attach**       — close `tu`, then attach the parent shell to the chosen
                     session (or, when run inside tmux, ``switch-client`` to
                     it and let the host shell take over).
* **New**          — create a session named ``tu-N`` and attach to it.
* **Detach**       — only enabled inside tmux. Detach the current client so
                     the user lands at their parent shell, then close `tu`.
* **Quit**         — close `tu`. No tmux side-effects.

For the "outside tmux" attach/new paths we don't run ``tmux attach-session``
ourselves while Textual is still running — we set
``post_exit_argv`` and let ``__main__.py`` ``execvp()`` into it after the
TUI has fully torn down. This avoids the alt-screen flicker you would get
from suspend/resume and matches the user-visible behaviour they asked for
("들어가면서 현재쉘의 tu창은 꺼짐").
"""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

from textual import on
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical
from textual.widgets import Button, DataTable, Footer, Header

from .models import Session
from .mouse_setup import (
    CONF_PATH,
    MouseSetupModal,
    apply_mouse_to_server,
    mark_dismissed,
    prepend_mouse_setting,
    should_prompt_for_mouse,
)
from .tmux import TmuxClient, attach_argv, is_inside_tmux

POLL_INTERVAL = 2.0  # seconds


class TmuxUIApp(App[None]):
    """Tiny session-only TUI on top of tmux."""

    CSS_PATH = "styles.tcss"

    # Enter is intentionally not bound at the app level: the focused widget
    # (DataTable row or a Button) handles it through its own default action.
    # Arrow keys (and j/k) are bound at the app level so the list stays
    # navigable even when a button currently holds focus.
    BINDINGS = [
        Binding("q", "quit", "Quit", show=True),
        Binding("ctrl+c", "quit", "Quit", show=False),
        Binding("n", "new_session", "New", show=True),
        Binding("a", "attach", "Attach", show=True),
        Binding("d", "detach", "Detach", show=True),
        Binding("r", "refresh_now", "Refresh", show=False),
        Binding("up", "move_cursor(-1)", "Up", show=False),
        Binding("k", "move_cursor(-1)", "Up", show=False),
        Binding("down", "move_cursor(1)", "Down", show=False),
        Binding("j", "move_cursor(1)", "Down", show=False),
    ]

    def __init__(self, tmux: TmuxClient | None = None) -> None:
        super().__init__()
        self.tmux = tmux or TmuxClient()
        self._sessions: list[Session] = []
        self._inside_tmux = is_inside_tmux()
        # When set, ``__main__.main()`` ``execvp()``s into this argv after
        # the TUI exits. Used for the "outside tmux attach/new" flows so
        # that ``tmux attach-session`` takes over the parent shell cleanly
        # instead of returning to a resumed `tu`.
        self.post_exit_argv: list[str] | None = None

    # ----------------------------------------------------------- compose

    def compose(self) -> ComposeResult:
        yield Header(show_clock=False)
        with Vertical(id="root"):
            yield DataTable(id="sessions", cursor_type="row", zebra_stripes=False)
            with Horizontal(id="buttons"):
                yield Button("New (n)", id="btn-new", variant="success")
                yield Button("Attach (a)", id="btn-attach", variant="primary")
                yield Button(
                    "Detach (d)",
                    id="btn-detach",
                    variant="warning",
                    disabled=not self._inside_tmux,
                )
                yield Button("Quit (q)", id="btn-quit", variant="error")
        yield Footer()

    def on_mount(self) -> None:
        self.title = "tu"
        self.sub_title = "in tmux" if self._inside_tmux else "outside tmux"

        table = self.query_one(DataTable)
        table.add_columns("Session", "Windows", "Attached")
        table.focus()

        self.refresh_sessions()
        self.set_interval(POLL_INTERVAL, self.refresh_sessions)

        # Offer to turn on mouse mode the first time the user lands here.
        if should_prompt_for_mouse(self.tmux):
            self.push_screen(MouseSetupModal(), self._handle_mouse_choice)

    # ----------------------------------------------------------- data

    def refresh_sessions(self) -> None:
        sessions = self.tmux.list_sessions()
        self._sessions = sessions

        table = self.query_one(DataTable)
        previous = self._selected_name(table)
        table.clear()
        for session in sessions:
            attached = "yes" if session.attached else ""
            table.add_row(session.name, str(session.windows), attached)

        # Restore cursor where possible.
        if not sessions:
            return
        target_row = 0
        if previous is not None:
            for idx, session in enumerate(sessions):
                if session.name == previous:
                    target_row = idx
                    break
        table.move_cursor(row=target_row)

    def _selected_name(self, table: DataTable | None = None) -> str | None:
        if table is None:
            table = self.query_one(DataTable)
        row = table.cursor_row
        if 0 <= row < len(self._sessions):
            return self._sessions[row].name
        return None

    # ----------------------------------------------------------- actions

    def action_attach(self) -> None:
        name = self._selected_name()
        if name is None:
            self.bell()
            return
        self._attach(name)

    def action_move_cursor(self, delta: int) -> None:
        """Move the session-list cursor by *delta*, focusing the table.

        Bound to up/down (and j/k) at the app level so the user can keep
        navigating the list while a button has focus.
        """

        table = self.query_one(DataTable)
        if table.row_count == 0:
            return
        if not table.has_focus:
            table.focus()
        new_row = max(0, min(table.row_count - 1, table.cursor_row + delta))
        if new_row != table.cursor_row:
            table.move_cursor(row=new_row)

    def action_new_session(self) -> None:
        name = self.tmux.next_default_name()
        result = self.tmux.new_session(name)
        if not result.ok:
            self.notify(
                f"새 세션 생성 실패: {result.stderr.strip() or 'unknown error'}",
                severity="error",
                timeout=6,
            )
            self.bell()
            return
        # _attach takes care of closing tu and (outside tmux) handing the
        # parent shell off to the freshly created session.
        self._attach(name)

    def action_detach(self) -> None:
        """Detach the tmux client *and* close `tu`.

        On success the user lands at their parent shell with `tu` already
        gone. On failure we keep the menu open and show *why* in a toast
        so the user can tell us what went wrong.
        """

        if not self._inside_tmux:
            self.notify(
                "Detach는 tmux 안에서 tu를 실행했을 때만 동작해요.",
                severity="warning",
                timeout=4,
            )
            self.bell()
            return

        ok, detail = self._detach_session_for_current_pane()
        if not ok:
            # Last-resort: try the captured no-arg form via TmuxClient.
            # Unlikely to actually detach (the pane pty isn't a client tty),
            # but it surfaces tmux's own error message for diagnostics.
            fallback = self.tmux.detach_client()
            if fallback.ok:
                ok, detail = True, ""
            else:
                detail = (
                    detail or fallback.stderr.strip() or "unknown error"
                ).strip()

        if ok:
            # Detach worked — `tu` should go away too so the user lands
            # cleanly at the parent shell.
            self.exit()
            return

        self.notify(
            f"Detach 실패: {detail}",
            severity="error",
            timeout=8,
        )
        self.bell()

    def _detach_session_for_current_pane(self) -> tuple[bool, str]:
        """Detach every client attached to the session that owns $TMUX_PANE.

        Returns ``(ok, detail)`` where *detail* is either the session name
        on success or a short error string on failure (suitable for showing
        to the user).
        """

        pane = os.environ.get("TMUX_PANE", "")
        if not pane:
            return False, "$TMUX_PANE is not set"

        info = subprocess.run(
            ["tmux", "display-message", "-p", "-t", pane, "#S"],
            capture_output=True,
            text=True,
            check=False,
        )
        session = info.stdout.strip()
        if info.returncode != 0 or not session:
            err = info.stderr.strip() or f"display-message exited {info.returncode}"
            return False, err

        result = subprocess.run(
            ["tmux", "detach-client", "-s", session],
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode != 0:
            err = (
                result.stderr.strip()
                or f"detach-client exited {result.returncode}"
            )
            return False, err
        return True, f"session “{session}”"

    def action_refresh_now(self) -> None:
        self.refresh_sessions()

    # ---------------------------------------------------------- mouse setup

    def _handle_mouse_choice(self, choice: str | None) -> None:
        if choice == "yes":
            try:
                prepend_mouse_setting()
            except OSError as exc:
                self.notify(
                    f"~/.tmux.conf 수정 실패: {exc}",
                    severity="error",
                    timeout=6,
                )
                return
            applied = apply_mouse_to_server(self.tmux)
            msg = f"{CONF_PATH} 에 set -g mouse on 추가 완료"
            if applied:
                msg += " · 현재 세션에도 적용됨"
            else:
                msg += " · 다음 tmux 시작부터 적용"
            self.notify(msg, severity="information", timeout=6)
        elif choice == "never":
            try:
                mark_dismissed()
            except OSError:
                # Best-effort; ignore if we can't persist the decision.
                pass
            self.notify("다음부터 묻지 않을게요.", timeout=3)
        # "later" (or dismiss): silent, ask again next launch.

    # ---------------------------------------------------------- mouse

    @on(Button.Pressed, "#btn-new")
    def _click_new(self) -> None:
        self.action_new_session()

    @on(Button.Pressed, "#btn-attach")
    def _click_attach(self) -> None:
        self.action_attach()

    @on(Button.Pressed, "#btn-detach")
    def _click_detach(self) -> None:
        self.action_detach()

    @on(Button.Pressed, "#btn-quit")
    def _click_quit(self) -> None:
        # ``App.action_quit`` is an async coroutine; calling it from a sync
        # event handler creates an un-awaited coroutine and the click ends
        # up being a no-op. The plain ``exit()`` method is sync.
        self.exit()

    @on(DataTable.RowSelected)
    def _row_selected(self, event: DataTable.RowSelected) -> None:
        row = event.cursor_row
        if 0 <= row < len(self._sessions):
            self._attach(self._sessions[row].name)

    # ---------------------------------------------------------- attach

    def _attach(self, target: str) -> None:
        """Close `tu` and hand the parent shell off to *target*.

        Inside tmux: ``switch-client`` moves the existing client and we
        exit. Outside tmux: we record the desired ``tmux attach-session``
        argv and exit — ``__main__.main()`` does the actual ``execvp`` once
        Textual has fully cleaned up.
        """

        if self._inside_tmux:
            result = self.tmux.switch_client(target)
            if not result.ok:
                self.notify(
                    f"세션 전환 실패: {result.stderr.strip() or 'unknown error'}",
                    severity="error",
                    timeout=6,
                )
                self.bell()
                return
            self.exit()
            return

        self.post_exit_argv = attach_argv(target)
        self.exit()


# Resolve the stylesheet relative to this module so the wheel ships it.
TmuxUIApp.CSS_PATH = str(Path(__file__).with_name("styles.tcss"))
