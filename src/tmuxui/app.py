"""The Textual ``App`` that powers both Hub and Popup modes."""

from __future__ import annotations

from pathlib import Path
from typing import Literal

from textual import on
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal
from textual.css.query import NoMatches
from textual.reactive import reactive
from textual.widgets import Header, Input

from .actions import ActionController
from .commands import COMMANDS, CommandSpec, by_id
from .models import Session, Window
from .tmux import TmuxClient
from .widgets.help import HelpOverlay
from .widgets.hints import KeyHintBar
from .widgets.layout_picker import LayoutPicker
from .widgets.modals import ConfirmModal, NameModal
from .widgets.palette import CommandPalette
from .widgets.pane_picker import PanePicker
from .widgets.preview import PanePreview
from .widgets.resize_overlay import ResizeOverlay
from .widgets.sessions import SessionList
from .widgets.windows import WindowList

Mode = Literal["hub", "popup"]

# Polling interval (seconds). tmuxui doesn't subscribe to control mode events
# yet — instead the table refreshes on a steady timer plus immediately after
# every user action.
POLL_INTERVAL = 2.0


class TmuxUIApp(App[None]):
    """The tu (tmuxui) Textual application."""

    CSS_PATH = "styles.tcss"

    BINDINGS = [
        Binding("q", "quit", "Quit", show=False, priority=False),
        Binding("ctrl+c", "quit", "Quit", show=False, priority=False),
        Binding("tab", "focus_next_column", "Focus next column", show=False),
        Binding("shift+tab", "focus_prev_column", "Focus previous column", show=False),
        Binding("enter", "attach_selected", "Attach", show=False, priority=True),
        Binding("n", "new_session", "New session", show=False),
        Binding("N", "new_window", "New window", show=False),
        Binding("r", "rename_selected", "Rename", show=False),
        Binding("x", "kill_selected", "Kill", show=False),
        Binding("m", "toggle_move_mode", "Move window", show=False),
        Binding("s", "split_horizontal", "Split", show=False),
        Binding("v", "split_vertical", "Split V", show=False),
        Binding("z", "zoom", "Zoom", show=False),
        Binding("o", "pick_pane", "Pick pane", show=False),
        Binding("R", "resize_mode", "Resize mode", show=False),
        Binding("left_square_bracket", "layout_picker", "Layout", show=False),
        Binding("S", "toggle_sync", "Sync panes", show=False),
        Binding("y", "copy_mode", "Copy mode", show=False),
        Binding("L", "last_session", "Last session", show=False),
        Binding("d", "detach", "Detach (popup)", show=False),
        Binding("slash", "open_search", "Search", show=False),
        Binding("escape", "close_search", "Close search", show=False),
        Binding("colon", "open_palette", "Command palette", show=False),
        Binding("question_mark", "open_help", "Help", show=False),
    ]

    mode: reactive[Mode] = reactive("hub")
    search_query: reactive[str] = reactive("")

    # ---------------------------------------------------------------- lifecycle

    def __init__(
        self,
        tmux: TmuxClient | None = None,
        *,
        mode: Mode = "hub",
        stay_open: bool = False,
    ) -> None:
        super().__init__()
        self.tmux = tmux or TmuxClient()
        self.mode = mode
        self.controller = ActionController(self.tmux, mode=mode)
        self.controller.stay_open = stay_open
        self._all_sessions: list[Session] = []
        self._current_session: Session | None = None
        self._current_window: Window | None = None

    def compose(self) -> ComposeResult:
        yield Header(show_clock=False)
        # Search bar — hidden by default, toggled with ``/``.
        yield Input(placeholder="Filter (Esc to close)", id="search-bar")

        # Main content depends on mode.
        if self.mode == "hub":
            with Horizontal(id="main-hub"):
                yield SessionList(id="sessions")
                yield WindowList(id="windows")
                yield PanePreview(id="preview")
        else:
            with Horizontal(id="main-popup"):
                yield SessionList(id="sessions")
                yield WindowList(id="windows")

        yield KeyHintBar(mode=self.mode, id="hints")

    def on_mount(self) -> None:
        self.title = "tu — tmuxui"
        self.sub_title = "Hub" if self.mode == "hub" else "Popup"
        self.controller.bind_app(self)
        self.refresh_sessions()
        self.set_interval(POLL_INTERVAL, self.refresh_sessions)
        # Initial focus.
        self.query_one("#sessions", SessionList).focus()

    # --------------------------------------------------------------- helpers

    @property
    def sessions(self) -> SessionList:
        return self.query_one("#sessions", SessionList)

    @property
    def windows(self) -> WindowList:
        return self.query_one("#windows", WindowList)

    @property
    def preview(self) -> PanePreview | None:
        try:
            return self.query_one(PanePreview)
        except NoMatches:
            return None

    @property
    def hints(self) -> KeyHintBar:
        return self.query_one("#hints", KeyHintBar)

    @property
    def search_input(self) -> Input:
        return self.query_one("#search-bar", Input)

    def _selected_session(self) -> Session | None:
        return self.sessions.current

    def _selected_window(self) -> Window | None:
        return self.windows.current

    def _active_target_for_action(self) -> str | None:
        """The tmux target string an action like split / zoom should act on.

        Hub: derive from the highlighted session+window.
        Popup: prefer the *current* attached pane via the highlighted window.
        """

        window = self._selected_window()
        if window is not None:
            return window.target
        session = self._selected_session()
        if session is not None:
            return session.target
        return None

    # ------------------------------------------------------------ data refresh

    def refresh_sessions(self) -> None:
        sessions = self.tmux.list_sessions()
        self._all_sessions = sessions
        self.sessions.set_sessions(self._filtered_sessions())

    def refresh_windows(self, session: Session | None) -> None:
        if session is None:
            self.windows.set_windows([])
            if self.preview:
                self.preview.clear()
            self._current_session = None
            return
        self._current_session = session
        windows = self.tmux.list_windows(session.name)
        self.windows.set_windows(self._filter_windows(windows))

    def refresh_preview(self, window: Window | None) -> None:
        self._current_window = window
        if self.preview is None:
            return
        if window is None:
            self.preview.clear()
            return
        capture = self.tmux.capture_pane(window.target)
        self.preview.show_capture(capture)

    # ------------------------------------------------------------------ search

    def _filtered_sessions(self) -> list[Session]:
        query = self.search_query.strip().lower()
        if not query:
            return self._all_sessions
        return [s for s in self._all_sessions if query in s.name.lower()]

    def _filter_windows(self, windows: list[Window]) -> list[Window]:
        query = self.search_query.strip().lower()
        if not query:
            return windows
        return [w for w in windows if query in w.name.lower()]

    # ---------------------------------------------------------------- events

    @on(SessionList.Highlighted)
    def _on_session_highlighted(self, event: SessionList.Highlighted) -> None:
        self.refresh_windows(event.session)

    @on(WindowList.Highlighted)
    def _on_window_highlighted(self, event: WindowList.Highlighted) -> None:
        self.refresh_preview(event.window)

    @on(Input.Changed, "#search-bar")
    def _on_search_changed(self, event: Input.Changed) -> None:
        self.search_query = event.value
        self.sessions.set_sessions(self._filtered_sessions())
        if self._current_session is not None:
            windows = self.tmux.list_windows(self._current_session.name)
            self.windows.set_windows(self._filter_windows(windows))

    @on(Input.Submitted, "#search-bar")
    def _on_search_submitted(self) -> None:
        self.action_close_search()

    # ----------------------------------------------------------------- actions

    def action_focus_next_column(self) -> None:
        current = self.focused
        order = ["sessions", "windows", "preview"]
        order = [name for name in order if self.query(f"#{name}")]
        if not order:
            return
        try:
            idx = order.index(current.id) if current and current.id in order else -1
        except ValueError:
            idx = -1
        target = order[(idx + 1) % len(order)]
        self.query_one(f"#{target}").focus()

    def action_focus_prev_column(self) -> None:
        current = self.focused
        order = ["sessions", "windows", "preview"]
        order = [name for name in order if self.query(f"#{name}")]
        if not order:
            return
        try:
            idx = order.index(current.id) if current and current.id in order else 0
        except ValueError:
            idx = 0
        target = order[(idx - 1) % len(order)]
        self.query_one(f"#{target}").focus()

    def action_attach_selected(self) -> None:
        window = self._selected_window()
        if window is not None:
            self.controller.attach(window.target)
            self.refresh_sessions()
            return
        session = self._selected_session()
        if session is not None:
            self.controller.attach(session.target)
            self.refresh_sessions()

    def action_last_session(self) -> None:
        self.controller.toggle_last()
        self.refresh_sessions()

    def action_detach(self) -> None:
        if self.mode == "popup":
            self.controller.detach()

    # -- session ---------------------------------------------------------------

    def action_new_session(self) -> None:
        def _on_name(name: str | None) -> None:
            if name:
                self.controller.new_session(name)
                self.refresh_sessions()

        self.push_screen(NameModal("Session name:"), _on_name)

    def action_rename_selected(self) -> None:
        focus = self.focused
        if focus is self.windows and self._selected_window() is not None:
            window = self._selected_window()
            assert window is not None

            def _on_name(name: str | None) -> None:
                if name:
                    self.controller.rename_window(window.target, name)
                    self.refresh_sessions()
                    if self._current_session is not None:
                        self.refresh_windows(self._current_session)

            self.push_screen(
                NameModal("Window name:", initial=window.name), _on_name
            )
            return

        session = self._selected_session()
        if session is None:
            return

        def _on_session_rename(name: str | None) -> None:
            if name:
                self.controller.rename_session(session.target, name)
                self.refresh_sessions()

        self.push_screen(
            NameModal("Session name:", initial=session.name), _on_session_rename
        )

    def action_kill_selected(self) -> None:
        focus = self.focused
        if focus is self.windows and self._selected_window() is not None:
            window = self._selected_window()
            assert window is not None

            def _on_confirm(ok: bool) -> None:
                if ok:
                    self.controller.kill_window(window.target)
                    self.refresh_sessions()
                    if self._current_session is not None:
                        self.refresh_windows(self._current_session)

            self.push_screen(
                ConfirmModal(f"Kill window '{window.name}'?"), _on_confirm
            )
            return

        session = self._selected_session()
        if session is None:
            return

        def _on_confirm_session(ok: bool) -> None:
            if ok:
                self.controller.kill_session(session.target)
                self.refresh_sessions()

        self.push_screen(
            ConfirmModal(f"Kill session '{session.name}'?"), _on_confirm_session
        )

    # -- window ----------------------------------------------------------------

    def action_new_window(self) -> None:
        session = self._selected_session()
        if session is None:
            return
        self.controller.new_window(session.target)
        self.refresh_windows(session)

    def action_toggle_move_mode(self) -> None:
        """Swap the highlighted window with the next one (wraps).

        Earlier drafts used a separate "move mode" but a single keystroke
        ``m`` that swaps with the next window is friendlier and matches the
        way users mostly want to reorder.
        """

        window = self._selected_window()
        if window is None or self._current_session is None:
            return
        windows = self.windows.windows
        if len(windows) < 2:
            return
        try:
            idx = next(i for i, w in enumerate(windows) if w.index == window.index)
        except StopIteration:
            return
        nxt = windows[(idx + 1) % len(windows)]
        if nxt.index == window.index:
            return
        self.controller.swap_window(window.target, nxt.target)
        self.refresh_windows(self._current_session)

    def action_toggle_sync(self) -> None:
        window = self._selected_window()
        if window is None:
            return
        # No tmux query for the current value yet — toggle naively by flipping
        # based on a single round-trip: if synchronize-panes is off, turn on,
        # else off. We probe with show-window-options.
        result = self.tmux._run(
            ["show-window-options", "-v", "-t", window.target, "synchronize-panes"]
        )
        currently_on = result.stdout.strip() == "on"
        self.controller.toggle_synchronize_panes(window.target, not currently_on)
        if self._current_session is not None:
            self.refresh_windows(self._current_session)

    def action_copy_mode(self) -> None:
        window = self._selected_window()
        if window is None:
            return
        self.controller.copy_mode(window.target)

    # -- pane ------------------------------------------------------------------

    def action_split_horizontal(self) -> None:
        target = self._active_target_for_action()
        if target is None:
            return
        self.controller.split(target, vertical=False)
        self._refresh_after_pane_action()

    def action_split_vertical(self) -> None:
        target = self._active_target_for_action()
        if target is None:
            return
        self.controller.split(target, vertical=True)
        self._refresh_after_pane_action()

    def action_zoom(self) -> None:
        target = self._active_target_for_action()
        if target is None:
            return
        self.controller.zoom_pane(target)
        self._refresh_after_pane_action()

    def action_pick_pane(self) -> None:
        window = self._selected_window()
        if window is None:
            return
        panes = self.tmux.list_panes(window.target)
        if not panes:
            return

        def _on_pane(pane) -> None:
            if pane is not None:
                self.controller.select_pane(pane.target)
                self._refresh_after_pane_action()

        self.push_screen(PanePicker(panes), _on_pane)

    def action_resize_mode(self) -> None:
        target = self._active_target_for_action()
        if target is None:
            return
        self.push_screen(ResizeOverlay(self.controller, target))

    def action_layout_picker(self) -> None:
        window = self._selected_window()
        if window is None:
            return
        # Capture the existing layout so Esc can revert.
        result = self.tmux._run(
            ["display-message", "-p", "-t", window.target, "#{window_layout}"]
        )
        previous = result.stdout.strip() or None

        def _on_layout(_picked) -> None:
            self._refresh_after_pane_action()

        self.push_screen(
            LayoutPicker(self.controller, window.target, previous_layout=previous),
            _on_layout,
        )

    def _refresh_after_pane_action(self) -> None:
        if self._current_session is not None:
            self.refresh_windows(self._current_session)
        window = self._selected_window()
        if window is not None:
            self.refresh_preview(window)

    # -- search ----------------------------------------------------------------

    def action_open_search(self) -> None:
        bar = self.search_input
        bar.add_class("visible")
        bar.focus()

    def action_close_search(self) -> None:
        bar = self.search_input
        if "visible" not in bar.classes:
            return
        bar.value = ""
        self.search_query = ""
        bar.remove_class("visible")
        self.sessions.focus()
        self.refresh_sessions()

    # -- meta -----------------------------------------------------------------

    def action_open_palette(self) -> None:
        def _on_command(spec: CommandSpec | None) -> None:
            if spec is not None:
                self._dispatch_command(spec.id)

        self.push_screen(CommandPalette(mode=self.mode), _on_command)

    def action_open_help(self) -> None:
        self.push_screen(HelpOverlay())

    # ---------------------------------------------------------------- dispatch

    def _dispatch_command(self, command_id: str) -> None:
        # Map command IDs to action_ methods so the palette and key bindings
        # share exactly one execution path.
        mapping = {
            "session.new": self.action_new_session,
            "session.rename": self.action_rename_selected,
            "session.kill": self.action_kill_selected,
            "session.attach": self.action_attach_selected,
            "session.last": self.action_last_session,
            "window.new": self.action_new_window,
            "window.rename": self.action_rename_selected,
            "window.kill": self.action_kill_selected,
            "window.move": self.action_toggle_move_mode,
            "window.sync": self.action_toggle_sync,
            "window.copy_mode": self.action_copy_mode,
            "pane.split_h": self.action_split_horizontal,
            "pane.split_v": self.action_split_vertical,
            "pane.pick": self.action_pick_pane,
            "pane.zoom": self.action_zoom,
            "pane.resize": self.action_resize_mode,
            "pane.layout": self.action_layout_picker,
            "pane.kill": self.action_kill_selected,
            "nav.search": self.action_open_search,
            "nav.focus_next": self.action_focus_next_column,
            "nav.detach": self.action_detach,
            "meta.palette": self.action_open_palette,
            "meta.help": self.action_open_help,
            "meta.quit": self.action_quit,
        }
        handler = mapping.get(command_id)
        if handler is not None:
            handler()


# Re-exports for convenience.
__all__ = ["TmuxUIApp", "Mode", "COMMANDS", "by_id"]

# Make sure styles file resolves regardless of CWD.
TmuxUIApp.CSS_PATH = str(Path(__file__).with_name("styles.tcss"))
