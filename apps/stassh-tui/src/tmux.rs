use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use stassh_core::LocalConfig;
use stassh_core::model::ResolvedHost;
use stassh_core::openssh::{
    command_for_config, command_for_host, config_execution_required,
    config_for_host_with_identity_path,
};
use uuid::Uuid;

pub const STALE_CONFIG_AGE: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxWindowCommand {
    pub title: String,
    pub shell_command: String,
    pub config_path: Option<PathBuf>,
}

pub fn is_inside_tmux() -> bool {
    env::var_os("TMUX").is_some()
}

pub fn default_temp_config_dir() -> PathBuf {
    let owner = env::var("UID")
        .or_else(|_| env::var("USER"))
        .unwrap_or_else(|_| std::process::id().to_string());
    env::temp_dir().join(format!("stassh-tui-{owner}"))
}

pub fn prepare_window_command(
    host: &ResolvedHost,
    local_config: &LocalConfig,
    temp_dir: &Path,
) -> io::Result<TmuxWindowCommand> {
    let (command, config_path) = if config_execution_required(host) {
        let identity_path = host
            .identity_fingerprint
            .as_deref()
            .and_then(|fingerprint| local_config.identity_path(fingerprint));
        let config = config_for_host_with_identity_path(host, identity_path);
        let config_path = write_persistent_config(temp_dir, &config.contents)?;
        (
            command_for_config(&config_path, &config.alias),
            Some(config_path),
        )
    } else {
        (command_for_host(host), None)
    };

    Ok(TmuxWindowCommand {
        title: sanitize_window_title(&host.path),
        shell_command: command.render_for_shell(),
        config_path,
    })
}

pub fn launch_window(command: &TmuxWindowCommand) -> io::Result<std::process::ExitStatus> {
    Command::new("tmux")
        .args(["new-window", "-n", &command.title, &command.shell_command])
        .status()
}

pub fn sanitize_window_title(value: &str) -> String {
    let mut title = value
        .chars()
        .map(|character| match character {
            ':' | '#' | '"' | '\'' | '\n' | '\r' | '\t' => '-',
            character if character.is_control() => '-',
            character => character,
        })
        .collect::<String>();
    title = title.trim_matches('-').trim().to_string();
    if title.is_empty() {
        return "stassh".to_string();
    }
    title.chars().take(48).collect()
}

pub fn cleanup_stale_temp_configs(temp_dir: &Path, max_age: Duration) -> io::Result<usize> {
    let Ok(entries) = fs::read_dir(temp_dir) else {
        return Ok(0);
    };
    let now = SystemTime::now();
    let mut removed = 0;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !is_stassh_config_path(&path) {
            continue;
        }
        let metadata = entry.metadata()?;
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if now
            .duration_since(modified)
            .map(|age| age >= max_age)
            .unwrap_or(false)
        {
            fs::remove_file(&path)?;
            removed += 1;
        }
    }
    Ok(removed)
}

fn write_persistent_config(temp_dir: &Path, contents: &str) -> io::Result<PathBuf> {
    fs::create_dir_all(temp_dir)?;
    let path = temp_dir.join(format!("stassh-tui-{}.ssh_config", Uuid::new_v4().simple()));
    fs::write(&path, contents)?;
    Ok(path)
}

fn is_stassh_config_path(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    name.starts_with("stassh-tui-") && name.ends_with(".ssh_config")
}

#[cfg(test)]
mod tests {
    use stassh_core::ResolvedHost;

    use super::*;

    fn resolved_host(path: &str) -> ResolvedHost {
        ResolvedHost {
            id: Uuid::new_v4(),
            path: path.to_string(),
            display_name: "web".to_string(),
            hostname: "web.example".to_string(),
            port: 22,
            username: Some("deploy".to_string()),
            identity_fingerprint: None,
            jump_chain: Vec::new(),
            ssh_options: Vec::new(),
            forwards: Vec::new(),
            actions: Vec::new(),
            tags: Vec::new(),
            notes: None,
        }
    }

    #[test]
    fn sanitizes_window_title() {
        assert_eq!(
            sanitize_window_title("Customers/Acme:prod#web\t01"),
            "Customers/Acme-prod-web-01"
        );
        assert_eq!(sanitize_window_title("::"), "stassh");
        assert_eq!(sanitize_window_title(&"a".repeat(80)).len(), 48);
    }

    #[test]
    fn builds_simple_tmux_window_command() {
        let command = prepare_window_command(
            &resolved_host("Customers/web"),
            &LocalConfig::default(),
            &env::temp_dir(),
        )
        .unwrap();

        assert_eq!(command.title, "Customers/web");
        assert_eq!(command.shell_command, "ssh -p 22 -l deploy web.example");
        assert!(command.config_path.is_none());
    }

    #[test]
    fn builds_config_backed_tmux_window_command() {
        let temp_dir = env::temp_dir().join(format!("stassh-tui-test-{}", Uuid::new_v4()));
        let mut host = resolved_host("web");
        host.identity_fingerprint = Some("SHA256:test".to_string());

        let command = prepare_window_command(&host, &LocalConfig::default(), &temp_dir).unwrap();

        assert!(command.shell_command.starts_with("ssh -F "));
        assert!(command.config_path.as_ref().unwrap().exists());
        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn cleanup_removes_only_stale_stassh_configs() {
        let temp_dir = env::temp_dir().join(format!("stassh-tui-cleanup-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let stale = temp_dir.join("stassh-tui-stale.ssh_config");
        let fresh = temp_dir.join("stassh-tui-fresh.ssh_config");
        let unrelated = temp_dir.join("other.ssh_config");
        fs::write(&stale, "").unwrap();
        fs::write(&fresh, "").unwrap();
        fs::write(&unrelated, "").unwrap();

        let removed = cleanup_stale_temp_configs(&temp_dir, Duration::from_secs(0)).unwrap();

        assert_eq!(removed, 2);
        assert!(!stale.exists());
        assert!(!fresh.exists());
        assert!(unrelated.exists());
        fs::remove_dir_all(temp_dir).unwrap();
    }
}
