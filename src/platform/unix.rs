use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const INSTALL_DIR_NAME: &str = "YameteKudasai";
const APP_ID: &str = "com.yamete-kudasai.agent";

pub fn installed_exe_name() -> &'static str {
    "yamete-kudasai-system"
}

pub fn install_dir() -> Result<PathBuf, String> {
    let base = dirs::data_local_dir()
        .ok_or_else(|| String::from("Could not determine local data directory."))?;
    Ok(base.join(INSTALL_DIR_NAME))
}

pub fn configure_startup(installed_exe: &Path) -> Result<(), String> {
    if cfg!(target_os = "macos") {
        configure_startup_macos(installed_exe)
    } else {
        configure_startup_linux(installed_exe)
    }
}

pub fn remove_startup() -> Result<(), String> {
    if cfg!(target_os = "macos") {
        let launch_agents = dirs::home_dir()
            .ok_or_else(|| String::from("Could not determine home directory."))?
            .join("Library")
            .join("LaunchAgents");
        let plist = launch_agents.join(format!("{APP_ID}.plist"));
        if plist.exists() {
            fs::remove_file(&plist)
                .map_err(|err| format!("Failed to remove LaunchAgent plist: {err}"))?;
        }
    } else {
        let autostart = dirs::config_dir()
            .ok_or_else(|| String::from("Could not determine config directory."))?
            .join("autostart");
        let desktop_file = autostart.join(format!("{APP_ID}.desktop"));
        if desktop_file.exists() {
            fs::remove_file(&desktop_file)
                .map_err(|err| format!("Failed to remove autostart desktop file: {err}"))?;
        }
    }
    Ok(())
}

pub fn startup_status() -> String {
    if cfg!(target_os = "macos") {
        let plist = dirs::home_dir()
            .map(|h| h.join("Library").join("LaunchAgents").join(format!("{APP_ID}.plist")));
        match plist {
            Some(path) if path.exists() => format!("startup: {}", path.display()),
            Some(_) => String::from("startup: (missing)"),
            None => String::from("startup: (unknown home)"),
        }
    } else {
        let desktop_file = dirs::config_dir()
            .map(|c| c.join("autostart").join(format!("{APP_ID}.desktop")));
        match desktop_file {
            Some(path) if path.exists() => format!("startup: {}", path.display()),
            Some(_) => String::from("startup: (missing)"),
            None => String::from("startup: (unknown config dir)"),
        }
    }
}

pub fn start_agent(installed_exe: &Path) -> Result<(), String> {
    let mut command = Command::new(installed_exe);
    command
        .arg("--agent")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // Use setsid to detach the child from the current session on Unix.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            command.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }

    command
        .spawn()
        .map_err(|err| format!("Failed to start agent: {err}"))?;
    Ok(())
}

pub fn install_platform_shell_hooks(_event_file: &Path) -> Result<(), String> {
    // Bash hook is handled in the shared app module.
    // No additional platform-specific shell hooks on Unix.
    Ok(())
}

pub fn remove_platform_shell_hooks() -> Result<(), String> {
    // Bash hook removal is handled in the shared app module.
    Ok(())
}

pub fn platform_profile_hooks_state() -> Vec<String> {
    // No platform-specific profile hooks on Unix (bash is shared).
    Vec::new()
}

// --- Linux autostart via XDG .desktop file ---

fn configure_startup_linux(installed_exe: &Path) -> Result<(), String> {
    let autostart = dirs::config_dir()
        .ok_or_else(|| String::from("Could not determine config directory."))?
        .join("autostart");
    fs::create_dir_all(&autostart)
        .map_err(|err| format!("Failed to create autostart dir: {err}"))?;

    let desktop_file = autostart.join(format!("{APP_ID}.desktop"));
    let content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=YameteKudasai\n\
         Exec=\"{}\" --agent\n\
         Hidden=false\n\
         NoDisplay=true\n\
         X-GNOME-Autostart-enabled=true\n\
         Comment=Terminal error sound agent\n",
        installed_exe.display()
    );
    fs::write(&desktop_file, content)
        .map_err(|err| format!("Failed to write autostart desktop file: {err}"))
}

// --- macOS autostart via LaunchAgent plist ---

fn configure_startup_macos(installed_exe: &Path) -> Result<(), String> {
    let launch_agents = dirs::home_dir()
        .ok_or_else(|| String::from("Could not determine home directory."))?
        .join("Library")
        .join("LaunchAgents");
    fs::create_dir_all(&launch_agents)
        .map_err(|err| format!("Failed to create LaunchAgents dir: {err}"))?;

    let plist = launch_agents.join(format!("{APP_ID}.plist"));
    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{app_id}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
        <string>--agent</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
</dict>
</plist>
"#,
        app_id = APP_ID,
        exe = installed_exe.display()
    );
    fs::write(&plist, content)
        .map_err(|err| format!("Failed to write LaunchAgent plist: {err}"))
}
