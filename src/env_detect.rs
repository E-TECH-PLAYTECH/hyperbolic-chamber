use std::process::Command;

use serde::Serialize;
use sysinfo::{System, SystemExt};
use which::which;

use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize)]
pub struct Environment {
    pub os: String,
    pub os_version: String,
    pub cpu_arch: String,
    pub ram_gb: u64,
    pub pkg_managers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<Fingerprint>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Fingerprint {
    pub os: String,
    pub os_version: String,
    pub cpu_arch: String,
    pub ram_gb: u64,
    pub hostname: Option<String>,
    pub extra: Option<serde_json::Value>,
    pub hash: String,
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

    let fingerprint = compute_fingerprint(&os, &os_version, &cpu_arch, ram_gb, system.host_name());

    Ok(Environment {
        os,
        os_version,
        cpu_arch,
        ram_gb,
        pkg_managers,
        fingerprint: Some(fingerprint),
    })
}

fn compute_fingerprint(
    os: &str,
    os_version: &str,
    cpu_arch: &str,
    ram_gb: u64,
    hostname: Option<String>,
) -> Fingerprint {
    let mut hasher = Sha256::new();
    hasher.update(os.as_bytes());
    hasher.update(os_version.as_bytes());
    hasher.update(cpu_arch.as_bytes());
    hasher.update(ram_gb.to_le_bytes());
    if let Some(ref host) = hostname {
        hasher.update(host.as_bytes());
    }
    let hash = format!("{:x}", hasher.finalize());

    Fingerprint {
        os: os.to_string(),
        os_version: os_version.to_string(),
        cpu_arch: cpu_arch.to_string(),
        ram_gb,
        hostname,
        extra: None,
        hash,
    }
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
