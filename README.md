# tanaterm

A minimal terminal GUI for macOS, written in Rust — think of it as a tiny,
early-stage sibling of [Warp](https://www.warp.dev/) or
[iTerm2](https://iterm2.com/).

> 日本語版の README は [README_JA.md](./README_JA.md) を参照してください。

## Status

Very early. This is the bootstrap stage: tanaterm opens **one** window with
**one** terminal and lets you use your shell. That's it — on purpose. More
features (tabs, splits, themes, a command palette) will come later.

## Features

- Single resizable terminal window backed by a real PTY
- Choose `bash` or `zsh` via a config file
- 256-color ANSI rendering
- Login-shell support so your `~/.zprofile` / `~/.bash_profile` is loaded

## Requirements

- macOS
- A recent stable [Rust toolchain](https://rustup.rs/) (1.74+)

## Build & run

```sh
git clone https://github.com/sny-tanaka/tanaterm.git
cd tanaterm
cargo run --release
```

## Packaging (macOS .app)

`cargo run` launches a bare binary, which Finder shows with a generic
executable icon. To get a proper Finder/Dock app icon, build a `.app` bundle:

```sh
./scripts/bundle-macos.sh
```

This produces `target/release/tanaterm.app` (icon generated from
`assets/icon.png` via the built-in `sips`/`iconutil` — no extra tooling).
Double-click it in
Finder, or drag it into `/Applications`.

## Configuration

On first launch, tanaterm writes a default config file and prints its path to
stderr. It lives at:

```
~/Library/Application Support/tanaterm/config.toml
```

Example:

```toml
# Which shell to launch: "bash" or "zsh"
shell = "zsh"

# Terminal font size in points
font_size = 14.0

# Start the shell as a login shell (loads ~/.zprofile, ~/.bash_profile, ...)
login_shell = true
```

Edit the file and restart tanaterm to apply changes.

| Key           | Type            | Default | Description                                  |
| ------------- | --------------- | ------- | -------------------------------------------- |
| `shell`       | `"bash"`/`"zsh"`| `"zsh"` | Shell program to run (`/bin/bash`/`/bin/zsh`)|
| `font_size`   | float           | `14.0`  | Monospace font size in points                |
| `login_shell` | bool            | `true`  | Run the shell with `-l` (login shell)        |

## Project layout

| File             | Responsibility                                  |
| ---------------- | ----------------------------------------------- |
| `src/main.rs`    | Entry point, window setup                       |
| `src/config.rs`  | Loading/writing `config.toml`                   |
| `src/pty.rs`     | Spawning the shell on a PTY, reader thread      |
| `src/app.rs`     | egui rendering of the terminal grid + key input |
| `scripts/bundle-macos.sh` | Package the release binary as a `.app` bundle |

## License

[MIT](./LICENSE)
