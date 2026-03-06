# Yamete Kudasai

Cross-platform terminal error listener and sound agent. Monitors your shell for errors and plays a sound effect when one occurs.

Supported platforms: **Windows**, **Linux**, **macOS**.

## Build

```bash
cargo build --release
```

The release binary will be at `target/release/yamete-kudasai-system` (or `.exe` on Windows).

## Usage

Install and start the background agent with a sound URL (required):

```bash
yamete-kudasai-system --install --sound https://example.com/your-sound.mp3
```

| Command | Description |
| --- | --- |
| `--install --sound <URL>` | Install with a sound file from a URL (required) |
| `--uninstall` | Remove startup config and shell hooks |
| `--status` | Show installation status and sound path |
| `--self-test` | Trigger a test sound to verify audio playback |

> **Note:** A `--sound <URL>` pointing to an MP3 file is **required** when installing.
> Browse the [Trending Sounds](https://devfemibadmus.github.io/Yamete-Kudasai/#sounds) section on the website for ready-to-use URLs.

## How It Works

1. **Install** — copies itself to a platform-specific data directory, registers auto-start, and injects shell hooks into PowerShell, Bash, and Zsh.
2. **Agent** — runs in the background, watching a log file for error events from your shell.
3. **Shell hooks** — PowerShell (Windows), Bash, and Zsh hooks append to the log when a command exits with a non-zero code.
4. **Sound** — plays your chosen MP3 when an error is detected, with a 2-second cooldown to avoid spam.

## Project Structure

```text
src/
├── main.rs            # Entry point
├── app.rs             # Shared cross-platform logic
├── lib.rs             # Audio player library (rodio)
└── platform/
    ├── mod.rs         # Cfg-gated platform facade
    ├── windows.rs     # Windows: registry, LOCALAPPDATA, PowerShell hooks
    └── unix.rs        # Linux/macOS: XDG autostart, LaunchAgent, Bash/Zsh
```

## Status Output

```text
status: ok
install_dir: /path/to/install
installed_exe_exists: true
sound_path: /path/to/yamete-kudasai-sound.mp3
event_file_exists: true
event_file_size: 1234 bytes
startup: /path/to/exe --agent
profile_hooks: windows_powershell=true,pwsh=true,bash=true,zsh=true
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
