# tu

A tiny TUI menu on top of tmux. Run `tu`, see your sessions, pick one. That's it.

## What it does

- Run `tu` → a small menu opens showing every tmux session
- **Enter** or click a row → attach to that session
- **n** or click **New** → create a new session (auto-named `tu-1`, `tu-2`, …) and attach
- **d** or click **Detach** → detach the current client (only enabled when you ran `tu` inside tmux)
- **q** or click **Quit** → close the menu

No preview, no command palette, no F12 binding. Mouse fully supported.

```
+--------------------------------------+
|  tu                       in tmux    |
+--------------------------------------+
|  Session   Windows   Attached        |
|--------------------------------------|
| > work        3        yes           |
|   play        1                      |
|   scratch     2                      |
|                                      |
+--------------------------------------+
|     [ New (n) ]  [ Detach (d) ]      |
|          [ Quit (q) ]                |
+--------------------------------------+
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

| Context | What happens when you pick a session |
| --- | --- |
| Outside tmux | Textual suspends, `tmux attach -t <name>` takes over the terminal, and when you detach (`prefix d`) the `tu` menu reappears. |
| Inside tmux  | `tmux switch-client -t <name>` moves your client to the picked session and `tu` exits. |

The **Detach** button is disabled when `$TMUX` is not set.

## Tips

- For mouse clicks to reach `tu` inside tmux, enable mouse mode: `set -g mouse on` in your `~/.tmux.conf`.
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
