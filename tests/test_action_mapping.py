"""Tests that pin down the exact tmux argv produced by each high-level action.

These tests are the contract between widgets/keybindings and the tmux CLI: if
someone refactors :class:`TmuxClient`, an unexpected argv change breaks here.
"""

from __future__ import annotations

import pytest

from tmuxui.actions import ActionController
from tmuxui.commands import COMMANDS, by_id


# --------------------------------------------------------------- session ops


def test_new_session_argv(stub) -> None:
    controller = ActionController(stub, mode="hub")
    controller.new_session("work")
    assert stub.argvs[-1] == ["tmux", "new-session", "-d", "-s", "work"]


def test_kill_session_argv(stub) -> None:
    controller = ActionController(stub, mode="hub")
    controller.kill_session("work")
    assert stub.argvs[-1] == ["tmux", "kill-session", "-t", "work"]


def test_rename_session_argv(stub) -> None:
    controller = ActionController(stub, mode="hub")
    controller.rename_session("work", "wip")
    assert stub.argvs[-1] == ["tmux", "rename-session", "-t", "work", "wip"]


# ---------------------------------------------------------------- window ops


def test_new_window_argv(stub) -> None:
    controller = ActionController(stub, mode="hub")
    controller.new_window("work")
    assert stub.argvs[-1] == ["tmux", "new-window", "-t", "work"]


def test_new_window_with_name_argv(stub) -> None:
    controller = ActionController(stub, mode="hub")
    controller.new_window("work", "edit")
    assert stub.argvs[-1] == ["tmux", "new-window", "-t", "work", "-n", "edit"]


def test_swap_window_argv(stub) -> None:
    controller = ActionController(stub, mode="hub")
    controller.swap_window("work:0", "work:1")
    assert stub.argvs[-1] == ["tmux", "swap-window", "-s", "work:0", "-t", "work:1"]


def test_synchronize_panes_on(stub) -> None:
    controller = ActionController(stub, mode="hub")
    controller.toggle_synchronize_panes("work:0", on=True)
    assert stub.argvs[-1] == [
        "tmux",
        "set-window-option",
        "-t",
        "work:0",
        "synchronize-panes",
        "on",
    ]


def test_synchronize_panes_off(stub) -> None:
    controller = ActionController(stub, mode="hub")
    controller.toggle_synchronize_panes("work:0", on=False)
    assert stub.argvs[-1] == [
        "tmux",
        "set-window-option",
        "-t",
        "work:0",
        "synchronize-panes",
        "off",
    ]


def test_copy_mode_argv(stub) -> None:
    controller = ActionController(stub, mode="hub")
    controller.copy_mode("work:0")
    assert stub.argvs[-1] == ["tmux", "copy-mode", "-t", "work:0"]


# ------------------------------------------------------------------ pane ops


def test_split_horizontal_argv(stub) -> None:
    controller = ActionController(stub, mode="hub")
    controller.split("work:0", vertical=False)
    assert stub.argvs[-1] == ["tmux", "split-window", "-h", "-t", "work:0"]


def test_split_vertical_argv(stub) -> None:
    controller = ActionController(stub, mode="hub")
    controller.split("work:0", vertical=True)
    assert stub.argvs[-1] == ["tmux", "split-window", "-v", "-t", "work:0"]


def test_zoom_pane_argv(stub) -> None:
    controller = ActionController(stub, mode="hub")
    controller.zoom_pane("work:0.1")
    assert stub.argvs[-1] == ["tmux", "resize-pane", "-Z", "-t", "work:0.1"]


@pytest.mark.parametrize(
    "direction,flag",
    [("U", "-U"), ("D", "-D"), ("L", "-L"), ("R", "-R")],
)
def test_resize_pane_argv(stub, direction, flag) -> None:
    controller = ActionController(stub, mode="hub")
    controller.resize_pane("work:0", direction, 7)
    assert stub.argvs[-1] == [
        "tmux",
        "resize-pane",
        flag,
        "7",
        "-t",
        "work:0",
    ]


def test_select_layout_argv(stub) -> None:
    controller = ActionController(stub, mode="hub")
    controller.select_layout("work:0", "tiled")
    assert stub.argvs[-1] == ["tmux", "select-layout", "-t", "work:0", "tiled"]


def test_kill_pane_argv(stub) -> None:
    controller = ActionController(stub, mode="hub")
    controller.kill_pane("work:0.1")
    assert stub.argvs[-1] == ["tmux", "kill-pane", "-t", "work:0.1"]


def test_select_pane_argv(stub) -> None:
    controller = ActionController(stub, mode="hub")
    controller.select_pane("work:0.2")
    assert stub.argvs[-1] == ["tmux", "select-pane", "-t", "work:0.2"]


# ------------------------------------------------------------------- attach


def test_popup_attach_uses_switch_client(stub) -> None:
    controller = ActionController(stub, mode="popup")
    controller.attach("work")
    assert stub.argvs[-1] == ["tmux", "switch-client", "-t", "work"]


def test_popup_detach_argv(stub) -> None:
    controller = ActionController(stub, mode="popup")
    controller.detach()
    assert stub.argvs[-1] == ["tmux", "detach-client"]


def test_popup_toggle_last(stub) -> None:
    controller = ActionController(stub, mode="popup")
    controller.toggle_last()
    assert stub.argvs[-1] == ["tmux", "switch-client", "-l"]


# ------------------------------------------------------------------ catalog


def test_every_command_has_unique_id() -> None:
    ids = [c.id for c in COMMANDS]
    assert len(ids) == len(set(ids))


def test_by_id_returns_known_commands() -> None:
    assert by_id("pane.zoom") is not None
    assert by_id("does.not.exist") is None


def test_command_haystacks_include_korean_and_english() -> None:
    spec = by_id("pane.zoom")
    assert spec is not None
    haystack = spec.search_haystack()
    assert "zoom" in haystack
    assert "줌" in haystack
