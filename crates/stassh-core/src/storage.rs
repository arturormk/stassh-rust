use std::fs;
use std::io;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::model::{StasshError, Vault};

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
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
    #[error("failed to encode vault: {0}")]
    Encode(serde_json::Error),
    #[error(transparent)]
    Model(#[from] StasshError),
}

pub fn load_vault(path: impl AsRef<Path>) -> Result<Vault, StorageError> {
    let path = path.as_ref();
    let bytes = fs::read(path).map_err(|source| StorageError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let vault: Vault = serde_json::from_slice(&bytes).map_err(|source| StorageError::Parse {
        path: path.to_path_buf(),
        source,
    })?;
    vault.validate()?;
    Ok(vault)
}

pub fn save_vault(path: impl AsRef<Path>, vault: &Vault) -> Result<(), StorageError> {
    vault.validate()?;
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| StorageError::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let bytes = serde_json::to_vec_pretty(vault).map_err(StorageError::Encode)?;
    let tmp_path = temporary_path(path);
    fs::write(&tmp_path, bytes).map_err(|source| StorageError::Write {
        path: tmp_path.clone(),
        source,
    })?;
    set_private_file_permissions(&tmp_path).map_err(|source| StorageError::Write {
        path: tmp_path.clone(),
        source,
    })?;
    fs::rename(&tmp_path, path).map_err(|source| StorageError::Write {
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
    use std::fs;

    use crate::model::{AddHost, Vault};

    use super::*;

    #[test]
    fn vault_round_trips() {
        let dir = std::env::temp_dir().join(format!("stassh-test-{}", uuid::Uuid::new_v4()));
        let path = dir.join("vault.json");
        let mut vault = Vault::new();
        vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "web".to_string(),
                hostname: "web.example".to_string(),
                port: None,
                username: Some("deploy".to_string()),
                identity_fingerprint: None,
                jump_chain: Vec::new(),
                ssh_options: Vec::new(),
                forwards: Vec::new(),
                tags: Vec::new(),
                notes: None,
            })
            .unwrap();

        save_vault(&path, &vault).unwrap();
        let loaded = load_vault(&path).unwrap();

        assert_eq!(loaded.hosts.len(), 1);
        assert_eq!(loaded.hosts[0].display_name, "web");

        fs::remove_dir_all(dir).unwrap();
    }
}
