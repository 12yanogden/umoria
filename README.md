# umoria

Installable `umoria` command for macOS and Linux. This repo contains only the
Rust launcher — the C++ game is downloaded at build time from a pinned upstream
release and embedded into the binary.

## Install

Via [bin](https://github.com/12yanogden/bin):

```sh
bin   # select umoria
```

Or from GitHub releases:

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/12yanogden/umoria-rust/releases/latest/download/umoria-installer.sh | sh
```

## Build locally

Requires Rust, cmake, make, ncurses dev headers, and curl.

```sh
cargo build --release
```

The game source pin lives in `game-source.toml`. Override the checkout path for
local game development:

```sh
UMORIA_GAME_SRC=/path/to/umoria-game cargo build
```

## Runtime data

On first run the launcher extracts the embedded game bundle into a per-user data
directory:

- macOS: `~/Library/Application Support/umoria`
- Linux: `~/.local/share/umoria`

Override with `UMORIA_DATA_DIR`.

Saves, archives, and scores persist across launcher upgrades.

## Game source

The bundled game is built from the tag in `game-source.toml`, currently
[dungeons-of-moria/umoria](https://github.com/dungeons-of-moria/umoria). Bump
the tag there when you want to ship a new game version.
