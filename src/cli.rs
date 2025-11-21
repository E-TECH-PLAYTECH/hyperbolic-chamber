use std::path::PathBuf;

use chrono::Utc;
use clap::{Parser, Subcommand};
use serde::Serialize;

use crate::env_detect::detect_environment;
use crate::executor::{ExecutionError, ExecutionResult, execute_plan};
use crate::manifest::load_manifest;
use crate::planner::{InstallPlan, PlannerError, plan_install};
use crate::state::{InstallRecord, InstallStatus, add_install_record, load_state};

#[derive(Debug, Parser)]
#[command(
    name = "enzyme-installer",
    version,
    about = "Adaptive installer for heterogeneous machines"
)]
pub struct Cli {
    /// Emit JSON output for the selected subcommand
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Detect the current environment
    Detect,
    /// Build an installation plan from a manifest without executing it
    Plan {
        /// Path to the manifest file
        manifest_path: PathBuf,
    },
    /// Build and execute an installation plan
    Install {
        /// Path to the manifest file
        manifest_path: PathBuf,
    },
    /// List previous installs recorded on this machine
    #[command(name = "list-installed")]
    ListInstalled,
}

#[derive(Debug, Serialize)]
struct DetectResponse<T> {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    environment: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct PlanResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan: Option<InstallPlan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<PlanErrorResponse>,
}

#[derive(Debug, Serialize)]
struct PlanErrorResponse {
    message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    details: Vec<String>,
    environment: Option<crate::env_detect::Environment>,
}

#[derive(Debug, Serialize)]
struct InstallResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan: Option<InstallPlan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<ExecutionResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<InstallErrorResponse>,
}

#[derive(Debug, Serialize)]
struct InstallErrorResponse {
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    failed_step_index: Option<usize>,
}

#[derive(Debug, Serialize)]
struct ListResponse {
    ok: bool,
    installs: Vec<InstallRecord>,
}

pub fn run() -> i32 {
    let cli = Cli::parse();
    let json = cli.json;

    let exit_code = match cli.command {
        Commands::Detect => handle_detect(json),
        Commands::Plan { manifest_path } => handle_plan(json, manifest_path),
        Commands::Install { manifest_path } => handle_install(json, manifest_path),
        Commands::ListInstalled => handle_list_installed(json),
    };

    if exit_code != 0 {
        std::process::exit(exit_code);
    }

    0
}

fn handle_detect(json: bool) -> i32 {
    match detect_environment() {
        Ok(env) => {
            if json {
                print_json(&DetectResponse {
                    ok: true,
                    environment: Some(env),
                    error: None,
                });
            } else {
                println!("{}", serde_json::to_string_pretty(&env).unwrap());
            }
            0
        }
        Err(err) => {
            if json {
                print_json(&DetectResponse::<()> {
                    ok: false,
                    environment: None,
                    error: Some(err.to_string()),
                });
            } else {
                eprintln!("{err}");
            }
            1
        }
    }
}

fn handle_plan(json: bool, manifest_path: PathBuf) -> i32 {
    let manifest = match load_manifest(&manifest_path) {
        Ok(m) => m,
        Err(err) => {
            if json {
                print_json(&PlanResponse {
                    ok: false,
                    plan: None,
                    error: Some(PlanErrorResponse {
                        message: err.to_string(),
                        details: Vec::new(),
                        environment: None,
                    }),
                });
            } else {
                eprintln!("{err}");
            }
            return 1;
        }
    };

    let env = match detect_environment() {
        Ok(env) => env,
        Err(err) => {
            if json {
                print_json(&PlanResponse {
                    ok: false,
                    plan: None,
                    error: Some(PlanErrorResponse {
                        message: err.to_string(),
                        details: Vec::new(),
                        environment: None,
                    }),
                });
            } else {
                eprintln!("{err}");
            }
            return 1;
        }
    };

    match plan_install(&manifest, &env) {
        Ok(plan) => {
            if json {
                print_json(&PlanResponse {
                    ok: true,
                    plan: Some(plan),
                    error: None,
                });
            } else {
                println!(
                    "Plan for {} {} using '{}' mode ({} steps)",
                    manifest.name,
                    manifest.version,
                    plan.chosen_mode,
                    plan.steps.len()
                );
                println!("{}", serde_json::to_string_pretty(&plan).unwrap());
            }
            0
        }
        Err(PlannerError::NoCompatibleMode {
            environment,
            reasons,
        }) => {
            if json {
                print_json(&PlanResponse {
                    ok: false,
                    plan: None,
                    error: Some(PlanErrorResponse {
                        message: "No compatible modes for this environment".to_string(),
                        details: reasons,
                        environment: Some(environment),
                    }),
                });
            } else {
                eprintln!("No compatible modes found:");
                for reason in reasons {
                    eprintln!("- {reason}");
                }
            }
            2
        }
    }
}

fn handle_install(json: bool, manifest_path: PathBuf) -> i32 {
    let manifest = match load_manifest(&manifest_path) {
        Ok(m) => m,
        Err(err) => {
            emit_install_error(json, None, &err.to_string(), None);
            return 1;
        }
    };

    let env = match detect_environment() {
        Ok(env) => env,
        Err(err) => {
            emit_install_error(json, None, &err.to_string(), None);
            return 1;
        }
    };

    let plan = match plan_install(&manifest, &env) {
        Ok(plan) => plan,
        Err(PlannerError::NoCompatibleMode {
            environment,
            reasons,
        }) => {
            let message = "No compatible modes for this environment".to_string();
            let detail = PlanErrorResponse {
                message: message.clone(),
                details: reasons.clone(),
                environment: Some(environment),
            };
            if json {
                print_json(&InstallResponse {
                    ok: false,
                    plan: None,
                    result: None,
                    error: Some(InstallErrorResponse {
                        message: format!("{message}: {}", reasons.join("; ")),
                        failed_step_index: None,
                    }),
                });
            } else {
                eprintln!("No compatible modes found for install.");
                eprintln!("{}", serde_json::to_string_pretty(&detail).unwrap());
            }
            return 2;
        }
    };

    if !json {
        println!(
            "Preparing to install {} {} using mode '{}' ({} steps)",
            manifest.name,
            manifest.version,
            plan.chosen_mode,
            plan.steps.len()
        );
    }

    match execute_plan(&plan) {
        Ok(result) => {
            let record = InstallRecord {
                app_name: manifest.name.clone(),
                app_version: manifest.version.clone(),
                mode: plan.chosen_mode.clone(),
                os: env.os.clone(),
                cpu_arch: env.cpu_arch.clone(),
                timestamp: Utc::now(),
                status: InstallStatus::Success,
            };
            let _ = add_install_record(record);

            if json {
                print_json(&InstallResponse {
                    ok: true,
                    plan: Some(plan),
                    result: Some(result),
                    error: None,
                });
            } else {
                println!("Installation complete ({} steps).", result.completed_steps);
            }
            0
        }
        Err(err) => {
            let failed_index = match &err {
                ExecutionError::StepFailed { index, .. } => Some(*index),
                _ => None,
            };
            let record = InstallRecord {
                app_name: manifest.name.clone(),
                app_version: manifest.version.clone(),
                mode: plan.chosen_mode.clone(),
                os: env.os.clone(),
                cpu_arch: env.cpu_arch.clone(),
                timestamp: Utc::now(),
                status: InstallStatus::Failed,
            };
            let _ = add_install_record(record);

            emit_install_error(json, Some(&plan), &err.to_string(), failed_index);
            3
        }
    }
}

fn handle_list_installed(json: bool) -> i32 {
    match load_state() {
        Ok(state) => {
            if json {
                print_json(&ListResponse {
                    ok: true,
                    installs: state.installs,
                });
            } else {
                if state.installs.is_empty() {
                    println!("No installs recorded yet.");
                } else {
                    println!("Recorded installs:");
                    for record in state.installs {
                        println!(
                            "- {} {} [{}] on {} ({}): {:?} at {}",
                            record.app_name,
                            record.app_version,
                            record.mode,
                            record.os,
                            record.cpu_arch,
                            record.status,
                            record.timestamp
                        );
                    }
                }
            }
            0
        }
        Err(err) => {
            if json {
                print_json(&PlanResponse {
                    ok: false,
                    plan: None,
                    error: Some(PlanErrorResponse {
                        message: err.to_string(),
                        details: Vec::new(),
                        environment: None,
                    }),
                });
            } else {
                eprintln!("{err}");
            }
            1
        }
    }
}

fn emit_install_error(
    json: bool,
    plan: Option<&InstallPlan>,
    message: &str,
    failed_index: Option<usize>,
) {
    if json {
        print_json(&InstallResponse {
            ok: false,
            plan: plan.cloned(),
            result: None,
            error: Some(InstallErrorResponse {
                message: message.to_string(),
                failed_step_index: failed_index,
            }),
        });
    } else {
        eprintln!("{message}");
        if let Some(plan) = plan {
            eprintln!(
                "Plan '{}' with {} steps was in progress.",
                plan.chosen_mode,
                plan.steps.len()
            );
        }
    }
}

fn print_json<T: Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(text) => println!("{text}"),
        Err(err) => eprintln!("failed to render JSON: {err}"),
    }
}
