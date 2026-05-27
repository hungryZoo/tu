# tu

A tiny TUI menu on top of tmux. Run `tu`, see your sessions, pick one. That's it.

## What it does

- Run `tu` → a small menu opens showing every tmux session
- **↑ / ↓** (or **j / k**) navigate the list — even while a button is focused
- **Enter** or click a row → attach to that session
- **n** or click **New** → create a new session (auto-named `tu-1`, `tu-2`, …) and attach
- **a** or click **Attach** → attach to whichever session is highlighted
- **d** or click **Detach** → detach the current tmux client and close `tu`
  (only enabled when run inside tmux)
- **q** or click **Quit** → close `tu`. No tmux side-effects.
- **Delete key** or click the red **Delete** button → confirmation modal
  asks if you really mean it; only then is the session killed

Every action closes the `tu` window — what differs is what it does to tmux
first. See [Behavior](#behavior) for the exact spec.

No preview, no command palette. Mouse fully supported.

```
+------------------------------------------------------------+
|  tu                                  in tmux               |
+------------------------------------------------------------+
|  Session   Windows   Attached                              |
|------------------------------------------------------------|
| > work        3        yes                                 |
|   play        1                                            |
|   scratch     2                                            |
|                                                            |
+------------------------------------------------------------+
| [New (n)] [Attach (a)] [Detach (d)] [Quit (q)] [Delete]    |
+------------------------------------------------------------+
|  n New   a Attach   d Detach   q Quit   del Delete         |
+------------------------------------------------------------+
```

## Install

Requires Python 3.10+ and a `tmux` binary on PATH.

```bash
# from PyPI (when published)
pipx install tmuxui

# or from source
git clone https://github.com/hungryZoo/tu.git
cd tu
uv sync
uv run tu
```

## Behavior

`tu` behaves a little differently depending on whether you launched it
from your parent shell or from inside a tmux pane.

### 1. From the parent shell (outside tmux)

1. Run `tu` — the menu opens in the parent shell.
1a. Pick a session with ↑/↓ and **Enter** (or click **Attach**) — `tu`
    closes and the parent shell hands itself over to `tmux attach-session
    -t <name>`. When you later detach from tmux you land back at the
    parent shell, not at `tu`.
1b. Press **n** (or click **New**) — `tu` creates a fresh `tu-N` session
    and attaches the parent shell to it (same hand-off as 1a).
1c. Press **q** (or click **Quit**) — `tu` simply closes.

The **Detach** button is disabled in this mode because there is no tmux
client to detach.

### 2. From a tmux pane (inside tmux)

2. Run `tu` inside a tmux pane — the menu opens in that pane.
2a. Same as 1a, except the existing tmux client is moved to the picked
    session via `tmux switch-client -t <name>` and `tu` exits.
2b. Same as 1b, with the new session reached via `switch-client`.
2c. Same as 1c.
2d. Press **d** (or click **Detach**) — `tu` runs
    `tmux detach-client -s <session>` (the session is resolved from
    `$TMUX_PANE`) so the current client is detached and you land at the
    parent shell, then `tu` closes too.

Under the hood, the "outside tmux" Attach/New flows don't keep `tu`
suspended in the background — `tu` exits cleanly first and the launcher
``execvp``s into `tmux attach-session`. That means there's no flicker
when you eventually detach: you go straight from your tmux session back
to the parent shell prompt.

If a detach fails for any reason, `tu` stays open and shows the actual
tmux error in a toast so you can see what went wrong.

## Deleting a session

Sessions can be deleted from the menu — but the action is gated by a
confirmation modal so a single keystroke can't nuke anything.

1. Press the **Delete** key (or click the red **Delete** button) on
   the highlighted row.
2. A confirmation modal opens: *"Really delete session '<name>'? This
   cannot be undone."* The initial focus is on **Back**, so an
   accidental Enter cancels.
3. Tab to **Delete** and press Enter (or click it). To bail out:
   press Escape, click **Back**, or just Enter the focused Back
   button. On confirm, `tu` runs `tmux kill-session -t <name>` and
   refreshes the list.

If you ran `tu` from inside the very session you are deleting, tmux
also tears down that pane — you'll land at the parent shell.

Tip: on macOS laptops the dedicated *forward-delete* key is **fn +
delete**.

## `~/.tmux.conf` baseline

Every time `tu` launches, it checks `~/.tmux.conf` for two directives:

| Directive                             | Why `tu` wants it                          |
| ------------------------------------- | ------------------------------------------ |
| `set -g mouse on`                     | clicking and scrolling in tmux Just Work   |
| `set -g history-limit 10000000`       | a generously-sized scrollback buffer       |

If either is missing, a small modal asks whether to add it:

> Add the following to `~/.tmux.conf`?
>
> &nbsp;&nbsp;**set -g mouse on**   (mouse support)
> &nbsp;&nbsp;**set -g history-limit 10000000**   (scrollback size)

Pick **Yes, add** and `tu` will:

1. Append the missing directives to the **end** of `~/.tmux.conf`
   (creating the file if absent) under a `# Added by tu` header
   comment. tmux's last-line-wins rule means our values stay
   authoritative even if an older conflicting line lives higher up.
2. Apply them to the running tmux server immediately via
   `tmux set-option -g <opt> <value>`.

Pick **Later** to skip just this run — `tu` re-checks every launch
until both directives are present (or you've explicitly disabled them in
your conf, in which case your preference is respected and the modal
stays away).

## Tips

- Want a hotkey to pop `tu` open? Bind it in your `~/.tmux.conf`:

  ```tmux
  bind-key -n F12 display-popup -E "tu"
  ```

## Develop

```bash
uv sync
uv run pytest
uv run tu
```

## License

MIT
