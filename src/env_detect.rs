use std::process::Command;

use serde::Serialize;
use sysinfo::{System, SystemExt};
use which::which;

#[derive(Debug, Clone, Serialize)]
pub struct Environment {
    pub os: String,
    pub os_version: String,
    pub cpu_arch: String,
    pub ram_gb: u64,
    pub pkg_managers: Vec<String>,
}

pub fn detect_environment() -> anyhow::Result<Environment> {
    let mut system = System::new_all();
    system.refresh_all();

    let os = normalize_os(std::env::consts::OS);
    let os_version = system
        .long_os_version()
        .or_else(|| system.os_version())
        .unwrap_or_else(|| "unknown".to_string());
    let cpu_arch = normalize_arch(std::env::consts::ARCH);
    let ram_gb = system.total_memory() / 1_048_576; // KiB to GiB
    let pkg_managers = detect_package_managers(&os);

    Ok(Environment {
        os,
        os_version,
        cpu_arch,
        ram_gb,
        pkg_managers,
    })
}

fn normalize_os(raw: &str) -> String {
    match raw {
        "macos" => "macos".to_string(),
        "windows" => "windows".to_string(),
        other => other.to_lowercase(),
    }
}

fn normalize_arch(raw: &str) -> String {
    match raw {
        "x86_64" => "x64".to_string(),
        "aarch64" => "arm64".to_string(),
        other => other.to_lowercase(),
    }
}

fn detect_package_managers(os: &str) -> Vec<String> {
    let mut managers = Vec::new();

    match os {
        "macos" => {
            if has_command("brew") {
                managers.push("brew".to_string());
            }
        }
        "windows" => {
            if has_command("winget") {
                managers.push("winget".to_string());
            }
            if has_command("choco") {
                managers.push("choco".to_string());
            }
            if has_command("scoop") {
                managers.push("scoop".to_string());
            }
        }
        _ => {}
    }

    managers
}

fn has_command(cmd: &str) -> bool {
    which(cmd).is_ok()
        || Command::new(cmd)
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
}
