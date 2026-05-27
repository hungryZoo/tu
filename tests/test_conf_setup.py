"""Tests for :mod:`tmuxui.conf_setup` (mouse + history-limit baseline)."""

from __future__ import annotations

from pathlib import Path

from tmuxui.conf_setup import (
    HEADER_COMMENT,
    HISTORY_LIMIT,
    MANAGED_DIRECTIVES,
    MOUSE,
    Directive,
    append_directives,
    apply_directives_to_server,
    conf_has_mouse_directive,
    missing_directives,
    should_prompt,
)
from tmuxui.tmux import TmuxResult

from .conftest import StubTmuxClient

# ---------------------------------------------------------- detection


def test_missing_directives_returns_all_when_file_missing(tmp_path: Path) -> None:
    assert missing_directives(tmp_path / "nope.conf") == list(MANAGED_DIRECTIVES)


def test_missing_directives_detects_partial_config(tmp_path: Path) -> None:
    conf = tmp_path / "tmux.conf"
    conf.write_text("set -g mouse on\n")
    assert missing_directives(conf) == [HISTORY_LIMIT]


def test_missing_directives_treats_explicit_off_as_configured(tmp_path: Path) -> None:
    """``set -g mouse off`` is still "configured" — leave the user alone."""

    conf = tmp_path / "tmux.conf"
    conf.write_text("set -g mouse off\nset -g history-limit 5000\n")
    assert missing_directives(conf) == []


def test_missing_directives_ignores_comments_and_setw(tmp_path: Path) -> None:
    conf = tmp_path / "tmux.conf"
    conf.write_text(
        "# set -g mouse on\n"
        "setw -g mouse on\n"  # window-scoped form is irrelevant
        "set-option -g history-limit 42\n"  # set-option form counts
    )
    # Mouse is still missing (commented + setw don't count); history-limit
    # is configured via the ``set-option`` form.
    assert missing_directives(conf) == [MOUSE]


def test_should_prompt_true_when_anything_missing(tmp_path: Path) -> None:
    conf = tmp_path / "tmux.conf"
    conf.write_text("set -g mouse on\n")
    assert should_prompt(conf) is True


def test_should_prompt_false_when_all_present(tmp_path: Path) -> None:
    conf = tmp_path / "tmux.conf"
    conf.write_text(
        "set -g mouse on\n"
        "set -g history-limit 10000000\n"
    )
    assert should_prompt(conf) is False


def test_conf_has_mouse_directive_returns_on(tmp_path: Path) -> None:
    conf = tmp_path / "tmux.conf"
    conf.write_text("set -g status on\nset -g mouse on\n")
    assert conf_has_mouse_directive(conf) == "on"


def test_conf_has_mouse_directive_returns_off(tmp_path: Path) -> None:
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


# ----------------------------------------------------------- mutations


def test_append_directives_appends_at_end(tmp_path: Path) -> None:
    """We append (not prepend) so tmux's last-wins rule keeps our values
    authoritative even if the user has an older conflicting line."""

    conf = tmp_path / "tmux.conf"
    conf.write_text("set -g status on\nset -g history-limit 1000\n")

    append_directives([HISTORY_LIMIT], conf)

    body = conf.read_text(encoding="utf-8")
    # Original lines preserved up front.
    assert body.startswith("set -g status on\nset -g history-limit 1000\n")
    # Our block lives at the tail.
    assert body.rstrip().endswith(HISTORY_LIMIT.line)
    assert HEADER_COMMENT in body


def test_append_directives_creates_file_with_block(tmp_path: Path) -> None:
    conf = tmp_path / "nested" / "tmux.conf"
    append_directives(list(MANAGED_DIRECTIVES), conf)

    body = conf.read_text(encoding="utf-8")
    assert HEADER_COMMENT in body
    assert MOUSE.line in body
    assert HISTORY_LIMIT.line in body
    assert body.endswith("\n")


def test_append_directives_is_a_noop_for_empty_input(tmp_path: Path) -> None:
    conf = tmp_path / "tmux.conf"
    conf.write_text("set -g status on\n")

    append_directives([], conf)

    assert conf.read_text(encoding="utf-8") == "set -g status on\n"


def test_append_directives_handles_missing_trailing_newline(tmp_path: Path) -> None:
    conf = tmp_path / "tmux.conf"
    conf.write_text("set -g status on")  # no newline at EOF

    append_directives([MOUSE], conf)

    body = conf.read_text(encoding="utf-8")
    assert "set -g status on\n" in body
    assert MOUSE.line in body


def test_append_directives_preserves_other_user_directives(tmp_path: Path) -> None:
    conf = tmp_path / "tmux.conf"
    conf.write_text("bind r source-file ~/.tmux.conf\n")
    append_directives([MOUSE], conf)
    assert "bind r source-file" in conf.read_text(encoding="utf-8")


# ----------------------------------------------------------- apply


def test_apply_directives_to_server_runs_set_option_for_each(
    stub: StubTmuxClient,
) -> None:
    stub.results["info"] = TmuxResult(
        argv=["tmux", "info"], returncode=0, stdout="", stderr=""
    )

    assert apply_directives_to_server(list(MANAGED_DIRECTIVES), stub) is True

    calls = [call.args for call in stub.calls]
    assert ["set-option", "-g", "mouse", "on"] in calls
    assert ["set-option", "-g", "history-limit", "10000000"] in calls


def test_apply_directives_to_server_skips_when_no_server(stub: StubTmuxClient) -> None:
    stub.results["info"] = TmuxResult(
        argv=["tmux", "info"], returncode=1, stdout="", stderr=""
    )

    assert apply_directives_to_server([MOUSE], stub) is False
    # No set-option call should have been issued.
    assert not any(call.args and call.args[0] == "set-option" for call in stub.calls)


def test_apply_directives_to_server_reports_partial_failure(
    stub: StubTmuxClient,
) -> None:
    stub.results["info"] = TmuxResult(
        argv=["tmux", "info"], returncode=0, stdout="", stderr=""
    )
    stub.results["set-option"] = TmuxResult(
        argv=["tmux", "set-option"], returncode=1, stdout="", stderr="bad"
    )

    assert apply_directives_to_server([MOUSE], stub) is False


def test_apply_directives_to_server_is_a_noop_for_empty_input(
    stub: StubTmuxClient,
) -> None:
    assert apply_directives_to_server([], stub) is True
    assert stub.calls == []


# ----------------------------------------------------------- directive


def test_directive_line_renders_canonical_form() -> None:
    d = Directive(option="status", value="on", label="status line")
    assert d.line == "set -g status on"
