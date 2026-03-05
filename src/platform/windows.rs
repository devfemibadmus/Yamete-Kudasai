use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use crate::app::{
    has_marker, ps_single_quote, remove_marked_block, upsert_marked_block, MARKER_END,
    MARKER_START,
};

const INSTALL_DIR_NAME: &str = "YameteKudasai";
const STARTUP_VALUE_NAME: &str = "YameteKudasai";
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn installed_exe_name() -> &'static str {
    "yamete-kudasai-system.exe"
}

pub fn install_dir() -> Result<PathBuf, String> {
    let local_app_data = env::var("LOCALAPPDATA")
        .map_err(|_| String::from("LOCALAPPDATA environment variable is not set."))?;
    Ok(PathBuf::from(local_app_data).join(INSTALL_DIR_NAME))
}

pub fn configure_startup(installed_exe: &Path) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu
        .open_subkey_with_flags(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
            winreg::enums::KEY_SET_VALUE,
        )
        .map_err(|err| format!("Failed to open startup registry key: {err}"))?;
    let value = format!("\"{}\" --agent", installed_exe.display());
    run_key
        .set_value(STARTUP_VALUE_NAME, &value)
        .map_err(|err| format!("Failed to write startup entry: {err}"))
}

pub fn remove_startup() -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu
        .open_subkey_with_flags(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
            winreg::enums::KEY_SET_VALUE,
        )
        .map_err(|err| format!("Failed to open startup registry key: {err}"))?;
    let _ = run_key.delete_value(STARTUP_VALUE_NAME);
    Ok(())
}

pub fn startup_status() -> String {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = match hkcu.open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run") {
        Ok(key) => key,
        Err(err) => return format!("startup: error ({err})"),
    };
    let startup: Result<String, _> = run_key.get_value(STARTUP_VALUE_NAME);
    match startup {
        Ok(value) => format!("startup: {value}"),
        Err(_) => String::from("startup: (missing)"),
    }
}

pub fn start_agent(installed_exe: &Path) -> Result<(), String> {
    let mut command = Command::new(installed_exe);
    command
        .arg("--agent")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);

    command
        .spawn()
        .map_err(|err| format!("Failed to start agent: {err}"))?;
    Ok(())
}

pub fn install_platform_shell_hooks(event_file: &Path) -> Result<(), String> {
    install_powershell_hooks(event_file)
}

pub fn remove_platform_shell_hooks() -> Result<(), String> {
    if let Some(documents) = dirs::document_dir() {
        let ps_profiles = [
            documents
                .join("WindowsPowerShell")
                .join("Microsoft.PowerShell_profile.ps1"),
            documents
                .join("PowerShell")
                .join("Microsoft.PowerShell_profile.ps1"),
        ];
        for profile in ps_profiles {
            remove_marked_block(&profile, MARKER_START, MARKER_END)?;
        }
    }
    Ok(())
}

pub fn platform_profile_hooks_state() -> Vec<String> {
    let mut states = Vec::new();
    if let Some(documents) = dirs::document_dir() {
        let ps1 = documents
            .join("WindowsPowerShell")
            .join("Microsoft.PowerShell_profile.ps1");
        let ps2 = documents
            .join("PowerShell")
            .join("Microsoft.PowerShell_profile.ps1");
        states.push(format!("windows_powershell={}", has_marker(&ps1)));
        states.push(format!("pwsh={}", has_marker(&ps2)));
    } else {
        states.push(String::from("windows_powershell=unknown"));
        states.push(String::from("pwsh=unknown"));
    }
    states
}

fn install_powershell_hooks(event_file: &Path) -> Result<(), String> {
    let Some(documents) = dirs::document_dir() else {
        return Err(String::from(
            "Could not resolve Documents directory for PowerShell profiles.",
        ));
    };

    let profiles = [
        documents
            .join("WindowsPowerShell")
            .join("Microsoft.PowerShell_profile.ps1"),
        documents
            .join("PowerShell")
            .join("Microsoft.PowerShell_profile.ps1"),
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
