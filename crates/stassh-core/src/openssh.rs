use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::model::{ForwardDefinition, ResolvedHost};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSshCommand {
    pub program: OsString,
    pub args: Vec<OsString>,
}

impl OpenSshCommand {
    pub fn render_for_display(&self) -> String {
        self.render_for_shell()
    }

    pub fn render_for_shell(&self) -> String {
        let mut parts = vec![shell_quote(&self.program.to_string_lossy())];
        parts.extend(
            self.args
                .iter()
                .map(|arg| shell_quote(&arg.to_string_lossy())),
        );
        parts.join(" ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSshConfig {
    pub alias: String,
    pub contents: String,
}

#[derive(Debug)]
pub struct TempOpenSshConfig {
    path: PathBuf,
    alias: String,
}

impl TempOpenSshConfig {
    pub fn write(config: &OpenSshConfig) -> io::Result<Self> {
        Self::write_in(std::env::temp_dir(), config)
    }

    pub fn write_in(dir: impl AsRef<Path>, config: &OpenSshConfig) -> io::Result<Self> {
        let path = dir
            .as_ref()
            .join(format!("stassh-{}.ssh_config", Uuid::new_v4().simple()));
        fs::write(&path, &config.contents)?;
        Ok(Self {
            path,
            alias: config.alias.clone(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn alias(&self) -> &str {
        &self.alias
    }

    pub fn command(&self) -> OpenSshCommand {
        command_for_config(self.path(), self.alias())
    }
}

impl Drop for TempOpenSshConfig {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn command_for_host(host: &ResolvedHost) -> OpenSshCommand {
    let mut args = Vec::new();
    args.push("-p".into());
    args.push(host.port.to_string().into());

    if let Some(username) = &host.username {
        args.push("-l".into());
        args.push(username.into());
    }

    if !host.jump_chain.is_empty() {
        args.push("-J".into());
        args.push(proxy_jump_arg(host).into());
    }

    for forward in &host.forwards {
        let (flag, spec) = forward_arg(forward);
        args.push(flag.into());
        args.push(spec.into());
    }

    for option in &host.ssh_options {
        args.push("-o".into());
        args.push(option.into());
    }

    args.push(host.hostname.as_str().into());

    OpenSshCommand {
        program: "ssh".into(),
        args,
    }
}

pub fn config_execution_required(host: &ResolvedHost) -> bool {
    !host.jump_chain.is_empty()
        || !host.forwards.is_empty()
        || !host.ssh_options.is_empty()
        || host.identity_fingerprint.is_some()
}

pub fn command_for_config(config_path: impl AsRef<Path>, alias: &str) -> OpenSshCommand {
    OpenSshCommand {
        program: "ssh".into(),
        args: vec!["-F".into(), config_path.as_ref().into(), alias.into()],
    }
}

pub fn config_for_host(host: &ResolvedHost) -> OpenSshConfig {
    config_for_host_with_identity_path(host, None)
}

pub fn config_for_host_with_identity_path(
    host: &ResolvedHost,
    identity_path: Option<&Path>,
) -> OpenSshConfig {
    let alias = format!("stassh-{}", host.id.simple());
    let mut contents = String::new();

    for jump in &host.jump_chain {
        let jump_alias = format!("stassh-{}", jump.id.simple());
        contents.push_str(&format!("Host {jump_alias}\n"));
        contents.push_str(&format!("    HostName {}\n", jump.hostname));
        contents.push_str(&format!("    Port {}\n", jump.port));
        if let Some(username) = &jump.username {
            contents.push_str(&format!("    User {username}\n"));
        }
        contents.push('\n');
    }

    contents.push_str(&format!("Host {alias}\n"));
    contents.push_str(&format!("    HostName {}\n", host.hostname));
    contents.push_str(&format!("    Port {}\n", host.port));
    if let Some(username) = &host.username {
        contents.push_str(&format!("    User {username}\n"));
    }
    if let Some(identity_path) = identity_path {
        contents.push_str(&format!("    IdentityFile {}\n", identity_path.display()));
        contents.push_str("    IdentitiesOnly yes\n");
    }
    if !host.jump_chain.is_empty() {
        let jump_aliases = host
            .jump_chain
            .iter()
            .map(|jump| format!("stassh-{}", jump.id.simple()))
            .collect::<Vec<_>>()
            .join(",");
        contents.push_str(&format!("    ProxyJump {jump_aliases}\n"));
    }
    for forward in &host.forwards {
        contents.push_str(&format!("    {}\n", forward_config_line(forward)));
    }
    for option in &host.ssh_options {
        contents.push_str(&format!("    {}\n", option));
    }

    OpenSshConfig { alias, contents }
}

pub fn forward_arg(forward: &ForwardDefinition) -> (&'static str, String) {
    match forward {
        ForwardDefinition::Local {
            bind_address,
            local_port,
            destination_host,
            destination_port,
        } => (
            "-L",
            format!("{bind_address}:{local_port}:{destination_host}:{destination_port}"),
        ),
        ForwardDefinition::Remote {
            bind_address,
            remote_port,
            destination_host,
            destination_port,
        } => (
            "-R",
            format!("{bind_address}:{remote_port}:{destination_host}:{destination_port}"),
        ),
        ForwardDefinition::Dynamic {
            bind_address,
            local_port,
        } => ("-D", format!("{bind_address}:{local_port}")),
    }
}

pub fn forward_config_line(forward: &ForwardDefinition) -> String {
    match forward {
        ForwardDefinition::Local {
            bind_address,
            local_port,
            destination_host,
            destination_port,
        } => format!(
            "LocalForward {bind_address}:{local_port} {destination_host}:{destination_port}"
        ),
        ForwardDefinition::Remote {
            bind_address,
            remote_port,
            destination_host,
            destination_port,
        } => {
            format!(
                "RemoteForward {bind_address}:{remote_port} {destination_host}:{destination_port}"
            )
        }
        ForwardDefinition::Dynamic {
            bind_address,
            local_port,
        } => format!("DynamicForward {bind_address}:{local_port}"),
    }
}

fn proxy_jump_arg(host: &ResolvedHost) -> String {
    host.jump_chain
        .iter()
        .map(|jump| {
            let destination = if let Some(username) = &jump.username {
                format!("{username}@{}", jump.hostname)
            } else {
                jump.hostname.clone()
            };
            if jump.port == 22 {
                destination
            } else {
                format!("{destination}:{}", jump.port)
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn shell_quote(value: &str) -> String {
    if value.chars().all(|c| {
        c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '=' | '@' | ',')
    }) {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::model::{ResolvedHost, ResolvedJump};

    use super::*;

    #[test]
    fn command_includes_user_port_and_jump_chain() {
        let host = ResolvedHost {
            id: Uuid::new_v4(),
            path: "db".to_string(),
            display_name: "db".to_string(),
            hostname: "10.0.0.5".to_string(),
            port: 2222,
            username: Some("root".to_string()),
            identity_fingerprint: None,
            jump_chain: vec![ResolvedJump {
                id: Uuid::new_v4(),
                display_name: "bastion".to_string(),
                hostname: "bastion.example".to_string(),
                port: 22,
                username: Some("admin".to_string()),
            }],
            ssh_options: vec!["ServerAliveInterval=30".to_string()],
            forwards: Vec::new(),
            actions: Vec::new(),
            tags: Vec::new(),
            notes: None,
        };

        let command = command_for_host(&host).render_for_display();
        assert!(command.contains("-p 2222"));
        assert!(command.contains("-l root"));
        assert!(command.contains("-J admin@bastion.example"));
        assert!(command.contains("-o ServerAliveInterval=30"));
    }

    #[test]
    fn shell_rendering_quotes_special_characters() {
        let command = OpenSshCommand {
            program: "ssh".into(),
            args: vec![
                "-o".into(),
                "ProxyCommand ssh jump.example -W %h:%p".into(),
                "host with spaces.example".into(),
                "quoted'arg".into(),
            ],
        };

        assert_eq!(
            command.render_for_shell(),
            "ssh -o 'ProxyCommand ssh jump.example -W %h:%p' 'host with spaces.example' 'quoted'\\''arg'"
        );
    }

    #[test]
    fn command_and_config_include_forwards() {
        let host = ResolvedHost {
            id: Uuid::new_v4(),
            path: "web".to_string(),
            display_name: "web".to_string(),
            hostname: "web.example".to_string(),
            port: 22,
            username: None,
            identity_fingerprint: None,
            jump_chain: Vec::new(),
            ssh_options: Vec::new(),
            forwards: vec![
                ForwardDefinition::Local {
                    bind_address: "127.0.0.1".to_string(),
                    local_port: 8080,
                    destination_host: "10.0.0.7".to_string(),
                    destination_port: 80,
                },
                ForwardDefinition::Dynamic {
                    bind_address: "127.0.0.1".to_string(),
                    local_port: 1080,
                },
            ],
            actions: Vec::new(),
            tags: Vec::new(),
            notes: None,
        };

        let command = command_for_host(&host).render_for_display();
        let config = config_for_host(&host).contents;

        assert!(command.contains("-L 127.0.0.1:8080:10.0.0.7:80"));
        assert!(command.contains("-D 127.0.0.1:1080"));
        assert!(config.contains("LocalForward 127.0.0.1:8080 10.0.0.7:80"));
        assert!(config.contains("DynamicForward 127.0.0.1:1080"));
    }

    #[test]
    fn config_includes_mapped_identity_file() {
        let host = ResolvedHost {
            id: Uuid::new_v4(),
            path: "web".to_string(),
            display_name: "web".to_string(),
            hostname: "web.example".to_string(),
            port: 22,
            username: None,
            identity_fingerprint: None,
            jump_chain: Vec::new(),
            ssh_options: Vec::new(),
            forwards: Vec::new(),
            actions: Vec::new(),
            tags: Vec::new(),
            notes: None,
        };

        let config =
            config_for_host_with_identity_path(&host, Some(Path::new("/home/alice/.ssh/acme")))
                .contents;

        assert!(config.contains("    IdentityFile /home/alice/.ssh/acme\n"));
        assert!(config.contains("    IdentitiesOnly yes\n"));
    }

    #[test]
    fn config_command_uses_temp_config_path_and_alias() {
        let path = PathBuf::from("/tmp/stassh-test-config");
        let command = command_for_config(&path, "stassh-example").render_for_display();

        assert_eq!(command, "ssh -F /tmp/stassh-test-config stassh-example");
    }

    #[test]
    fn temp_config_is_removed_on_drop() {
        let dir = std::env::temp_dir().join(format!("stassh-openssh-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let config = OpenSshConfig {
            alias: format!("stassh-{}", Uuid::new_v4().simple()),
            contents: "Host stassh-test\n    HostName example.test\n".to_string(),
        };
        let path;

        {
            let temp_config = TempOpenSshConfig::write_in(&dir, &config).unwrap();
            path = temp_config.path().to_path_buf();
            assert!(path.exists());
            assert_eq!(fs::read_to_string(&path).unwrap(), config.contents);
        }

        assert!(!path.exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
