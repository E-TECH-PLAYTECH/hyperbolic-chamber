use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};

use crate::env_detect::detect_environment;
use crate::executor::execute_plan;
use crate::manifest::load_manifest;
use crate::planner::plan_install;

#[derive(Debug, Parser)]
#[command(
    name = "enzyme-installer",
    version,
    about = "Adaptive installer for heterogeneous machines"
)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Detect the current environment and print it as JSON
    Detect {
        /// Emit compact JSON instead of pretty output
        #[arg(long)]
        raw: bool,
    },
    /// Build an installation plan from a manifest without executing it
    Plan {
        /// Path to the manifest file
        manifest_path: PathBuf,
        /// Emit compact JSON instead of pretty output
        #[arg(long)]
        raw: bool,
    },
    /// Build and execute an installation plan
    Install {
        /// Path to the manifest file
        manifest_path: PathBuf,
        /// Emit compact JSON for the plan summary
        #[arg(long)]
        raw: bool,
    },
}

pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Detect { raw } => {
            let env = detect_environment()?;
            if raw {
                println!("{}", serde_json::to_string(&env)?);
            } else {
                println!("{}", serde_json::to_string_pretty(&env)?);
            }
        }
        Commands::Plan { manifest_path, raw } => {
            let manifest = load_manifest(&manifest_path)
                .with_context(|| format!("loading manifest from {}", manifest_path.display()))?;
            let env = detect_environment()?;
            let plan = plan_install(&manifest, &env)?;

            if raw {
                println!("{}", serde_json::to_string(&plan)?);
            } else {
                println!("{}", serde_json::to_string_pretty(&plan)?);
            }
        }
        Commands::Install { manifest_path, raw } => {
            let manifest = load_manifest(&manifest_path)
                .with_context(|| format!("loading manifest from {}", manifest_path.display()))?;
            let env = detect_environment()?;
            let plan = plan_install(&manifest, &env)?;

            if raw {
                println!("{}", serde_json::to_string(&plan)?);
            } else {
                println!(
                    "Preparing to install using mode '{}' ({} steps)",
                    plan.chosen_mode,
                    plan.steps.len()
                );
            }

            execute_plan(&plan)?;
        }
    }

    Ok(())
}
