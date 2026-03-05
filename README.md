# Yamete Kudasai (Rust Only)

This repository is now Rust-only.

## Binary

- `yamete-kudasai-system`: Windows one-click installer + background listener agent.

## Build

```bash
cargo build --release
```

Default output:
- `target/release/yamete-kudasai-system.exe`

## One-Click Install (Windows)

1. Build `yamete-kudasai-system.exe`.
2. Double-click `target/release/yamete-kudasai-system.exe`.

It auto-installs to `%LOCALAPPDATA%\YameteKudasai`, registers startup, installs shell hooks, and starts the background agent.

Optional removal:

```bash
yamete-kudasai-system.exe --uninstall
```

## Quick Run

```bash
cargo run --release
```
