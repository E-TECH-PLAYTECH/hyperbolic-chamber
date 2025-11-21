use std::fs;
use std::path::PathBuf;

use anyhow::{Context, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum InstallStatus {
    Success,
    Failed,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InstallRecord {
    pub app_name: String,
    pub app_version: String,
    pub mode: String,
    pub os: String,
    pub cpu_arch: String,
    pub timestamp: DateTime<Utc>,
    pub status: InstallStatus,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct State {
    #[serde(default)]
    pub installs: Vec<InstallRecord>,
}

pub fn load_state() -> anyhow::Result<State> {
    let path = state_file_path()?;
    if !path.exists() {
        return Ok(State::default());
    }

    let data = fs::read_to_string(&path)
        .with_context(|| format!("reading state file at {}", path.display()))?;
    let state: State = serde_json::from_str(&data)
        .with_context(|| format!("parsing state file at {}", path.display()))?;
    Ok(state)
}

pub fn save_state(state: &State) -> anyhow::Result<()> {
    let path = state_file_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating state directory {}", parent.display()))?;
    }

    let tmp_path = path.with_extension("tmp");
    let data = serde_json::to_string_pretty(state)?;
    fs::write(&tmp_path, data)
        .with_context(|| format!("writing temp state file {}", tmp_path.display()))?;
    fs::rename(&tmp_path, &path)
        .with_context(|| format!("committing state file to {}", path.display()))?;
    Ok(())
}

pub fn add_install_record(record: InstallRecord) -> anyhow::Result<()> {
    let mut state = load_state()?;
    state.installs.push(record);
    save_state(&state)
}

fn state_file_path() -> anyhow::Result<PathBuf> {
    let base = dirs::data_dir()
        .ok_or_else(|| anyhow!("could not determine platform data directory"))?
        .join("enzyme-installer");
    Ok(base.join("state.json"))
}
