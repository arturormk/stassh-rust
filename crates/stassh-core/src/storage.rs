use std::fs;
use std::io::{self, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use tempfile::NamedTempFile;

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
    #[error("{path} changed on disk; reload before saving to avoid overwriting external edits")]
    ChangedOnDisk { path: PathBuf },
    #[error(transparent)]
    Model(#[from] StasshError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStamp {
    len: u64,
    modified: Option<SystemTime>,
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

pub fn file_stamp(path: impl AsRef<Path>) -> Result<Option<FileStamp>, StorageError> {
    let path = path.as_ref();
    match fs::metadata(path) {
        Ok(metadata) => Ok(Some(FileStamp {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        })),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(StorageError::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}

pub fn ensure_file_unchanged(
    path: impl AsRef<Path>,
    expected: &Option<FileStamp>,
) -> Result<(), StorageError> {
    let path = path.as_ref();
    let current = file_stamp(path)?;
    if &current == expected {
        Ok(())
    } else {
        Err(StorageError::ChangedOnDisk {
            path: path.to_path_buf(),
        })
    }
}

pub fn save_vault(path: impl AsRef<Path>, vault: &Vault) -> Result<(), StorageError> {
    vault.validate()?;
    save_json_private(path, vault)
}

pub fn save_json_private<T: serde::Serialize>(
    path: impl AsRef<Path>,
    value: &T,
) -> Result<(), StorageError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| StorageError::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let bytes = serde_json::to_vec_pretty(value).map_err(StorageError::Encode)?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = NamedTempFile::new_in(parent).map_err(|source| StorageError::Write {
        path: parent.to_path_buf(),
        source,
    })?;
    set_private_file_permissions(tmp.path()).map_err(|source| StorageError::Write {
        path: tmp.path().to_path_buf(),
        source,
    })?;
    tmp.write_all(&bytes)
        .map_err(|source| StorageError::Write {
            path: tmp.path().to_path_buf(),
            source,
        })?;
    tmp.flush().map_err(|source| StorageError::Write {
        path: tmp.path().to_path_buf(),
        source,
    })?;
    tmp.as_file()
        .sync_all()
        .map_err(|source| StorageError::Write {
            path: tmp.path().to_path_buf(),
            source,
        })?;
    tmp.persist(path).map_err(|error| StorageError::Write {
        path: path.to_path_buf(),
        source: error.error,
    })?;
    sync_parent_directory(parent)?;
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

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> Result<(), StorageError> {
    let directory = fs::File::open(path).map_err(|source| StorageError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    directory.sync_all().map_err(|source| StorageError::Write {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> Result<(), StorageError> {
    Ok(())
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
                secrets: None,
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

    #[test]
    fn file_stamp_detects_external_change() {
        let dir = std::env::temp_dir().join(format!("stassh-stamp-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("vault.json");
        fs::write(&path, br#"{"format_version":0,"folders":[],"hosts":[]}"#).unwrap();
        let stamp = file_stamp(&path).unwrap();

        fs::write(
            &path,
            br#"{"format_version":0,"folders":[],"hosts":[],"actions":[]}"#,
        )
        .unwrap();

        assert!(matches!(
            ensure_file_unchanged(&path, &stamp),
            Err(StorageError::ChangedOnDisk { .. })
        ));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn file_stamp_allows_unchanged_file() {
        let dir = std::env::temp_dir().join(format!("stassh-stamp-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("vault.json");
        fs::write(&path, br#"{"format_version":0,"folders":[],"hosts":[]}"#).unwrap();
        let stamp = file_stamp(&path).unwrap();

        ensure_file_unchanged(&path, &stamp).unwrap();

        fs::remove_dir_all(dir).unwrap();
    }
}
