# tu (Rust port)

A Rust port of the [`tu`](../README.md) tmux session menu, living on the
`rust-port` branch. Same UX, same `~/.tmux.conf` baseline checks, same
`execvp` hand-off after the TUI exits — shipped as a single
statically-friendly binary instead of a Python package.

History on this branch:

* **v0.10.0** — rewrote the UI layer on **ratatui + crossterm**.
  Adds real mouse hover, per-button colours (Quit grey, Delete red),
  press feedback, and a button-aware focus ring. This is the version
  you almost certainly want.
* **v0.9.0** — first Rust port, built on `cursive`. Functional but no
  hover and uniform button colours. Kept in the tag history for
  reference.

Status: feature parity with the Python `v0.9.0`. Tested on macOS
(Apple Silicon).

## Why a port?

The Python version is great when you already have a working Python
toolchain. The Rust port exists so you can drop a single binary onto a
fresh machine and have `tu` work — no venvs, no `uv sync`, no Textual
runtime.

## Layout

```
rust/
├── Cargo.toml
├── src/
│   ├── lib.rs          # crate root for unit tests
│   ├── main.rs         # binary entry point (clap + execvp)
│   ├── models.rs       # Session struct + tab-delimited parser
│   ├── tmux.rs         # thin wrapper over the `tmux` CLI
│   ├── conf_setup.rs   # ~/.tmux.conf directive detection / patching
│   ├── state.rs        # AppState, Screen, Focus, ButtonId, hit-test
│   ├── theme.rs        # per-variant button colours + state matrix
│   ├── view.rs         # render(): pure ratatui draw functions
│   └── app.rs          # crossterm event loop + action dispatch
└── dist/               # build artefacts (gitignored)
```

The module split mirrors the Python package one-to-one — if you've
read the Textual implementation, you can read this one.

## Differences vs. the Python build

| Behaviour                          | Python (Textual) | Rust v0.9.0 (Cursive) | Rust v0.10.0 (Ratatui) |
|---|---|---|---|
| Session list / 5-button layout     | ✅ | ✅ | ✅ |
| `~/.tmux.conf` baseline prompt     | ✅ | ✅ | ✅ |
| `attach-session` via `execvp`      | ✅ | ✅ | ✅ |
| `del` key opens confirm modal      | ✅ | ✅ | ✅ |
| Per-button colours (Quit grey, Delete red) | ✅ | ❌ (uniform) | ✅ |
| **Mouse hover feedback on buttons**| ✅ | ❌ | ✅ |
| **Mouse press / release feedback** | ✅ | ⚠ partial | ✅ |
| Keyboard focus ring (Tab/arrows)   | ✅ | ✅ | ✅ |
| Live session refresh every 2 s     | ✅ | ✅ | ✅ |
| Status line at the bottom          | toast | status line | status line (Info/Error) |
| Wheel scroll moves selection       | ✅ | ✅ | ✅ |

## Build

Requires a stable Rust toolchain (1.78+).

```bash
cd rust
cargo build --release            # debug build is also fine for hacking
cargo test                       # 32 unit tests across models/tmux/conf_setup/state/app
```

The release binary lands at `target/release/tu` (or
`target/<triple>/release/tu` when you pass `--target`).

### Cross-compile (macOS → Linux)

We cross-compile to Linux via [`cargo-zigbuild`](https://github.com/rust-cross/cargo-zigbuild),
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
cargo zigbuild --release --target x86_64-unknown-linux-musl   # fully static
cargo zigbuild --release --target aarch64-unknown-linux-musl  # fully static
```

The `musl` builds are statically linked and run on essentially any
glibc-or-musl Linux distro; the `gnu` builds are smaller but require
glibc ≥ 2.17 (CentOS 7 era).

## Release artefacts (v0.10.0)

| Target                          | Binary size | Notes                              |
|---------------------------------|-------------|------------------------------------|
| `aarch64-apple-darwin`          | ~1.3 MB     | macOS, Apple Silicon (M1/M2/M3/M4) |
| `x86_64-unknown-linux-gnu`      | ~1.9 MB     | Linux x86_64, dynamic glibc        |
| `aarch64-unknown-linux-gnu`     | ~1.6 MB     | Linux ARM64, dynamic glibc         |
| `x86_64-unknown-linux-musl`     | ~1.7 MB     | Linux x86_64, fully static         |
| `aarch64-unknown-linux-musl`    | ~1.5 MB     | Linux ARM64, fully static          |

The corresponding tarballs and SHA256 checksums live under `rust/dist/`
once you build, and are attached to the GitHub release.

Install from a tarball:

```bash
tar -xzf tu-v0.10.0-<triple>.tar.gz
chmod +x tu
mv tu /usr/local/bin/tu
```

## Mouse: what works

Ratatui leaves event handling to us, and crossterm exposes the full
mouse event stream — including `MouseEventKind::Moved` — so the port
implements:

* **Hover** — buttons brighten when the mouse passes over them; list
  rows under the cursor get a subtle background tint.
* **Press / Release** — pressing the mouse down latches a button into
  its "pressed" colour. Releasing on the same button fires the action;
  releasing off the button (cancel) just clears the latch.
* **Click-to-focus** — clicking a button moves keyboard focus there
  too, so Tab/arrow follow-ups feel coherent.
* **Wheel** — scrolling moves the selection up/down in the session
  list regardless of where the mouse is.

If you don't see hover effects, your terminal probably isn't reporting
mouse-motion events (xterm 1003 mode). Modern terminals — iTerm2,
Alacritty, kitty, recent Apple Terminal, recent gnome-terminal — all
do. Inside tmux, our `~/.tmux.conf` baseline (`set -g mouse on`) makes
sure tmux forwards them, which is why the prompt offers to add it on
first launch.

## Running

Same as the Python build — type `tu` from any shell:

```
$ tu          # outside tmux: pick or create a session, then exec into tmux
$ tu          # inside tmux : pick a session (switch-client) or detach (d)
```

The first launch on a fresh `~/.tmux.conf` will prompt to append:

```
set -g mouse on
set -g history-limit 10000000
```

…and apply them to the running tmux server.
