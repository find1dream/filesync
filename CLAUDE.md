# filesync — Claude context

## What this is

A Rust TUI SSH file manager. Single binary, no config files. Users run `filesync user@host`, enter a password, and get a dual-pane file browser.

## Build

```bash
make          # static Linux x86_64 binary via Docker → dist/filesync-linux-x86_64
make native   # cargo build --release for current platform → dist/filesync-native
```

Requires podman. The Dockerfile uses `rust:alpine` + musl for a fully static Linux binary.

## Architecture

| File | Role |
|------|------|
| `src/main.rs` | CLI parsing, terminal setup/teardown, event loop |
| `src/app.rs` | All app state (`App` struct), cursor/scroll logic, local directory listing |
| `src/ssh.rs` | SSH connection (`SshClient`), remote directory listing, delete operations |
| `src/file_ops.rs` | Upload/download in background threads, progress tracking, delete helpers |
| `src/events.rs` | Keyboard event handling, transfer tick, confirm/cancel logic |
| `src/ui.rs` | All ratatui rendering (panels, progress bar, help bar, popups) |
| `src/util.rs` | Shared helpers: `sort_dir_first`, `selected_entries` |

## Key design decisions

- **SFTP for all transfers**: both uploads and downloads use `Arc<Mutex<ssh2::Sftp>>`. SCP was removed because it blocked the main thread and conflicted with the SFTP subsystem.
- **Threaded transfers**: `start_upload` and `start_download` spawn a background thread and report progress via `Arc<Mutex<ProgressState>>`. The main thread polls every 50 ms.
- **Cancellation**: `TransferJob` holds an `Arc<AtomicBool>` cancel flag checked between every 64 KB chunk. Press `Esc` to cancel.
- **sftp lock discipline**: the sftp mutex is held for the full duration of each file transfer (same as the upload pattern) to avoid concurrent subsystem use.
- **Responsive help bar**: `needed_help_height` measures total item width and allocates 1 or 2 content rows; items wrap at the natural break point.
- **No Windows support**: `compile_error!` in `main.rs` prevents compilation on Windows.

## What NOT to do

- Do not add `libssh2-sys/vendored` — that feature does not exist.
- Do not use `session.scp_recv()` for downloads — it blocks the main thread.
- Do not move `ssh2::Session` to a background thread — use the SFTP subsystem instead.
