use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, anyhow};
use which::which;

use crate::manifest::{RuntimeEnv, RuntimeEnvType};
use crate::planner::InstallPlan;

#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub env: HashMap<String, String>,
    pub path_prefixes: Vec<PathBuf>,
}

impl ExecutionContext {
    pub fn new() -> Self {
        Self {
            env: HashMap::new(),
            path_prefixes: Vec::new(),
        }
    }

    pub fn merged_path(&self) -> Option<String> {
        if self.path_prefixes.is_empty() {
            return None;
        }

        let existing = std::env::var_os("PATH")
            .map(|p| p.into_string().unwrap_or_default())
            .unwrap_or_default();
        let separator = if cfg!(windows) { ';' } else { ':' };
        let mut segments: Vec<String> = self
            .path_prefixes
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        if !existing.is_empty() {
            segments.push(existing);
        }

        Some(segments.join(&separator.to_string()))
    }
}

pub fn prepare_runtime_env(plan: &InstallPlan) -> anyhow::Result<Option<ExecutionContext>> {
    let Some(runtime) = plan.runtime_env.as_ref() else {
        return Ok(None);
    };

    let mut ctx = ExecutionContext::new();
    match runtime.kind {
        RuntimeEnvType::NodeLocal => prepare_node_env(runtime, &plan.os, &mut ctx)?,
        RuntimeEnvType::PythonVenv => prepare_python_env(runtime, &plan.os, &mut ctx)?,
    }

    if let Some(path) = ctx.merged_path() {
        ctx.env.insert("PATH".to_string(), path);
    }

    Ok(Some(ctx))
}

fn prepare_node_env(
    runtime: &RuntimeEnv,
    os: &str,
    ctx: &mut ExecutionContext,
) -> anyhow::Result<()> {
    let root = resolve_root(&runtime.root)?;
    let node_root = root.join("node");
    std::fs::create_dir_all(&node_root)
        .with_context(|| format!("creating node runtime at {}", node_root.display()))?;

    let bin_dir = if os == "windows" {
        node_root.join("bin")
    } else {
        node_root.join("bin")
    };

    let local_node = bin_dir.join(if os == "windows" { "node.exe" } else { "node" });
    if local_node.exists() {
        ctx.path_prefixes.push(bin_dir);
        return Ok(());
    }

    // Fallback to global node if allowed.
    let strategy = runtime
        .node
        .as_ref()
        .and_then(|n| n.install_strategy.clone())
        .unwrap_or_else(|| "local_bundle_or_global".to_string());
    if strategy.contains("global") {
        if which("node").is_ok() {
            return Ok(());
        }
    }

    Err(anyhow!(
        "Node runtime not available locally and no compatible global installation found"
    ))
}

fn prepare_python_env(
    runtime: &RuntimeEnv,
    os: &str,
    ctx: &mut ExecutionContext,
) -> anyhow::Result<()> {
    let root = resolve_root(&runtime.root)?;
    let venv_dir = root.join("venv");
    let bin_dir = if os == "windows" {
        venv_dir.join("Scripts")
    } else {
        venv_dir.join("bin")
    };

    if !venv_exists(&bin_dir, os) {
        create_venv(&venv_dir, os)?;
    }

    ctx.env.insert(
        "VIRTUAL_ENV".to_string(),
        venv_dir.to_string_lossy().to_string(),
    );
    ctx.path_prefixes.push(bin_dir);

    Ok(())
}

fn venv_exists(bin_dir: &Path, os: &str) -> bool {
    let python_bin = if os == "windows" {
        "python.exe"
    } else {
        "python"
    };
    bin_dir.join(python_bin).exists()
}

fn create_venv(path: &Path, os: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
    let python_cmd = match os {
        "windows" => "py",
        _ => "python3",
    };

    let status = Command::new(python_cmd)
        .args(["-m", "venv", path.to_string_lossy().as_ref()])
        .status()
        .with_context(|| "creating python virtual environment")?;

    if status.success() {
        return Ok(());
    }

    // fallback to python
    let status = Command::new("python")
        .args(["-m", "venv", path.to_string_lossy().as_ref()])
        .status()
        .with_context(|| "creating python virtual environment with python")?;

    if !status.success() {
        return Err(anyhow!("failed to create python virtual environment"));
    }

    Ok(())
}

fn resolve_root(root: &Path) -> anyhow::Result<PathBuf> {
    if root.is_absolute() {
        return Ok(root.to_path_buf());
    }

    let cwd = std::env::current_dir().context("resolving runtime_env root")?;
    Ok(cwd.join(root))
}
