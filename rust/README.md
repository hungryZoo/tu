# tu (Rust port)

A Rust port of the [`tu`](../README.md) tmux session menu, living on the
`rust-port` branch. Same UX, same `~/.tmux.conf` baseline checks, same
`execvp` hand-off after the TUI exits — just shipped as a single
statically-friendly binary instead of a Python package.

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
│   ├── modals.rs       # ConfSetupModal + ConfirmDeleteModal helpers
│   └── app.rs          # Cursive event loop + 5-button main view
└── dist/               # build artefacts (gitignored)
```

The module split tracks the Python package one-to-one — if you've read
the Textual implementation, you can read this one.

## Differences vs. the Python build

| Behaviour                          | Python (Textual) | Rust (Cursive) |
|------------------------------------|------------------|----------------|
| Session list / 5-button layout     | ✅                | ✅              |
| `~/.tmux.conf` baseline prompt     | ✅                | ✅              |
| `attach-session` via `execvp`      | ✅                | ✅              |
| `del` key opens confirm modal      | ✅                | ✅              |
| Per-button colours (Quit grey, Delete red) | ✅      | ⚠️  Cursive renders all buttons with the theme highlight; labels still read `Quit (q)` / `Delete (del)` |
| Live session refresh every 2 s     | ✅ (Textual reactive) | ✅ (background thread + `cb_sink`) |
| Toast notifications                | ✅                | Replaced by a single status line at the bottom |

The colour gap is a Cursive limitation, not a porting oversight — its
`Button` widget pulls its style from the global palette, so per-button
colours would need a custom widget. Layout, labels, behaviour, and
keybindings all match.

## Build

Requires a stable Rust toolchain (1.78+).

```bash
cd rust
cargo build --release            # debug build is also fine for hacking
cargo test                       # 13 unit tests (models / tmux / conf_setup)
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

## Release artefacts (v0.9.0)

| Target                          | Size (binary) | Notes                              |
|---------------------------------|---------------|------------------------------------|
| `aarch64-apple-darwin`          | ~1.4 MB       | macOS, Apple Silicon (M1/M2/M3/M4) |
| `x86_64-unknown-linux-gnu`      | ~2.0 MB       | Linux x86_64, dynamic glibc        |
| `aarch64-unknown-linux-gnu`     | ~1.8 MB       | Linux ARM64, dynamic glibc         |
| `x86_64-unknown-linux-musl`     | ~1.8 MB       | Linux x86_64, fully static         |
| `aarch64-unknown-linux-musl`    | ~1.6 MB       | Linux ARM64, fully static          |

The corresponding tarballs and SHA256 checksums live under `rust/dist/`
once you build, and are attached to the GitHub release.

Install from a tarball:

```bash
tar -xzf tu-v0.9.0-<triple>.tar.gz
chmod +x tu
mv tu /usr/local/bin/tu
```

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
