use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub modes: BTreeMap<String, Mode>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Mode {
    pub requirements: Option<Requirements>,
    pub steps: BTreeMap<String, Vec<Step>>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Requirements {
    #[serde(default)]
    pub os: Vec<OsConstraint>,
    #[serde(default)]
    pub cpu_arch: Vec<String>,
    pub ram_gb: Option<u64>,
}

#[derive(Debug, Serialize, Clone)]
pub struct OsConstraint {
    pub family: String,
    pub min_version: Option<String>,
}

impl<'de> Deserialize<'de> for OsConstraint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        parse_os_constraint(&raw).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum Step {
    Run { run: String },
}

impl Step {
    pub fn validate(&self) -> Result<(), ManifestValidationError> {
        match self {
            Step::Run { run } if run.trim().is_empty() => Err(
                ManifestValidationError::InvalidStep("run command cannot be empty".to_string()),
            ),
            _ => Ok(()),
        }
    }

    pub fn description(&self) -> String {
        match self {
            Step::Run { run } => format!("Run: {run}"),
        }
    }

    pub fn command(&self) -> String {
        match self {
            Step::Run { run } => run.clone(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("invalid OS constraint format: {0}")]
    InvalidOsConstraint(String),
}

#[derive(Debug, Error)]
pub enum ManifestValidationError {
    #[error("manifest missing required field: {0}")]
    MissingField(String),
    #[error("manifest has no modes defined")]
    EmptyModes,
    #[error("mode '{0}' has no steps defined for any platform")]
    ModeMissingSteps(String),
    #[error("mode '{mode}' has no steps for platform '{platform}'")]
    ModeMissingPlatformSteps { mode: String, platform: String },
    #[error("unsupported platform '{0}' in manifest")]
    UnsupportedPlatform(String),
    #[error("mode '{0}' has an invalid requirement: {1}")]
    InvalidRequirement(String, String),
    #[error("mode '{0}' has an invalid step: {1}")]
    InvalidStep(String, String),
}

pub fn parse_os_constraint(raw: &str) -> Result<OsConstraint, ManifestError> {
    if let Some((family_part, version_part)) = raw.split_once(">=") {
        let family = family_part.trim().to_lowercase();
        let min_version = Some(version_part.trim().to_string());
        if family.is_empty() || version_part.trim().is_empty() {
            return Err(ManifestError::InvalidOsConstraint(raw.to_string()));
        }

        return Ok(OsConstraint {
            family,
            min_version,
        });
    }

    if !raw.trim().is_empty() {
        return Ok(OsConstraint {
            family: raw.trim().to_lowercase(),
            min_version: None,
        });
    }

    Err(ManifestError::InvalidOsConstraint(raw.to_string()))
}

pub fn load_manifest(path: &Path) -> anyhow::Result<Manifest> {
    let data = fs::read_to_string(path)
        .with_context(|| format!("reading manifest at {}", path.display()))?;
    let manifest: Manifest = serde_json::from_str(&data)
        .with_context(|| format!("parsing manifest at {}", path.display()))?;
    validate_manifest(manifest)
        .with_context(|| format!("validating manifest at {}", path.display()))
}

fn validate_manifest(manifest: Manifest) -> Result<Manifest, ManifestValidationError> {
    if manifest.name.trim().is_empty() {
        return Err(ManifestValidationError::MissingField("name".to_string()));
    }

    if manifest.version.trim().is_empty() {
        return Err(ManifestValidationError::MissingField("version".to_string()));
    }

    if manifest.modes.is_empty() {
        return Err(ManifestValidationError::EmptyModes);
    }

    for (mode_name, mode) in manifest.modes.iter() {
        if mode.steps.is_empty() {
            return Err(ManifestValidationError::ModeMissingSteps(mode_name.clone()));
        }

        if let Some(requirements) = &mode.requirements {
            for constraint in &requirements.os {
                validate_os_family(&constraint.family)?;
            }

            for arch in &requirements.cpu_arch {
                if arch.trim().is_empty() {
                    return Err(ManifestValidationError::InvalidRequirement(
                        mode_name.clone(),
                        "cpu_arch entries must not be empty".to_string(),
                    ));
                }
            }
        }

        for (platform, steps) in mode.steps.iter() {
            if steps.is_empty() {
                return Err(ManifestValidationError::ModeMissingPlatformSteps {
                    mode: mode_name.clone(),
                    platform: platform.clone(),
                });
            }

            validate_os_family(platform)?;

            for step in steps {
                step.validate().map_err(|err| {
                    ManifestValidationError::InvalidStep(mode_name.clone(), err.to_string())
                })?;
            }
        }
    }

    Ok(manifest)
}

fn validate_os_family(os: &str) -> Result<(), ManifestValidationError> {
    match os {
        "windows" | "macos" => Ok(()),
        other => Err(ManifestValidationError::UnsupportedPlatform(
            other.to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Manifest, ManifestValidationError, Mode, Requirements, Step, load_manifest,
        parse_os_constraint,
    };
    use std::collections::BTreeMap;

    #[test]
    fn parses_os_constraint_with_version() {
        let parsed = parse_os_constraint("windows>=10").expect("should parse valid constraint");
        assert_eq!(parsed.family, "windows");
        assert_eq!(parsed.min_version.as_deref(), Some("10"));
    }

    #[test]
    fn validates_manifest_structure() {
        let manifest = Manifest {
            name: "demo".to_string(),
            version: "1.0.0".to_string(),
            modes: {
                let mut modes = BTreeMap::new();
                modes.insert(
                    "full".to_string(),
                    Mode {
                        requirements: Some(Requirements {
                            os: vec![parse_os_constraint("windows>=10").unwrap()],
                            cpu_arch: vec!["x64".to_string()],
                            ram_gb: Some(8),
                        }),
                        steps: {
                            let mut steps = BTreeMap::new();
                            steps.insert(
                                "windows".to_string(),
                                vec![Step::Run {
                                    run: "echo ok".to_string(),
                                }],
                            );
                            steps
                        },
                    },
                );
                modes
            },
        };

        validate_manifest(manifest).expect("manifest should be valid");
    }

    #[test]
    fn rejects_empty_steps() {
        let manifest = Manifest {
            name: "demo".to_string(),
            version: "1.0.0".to_string(),
            modes: {
                let mut modes = BTreeMap::new();
                modes.insert(
                    "full".to_string(),
                    Mode {
                        requirements: None,
                        steps: BTreeMap::new(),
                    },
                );
                modes
            },
        };

        let err = validate_manifest(manifest).expect_err("manifest should be invalid");
        matches!(err, ManifestValidationError::ModeMissingSteps(_));
    }

    #[test]
    fn refuses_unsupported_platforms() {
        let manifest = Manifest {
            name: "demo".to_string(),
            version: "1.0.0".to_string(),
            modes: {
                let mut modes = BTreeMap::new();
                let mut steps = BTreeMap::new();
                steps.insert("linux".to_string(), vec![Step::Run { run: "echo".into() }]);
                modes.insert(
                    "full".to_string(),
                    Mode {
                        requirements: None,
                        steps,
                    },
                );
                modes
            },
        };

        let err = validate_manifest(manifest).expect_err("manifest should be invalid");
        matches!(err, ManifestValidationError::UnsupportedPlatform(_));
    }

    #[test]
    fn load_manifest_produces_validation_error_context() {
        // create temp file with invalid manifest (missing modes)
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let file_path = dir.path().join("manifest.json");
        std::fs::write(&file_path, "{\"name\":\"demo\",\"version\":\"1\"}").unwrap();

        let err = load_manifest(&file_path).expect_err("should surface validation errors");
        assert!(err.to_string().contains("validating manifest"));
    }

    fn validate_manifest(manifest: Manifest) -> Result<Manifest, ManifestValidationError> {
        super::validate_manifest(manifest)
    }
}
