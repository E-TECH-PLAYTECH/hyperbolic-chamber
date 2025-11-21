use std::cmp::Ordering;

use serde::Serialize;
use thiserror::Error;

use crate::env_detect::Environment;
use crate::manifest::{Manifest, Mode, RuntimeEnv, Step};

#[derive(Debug, Serialize, Clone)]
pub struct InstallPlan {
    pub app_name: String,
    pub app_version: String,
    pub chosen_mode: String,
    pub os: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_env: Option<RuntimeEnv>,
    pub steps: Vec<PlannedStep>,
}

#[derive(Debug, Serialize, Clone)]
pub struct PlannedStep {
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    pub step: Step,
    #[serde(skip)]
    pub index: usize,
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
        .enumerate()
        .map(|(idx, step)| PlannedStep {
            description: step.description(),
            command: step.command(),
            step: step.clone(),
            index: idx,
        })
        .collect();

    Ok(InstallPlan {
        app_name: manifest.name.clone(),
        app_version: manifest.version.clone(),
        chosen_mode: chosen.0.clone(),
        os: env.os.clone(),
        runtime_env: chosen.1.runtime_env.clone(),
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
    use std::collections::BTreeMap;

    use super::{compare_versions, parse_version, plan_install, version_meets};
    use crate::env_detect::Environment;
    use crate::manifest::{Manifest, Mode, Requirements, Step, parse_os_constraint};

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

    fn base_env() -> Environment {
        Environment {
            os: "macos".to_string(),
            os_version: "14.0".to_string(),
            cpu_arch: "arm64".to_string(),
            ram_gb: 16,
            pkg_managers: vec![],
        }
    }

    fn manifest_with_modes(full_ram: u64, light_ram: u64) -> Manifest {
        let mut modes = BTreeMap::new();
        modes.insert(
            "full".to_string(),
            Mode {
                requirements: Some(Requirements {
                    os: vec![parse_os_constraint("macos>=13").unwrap()],
                    cpu_arch: vec!["arm64".to_string()],
                    ram_gb: Some(full_ram),
                }),
                steps: {
                    let mut steps = BTreeMap::new();
                    steps.insert(
                        "macos".to_string(),
                        vec![Step::Run {
                            run: "echo full".into(),
                        }],
                    );
                    steps
                },
            },
        );
        modes.insert(
            "light".to_string(),
            Mode {
                requirements: Some(Requirements {
                    os: vec![parse_os_constraint("macos>=12").unwrap()],
                    cpu_arch: vec![],
                    ram_gb: Some(light_ram),
                }),
                steps: {
                    let mut steps = BTreeMap::new();
                    steps.insert(
                        "macos".to_string(),
                        vec![Step::Run {
                            run: "echo light".into(),
                        }],
                    );
                    steps
                },
            },
        );

        Manifest {
            name: "demo".into(),
            version: "1.0.0".into(),
            modes,
        }
    }

    #[test]
    fn prefers_full_when_multiple_modes_compatible() {
        let manifest = manifest_with_modes(8, 4);
        let env = base_env();

        let plan = plan_install(&manifest, &env).expect("plan should succeed");
        assert_eq!(plan.chosen_mode, "full");
    }

    #[test]
    fn falls_back_to_light_when_resources_lower() {
        let manifest = manifest_with_modes(12, 4);
        let mut env = base_env();
        env.ram_gb = 8;

        let plan = plan_install(&manifest, &env).expect("plan should succeed");
        assert_eq!(plan.chosen_mode, "light");
    }

    #[test]
    fn surfaces_reasons_when_no_modes_match() {
        let manifest = manifest_with_modes(32, 16);
        let mut env = base_env();
        env.os_version = "11.0".into();
        env.ram_gb = 8;

        let err = plan_install(&manifest, &env).expect_err("planning should fail");
        match err {
            super::PlannerError::NoCompatibleMode { reasons, .. } => {
                assert!(reasons.iter().any(|r| r.contains("requires")));
            }
        }
    }

    #[test]
    fn parses_manifest_json_into_steps() {
        let json = r#"{
            "name": "templated",
            "version": "2.0.0",
            "modes": {
                "full": {
                    "steps": {
                        "macos": [
                            {"run": "echo hi"},
                            {"download": {"url": "https://example.com/file", "dest": "downloads/file.zip"}}
                        ]
                    }
                }
            }
        }"#;

        let manifest: Manifest = serde_json::from_str(json).expect("manifest should parse");
        let steps = &manifest.modes.get("full").unwrap().steps["macos"];
        assert!(matches!(steps[0], Step::Run { .. }));
        assert!(matches!(steps[1], Step::Download { .. }));
    }
}
