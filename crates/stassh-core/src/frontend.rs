use std::env;
#[cfg(unix)]
use std::fs;
use std::io;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::local::LocalConfig;
use crate::model::{HostSelector, ResolvedHost};
use crate::openssh::{
    OpenSshCommand, TempOpenSshConfig, command_for_host, command_for_host_with_identity_path,
    config_execution_required, config_for_host_with_identity_path,
};

pub fn vault_path(path: Option<PathBuf>) -> io::Result<PathBuf> {
    if let Some(path) = path {
        return Ok(path);
    }
    if let Ok(path) = env::var("STASSH_VAULT") {
        return Ok(PathBuf::from(path));
    }
    let current_dir_path = env::current_dir()?.join("vault.json");
    let Some(home_path) = home_stassh_path().map(|path| path.join("vault.json")) else {
        return Ok(current_dir_path);
    };
    if home_path.exists() {
        return Ok(home_path);
    }
    if current_dir_path.exists() {
        return Ok(current_dir_path);
    }
    Ok(home_path)
}

pub fn local_config_path(path: Option<PathBuf>, vault_path: &Path) -> PathBuf {
    if let Some(path) = path {
        return path;
    }
    if let Ok(path) = env::var("STASSH_LOCAL_CONFIG") {
        return PathBuf::from(path);
    }
    let adjacent_path = vault_adjacent_local_config_path(vault_path);
    let Some(home_path) = home_stassh_path().map(|path| path.join("local.json")) else {
        return adjacent_path;
    };
    if home_stassh_path()
        .map(|path| path.join("vault.json") == vault_path)
        .unwrap_or(false)
    {
        home_path
    } else {
        adjacent_path
    }
}

pub fn secrets_path(path: Option<PathBuf>, vault_path: &Path) -> PathBuf {
    if let Some(path) = path {
        return path;
    }
    if let Ok(path) = env::var("STASSH_SECRETS") {
        return PathBuf::from(path);
    }
    let parent = vault_path.parent().unwrap_or_else(|| Path::new("."));
    let adjacent_path = parent.join("secrets.json");
    let Some(home_path) = home_stassh_path().map(|path| path.join("secrets.json")) else {
        return adjacent_path;
    };
    if home_stassh_path()
        .map(|path| path.join("vault.json") == vault_path)
        .unwrap_or(false)
    {
        home_path
    } else {
        adjacent_path
    }
}

pub fn vault_adjacent_local_config_path(vault_path: &Path) -> PathBuf {
    let parent = vault_path.parent().unwrap_or_else(|| Path::new("."));
    parent.join(".stassh-local.json")
}

pub fn ensure_home_stassh_permissions(paths: &[&Path]) -> io::Result<()> {
    let Some(home_path) = home_stassh_path() else {
        return Ok(());
    };
    if !paths.iter().any(|path| path.starts_with(&home_path)) {
        return Ok(());
    }
    ensure_home_stassh_directory(&home_path)?;
    for path in paths {
        if path.starts_with(&home_path) {
            ensure_home_stassh_file(path)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn ensure_home_stassh_directory(path: &Path) -> io::Result<()> {
    if !path.exists() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)?;
    }
    let metadata = fs::metadata(path)?;
    if !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} must be a directory", path.display()),
        ));
    }
    let mode = metadata.permissions().mode() & 0o777;
    if mode != 0o700 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{} has permissions {:03o}; expected 700",
                path.display(),
                mode
            ),
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_home_stassh_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn ensure_home_stassh_file(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} must be a file", path.display()),
        ));
    }
    let mode = metadata.permissions().mode() & 0o777;
    if mode != 0o600 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{} has permissions {:03o}; expected 600",
                path.display(),
                mode
            ),
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_home_stassh_file(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn home_stassh_path() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".ssh").join("stassh"))
}

pub fn selector(value: &str) -> HostSelector<'_> {
    Uuid::parse_str(value)
        .map(HostSelector::Id)
        .unwrap_or(HostSelector::Query(value))
}

pub fn prepare_openssh_command(
    host: &ResolvedHost,
    local_config: &LocalConfig,
) -> io::Result<(OpenSshCommand, Option<TempOpenSshConfig>)> {
    if config_execution_required(host) {
        let identity_path = host
            .identity_fingerprint
            .as_deref()
            .and_then(|fingerprint| local_config.identity_path(fingerprint));
        let config = config_for_host_with_identity_path(host, identity_path);
        let temp_config = TempOpenSshConfig::write(&config)?;
        let command = temp_config.command();
        Ok((command, Some(temp_config)))
    } else {
        Ok((command_for_host(host), None))
    }
}

pub fn standalone_openssh_command(
    host: &ResolvedHost,
    local_config: &LocalConfig,
) -> OpenSshCommand {
    let identity_path = host
        .identity_fingerprint
        .as_deref()
        .and_then(|fingerprint| local_config.identity_path(fingerprint));
    command_for_host_with_identity_path(host, identity_path)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use uuid::Uuid;

    use crate::local::LocalConfig;
    use crate::model::ResolvedHost;

    use super::*;

    #[test]
    fn standalone_command_uses_local_identity_mapping() {
        let host = ResolvedHost {
            id: Uuid::new_v4(),
            path: "web".to_string(),
            display_name: "web".to_string(),
            hostname: "web.example".to_string(),
            port: 2222,
            username: Some("deploy".to_string()),
            identity_fingerprint: Some("SHA256:deploy".to_string()),
            secrets: None,
            jump_chain: Vec::new(),
            ssh_options: Vec::new(),
            forwards: Vec::new(),
            actions: Vec::new(),
            tags: Vec::new(),
            notes: None,
        };
        let mut local_config = LocalConfig::new();
        local_config
            .map_identity(
                "SHA256:deploy".to_string(),
                PathBuf::from("/home/alice/.ssh/deploy key"),
                None,
            )
            .unwrap();

        let command = standalone_openssh_command(&host, &local_config).render_for_display();

        assert_eq!(
            command,
            "ssh -p 2222 -l deploy -i '/home/alice/.ssh/deploy key' -o IdentitiesOnly=yes web.example"
        );
    }
}
