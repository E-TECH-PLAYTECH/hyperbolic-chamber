use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

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
    #[serde(default)]
    pub runtime_env: Option<RuntimeEnv>,
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
    Download { download: DownloadStep },
    Extract { extract: ExtractStep },
    TemplateConfig { template_config: TemplateConfigStep },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DownloadStep {
    pub url: String,
    pub dest: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExtractStep {
    pub archive: PathBuf,
    pub dest: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TemplateConfigStep {
    pub source: PathBuf,
    pub dest: PathBuf,
    #[serde(default)]
    pub vars: HashMap<String, String>,
}

fn default_runtime_root() -> PathBuf {
    PathBuf::from(".enzyme_env")
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RuntimeEnv {
    #[serde(rename = "type")]
    pub kind: RuntimeEnvType,
    #[serde(default = "default_runtime_root")]
    pub root: PathBuf,
    #[serde(default)]
    pub node: Option<NodeRuntime>,
    #[serde(default)]
    pub python: Option<PythonRuntime>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeEnvType {
    NodeLocal,
    PythonVenv,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct NodeRuntime {
    pub version: Option<String>,
    #[serde(default)]
    pub install_strategy: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PythonRuntime {
    pub version: Option<String>,
    #[serde(default)]
    pub install_strategy: Option<String>,
}

impl Step {
    pub fn validate(&self) -> Result<(), ManifestValidationError> {
        match self {
            Step::Run { run } if run.trim().is_empty() => Err(
                ManifestValidationError::InvalidStep("run command cannot be empty".to_string()),
            ),
            Step::Download { download } if download.url.trim().is_empty() => Err(
                ManifestValidationError::InvalidStep("download url cannot be empty".to_string()),
            ),
            Step::Download { download } if download.dest.as_os_str().is_empty() => Err(
                ManifestValidationError::InvalidStep("download dest cannot be empty".to_string()),
            ),
            Step::Extract { extract } if extract.archive.as_os_str().is_empty() => Err(
                ManifestValidationError::InvalidStep("extract archive cannot be empty".to_string()),
            ),
            Step::Extract { extract } if extract.dest.as_os_str().is_empty() => Err(
                ManifestValidationError::InvalidStep("extract dest cannot be empty".to_string()),
            ),
            Step::TemplateConfig { template_config }
                if template_config.source.as_os_str().is_empty() =>
            {
                Err(ManifestValidationError::InvalidStep(
                    "template_config source cannot be empty".to_string(),
                ))
            }
            Step::TemplateConfig { template_config }
                if template_config.dest.as_os_str().is_empty() =>
            {
                Err(ManifestValidationError::InvalidStep(
                    "template_config dest cannot be empty".to_string(),
                ))
            }
            _ => Ok(()),
        }
    }

    pub fn description(&self) -> String {
        match self {
            Step::Run { run } => format!("Run: {run}"),
            Step::Download { download } => {
                format!("Download {} to {}", download.url, download.dest.display())
            }
            Step::Extract { extract } => {
                format!(
                    "Extract {} to {}",
                    extract.archive.display(),
                    extract.dest.display()
                )
            }
            Step::TemplateConfig { template_config } => format!(
                "Template {} to {}",
                template_config.source.display(),
                template_config.dest.display()
            ),
        }
    }

    pub fn command(&self) -> Option<String> {
        match self {
            Step::Run { run } => Some(run.clone()),
            _ => None,
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

        if let Some(runtime_env) = &mode.runtime_env {
            validate_runtime_env(mode_name, runtime_env)?;
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

fn validate_runtime_env(
    mode_name: &str,
    runtime: &RuntimeEnv,
) -> Result<(), ManifestValidationError> {
    if runtime.root.as_os_str().is_empty() {
        return Err(ManifestValidationError::InvalidRequirement(
            mode_name.to_string(),
            "runtime_env root cannot be empty".to_string(),
        ));
    }

    match runtime.kind {
        RuntimeEnvType::NodeLocal => {
            if runtime.node.is_none() {
                return Err(ManifestValidationError::InvalidRequirement(
                    mode_name.to_string(),
                    "runtime_env.node must be provided for node_local".to_string(),
                ));
            }
        }
        RuntimeEnvType::PythonVenv => {
            if runtime.python.is_none() {
                return Err(ManifestValidationError::InvalidRequirement(
                    mode_name.to_string(),
                    "runtime_env.python must be provided for python_venv".to_string(),
                ));
            }
        }
    }

    Ok(())
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
