use std::process::Command;

use anyhow::{Context, anyhow};

use crate::planner::InstallPlan;

pub fn execute_plan(plan: &InstallPlan) -> anyhow::Result<()> {
    println!(
        "Executing plan for {} {} using '{}' mode ({} steps)",
        plan.app_name,
        plan.app_version,
        plan.chosen_mode,
        plan.steps.len()
    );

    let shell = if plan.os == "windows" {
        ("cmd", vec!["/C"])
    } else {
        ("/bin/sh", vec!["-c"])
    };

    for (idx, step) in plan.steps.iter().enumerate() {
        println!(
            "==> [{}/{}] {}",
            idx + 1,
            plan.steps.len(),
            step.description
        );
        let status = Command::new(shell.0)
            .args(&shell.1)
            .arg(&step.command)
            .status()
            .with_context(|| format!("running step {}: {}", idx + 1, step.command))?;

        if !status.success() {
            return Err(anyhow!(
                "Step {} failed with exit code {:?}",
                idx + 1,
                status.code()
            ));
        }
    }

    Ok(())
}
