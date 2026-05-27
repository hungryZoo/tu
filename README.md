# tu

A tiny TUI menu on top of tmux. Run `tu`, see your sessions, pick one. That's it.

## What it does

- Run `tu` → a small menu opens showing every tmux session
- **↑ / ↓** (or **j / k**) navigate the list — even while a button is focused
- **Enter** or click a row → attach to that session
- **n** or click **New** → create a new session (auto-named `tu-1`, `tu-2`, …) and attach
- **a** or click **Attach** → attach to whichever session is highlighted
- **d** or click **Detach & Quit** → detach the current client and close `tu`
  (only enabled when you ran `tu` inside tmux)
- **q** or click **Quit** → close the menu

No preview, no command palette, no F12 binding. Mouse fully supported.

```
+--------------------------------------------------+
|  tu                                in tmux       |
+--------------------------------------------------+
|  Session   Windows   Attached                    |
|--------------------------------------------------|
| > work        3        yes                       |
|   play        1                                  |
|   scratch     2                                  |
|                                                  |
+--------------------------------------------------+
| [ New (n) ] [ Attach (a) ] [ Detach & Quit (d) ] |
|                  [ Quit (q) ]                    |
+--------------------------------------------------+
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

The **Detach & Quit** button is disabled when `$TMUX` is not set.

## Mouse mode

The first time you launch `tu`, if tmux mouse mode is off and your
`~/.tmux.conf` does not configure it, you'll see a small prompt:

> tmux 마우스 모드가 꺼져 있어요.
> `~/.tmux.conf` 맨 위에 `set -g mouse on` 을 추가할까요?

Pick **예, 추가** and `tu` will:

1. Prepend `set -g mouse on` (with a `# Added by tu` header comment) to
   `~/.tmux.conf`, creating the file if missing.
2. Apply the option to the running server immediately via
   `tmux set-option -g mouse on`.

Pick **다음에** to skip just this run, or **묻지 않기** to record the
decision in `~/.config/tu/no-mouse-prompt` and never see the prompt again.
The prompt also stays away whenever your conf already contains any
`set ... mouse ...` directive (on or off) — your existing preference is
respected.

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
