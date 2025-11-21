use std::cmp::Ordering;

use serde::Serialize;
use thiserror::Error;

use crate::env_detect::Environment;
use crate::manifest::{Manifest, Mode};

#[derive(Debug, Serialize)]
pub struct InstallPlan {
    pub app_name: String,
    pub app_version: String,
    pub chosen_mode: String,
    pub os: String,
    pub steps: Vec<PlannedStep>,
}

#[derive(Debug, Serialize)]
pub struct PlannedStep {
    pub description: String,
    pub command: String,
}

#[derive(Debug, Error)]
pub enum PlannerError {
    #[error("no compatible mode found for environment {environment:?}: {reasons:?}")]
    NoCompatibleMode {
        environment: Environment,
        reasons: Vec<String>,
    },
}

pub fn plan_install(manifest: &Manifest, env: &Environment) -> Result<InstallPlan, PlannerError> {
    let mut compatible_modes: Vec<(&String, &Mode)> = Vec::new();
    let mut reasons = Vec::new();

    for (mode_name, mode) in manifest.modes.iter() {
        match is_mode_compatible(mode_name, mode, env) {
            Ok(true) => compatible_modes.push((mode_name, mode)),
            Ok(false) => reasons.push(format!(
                "{mode_name}: missing required steps for {}",
                env.os
            )),
            Err(reason) => reasons.push(format!("{mode_name}: {reason}")),
        }
    }

    if compatible_modes.is_empty() {
        return Err(PlannerError::NoCompatibleMode {
            environment: env.clone(),
            reasons,
        });
    }

    let chosen = choose_best_mode(&compatible_modes);
    let steps = chosen
        .1
        .steps
        .get(&env.os)
        .expect("validated in compatibility check")
        .iter()
        .map(|step| PlannedStep {
            description: step.description(),
            command: step.command(),
        })
        .collect();

    Ok(InstallPlan {
        app_name: manifest.name.clone(),
        app_version: manifest.version.clone(),
        chosen_mode: chosen.0.clone(),
        os: env.os.clone(),
        steps,
    })
}

fn is_mode_compatible(mode_name: &str, mode: &Mode, env: &Environment) -> Result<bool, String> {
    if !mode.steps.contains_key(&env.os) {
        return Ok(false);
    }

    if let Some(requirements) = &mode.requirements {
        if !requirements.os.is_empty()
            && !requirements
                .os
                .iter()
                .any(|constraint| os_matches(env, constraint))
        {
            return Err(format!(
                "requires {:?}, found {} {}",
                requirements.os, env.os, env.os_version
            ));
        }

        if !requirements.cpu_arch.is_empty()
            && !requirements
                .cpu_arch
                .iter()
                .any(|arch| arch.eq_ignore_ascii_case(&env.cpu_arch))
        {
            return Err(format!(
                "requires CPU in {:?}, found {}",
                requirements.cpu_arch, env.cpu_arch
            ));
        }

        if let Some(required_ram) = requirements.ram_gb {
            if env.ram_gb < required_ram {
                return Err(format!(
                    "requires >= {required_ram} GiB RAM, found {} GiB",
                    env.ram_gb
                ));
            }
        }
    }

    Ok(true)
}

fn os_matches(env: &Environment, constraint: &crate::manifest::OsConstraint) -> bool {
    if constraint.family != env.os {
        return false;
    }

    if let Some(min_version) = &constraint.min_version {
        version_meets(min_version, &env.os_version)
    } else {
        true
    }
}

fn choose_best_mode(modes: &[(&String, &Mode)]) -> (&String, &Mode) {
    if let Some(full_mode) = modes.iter().find(|(name, _)| name.as_str() == "full") {
        return full_mode;
    }

    modes
        .iter()
        .max_by(|(_, a), (_, b)| required_ram(a).cmp(&required_ram(b)))
        .copied()
        .expect("at least one mode available")
}

fn required_ram(mode: &Mode) -> u64 {
    mode.requirements
        .as_ref()
        .and_then(|req| req.ram_gb)
        .unwrap_or(0)
}

fn version_meets(min_version: &str, actual: &str) -> bool {
    let min_parts: Option<Vec<u64>> = parse_version(min_version);
    let actual_parts: Option<Vec<u64>> = parse_version(actual);

    match (min_parts, actual_parts) {
        (Some(min), Some(actual)) => compare_versions(&actual, &min) != Ordering::Less,
        _ => actual == min_version,
    }
}

fn parse_version(version: &str) -> Option<Vec<u64>> {
    let mut parts = Vec::new();
    for part in version.split('.') {
        parts.push(part.trim().parse::<u64>().ok()?);
    }
    Some(parts)
}

fn compare_versions(a: &[u64], b: &[u64]) -> Ordering {
    let max_len = a.len().max(b.len());
    for i in 0..max_len {
        let left = *a.get(i).unwrap_or(&0);
        let right = *b.get(i).unwrap_or(&0);
        match left.cmp(&right) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
    Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::{compare_versions, parse_version, version_meets};

    #[test]
    fn compares_versions_with_padding() {
        let a = parse_version("10.0.1").unwrap();
        let b = parse_version("10.0").unwrap();
        assert_eq!(compare_versions(&a, &b), std::cmp::Ordering::Greater);
    }

    #[test]
    fn version_meets_handles_unparseable_versions() {
        assert!(!version_meets("abc", "10.0"));
        assert!(version_meets("10.0", "10"));
    }

    #[test]
    fn parse_version_rejects_invalid_numbers() {
        assert!(parse_version("10.x").is_none());
    }
}
