# filesync

A terminal UI file manager for copying files over SSH. Browse local and remote directories side-by-side, select files, and transfer them in either direction with a live progress bar.

## Usage

```
filesync user@hostname
filesync user@192.168.1.100
```

You will be prompted for a password. No SSH key or config file needed.

## Keys

| Key | Action |
|-----|--------|
| `Tab` | Switch panel |
| `↑` / `↓` / `j` / `k` | Move cursor |
| `Enter` / `→` / `l` | Enter directory |
| `←` / `Backspace` / `h` | Go to parent |
| `Space` | Select / deselect item |
| `c` | Copy selected item(s) to the other panel |
| `d` / `Delete` | Delete selected item(s) |
| `H` | Toggle hidden files |
| `r` | Refresh current panel |
| `Esc` | Cancel in-progress transfer |
| `q` | Quit |

Select multiple items with `Space`, then press `c` to copy them all at once. If nothing is selected, the item under the cursor is used.

## Build

### Linux x86_64 (static binary, runs on any Linux)

Requires [podman](https://podman.io/). Builds inside an Alpine container using musl — no system dependencies needed.

```bash
make
# → dist/filesync-linux-x86_64
```

To use Docker instead, replace `podman` with `docker` in the Makefile.

### macOS arm64 (Apple Silicon)

Requires Rust and the system libraries (`openssl`, `libssh2`). Install them with Homebrew if needed:

```bash
brew install openssl libssh2
```

Then build natively:

```bash
make native
# → dist/filesync-native
```

### From source (any platform)

```bash
cargo build --release
# → target/release/filesync
```

## Requirements

- Linux or macOS (Windows is not supported)
- SSH server with password authentication enabled
