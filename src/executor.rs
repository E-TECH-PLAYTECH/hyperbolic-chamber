use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, anyhow};
use serde::Serialize;

use crate::manifest::{DownloadStep, ExtractStep, Step, TemplateConfigStep};
use crate::planner::{InstallPlan, PlannedStep};

#[derive(Debug, Serialize)]
pub struct ExecutionResult {
    pub completed_steps: usize,
    pub total_steps: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ExecutionError {
    #[error("step {index} failed: {message}")]
    StepFailed { index: usize, message: String },
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub fn execute_plan(plan: &InstallPlan) -> Result<ExecutionResult, ExecutionError> {
    println!(
        "Executing plan for {} {} using '{}' mode ({} steps)",
        plan.app_name,
        plan.app_version,
        plan.chosen_mode,
        plan.steps.len()
    );

    for (idx, step) in plan.steps.iter().enumerate() {
        println!(
            "==> [{}/{}] {}",
            idx + 1,
            plan.steps.len(),
            step.description
        );
        if let Err(err) = execute_step(&plan.os, step) {
            return Err(match err {
                ExecutionError::StepFailed { .. } => err,
                ExecutionError::Other(source) => ExecutionError::StepFailed {
                    index: idx,
                    message: source.to_string(),
                },
            });
        }
    }

    Ok(ExecutionResult {
        completed_steps: plan.steps.len(),
        total_steps: plan.steps.len(),
    })
}

fn execute_step(os: &str, step: &PlannedStep) -> Result<(), ExecutionError> {
    match &step.step {
        Step::Run { run } => run_command(os, run).map_err(|err| ExecutionError::StepFailed {
            index: step.index,
            message: err.to_string(),
        }),
        Step::Download { download } => {
            perform_download(download).map_err(|err| ExecutionError::StepFailed {
                index: step.index,
                message: err.to_string(),
            })
        }
        Step::Extract { extract } => {
            perform_extract(extract).map_err(|err| ExecutionError::StepFailed {
                index: step.index,
                message: err.to_string(),
            })
        }
        Step::TemplateConfig { template_config } => {
            render_template(template_config).map_err(|err| ExecutionError::StepFailed {
                index: step.index,
                message: err.to_string(),
            })
        }
    }
}

fn run_command(os: &str, command: &str) -> anyhow::Result<()> {
    let shell = if os == "windows" {
        ("cmd", vec!["/C"])
    } else {
        ("/bin/sh", vec!["-c"])
    };

    let status = Command::new(shell.0)
        .args(&shell.1)
        .arg(command)
        .status()
        .with_context(|| format!("running shell command: {command}"))?;

    if !status.success() {
        return Err(anyhow!("command exited with status {:?}", status.code()));
    }

    Ok(())
}

fn perform_download(step: &DownloadStep) -> anyhow::Result<()> {
    let response =
        reqwest::blocking::get(&step.url).with_context(|| format!("requesting {}", step.url))?;

    if !response.status().is_success() {
        return Err(anyhow!("download failed with status {}", response.status()));
    }

    if let Some(parent) = step.dest.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut dest_file = File::create(&step.dest)
        .with_context(|| format!("creating destination file {}", step.dest.display()))?;
    let bytes = response
        .bytes()
        .with_context(|| format!("reading response from {}", step.url))?;
    dest_file.write_all(&bytes)?;
    Ok(())
}

fn perform_extract(step: &ExtractStep) -> anyhow::Result<()> {
    let archive_path = &step.archive;
    let dest_dir = &step.dest;

    if let Some(parent) = dest_dir.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir_all(dest_dir)?;

    let file = File::open(archive_path)
        .with_context(|| format!("opening archive {}", archive_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("reading zip archive {}", archive_path.display()))?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = sanitize_extract_path(dest_dir, file.name())?;

        if file.is_dir() {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut outfile = File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(mode) = file.unix_mode() {
                    fs::set_permissions(&outpath, fs::Permissions::from_mode(mode))?;
                }
            }
        }
    }

    Ok(())
}

fn sanitize_extract_path(dest: &PathBuf, name: &str) -> anyhow::Result<PathBuf> {
    let mut path = PathBuf::from(name);
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(anyhow!("archive entry escapes destination: {name}"));
    }

    if path.is_absolute() {
        return Err(anyhow!("archive entry has absolute path: {name}"));
    }

    let full_path = dest.join(&path);
    let canonical_dest = fs::canonicalize(dest).unwrap_or_else(|_| dest.clone());
    let canonical_full = fs::canonicalize(&full_path).unwrap_or(full_path.clone());
    if !canonical_full.starts_with(&canonical_dest) {
        return Err(anyhow!("archive entry outside destination: {name}"));
    }

    Ok(full_path)
}

fn render_template(step: &TemplateConfigStep) -> anyhow::Result<()> {
    let mut source = String::new();
    File::open(&step.source)
        .with_context(|| format!("opening template {}", step.source.display()))?
        .read_to_string(&mut source)?;

    let rendered = replace_placeholders(&source, &step.vars);

    if let Some(parent) = step.dest.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut dest_file = File::create(&step.dest)
        .with_context(|| format!("writing templated config {}", step.dest.display()))?;
    dest_file.write_all(rendered.as_bytes())?;
    Ok(())
}

fn replace_placeholders(
    template: &str,
    vars: &std::collections::HashMap<String, String>,
) -> String {
    let mut result = String::new();
    let mut remainder = template;
    while let Some(start) = remainder.find("{{") {
        if let Some(end) = remainder[start + 2..].find("}}") {
            let end_index = start + 2 + end;
            result.push_str(&remainder[..start]);
            let key = &remainder[start + 2..end_index];
            if let Some(value) = vars.get(key.trim()) {
                result.push_str(value);
            } else {
                result.push_str("{{");
                result.push_str(key);
                result.push_str("}}");
            }
            remainder = &remainder[end_index + 2..];
            continue;
        }
        break;
    }
    result.push_str(remainder);
    result
}
