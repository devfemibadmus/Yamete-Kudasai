# Yamete Kudasai

Cross-platform terminal error listener and sound agent. Monitors your shell for errors and plays a sound effect when one occurs.

Supported platforms: **Windows**, **Linux**, **macOS**.

## Build

```bash
cargo build --release
```

The release binary will be at `target/release/yamete-kudasai-system` (or `.exe` on Windows).

## Run

Install and start the background agent:

```bash
cargo run --release
```

Common commands:

| Command | Description |
|---|---|
| `yamete-kudasai-system` | Install and start agent |
| `yamete-kudasai-system --status` | Show installation status |
| `yamete-kudasai-system --self-test` | Trigger a test sound |
| `yamete-kudasai-system --uninstall` | Remove startup + shell hooks |

## How It Works

1. **Install** — copies itself to a platform-specific data directory, registers auto-start, and injects shell hooks.
2. **Agent** — runs in the background, watching a log file for error events.
3. **Shell hooks** — PowerShell (Windows) and Bash hooks append to the log when a command fails.
4. **Sound** — plays **yamete-kudasai-sound.mp3** when an error is detected.

## Project Structure

```
src/
├── main.rs            # Entry point
├── app.rs             # Shared cross-platform logic
├── lib.rs             # Audio player library
└── platform/
    ├── mod.rs         # Cfg-gated platform facade
    ├── windows.rs     # Windows: registry, LOCALAPPDATA, PowerShell
    └── unix.rs        # Linux/macOS: XDG, .desktop, LaunchAgent
```

## CI/CD

Pushing to **main** automatically:

1. Bumps the patch version from the latest git tag (e.g. `v0.1.3` → `v0.1.4`)
2. Builds release binaries for **Windows**, **Linux**, and **macOS**
3. Creates a **GitHub Release** with all binaries attached

To trigger a **minor** or **major** bump, manually create and push a tag before your commit:

```bash
git tag v1.0.0
git push origin v1.0.0
```
The next push to main will increment from that tag.
