#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("yamete-kudasai-system is currently supported on Windows only.");
    std::process::exit(1);
}

#[cfg(target_os = "windows")]
mod windows_app {
    use fs2::FileExt;
    use regex::Regex;
    use std::env;
    use std::fs::{self, File, OpenOptions};
    use std::io::{Read, Seek, SeekFrom};
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    #[cfg(target_os = "windows")]
    use std::os::windows::process::CommandExt;

    const INSTALL_DIR_NAME: &str = "YameteKudasai";
    const INSTALLED_EXE_NAME: &str = "yamete-kudasai-system.exe";
    const EVENT_FILE_NAME: &str = "terminal-errors.log";
    const SOUND_FILE_NAME: &str = "yamete-kudasai-sound.mp3";
    const LOCK_FILE_NAME: &str = "agent.lock";
    const STARTUP_VALUE_NAME: &str = "YameteKudasai";
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    const MARKER_START: &str = "# >>> YAMETE_KUDASAI >>>";
    const MARKER_END: &str = "# <<< YAMETE_KUDASAI <<<";
    const ERROR_PATTERN: &str = r"(?i)\b(error|exception|traceback|failed|panic|fatal|command not found)\b";
    const COOLDOWN_MS: u64 = 2000;
    const SOUND_BYTES: &[u8] = include_bytes!("../yamete-kudasai-sound.mp3");

    pub fn run() -> i32 {
        let args: Vec<String> = env::args().skip(1).collect();
        let result = if args.first().map(|v| v.as_str()) == Some("--agent") {
            run_agent_loop()
        } else if args.first().map(|v| v.as_str()) == Some("--status") {
            status()
        } else if args.first().map(|v| v.as_str()) == Some("--self-test") {
            self_test()
        } else if args.first().map(|v| v.as_str()) == Some("--uninstall") {
            uninstall()
        } else {
            install_and_start()
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

    fn install_and_start() -> Result<String, String> {
        let install_dir = install_dir()?;
        fs::create_dir_all(&install_dir)
            .map_err(|err| format!("Failed to create install dir '{}': {err}", install_dir.display()))?;

        let current_exe = env::current_exe().map_err(|err| format!("Failed to get current exe path: {err}"))?;
        let installed_exe = install_dir.join(INSTALLED_EXE_NAME);
        copy_if_needed(&current_exe, &installed_exe)?;

        let sound_path = install_dir.join(SOUND_FILE_NAME);
        write_sound_file(&sound_path)?;

        let event_file = install_dir.join(EVENT_FILE_NAME);
        ensure_file_exists(&event_file)?;

        configure_startup(&installed_exe)?;
        install_shell_hooks(&event_file)?;
        start_agent(&installed_exe)?;

        Ok(format!(
            "Installed successfully.\nInstall dir: {}\nAgent startup: enabled\nError log: {}",
            install_dir.display(),
            event_file.display()
        ))
    }

    fn uninstall() -> Result<String, String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run_key = hkcu
            .open_subkey_with_flags("Software\\Microsoft\\Windows\\CurrentVersion\\Run", winreg::enums::KEY_SET_VALUE)
            .map_err(|err| format!("Failed to open startup registry key: {err}"))?;
        let _ = run_key.delete_value(STARTUP_VALUE_NAME);

        remove_shell_hooks()?;
        Ok(String::from("Uninstall steps completed (startup + shell hooks removed)."))
    }

    fn status() -> Result<String, String> {
        let install_dir = install_dir()?;
        let exe = install_dir.join(INSTALLED_EXE_NAME);
        let sound = install_dir.join(SOUND_FILE_NAME);
        let event_file = install_dir.join(EVENT_FILE_NAME);

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run_key = hkcu
            .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
            .map_err(|err| format!("Failed to open startup registry key: {err}"))?;
        let startup: Result<String, _> = run_key.get_value(STARTUP_VALUE_NAME);

        let startup_line = match startup {
            Ok(value) => format!("startup: {value}"),
            Err(_) => String::from("startup: (missing)"),
        };

        let profile_state = profile_hooks_state();
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
        let install_dir = install_dir()?;
        let event_file = install_dir.join(EVENT_FILE_NAME);
        let sound = install_dir.join(SOUND_FILE_NAME);
        ensure_file_exists(&event_file)?;

        let line = format!(
            "{}|self-test|1|manual self test",
            chrono_like_timestamp()
        );
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
            "self-test: event appended and direct audio playback executed.\nlog: {}",
            event_file.display()
        ))
    }

    fn run_agent_loop() -> Result<String, String> {
        let install_dir = install_dir()?;
        fs::create_dir_all(&install_dir)
            .map_err(|err| format!("Failed to create install dir '{}': {err}", install_dir.display()))?;

        let event_file = install_dir.join(EVENT_FILE_NAME);
        ensure_file_exists(&event_file)?;
        let sound_path = install_dir.join(SOUND_FILE_NAME);
        if !sound_path.exists() {
            write_sound_file(&sound_path)?;
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

    fn profile_hooks_state() -> String {
        let mut states = Vec::new();
        if let Some(documents) = dirs::document_dir() {
            let ps1 = documents.join("WindowsPowerShell").join("Microsoft.PowerShell_profile.ps1");
            let ps2 = documents.join("PowerShell").join("Microsoft.PowerShell_profile.ps1");
            states.push(format!("windows_powershell={}", has_marker(&ps1)));
            states.push(format!("pwsh={}", has_marker(&ps2)));
        } else {
            states.push(String::from("windows_powershell=unknown"));
            states.push(String::from("pwsh=unknown"));
        }
        if let Some(home) = home_dir() {
            let bash = home.join(".bashrc");
            states.push(format!("bash={}", has_marker(&bash)));
        } else {
            states.push(String::from("bash=unknown"));
        }
        states.join(",")
    }

    fn has_marker(path: &Path) -> bool {
        if !path.exists() {
            return false;
        }
        match fs::read_to_string(path) {
            Ok(content) => content.contains(MARKER_START) && content.contains(MARKER_END),
            Err(_) => false,
        }
    }

    fn chrono_like_timestamp() -> String {
        // Keep dependency-free timestamp generation with PowerShell-like format not required here.
        // UNIX seconds are sufficient for self-test marker.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_secs();
        format!("unix-{now}")
    }

    fn read_new_lines(path: &Path, start_offset: u64) -> Result<Vec<String>, String> {
        let mut file = File::open(path).map_err(|err| format!("Failed to open '{}': {err}", path.display()))?;
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

    fn install_shell_hooks(event_file: &Path) -> Result<(), String> {
        install_powershell_hooks(event_file)?;
        install_bash_hook(event_file)?;
        Ok(())
    }

    fn remove_shell_hooks() -> Result<(), String> {
        if let Some(documents) = dirs::document_dir() {
            let ps_profiles = [
                documents.join("WindowsPowerShell").join("Microsoft.PowerShell_profile.ps1"),
                documents.join("PowerShell").join("Microsoft.PowerShell_profile.ps1"),
            ];
            for profile in ps_profiles {
                remove_marked_block(&profile, MARKER_START, MARKER_END)?;
            }
        }

        if let Some(home) = home_dir() {
            let bashrc = home.join(".bashrc");
            remove_marked_block(&bashrc, MARKER_START, MARKER_END)?;
        }

        Ok(())
    }

    fn install_powershell_hooks(event_file: &Path) -> Result<(), String> {
        let Some(documents) = dirs::document_dir() else {
            return Err(String::from("Could not resolve Documents directory for PowerShell profiles."));
        };

        let profiles = [
            documents.join("WindowsPowerShell").join("Microsoft.PowerShell_profile.ps1"),
            documents.join("PowerShell").join("Microsoft.PowerShell_profile.ps1"),
        ];

        let escaped_event_file = ps_single_quote(&event_file.to_string_lossy());
        let block = format!(
            "{start}\n$env:YAMETE_KUDASAI_EVENT_FILE = '{path}'\nif (-not (Get-Variable -Scope Global -Name __yk_prompt_wrapped -ErrorAction SilentlyContinue)) {{\n  $global:__yk_prompt_wrapped = $true\n  $global:__yk_original_prompt = (Get-Command prompt -CommandType Function).ScriptBlock\n  function global:prompt {{\n    try {{\n      $yk_failed = -not $?\n      $yk_code = $global:LASTEXITCODE\n      $yk_native_failed = ($yk_code -is [int]) -and ($yk_code -ne 0)\n      if ($yk_failed -or $yk_native_failed) {{\n        if (-not ($yk_code -is [int])) {{ $yk_code = if ($yk_failed) {{ 1 }} else {{ 0 }} }}\n        $yk_cmd = (Get-History -Count 1 -ErrorAction SilentlyContinue).CommandLine\n        $yk_line = \"{{0}}|powershell|{{1}}|{{2}}\" -f (Get-Date -Format o), $yk_code, $yk_cmd\n        Add-Content -Path $env:YAMETE_KUDASAI_EVENT_FILE -Value $yk_line -Encoding UTF8\n      }}\n    }} catch {{}}\n    & $global:__yk_original_prompt\n  }}\n}}\n{end}\n",
            start = MARKER_START,
            end = MARKER_END,
            path = escaped_event_file
        );

        for profile in profiles {
            upsert_marked_block(&profile, MARKER_START, MARKER_END, &block)?;
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

    fn upsert_marked_block(path: &Path, marker_start: &str, marker_end: &str, block: &str) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("Failed to create '{}': {err}", parent.display()))?;
        }

        let mut content = if path.exists() {
            fs::read_to_string(path).map_err(|err| format!("Failed to read '{}': {err}", path.display()))?
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

    fn remove_marked_block(path: &Path, marker_start: &str, marker_end: &str) -> Result<(), String> {
        if !path.exists() {
            return Ok(());
        }

        let mut content =
            fs::read_to_string(path).map_err(|err| format!("Failed to read '{}': {err}", path.display()))?;
        remove_marked_ranges(&mut content, marker_start, marker_end);
        fs::write(path, content).map_err(|err| format!("Failed to write '{}': {err}", path.display()))
    }

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

    fn configure_startup(installed_exe: &Path) -> Result<(), String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run_key = hkcu
            .open_subkey_with_flags("Software\\Microsoft\\Windows\\CurrentVersion\\Run", winreg::enums::KEY_SET_VALUE)
            .map_err(|err| format!("Failed to open startup registry key: {err}"))?;
        let value = format!("\"{}\" --agent", installed_exe.display());
        run_key
            .set_value(STARTUP_VALUE_NAME, &value)
            .map_err(|err| format!("Failed to write startup entry: {err}"))
    }

    fn start_agent(installed_exe: &Path) -> Result<(), String> {
        let mut command = Command::new(installed_exe);
        command
            .arg("--agent")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW);

        command.spawn().map_err(|err| format!("Failed to start agent: {err}"))?;
        Ok(())
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

    fn install_dir() -> Result<PathBuf, String> {
        let local_app_data =
            env::var("LOCALAPPDATA").map_err(|_| String::from("LOCALAPPDATA environment variable is not set."))?;
        Ok(PathBuf::from(local_app_data).join(INSTALL_DIR_NAME))
    }

    fn home_dir() -> Option<PathBuf> {
        dirs::home_dir().or_else(|| env::var("USERPROFILE").ok().map(PathBuf::from))
    }

    fn ps_single_quote(value: &str) -> String {
        value.replace('\'', "''")
    }

    fn bash_single_quote(value: &str) -> String {
        value.replace('\'', "'\\''")
    }
}

#[cfg(target_os = "windows")]
fn main() {
    std::process::exit(windows_app::run());
}
