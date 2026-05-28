# tu

A tiny TUI menu on top of `tmux`. Run `tu`, see your sessions, pick
one. That's it.

Shipped as a single statically-friendly binary written in Rust
(`ratatui` + `crossterm`) — no Python, no venv, no Textual runtime.

> Looking for the original Python implementation? It's frozen on the
> [`python-legacy`](https://github.com/hungryZoo/tu/tree/python-legacy)
> branch and tagged as
> [`v0.9.0`](https://github.com/hungryZoo/tu/releases/tag/v0.9.0). All
> future development happens here.

## What it does

- Run `tu` → a small menu opens listing every tmux session
- **↑ / ↓** navigate the list — even while a button is focused
- **Single click** a row → just selects it
- **Double click** a row, **Enter** with the list focused, or click
  **Attach (a)** → attach to that session
- **n** or click **New** → create a new session (auto-named `tu-1`,
  `tu-2`, …) and attach
- **d** or click **Detach** → detach the current tmux client and close
  `tu` (only enabled when run inside tmux)
- **q** or click **Quit** → close `tu`. No tmux side-effects.
- **Delete** key or click the red **Delete** button → confirmation
  modal asks if you really mean it; only then is the session killed
- Mouse hover, press and release feedback on every button; wheel
  scrolls the session list.

Every action closes the `tu` window — what differs is what it does to
tmux first. See [Behavior](#behavior) for the exact spec.

```
+------------------------------------------------------------+
|  tu                                  · inside tmux         |
+------------------------------------------------------------+
|  ◆ Sessions                                                |
|------------------------------------------------------------|
| ▶ work        3w  · attached                               |
|   play        1w                                           |
|   scratch     2w                                           |
|                                                            |
+------------------------------------------------------------+
| [ New ] [ Attach ] [ Detach ] [ Quit ] [ Delete ]          |
+------------------------------------------------------------+
| n New  ·  a Attach  ·  d Detach  ·  q Quit  ·  del Delete  |
+------------------------------------------------------------+
```

## Install

Pre-built binaries for macOS (Apple Silicon) and Linux (x86_64 /
aarch64, both glibc and fully-static musl) are attached to every
[GitHub release](https://github.com/hungryZoo/tu/releases).

```bash
# macOS, Apple Silicon
curl -L -o tu.tar.gz \
  https://github.com/hungryZoo/tu/releases/latest/download/tu-aarch64-apple-darwin.tar.gz
tar -xzf tu.tar.gz
chmod +x tu
mv tu /usr/local/bin/tu
```

Swap the archive name for your target:

| Target                          | When to pick it                     |
| ------------------------------- | ----------------------------------- |
| `aarch64-apple-darwin`          | macOS, Apple Silicon (M1/M2/M3/M4)  |
| `x86_64-unknown-linux-gnu`      | Linux x86_64, dynamic glibc         |
| `aarch64-unknown-linux-gnu`     | Linux ARM64, dynamic glibc          |
| `x86_64-unknown-linux-musl`     | Linux x86_64, fully static          |
| `aarch64-unknown-linux-musl`    | Linux ARM64, fully static           |

The `musl` builds are statically linked and run on essentially any
glibc-or-musl Linux distro; the `gnu` builds are smaller but require
glibc ≥ 2.17 (CentOS 7 era).

## Build from source

Requires a stable Rust toolchain (1.78+).

```bash
git clone https://github.com/hungryZoo/tu.git
cd tu
cargo build --release
./target/release/tu
```

Run the test suite:

```bash
cargo test    # ~40 unit tests across models / tmux / conf / state / app
```

### Cross-compile (macOS → Linux)

We cross-compile to Linux via
[`cargo-zigbuild`](https://github.com/rust-cross/cargo-zigbuild),
which uses `zig` as the C linker so you don't need a Linux toolchain
or Docker.

```bash
brew install zig
cargo install --locked cargo-zigbuild
rustup target add \
  x86_64-unknown-linux-gnu \
  aarch64-unknown-linux-gnu \
  x86_64-unknown-linux-musl \
  aarch64-unknown-linux-musl

cargo zigbuild --release --target x86_64-unknown-linux-gnu
cargo zigbuild --release --target aarch64-unknown-linux-gnu
cargo zigbuild --release --target x86_64-unknown-linux-musl
cargo zigbuild --release --target aarch64-unknown-linux-musl
```

## Behavior

`tu` behaves a little differently depending on whether you launched it
from your parent shell or from inside a tmux pane.

### 1. From the parent shell (outside tmux)

1. Run `tu` — the menu opens in the parent shell.
1a. Pick a session with ↑/↓ and **Enter** (or double-click a row, or
    click **Attach**) — `tu` closes and the parent shell hands itself
    over to `tmux attach-session -t <name>`. When you later detach
    from tmux you land back at the parent shell, not at `tu`.
1b. Press **n** (or click **New**) — `tu` creates a fresh `tu-N`
    session and attaches the parent shell to it (same hand-off as 1a).
1c. Press **q** (or click **Quit**) — `tu` simply closes.

The **Detach** button is disabled in this mode because there is no
tmux client to detach.

### 2. From a tmux pane (inside tmux)

2. Run `tu` inside a tmux pane — the menu opens in that pane.
2a. Same as 1a, except the existing tmux client is moved to the
    picked session via `tmux switch-client -t <name>` and `tu` exits.
2b. Same as 1b, with the new session reached via `switch-client`.
2c. Same as 1c.
2d. Press **d** (or click **Detach**) — `tu` runs
    `tmux detach-client -s <session>` (the session is resolved from
    `$TMUX_PANE`) so the current client is detached and you land at
    the parent shell, then `tu` closes too.

Under the hood, the "outside tmux" Attach/New flows don't keep `tu`
suspended in the background — `tu` exits cleanly first and the
launcher `execvp`s into `tmux attach-session`. That means there's no
flicker when you eventually detach: you go straight from your tmux
session back to the parent shell prompt.

If a detach fails for any reason, `tu` stays open and shows the
actual tmux error in its status line so you can see what went wrong.

## Deleting a session

Sessions can be deleted from the menu — but the action is gated by a
confirmation modal so a single keystroke can't nuke anything.

1. Press the **Delete** key (or click the red **Delete** button) on
   the highlighted row.
2. A confirmation modal opens: *"Really delete session '<name>'?
   This cannot be undone."* The initial focus is on **Back**, so an
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

Every time `tu` launches, it checks `~/.tmux.conf` for two
directives:

| Directive                             | Why `tu` wants it                          |
| ------------------------------------- | ------------------------------------------ |
| `set -g mouse on`                     | clicking and scrolling in tmux Just Work   |
| `set -g history-limit 10000000`       | a generously-sized scrollback buffer       |

If either is missing, a small modal asks whether to add it. Pick
**Yes, add** and `tu` will:

1. Append the missing directives to the **end** of `~/.tmux.conf`
   (creating the file if absent) under a `# Added by tu` header
   comment. tmux's last-line-wins rule means our values stay
   authoritative even if an older conflicting line lives higher up.
2. Apply them to the running tmux server immediately via
   `tmux set-option -g <opt> <value>`.
3. Show a *"restart tu"* notice so the next `tu` launch picks up
   the fresh config from a clean slate — press Enter and `tu` quits.

Pick **Later** to skip just this run — `tu` re-checks every launch
until both directives are present (or you've explicitly disabled
them in your conf, in which case your preference is respected and
the modal stays away).

## Mouse: what works

Crossterm exposes the full mouse event stream — including
`MouseEventKind::Moved` — so `tu` implements:

* **Hover** — buttons brighten when the mouse passes over them; list
  rows under the cursor get a subtle background tint.
* **Press / Release** — pressing the mouse down latches a button into
  its "pressed" colour. Releasing on the same button fires the
  action; releasing off the button (cancel) just clears the latch.
* **Click-to-focus** — clicking a button moves keyboard focus there
  too, so Tab/arrow follow-ups feel coherent.
* **Single vs. double click on the list** — a single click selects a
  row without attaching; a double click within ~450 ms on the same
  row attaches.
* **Wheel** — scrolling moves the selection up/down in the session
  list regardless of where the mouse is.

If you don't see hover effects, your terminal probably isn't
reporting mouse-motion events (xterm 1003 mode). Modern terminals —
iTerm2, Alacritty, kitty, recent Apple Terminal, recent
gnome-terminal — all do. Inside tmux, the `set -g mouse on` baseline
above is what gets tmux to forward them, which is why the prompt
offers to add it on first launch.

## Tips

- Want a hotkey to pop `tu` open? Bind it in your `~/.tmux.conf`:

  ```tmux
  bind-key -n F12 display-popup -E "tu"
  ```

## Layout

```
src/
├── lib.rs          # crate root for unit tests
├── main.rs         # binary entry point (clap + execvp hand-off)
├── models.rs       # Session struct + tab-delimited parser
├── tmux.rs         # thin wrapper over the `tmux` CLI
├── conf_setup.rs   # ~/.tmux.conf directive detection / patching
├── state.rs        # AppState, Screen, Focus, ButtonId, hit-test
├── theme.rs        # Catppuccin Mocha palette + per-state styles
├── view.rs         # render(): pure ratatui draw functions
└── app.rs          # crossterm event loop + action dispatch
```

## License

MIT
