use std::fs;
use std::io;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const CURRENT_LOCAL_CONFIG_VERSION: u32 = 0;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalConfig {
    pub format_version: u32,
    pub identity_mappings: Vec<IdentityMapping>,
    #[serde(default)]
    pub capability_mappings: Vec<CapabilityMapping>,
}

impl Default for LocalConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalConfig {
    pub fn new() -> Self {
        Self {
            format_version: CURRENT_LOCAL_CONFIG_VERSION,
            identity_mappings: Vec::new(),
            capability_mappings: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), LocalConfigError> {
        if self.format_version != CURRENT_LOCAL_CONFIG_VERSION {
            return Err(LocalConfigError::UnsupportedFormat {
                found: self.format_version,
                expected: CURRENT_LOCAL_CONFIG_VERSION,
            });
        }

        for mapping in &self.identity_mappings {
            if mapping.fingerprint.trim().is_empty() {
                return Err(LocalConfigError::InvalidValue {
                    field: "identity_mappings.fingerprint",
                    reason: "must not be empty".to_string(),
                });
            }
            if mapping.path.as_os_str().is_empty() {
                return Err(LocalConfigError::InvalidValue {
                    field: "identity_mappings.path",
                    reason: "must not be empty".to_string(),
                });
            }
        }
        for mapping in &self.capability_mappings {
            if mapping.name.trim().is_empty() {
                return Err(LocalConfigError::InvalidValue {
                    field: "capability_mappings.name",
                    reason: "must not be empty".to_string(),
                });
            }
            if mapping.path.as_os_str().is_empty() {
                return Err(LocalConfigError::InvalidValue {
                    field: "capability_mappings.path",
                    reason: "must not be empty".to_string(),
                });
            }
        }

        Ok(())
    }

    pub fn map_identity(
        &mut self,
        fingerprint: String,
        path: PathBuf,
        preferred_name: Option<String>,
    ) -> Result<(), LocalConfigError> {
        if fingerprint.trim().is_empty() {
            return Err(LocalConfigError::InvalidValue {
                field: "fingerprint",
                reason: "must not be empty".to_string(),
            });
        }
        if path.as_os_str().is_empty() {
            return Err(LocalConfigError::InvalidValue {
                field: "path",
                reason: "must not be empty".to_string(),
            });
        }

        if let Some(mapping) = self
            .identity_mappings
            .iter_mut()
            .find(|mapping| mapping.fingerprint == fingerprint)
        {
            mapping.path = path;
            if preferred_name.is_some() {
                mapping.preferred_name = preferred_name;
            }
        } else {
            self.identity_mappings.push(IdentityMapping {
                fingerprint,
                path,
                preferred_name,
            });
        }

        Ok(())
    }

    pub fn unmap_identity(&mut self, fingerprint: &str) -> Option<IdentityMapping> {
        let index = self
            .identity_mappings
            .iter()
            .position(|mapping| mapping.fingerprint == fingerprint)?;
        Some(self.identity_mappings.remove(index))
    }

    pub fn identity_mapping_mut(&mut self, fingerprint: &str) -> Option<&mut IdentityMapping> {
        self.identity_mappings
            .iter_mut()
            .find(|mapping| mapping.fingerprint == fingerprint)
    }

    pub fn identity_path(&self, fingerprint: &str) -> Option<&Path> {
        self.identity_mappings
            .iter()
            .find(|mapping| mapping.fingerprint == fingerprint)
            .map(|mapping| mapping.path.as_path())
    }

    pub fn map_capability(&mut self, name: String, path: PathBuf) -> Result<(), LocalConfigError> {
        if name.trim().is_empty() {
            return Err(LocalConfigError::InvalidValue {
                field: "capability",
                reason: "must not be empty".to_string(),
            });
        }
        if path.as_os_str().is_empty() {
            return Err(LocalConfigError::InvalidValue {
                field: "path",
                reason: "must not be empty".to_string(),
            });
        }

        if let Some(mapping) = self
            .capability_mappings
            .iter_mut()
            .find(|mapping| mapping.name == name)
        {
            mapping.path = path;
        } else {
            self.capability_mappings
                .push(CapabilityMapping { name, path });
        }

        Ok(())
    }

    pub fn capability_path(&self, name: &str) -> Option<&Path> {
        self.capability_mappings
            .iter()
            .find(|mapping| mapping.name == name)
            .map(|mapping| mapping.path.as_path())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdentityMapping {
    pub fingerprint: String,
    pub path: PathBuf,
    #[serde(default)]
    pub preferred_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityMapping {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum LocalConfigError {
    #[error(
        "local config format version {found} is not supported by this build; expected {expected}"
    )]
    UnsupportedFormat { found: u32, expected: u32 },
    #[error("invalid value for {field}: {reason}")]
    InvalidValue { field: &'static str, reason: String },
    #[error("failed to read {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("failed to write {path}: {source}")]
    Write { path: PathBuf, source: io::Error },
    #[error("failed to create directory {path}: {source}")]
    CreateDir { path: PathBuf, source: io::Error },
    #[error("failed to parse {path}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("failed to encode local config: {0}")]
    Encode(serde_json::Error),
}

pub fn load_local_config(path: impl AsRef<Path>) -> Result<LocalConfig, LocalConfigError> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(LocalConfig::new());
    }

    let bytes = fs::read(path).map_err(|source| LocalConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let config: LocalConfig =
        serde_json::from_slice(&bytes).map_err(|source| LocalConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
    config.validate()?;
    Ok(config)
}

pub fn save_local_config(
    path: impl AsRef<Path>,
    config: &LocalConfig,
) -> Result<(), LocalConfigError> {
    config.validate()?;
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| LocalConfigError::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let bytes = serde_json::to_vec_pretty(config).map_err(LocalConfigError::Encode)?;
    let tmp_path = temporary_path(path);
    fs::write(&tmp_path, bytes).map_err(|source| LocalConfigError::Write {
        path: tmp_path.clone(),
        source,
    })?;
    set_private_file_permissions(&tmp_path).map_err(|source| LocalConfigError::Write {
        path: tmp_path.clone(),
        source,
    })?;
    fs::rename(&tmp_path, path).map_err(|source| LocalConfigError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_file_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut tmp = path.to_path_buf();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!("{extension}.tmp"))
        .unwrap_or_else(|| "tmp".to_string());
    tmp.set_extension(extension);
    tmp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_mapping_round_trips() {
        let dir = std::env::temp_dir().join(format!("stassh-local-test-{}", uuid::Uuid::new_v4()));
        let path = dir.join(".stassh-local.json");
        let mut config = LocalConfig::new();
        config
            .map_identity(
                "SHA256:abc".to_string(),
                PathBuf::from("/home/alice/.ssh/acme"),
                Some("acme".to_string()),
            )
            .unwrap();

        save_local_config(&path, &config).unwrap();
        let loaded = load_local_config(&path).unwrap();

        assert_eq!(
            loaded.identity_path("SHA256:abc"),
            Some(Path::new("/home/alice/.ssh/acme"))
        );
        assert_eq!(
            loaded.identity_mappings[0].preferred_name.as_deref(),
            Some("acme")
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_local_config_loads_as_empty() {
        let path = std::env::temp_dir()
            .join(format!("stassh-missing-local-{}", uuid::Uuid::new_v4()))
            .join(".stassh-local.json");

        let loaded = load_local_config(path).unwrap();

        assert!(loaded.identity_mappings.is_empty());
    }
}
