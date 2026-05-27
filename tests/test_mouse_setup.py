"""Tests for :mod:`tmuxui.mouse_setup`."""

from __future__ import annotations

from pathlib import Path

import pytest

from tmuxui.mouse_setup import (
    HEADER_COMMENT,
    MOUSE_DIRECTIVE,
    apply_mouse_to_server,
    conf_has_mouse_directive,
    mark_dismissed,
    mouse_enabled_at_runtime,
    prepend_mouse_setting,
    should_prompt_for_mouse,
)
from tmuxui.tmux import TmuxResult

from .conftest import StubTmuxClient

# ----------------------------------------------------- conf detection


def test_conf_has_mouse_directive_returns_none_for_missing_file(tmp_path: Path) -> None:
    assert conf_has_mouse_directive(tmp_path / "nope.conf") is None


def test_conf_has_mouse_directive_finds_on(tmp_path: Path) -> None:
    conf = tmp_path / "tmux.conf"
    conf.write_text("set -g status on\nset -g mouse on\n")
    assert conf_has_mouse_directive(conf) == "on"


def test_conf_has_mouse_directive_finds_off(tmp_path: Path) -> None:
    conf = tmp_path / "tmux.conf"
    conf.write_text("set-option -g mouse off\n")
    assert conf_has_mouse_directive(conf) == "off"


def test_conf_has_mouse_directive_handles_whitespace(tmp_path: Path) -> None:
    conf = tmp_path / "tmux.conf"
    conf.write_text("   set   -g   mouse   on   \n")
    assert conf_has_mouse_directive(conf) == "on"


def test_conf_has_mouse_directive_ignores_comments(tmp_path: Path) -> None:
    conf = tmp_path / "tmux.conf"
    conf.write_text(
        "# set -g mouse on\n"
        "set -g status on  # set -g mouse on\n"
    )
    assert conf_has_mouse_directive(conf) is None


def test_conf_has_mouse_directive_ignores_setw(tmp_path: Path) -> None:
    conf = tmp_path / "tmux.conf"
    conf.write_text("setw -g mouse on\n")
    assert conf_has_mouse_directive(conf) is None


# --------------------------------------------------------- prepend


def test_prepend_mouse_setting_creates_file(tmp_path: Path) -> None:
    conf = tmp_path / "tmux.conf"
    prepend_mouse_setting(conf)
    body = conf.read_text(encoding="utf-8")
    assert body.startswith(HEADER_COMMENT)
    assert MOUSE_DIRECTIVE in body
    # File ends with a newline.
    assert body.endswith("\n")


def test_prepend_mouse_setting_keeps_existing_lines(tmp_path: Path) -> None:
    conf = tmp_path / "tmux.conf"
    conf.write_text("set -g status on\nbind r source-file ~/.tmux.conf\n")
    prepend_mouse_setting(conf)
    body = conf.read_text(encoding="utf-8")
    assert body.startswith(f"{HEADER_COMMENT}\n{MOUSE_DIRECTIVE}\n")
    assert "set -g status on" in body
    assert "bind r source-file" in body


def test_prepend_mouse_setting_appends_trailing_newline(tmp_path: Path) -> None:
    conf = tmp_path / "tmux.conf"
    conf.write_text("set -g status on")  # no trailing newline
    prepend_mouse_setting(conf)
    body = conf.read_text(encoding="utf-8")
    assert "set -g status on\n" in body


def test_prepend_mouse_setting_creates_parent_dir(tmp_path: Path) -> None:
    conf = tmp_path / "nested" / "subdir" / "tmux.conf"
    prepend_mouse_setting(conf)
    assert conf.exists()


# --------------------------------------------------------- should_prompt


@pytest.fixture
def fresh_paths(tmp_path: Path) -> tuple[Path, Path]:
    return tmp_path / "tmux.conf", tmp_path / "no-mouse-prompt"


def _stub_with_runtime(stub: StubTmuxClient, *, server: bool, mouse: str | None) -> None:
    stub.results["info"] = TmuxResult(
        argv=["tmux", "info"],
        returncode=0 if server else 1,
        stdout="",
        stderr="",
    )
    if mouse is not None:
        stub.results["show-options"] = TmuxResult(
            argv=["tmux", "show-options", "-gv", "mouse"],
            returncode=0,
            stdout=mouse + "\n",
            stderr="",
        )


def test_should_prompt_when_nothing_configured(
    stub: StubTmuxClient, fresh_paths: tuple[Path, Path]
) -> None:
    conf, marker = fresh_paths
    _stub_with_runtime(stub, server=True, mouse="off")
    assert should_prompt_for_mouse(stub, conf_path=conf, marker=marker) is True


def test_no_prompt_when_marker_exists(
    stub: StubTmuxClient, fresh_paths: tuple[Path, Path]
) -> None:
    conf, marker = fresh_paths
    marker.parent.mkdir(parents=True, exist_ok=True)
    marker.touch()
    _stub_with_runtime(stub, server=True, mouse="off")
    assert should_prompt_for_mouse(stub, conf_path=conf, marker=marker) is False


def test_no_prompt_when_runtime_on(
    stub: StubTmuxClient, fresh_paths: tuple[Path, Path]
) -> None:
    conf, marker = fresh_paths
    _stub_with_runtime(stub, server=True, mouse="on")
    assert should_prompt_for_mouse(stub, conf_path=conf, marker=marker) is False


def test_no_prompt_when_conf_already_mentions_mouse(
    stub: StubTmuxClient, fresh_paths: tuple[Path, Path]
) -> None:
    conf, marker = fresh_paths
    conf.write_text("set -g mouse off\n")
    _stub_with_runtime(stub, server=True, mouse="off")
    assert should_prompt_for_mouse(stub, conf_path=conf, marker=marker) is False


def test_no_prompt_when_server_not_running_but_conf_set(
    stub: StubTmuxClient, fresh_paths: tuple[Path, Path]
) -> None:
    conf, marker = fresh_paths
    conf.write_text("set -g mouse on\n")
    _stub_with_runtime(stub, server=False, mouse=None)
    assert should_prompt_for_mouse(stub, conf_path=conf, marker=marker) is False


def test_prompt_when_server_not_running_and_no_conf(
    stub: StubTmuxClient, fresh_paths: tuple[Path, Path]
) -> None:
    conf, marker = fresh_paths
    _stub_with_runtime(stub, server=False, mouse=None)
    assert should_prompt_for_mouse(stub, conf_path=conf, marker=marker) is True


# --------------------------------------------------------- apply / mark


def test_apply_mouse_to_server_skips_when_no_server(stub: StubTmuxClient) -> None:
    _stub_with_runtime(stub, server=False, mouse=None)
    assert apply_mouse_to_server(stub) is False
    # No set-option call should have been issued.
    assert not any(call.args[0] == "set-option" for call in stub.calls)


def test_apply_mouse_to_server_runs_set_option(stub: StubTmuxClient) -> None:
    _stub_with_runtime(stub, server=True, mouse="off")
    assert apply_mouse_to_server(stub) is True
    assert ["set-option", "-g", "mouse", "on"] in [call.args for call in stub.calls]


def test_mouse_enabled_at_runtime_when_on(stub: StubTmuxClient) -> None:
    _stub_with_runtime(stub, server=True, mouse="on")
    assert mouse_enabled_at_runtime(stub) is True


def test_mouse_enabled_at_runtime_when_off(stub: StubTmuxClient) -> None:
    _stub_with_runtime(stub, server=True, mouse="off")
    assert mouse_enabled_at_runtime(stub) is False


def test_mark_dismissed_creates_marker(tmp_path: Path) -> None:
    marker = tmp_path / "deep" / "no-mouse-prompt"
    mark_dismissed(marker)
    assert marker.exists()
