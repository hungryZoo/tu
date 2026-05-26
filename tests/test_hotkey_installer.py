"""Tests for :class:`tmuxui.hotkey.HotkeyInstaller`."""

from __future__ import annotations

import pytest

from tmuxui.hotkey import DEFAULT_KEY, HotkeyInstaller
from tmuxui.tmux import TmuxResult


def _make_installer(stub):
    installer = HotkeyInstaller(stub, key=DEFAULT_KEY)
    return installer


def test_install_when_no_existing_binding(monkeypatch, stub, disable_atexit):
    monkeypatch.setattr("tmuxui.hotkey.is_tmux_installed", lambda: True)

    installer = _make_installer(stub)
    result = installer.install()

    assert result is not None and result.ok
    bind_call = next(call for call in stub.calls if call.args[0] == "bind-key")
    # ``tu --popup`` should appear at the very end of the bind-key argv and
    # the ``-n`` flag (no prefix) should be present.
    assert bind_call.args[-1].endswith("tu --popup")
    assert "-n" in bind_call.args
    assert DEFAULT_KEY in bind_call.args
    assert disable_atexit  # atexit register was called


def test_install_refuses_to_clobber_existing_binding(monkeypatch, stub, disable_atexit):
    monkeypatch.setattr("tmuxui.hotkey.is_tmux_installed", lambda: True)
    stub.results["list-keys"] = TmuxResult(
        argv=["tmux", "list-keys", "-T", "root"],
        returncode=0,
        stdout=f"bind-key  -T root  {DEFAULT_KEY}  display-popup -E whatever\n",
        stderr="",
    )

    installer = _make_installer(stub)
    result = installer.install()

    # The installer should bail and not call bind-key.
    assert result is None
    assert all(call.args[0] != "bind-key" for call in stub.calls)


def test_uninstall_calls_unbind_only_when_installed(monkeypatch, stub, disable_atexit):
    monkeypatch.setattr("tmuxui.hotkey.is_tmux_installed", lambda: True)

    installer = _make_installer(stub)

    # No-op when not yet installed.
    assert installer.uninstall() is None
    assert all(call.args[0] != "unbind-key" for call in stub.calls)

    installer.install()
    installer.uninstall()
    unbind_call = next(call for call in stub.calls if call.args[0] == "unbind-key")
    assert unbind_call.args == ["unbind-key", "-n", DEFAULT_KEY]


def test_use_context_manager_cleans_up_on_exception(monkeypatch, stub, disable_atexit):
    monkeypatch.setattr("tmuxui.hotkey.is_tmux_installed", lambda: True)

    installer = _make_installer(stub)

    with pytest.raises(RuntimeError, match="boom"):
        with installer.use():
            raise RuntimeError("boom")

    # Even though we raised, the binding should have been removed.
    last = stub.calls[-1]
    assert last.args == ["unbind-key", "-n", DEFAULT_KEY]


def test_install_skips_when_tmux_not_installed(monkeypatch, stub, disable_atexit):
    monkeypatch.setattr("tmuxui.hotkey.is_tmux_installed", lambda: False)

    installer = _make_installer(stub)
    assert installer.install() is None
    assert stub.calls == []


def test_install_skips_when_server_down(monkeypatch, stub, disable_atexit):
    monkeypatch.setattr("tmuxui.hotkey.is_tmux_installed", lambda: True)
    stub.running = False

    installer = _make_installer(stub)
    assert installer.install() is None
    assert stub.calls == []
