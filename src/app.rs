use fs2::FileExt;
use regex::Regex;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use crate::platform;

const CUSTOM_SOUND_FILE_NAME: &str = "yamete-kudasai-custom-sound.mp3";

pub const EVENT_FILE_NAME: &str = "terminal-errors.log";
pub const SOUND_FILE_NAME: &str = "yamete-kudasai-sound.mp3";
const LOCK_FILE_NAME: &str = "agent.lock";
pub const MARKER_START: &str = "# >>> YAMETE_KUDASAI >>>";
pub const MARKER_END: &str = "# <<< YAMETE_KUDASAI <<<";
const ERROR_PATTERN: &str =
    r"(?i)\b(error|exception|traceback|failed|panic|fatal|command not found)\b";
const COOLDOWN_MS: u64 = 2000;
const SOUND_BYTES: &[u8] = include_bytes!("../yamete-kudasai-sound.mp3");

pub fn run() -> i32 {
    let args: Vec<String> = env::args().skip(1).collect();
    let cmd = args.first().map(|v| v.as_str());
    let result = match cmd {
        Some("--install") => {
            let sound_url = parse_sound_arg(&args);
            install_and_start(sound_url.as_deref())
        }
        Some("--uninstall") => uninstall(),
        Some("--agent") => run_agent_loop(),
        Some("--status") => status(),
        Some("--self-test") => self_test(),
        _ => {
            print_usage();
            return 0;
        }
    };

    match result {
        Ok(message) => {
            println!("{message}");
            0
        }
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

fn print_usage() {
    let exe = env::args()
        .next()
        .unwrap_or_else(|| String::from("yamete-kudasai-system"));
    println!("Usage: {exe} <command> [options]");
    println!();
    println!("Commands:");
    println!(
        "  --install              Install agent, configure startup, and start background watcher"
    );
    println!("  --install --sound URL  Install with a custom sound file from a URL");
    println!("  --uninstall            Remove startup config and shell hooks");
    println!("  --status               Show current installation status");
    println!("  --self-test            Trigger a test sound to verify audio playback");
}

fn parse_sound_arg(args: &[String]) -> Option<String> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--sound" {
            return iter.next().cloned();
        }
    }
    None
}

fn install_and_start(sound_url: Option<&str>) -> Result<String, String> {
    let install_dir = platform::install_dir()?;
    fs::create_dir_all(&install_dir).map_err(|err| {
        format!(
            "Failed to create install dir '{}': {err}",
            install_dir.display()
        )
    })?;

    let current_exe =
        env::current_exe().map_err(|err| format!("Failed to get current exe path: {err}"))?;
    let installed_exe = install_dir.join(platform::installed_exe_name());
    copy_if_needed(&current_exe, &installed_exe)?;

    // Handle sound: custom URL takes priority, otherwise use built-in.
    let sound_msg = if let Some(url) = sound_url {
        let custom_path = install_dir.join(CUSTOM_SOUND_FILE_NAME);
        download_sound(url, &custom_path)?;
        // Also write default as fallback.
        let default_path = install_dir.join(SOUND_FILE_NAME);
        write_sound_file(&default_path)?;
        format!("Custom sound: {}", custom_path.display())
    } else {
        let sound_path = install_dir.join(SOUND_FILE_NAME);
        write_sound_file(&sound_path)?;
        String::from("Sound: built-in default")
    };

    let event_file = install_dir.join(EVENT_FILE_NAME);
    ensure_file_exists(&event_file)?;

    platform::configure_startup(&installed_exe)?;
    install_shell_hooks(&event_file)?;
    platform::start_agent(&installed_exe)?;

    Ok(format!(
        "Installed successfully.\nInstall dir: {}\n{}\nAgent startup: enabled\nError log: {}",
        install_dir.display(),
        sound_msg,
        event_file.display()
    ))
}

fn uninstall() -> Result<String, String> {
    platform::remove_startup()?;
    remove_shell_hooks()?;
    Ok(String::from(
        "Uninstall steps completed (startup + shell hooks removed).",
    ))
}

fn status() -> Result<String, String> {
    let install_dir = platform::install_dir()?;
    let exe = install_dir.join(platform::installed_exe_name());
    let sound = install_dir.join(SOUND_FILE_NAME);
    let event_file = install_dir.join(EVENT_FILE_NAME);

    let startup_line = platform::startup_status();

    let mut profile_parts = platform::platform_profile_hooks_state();
    // Add bash + zsh profile state (shared across all platforms).
    if let Some(home) = home_dir() {
        let bash = home.join(".bashrc");
        let zshrc = home.join(".zshrc");
        profile_parts.push(format!("bash={}", has_marker(&bash)));
        profile_parts.push(format!("zsh={}", has_marker(&zshrc)));
    } else {
        profile_parts.push(String::from("bash=unknown"));
        profile_parts.push(String::from("zsh=unknown"));
    }
    let profile_state = profile_parts.join(",");

    let log_size = if event_file.exists() {
        fs::metadata(&event_file).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };

    Ok(format!(
            "status: ok\ninstall_dir: {}\ninstalled_exe_exists: {}\nsound_exists: {}\nevent_file_exists: {}\nevent_file_size: {}\n{}\nprofile_hooks: {}",
            install_dir.display(),
            exe.exists(),
            sound.exists(),
            event_file.exists(),
            log_size,
            startup_line,
            profile_state
        ))
}

fn self_test() -> Result<String, String> {
    let install_dir = platform::install_dir()?;
    let event_file = install_dir.join(EVENT_FILE_NAME);
    let sound = resolve_sound_path(&install_dir);
    ensure_file_exists(&event_file)?;

    let line = format!("{}|self-test|1|manual self test", chrono_like_timestamp());
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&event_file)
        .and_then(|mut f| std::io::Write::write_all(&mut f, format!("{line}\n").as_bytes()))
        .map_err(|err| format!("Failed to append self-test event: {err}"))?;

    // Also play directly so user can validate audio path independent of hook timing.
    yamete_kudasai_player::play_file(&sound, 1.0)
        .map_err(|err| format!("Self-test audio playback failed: {err}"))?;

    Ok(format!(
        "self-test: event appended and direct audio playback executed.\nSound: {}\nlog: {}",
        sound.display(),
        event_file.display()
    ))
}

fn run_agent_loop() -> Result<String, String> {
    let install_dir = platform::install_dir()?;
    fs::create_dir_all(&install_dir).map_err(|err| {
        format!(
            "Failed to create install dir '{}': {err}",
            install_dir.display()
        )
    })?;

    let event_file = install_dir.join(EVENT_FILE_NAME);
    ensure_file_exists(&event_file)?;
    let sound_path = resolve_sound_path(&install_dir);
    // Ensure the default sound exists as fallback.
    let default_sound = install_dir.join(SOUND_FILE_NAME);
    if !default_sound.exists() {
        write_sound_file(&default_sound)?;
    }

    let lock_file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(install_dir.join(LOCK_FILE_NAME))
        .map_err(|err| format!("Failed to open lock file: {err}"))?;

    if lock_file.try_lock_exclusive().is_err() {
        return Ok(String::from("Agent already running."));
    }

    let matcher = Regex::new(ERROR_PATTERN).map_err(|err| format!("Invalid error regex: {err}"))?;
    let mut cursor = fs::metadata(&event_file)
        .map_err(|err| format!("Failed to stat event file: {err}"))?
        .len();
    let mut last_played = Instant::now() - Duration::from_millis(COOLDOWN_MS);

    loop {
        let file_len = fs::metadata(&event_file)
            .map_err(|err| format!("Failed to stat event file: {err}"))?
            .len();
        if file_len < cursor {
            cursor = 0;
        }

        if file_len > cursor {
            let lines = read_new_lines(&event_file, cursor)?;
            cursor = file_len;

            for line in lines {
                if !is_error_event(&line, &matcher) {
                    continue;
                }

                if last_played.elapsed() < Duration::from_millis(COOLDOWN_MS) {
                    continue;
                }

                last_played = Instant::now();
                if let Err(error) = yamete_kudasai_player::play_file(&sound_path, 1.0) {
                    eprintln!("Playback failed: {error}");
                }
            }
        }

        thread::sleep(Duration::from_millis(250));
    }
}

/// Prefer custom sound if it exists, otherwise use default.
fn resolve_sound_path(install_dir: &Path) -> PathBuf {
    let custom = install_dir.join(CUSTOM_SOUND_FILE_NAME);
    if custom.exists() {
        custom
    } else {
        install_dir.join(SOUND_FILE_NAME)
    }
}

fn download_sound(url: &str, dest: &Path) -> Result<(), String> {
    println!("Downloading custom sound from: {url}");
    let response = ureq::get(url)
        .call()
        .map_err(|err| format!("Failed to download sound from '{url}': {err}"))?;

    let mut bytes = Vec::new();
    response
        .into_body()
        .as_reader()
        .read_to_end(&mut bytes)
        .map_err(|err| format!("Failed to read sound data: {err}"))?;

    if bytes.is_empty() {
        return Err(format!("Downloaded file from '{url}' is empty."));
    }

    fs::write(dest, &bytes)
        .map_err(|err| format!("Failed to save sound to '{}': {err}", dest.display()))?;
    println!(
        "Custom sound saved to: {} ({} bytes)",
        dest.display(),
        bytes.len()
    );
    Ok(())
}

fn install_shell_hooks(event_file: &Path) -> Result<(), String> {
    platform::install_platform_shell_hooks(event_file)?;
    install_bash_hook(event_file)?;
    install_zsh_hook(event_file)?;
    Ok(())
}

fn remove_shell_hooks() -> Result<(), String> {
    platform::remove_platform_shell_hooks()?;

    if let Some(home) = home_dir() {
        let bashrc = home.join(".bashrc");
        let zshrc = home.join(".zshrc");
        remove_marked_block(&bashrc, MARKER_START, MARKER_END)?;
        remove_marked_block(&zshrc, MARKER_START, MARKER_END)?;
    }

    Ok(())
}

fn install_bash_hook(event_file: &Path) -> Result<(), String> {
    let Some(home) = home_dir() else {
        return Ok(());
    };
    let bashrc = home.join(".bashrc");

    let event_file_posix = event_file.to_string_lossy().replace('\\', "/");
    let escaped = bash_single_quote(&event_file_posix);
    let block = format!(
            "{start}\nexport YAMETE_KUDASAI_EVENT_FILE='{path}'\n__yamete_kudasai_precmd() {{\n  local yk_code=$?\n  if [ \"$yk_code\" -ne 0 ]; then\n    local yk_cmd\n    yk_cmd=\"$(history 1 2>/dev/null | sed 's/^ *[0-9]\\+ *//')\"\n    printf '%s|bash|%s|%s\\n' \"$(date +%Y-%m-%dT%H:%M:%S%z)\" \"$yk_code\" \"$yk_cmd\" >> \"$YAMETE_KUDASAI_EVENT_FILE\" 2>/dev/null\n  fi\n}}\ncase \";$PROMPT_COMMAND;\" in\n  *\";__yamete_kudasai_precmd;\"*) ;;\n  *) PROMPT_COMMAND=\"__yamete_kudasai_precmd${{PROMPT_COMMAND:+;$PROMPT_COMMAND}}\" ;;\nesac\n{end}\n",
            start = MARKER_START,
            end = MARKER_END,
            path = escaped
        );

    upsert_marked_block(&bashrc, MARKER_START, MARKER_END, &block)
}

fn install_zsh_hook(event_file: &Path) -> Result<(), String> {
    let Some(home) = home_dir() else {
        return Ok(());
    };
    let zshrc = home.join(".zshrc");

    let event_file_posix = event_file.to_string_lossy().replace('\\', "/");
    let block = format!(
            "{start}\nexport YAMETE_KUDASAI_EVENT_FILE='{path}'\n__yamete_kudasai_precmd() {{\n  local yk_code=$?\n  if [[ $yk_code -ne 0 ]]; then\n    local yk_cmd=\"${{history[$((HISTCMD-1))]}}\"\n    printf '%s|zsh|%s|%s\\n' \"$(date +%Y-%m-%dT%H:%M:%S%z)\" \"$yk_code\" \"$yk_cmd\" >> \"$YAMETE_KUDASAI_EVENT_FILE\" 2>/dev/null\n  fi\n}}\nautoload -Uz add-zsh-hook 2>/dev/null\nadd-zsh-hook precmd __yamete_kudasai_precmd\n{end}\n",
            start = MARKER_START,
            end = MARKER_END,
            path = event_file_posix
        );

    upsert_marked_block(&zshrc, MARKER_START, MARKER_END, &block)
}

// --- Public helpers used by platform modules ---

pub fn has_marker(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    match fs::read_to_string(path) {
        Ok(content) => content.contains(MARKER_START) && content.contains(MARKER_END),
        Err(_) => false,
    }
}

pub fn ps_single_quote(value: &str) -> String {
    value.replace('\'', "''")
}

pub fn upsert_marked_block(
    path: &Path,
    marker_start: &str,
    marker_end: &str,
    block: &str,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create '{}': {err}", parent.display()))?;
    }

    let mut content = if path.exists() {
        fs::read_to_string(path)
            .map_err(|err| format!("Failed to read '{}': {err}", path.display()))?
    } else {
        String::new()
    };

    remove_marked_ranges(&mut content, marker_start, marker_end);
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(block);

    fs::write(path, content).map_err(|err| format!("Failed to write '{}': {err}", path.display()))
}

pub fn remove_marked_block(
    path: &Path,
    marker_start: &str,
    marker_end: &str,
) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    let mut content = fs::read_to_string(path)
        .map_err(|err| format!("Failed to read '{}': {err}", path.display()))?;
    remove_marked_ranges(&mut content, marker_start, marker_end);
    fs::write(path, content).map_err(|err| format!("Failed to write '{}': {err}", path.display()))
}

// --- Private helpers ---

fn remove_marked_ranges(content: &mut String, marker_start: &str, marker_end: &str) {
    loop {
        let Some(start) = content.find(marker_start) else {
            break;
        };
        let Some(end_relative) = content[start..].find(marker_end) else {
            content.truncate(start);
            break;
        };
        let end = start + end_relative + marker_end.len();
        let mut remove_end = end;
        while remove_end < content.len() && content.as_bytes()[remove_end] == b'\n' {
            remove_end += 1;
        }
        content.replace_range(start..remove_end, "");
    }
}

fn chrono_like_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs();
    format!("unix-{now}")
}

fn read_new_lines(path: &Path, start_offset: u64) -> Result<Vec<String>, String> {
    let mut file =
        File::open(path).map_err(|err| format!("Failed to open '{}': {err}", path.display()))?;
    file.seek(SeekFrom::Start(start_offset))
        .map_err(|err| format!("Failed to seek '{}': {err}", path.display()))?;

    let mut buffer = String::new();
    file.read_to_string(&mut buffer)
        .map_err(|err| format!("Failed to read '{}': {err}", path.display()))?;
    Ok(buffer.lines().map(|line| line.to_string()).collect())
}

fn is_error_event(line: &str, matcher: &Regex) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }

    let mut parts = trimmed.splitn(4, '|');
    let _ts = parts.next();
    let _shell = parts.next();
    let code = parts.next();
    if let Some(code) = code {
        if let Ok(parsed) = code.trim().parse::<i32>() {
            if parsed != 0 {
                return true;
            }
        }
    }

    matcher.is_match(trimmed)
}

fn bash_single_quote(value: &str) -> String {
    value.replace('\'', "'\\''")
}

fn ensure_file_exists(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create '{}': {err}", parent.display()))?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| format!("Failed to create '{}': {err}", path.display()))?;
    Ok(())
}

fn write_sound_file(path: &Path) -> Result<(), String> {
    let should_write = if path.exists() {
        fs::metadata(path)
            .map(|meta| meta.len() != SOUND_BYTES.len() as u64)
            .unwrap_or(true)
    } else {
        true
    };

    if should_write {
        fs::write(path, SOUND_BYTES)
            .map_err(|err| format!("Failed to write sound file '{}': {err}", path.display()))?;
    }
    Ok(())
}

fn copy_if_needed(from: &Path, to: &Path) -> Result<(), String> {
    if same_path(from, to) {
        return Ok(());
    }

    match fs::copy(from, to) {
        Ok(_) => {}
        Err(err) => {
            if to.exists() {
                // If the installed binary is currently running, keep it and proceed.
                return Ok(());
            }
            return Err(format!(
                "Failed to copy '{}' to '{}': {err}",
                from.display(),
                to.display()
            ));
        }
    }
    Ok(())
}

fn same_path(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }

    let canon_a = a.canonicalize();
    let canon_b = b.canonicalize();
    match (canon_a, canon_b) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn home_dir() -> Option<PathBuf> {
    dirs::home_dir().or_else(|| env::var("HOME").ok().map(PathBuf::from))
}
