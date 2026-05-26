# tu — tmuxui

A friendly TUI on top of [tmux](https://github.com/tmux/tmux). The CLI is `tu` (the python package is `tmuxui`).

`tu` is meant to **lower the entry bar for tmux**. You type `tu` once and arrows, `Enter`, and on-screen hints take you everywhere — no `prefix + key` to memorize.

- Outside tmux → fullscreen Hub (sessions / windows / live preview).
- Inside tmux → automatically appears as a tmux `display-popup` overlay.
- Press `F12` from anywhere inside tmux to open the same popup (auto-bound while Hub is running).
- `?` opens the cheat-sheet, `:` opens the command palette (fuzzy search, Korean aliases supported).

## Install (development)

```bash
git clone https://github.com/hungryZoo/tu.git
cd tu
uv sync
uv run tu          # try it
```

Or install the wheel from the [Releases](https://github.com/hungryZoo/tu/releases) page:

```bash
pipx install <wheel-url>
# or
uv tool install <wheel-url>
```

`tmux` (>= 3.2 for `display-popup`) must already be installed.

## Quick start

```bash
tu                # opens Hub (outside tmux) or popup (inside tmux)
```

In Hub:

- `↑/↓` or `j/k` to move, `Tab` to switch column.
- `Enter` to attach to the highlighted session/window.
- `n` new session · `N` new window · `r` rename · `x` kill · `m` move window.
- `s/v` split · `o` pane picker · `z` zoom · `R` resize mode · `[` layout picker.
- `S` toggle synchronize-panes · `y` copy mode · `L` last session.
- `/` incremental search · `:` command palette · `?` help · `q` quit.

In Popup (inside tmux, via `tu` or `F12`):

- Everything above, plus `d` to detach the current client.
- `Esc` or `q` closes the popup without action.

## Optional: bind F12 permanently in tmux

While Hub is running it auto-installs the F12 binding and cleans it up on exit. If you mostly use `tu` from inside tmux, add this to `~/.tmux.conf`:

```tmux
bind-key -n F12 display-popup -E -h 80% -w 80% -T " tu " "tu --popup"
```

Reload with `tmux source-file ~/.tmux.conf`.

## Modes & flags

```text
tu                 # auto: Hub if outside tmux, popup overlay if inside
tu --popup         # force popup mode (used by F12 binding)
tu --key F1        # change the hotkey (default F12)
tu --stay          # popup stays open after each action
tu --version
```

## How it works

```
+----------------------------------------------------------------+
| tu (tmuxui)                                  [?] help  [q]uit  |
+----------------------+------------------+----------------------+
| Sessions             | Windows          | Preview (capture)    |
|  > work    3w  *att  |  0: edit   *     | $ vim app.py         |
|    play    1w        |  1: logs         |   ...                |
|    scratch 2w        |  2: shell        |                      |
+----------------------+------------------+----------------------+
| Enter:attach  Tab:focus  n:new  N:window  s/v:split  z:zoom    |
| o:pane  R:resize  [:layout  S:sync  /:search  :cmd  ?:help     |
+----------------------------------------------------------------+
```

`tu` is a thin Textual app over `tmux` CLI subprocess calls — no `tmux -CC` yet.
Attach uses `App.suspend()` + `tmux attach`, so detach (or the popup `d` action) brings you straight back to Hub.

## License

MIT. See [LICENSE](LICENSE).
