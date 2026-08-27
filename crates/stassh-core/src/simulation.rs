use std::collections::HashMap;
use std::path::PathBuf;

use uuid::Uuid;

use crate::local::LocalConfig;
use crate::model::{
    ActionDefinition, ActionForwardDefinition, ActionLocalCommand, ActionPort, Folder,
    ForwardDefinition, Host, ResolvedHost, Vault,
};
use crate::secrets::{SecretsError, SecretsStore};

pub const SIMULATION_MASTER_PASSWORD: &str = "simulation";

#[derive(Debug, Clone)]
pub struct SimulationWorkspace {
    pub vault: Vault,
    pub local_config: LocalConfig,
    pub secrets_store: SecretsStore,
}

pub fn demo_workspace() -> Result<SimulationWorkspace, SecretsError> {
    let ids = DemoIds::default();
    let vault = Vault {
        format_version: crate::model::CURRENT_FORMAT_VERSION,
        actions: demo_actions(),
        folders: vec![
            Folder {
                id: ids.root,
                parent_id: None,
                name: "Root".to_string(),
            },
            Folder {
                id: ids.edge,
                parent_id: Some(ids.root),
                name: "Edge".to_string(),
            },
            Folder {
                id: ids.prod,
                parent_id: Some(ids.root),
                name: "Production".to_string(),
            },
            Folder {
                id: ids.staging,
                parent_id: Some(ids.root),
                name: "Staging".to_string(),
            },
            Folder {
                id: ids.shared,
                parent_id: Some(ids.root),
                name: "Shared Services".to_string(),
            },
        ],
        hosts: vec![
            Host {
                id: ids.bastion,
                folder_id: ids.edge,
                display_name: "bastion-01".to_string(),
                hostname: "bastion.corp.example".to_string(),
                port: 22,
                username: Some("ops".to_string()),
                identity_fingerprint: Some("SHA256:sim-ops".to_string()),
                secrets: Some("edge-admin".to_string()),
                jump_chain: Vec::new(),
                ssh_options: vec!["ServerAliveInterval 30".to_string()],
                forwards: Vec::new(),
                actions: Vec::new(),
                tags: vec!["edge".to_string(), "jump".to_string()],
                notes: Some("Primary jump host for corporate network access.".to_string()),
            },
            Host {
                id: ids.web,
                folder_id: ids.prod,
                display_name: "web-prod-01".to_string(),
                hostname: "web01.prod.corp.example".to_string(),
                port: 22,
                username: Some("deploy".to_string()),
                identity_fingerprint: Some("SHA256:sim-deploy".to_string()),
                secrets: Some("web-prod".to_string()),
                jump_chain: vec![ids.bastion],
                ssh_options: Vec::new(),
                forwards: vec![ForwardDefinition::Local {
                    bind_address: "127.0.0.1".to_string(),
                    local_port: 8443,
                    destination_host: "127.0.0.1".to_string(),
                    destination_port: 443,
                }],
                actions: vec![ActionDefinition {
                    id: uuid(0x41),
                    name: "Tail nginx errors".to_string(),
                    local_prepare: None,
                    forwards: Vec::new(),
                    remote_command: Some("tail -n 80 /var/log/nginx/error.log".to_string()),
                    local_launch: None,
                    cleanup: Vec::new(),
                }],
                tags: vec!["prod".to_string(), "web".to_string(), "http".to_string()],
                notes: Some(
                    "Blue pool frontend. Use action palette for common checks.".to_string(),
                ),
            },
            Host {
                id: ids.db,
                folder_id: ids.prod,
                display_name: "db-prod-01".to_string(),
                hostname: "db01.prod.corp.example".to_string(),
                port: 2222,
                username: Some("dba".to_string()),
                identity_fingerprint: Some("SHA256:sim-missing".to_string()),
                secrets: Some("database-prod".to_string()),
                jump_chain: vec![ids.bastion],
                ssh_options: vec!["Compression yes".to_string()],
                forwards: Vec::new(),
                actions: Vec::new(),
                tags: vec!["prod".to_string(), "database".to_string()],
                notes: Some(
                    "Intentionally uses an unmapped identity to exercise diagnostics.".to_string(),
                ),
            },
            Host {
                id: ids.cache,
                folder_id: ids.prod,
                display_name: "cache-prod-01".to_string(),
                hostname: "cache01.prod.corp.example".to_string(),
                port: 22,
                username: Some("ops".to_string()),
                identity_fingerprint: Some("SHA256:sim-ops".to_string()),
                secrets: None,
                jump_chain: vec![ids.bastion],
                ssh_options: Vec::new(),
                forwards: Vec::new(),
                actions: Vec::new(),
                tags: vec!["prod".to_string(), "cache".to_string()],
                notes: Some("Redis cache node used by the web tier.".to_string()),
            },
            Host {
                id: ids.staging_web,
                folder_id: ids.staging,
                display_name: "web-staging-01".to_string(),
                hostname: "web01.staging.corp.example".to_string(),
                port: 22,
                username: Some("deploy".to_string()),
                identity_fingerprint: Some("SHA256:sim-deploy".to_string()),
                secrets: Some("web-staging".to_string()),
                jump_chain: vec![ids.bastion],
                ssh_options: Vec::new(),
                forwards: Vec::new(),
                actions: Vec::new(),
                tags: vec!["staging".to_string(), "web".to_string()],
                notes: Some("Safe target for screenshots and workflow demos.".to_string()),
            },
            Host {
                id: ids.metrics,
                folder_id: ids.shared,
                display_name: "metrics-01".to_string(),
                hostname: "metrics.shared.corp.example".to_string(),
                port: 22,
                username: Some("observer".to_string()),
                identity_fingerprint: None,
                secrets: None,
                jump_chain: Vec::new(),
                ssh_options: Vec::new(),
                forwards: vec![ForwardDefinition::Dynamic {
                    bind_address: "127.0.0.1".to_string(),
                    local_port: 1080,
                }],
                actions: Vec::new(),
                tags: vec!["shared".to_string(), "metrics".to_string()],
                notes: Some(
                    "Shared monitoring entry point with a SOCKS tunnel example.".to_string(),
                ),
            },
        ],
    };
    vault
        .validate()
        .map_err(|error| SecretsError::InvalidValue {
            field: "simulation.vault",
            reason: error.to_string(),
        })?;

    let mut local_config = LocalConfig::new();
    local_config
        .map_identity(
            "SHA256:sim-ops".to_string(),
            PathBuf::from("simulation://keys/ops_ed25519"),
            Some("ops simulation key".to_string()),
        )
        .map_err(|error| SecretsError::InvalidValue {
            field: "simulation.local_config",
            reason: error.to_string(),
        })?;
    local_config
        .map_identity(
            "SHA256:sim-deploy".to_string(),
            PathBuf::from("simulation://keys/deploy_ed25519"),
            Some("deploy simulation key".to_string()),
        )
        .map_err(|error| SecretsError::InvalidValue {
            field: "simulation.local_config",
            reason: error.to_string(),
        })?;
    local_config
        .map_capability(
            "browser".to_string(),
            PathBuf::from("simulation://bin/browser"),
        )
        .map_err(|error| SecretsError::InvalidValue {
            field: "simulation.local_config",
            reason: error.to_string(),
        })?;

    Ok(SimulationWorkspace {
        vault,
        local_config,
        secrets_store: demo_secrets()?,
    })
}

#[derive(Debug, Clone)]
pub struct SimulatedShell {
    host_path: String,
    hostname: String,
    username: String,
    cwd: String,
    line: String,
    closed: bool,
    files: HashMap<&'static str, &'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulatedOutput {
    pub data: String,
    pub closed: bool,
}

impl SimulatedShell {
    pub fn for_host(host: &ResolvedHost) -> Self {
        let hostname = host.hostname.clone();
        let username = host.username.clone().unwrap_or_else(|| "user".to_string());
        let mut files = HashMap::new();
        files.insert(
            "/etc/motd",
            "Authorized demo environment for stassh simulation.\r\nNo real network connection is active.\r\n",
        );
        files.insert(
            "/var/log/nginx/error.log",
            "2026/08/27 09:41:02 [warn] upstream response time exceeded simulation threshold\r\n2026/08/27 09:42:18 [info] worker process recycled cleanly\r\n",
        );
        files.insert(
            "README.txt",
            "This is a simulated SSH session. Try: ls, pwd, cat /etc/motd, uptime, exit\r\n",
        );
        Self {
            host_path: host.path.clone(),
            hostname,
            username,
            cwd: "/home/demo".to_string(),
            line: String::new(),
            closed: false,
            files,
        }
    }

    pub fn banner(&self) -> String {
        format!(
            "Connecting to {} ({})...\r\nstassh simulation mode: no real SSH connection is active.\r\n{}\r\n{}",
            self.host_path,
            self.hostname,
            self.files["/etc/motd"],
            self.prompt()
        )
    }

    pub fn handle_input(&mut self, data: &str) -> SimulatedOutput {
        if self.closed {
            return SimulatedOutput {
                data: String::new(),
                closed: true,
            };
        }

        let mut output = String::new();
        for ch in data.chars() {
            match ch {
                '\r' | '\n' => {
                    output.push_str("\r\n");
                    let line = self.line.trim().to_string();
                    self.line.clear();
                    let result = self.run_command(&line);
                    output.push_str(&result);
                    if self.closed {
                        break;
                    }
                    output.push_str(&self.prompt());
                }
                '\u{7f}' | '\u{8}' => {
                    if !self.line.is_empty() {
                        self.line.pop();
                        output.push_str("\u{8} \u{8}");
                    }
                }
                _ if ch.is_control() => {}
                _ => {
                    self.line.push(ch);
                    output.push(ch);
                }
            }
        }

        SimulatedOutput {
            data: output,
            closed: self.closed,
        }
    }

    pub fn close(&mut self) -> SimulatedOutput {
        if !self.closed {
            self.closed = true;
            return SimulatedOutput {
                data: format!("\r\nConnection to {} closed.\r\n", self.hostname),
                closed: true,
            };
        }
        SimulatedOutput {
            data: String::new(),
            closed: true,
        }
    }

    fn prompt(&self) -> String {
        format!("{}@{}:{}$ ", self.username, self.hostname, self.cwd)
    }

    fn run_command(&mut self, command: &str) -> String {
        let mut parts = command.split_whitespace();
        let Some(program) = parts.next() else {
            return String::new();
        };
        match program {
            "help" => {
                "available commands: help pwd ls cat hostname whoami uname uptime clear exit\r\n"
                    .to_string()
            }
            "pwd" => format!("{}\r\n", self.cwd),
            "ls" => "README.txt  deploy  logs  releases  tmp\r\n".to_string(),
            "cat" => {
                let Some(path) = parts.next() else {
                    return "cat: missing file operand\r\n".to_string();
                };
                self.files
                    .get(path)
                    .copied()
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("cat: {path}: No such file or directory\r\n"))
            }
            "hostname" => format!("{}\r\n", self.hostname),
            "whoami" => format!("{}\r\n", self.username),
            "uname" => "Linux stassh-sim 6.8.0-sim x86_64 GNU/Linux\r\n".to_string(),
            "uptime" => {
                " 11:28:45 up 42 days,  6:17,  3 users,  load average: 0.12, 0.09, 0.05\r\n"
                    .to_string()
            }
            "clear" => "\x1b[2J\x1b[H".to_string(),
            "exit" | "logout" => {
                self.closed = true;
                format!("logout\r\nConnection to {} closed.\r\n", self.hostname)
            }
            other => format!("{other}: command not found\r\n"),
        }
    }
}

fn demo_actions() -> Vec<ActionDefinition> {
    vec![
        ActionDefinition {
            id: uuid(0x31),
            name: "Open service dashboard".to_string(),
            local_prepare: None,
            forwards: vec![ActionForwardDefinition::Local {
                name: "dashboard".to_string(),
                bind_address: "127.0.0.1".to_string(),
                local_port: ActionPort::Fixed(8443),
                destination_host: "127.0.0.1".to_string(),
                destination_port: 443,
            }],
            remote_command: Some("echo dashboard tunnel ready".to_string()),
            local_launch: Some(ActionLocalCommand {
                capability: Some("browser".to_string()),
                program: None,
                args: vec!["https://127.0.0.1:8443/".to_string()],
                env: HashMap::new(),
            }),
            cleanup: Vec::new(),
        },
        ActionDefinition {
            id: uuid(0x32),
            name: "Disk summary".to_string(),
            local_prepare: None,
            forwards: Vec::new(),
            remote_command: Some("df -h".to_string()),
            local_launch: None,
            cleanup: Vec::new(),
        },
    ]
}

fn demo_secrets() -> Result<SecretsStore, SecretsError> {
    let (mut store, key) = SecretsStore::create(SIMULATION_MASTER_PASSWORD)?;
    store.create_set("edge-admin".to_string(), Some("Edge admin".to_string()))?;
    store.set_plain(
        "edge-admin",
        "owner".to_string(),
        "Network Operations".to_string(),
    )?;
    store.set_secret(
        &key,
        "edge-admin",
        "password".to_string(),
        "sim-edge-password",
    )?;
    store.create_set("web-prod".to_string(), Some("Production web".to_string()))?;
    store.set_plain("web-prod", "rotation".to_string(), "quarterly".to_string())?;
    store.set_secret(&key, "web-prod", "token".to_string(), "sim-web-prod-token")?;
    store.create_set("web-staging".to_string(), Some("Staging web".to_string()))?;
    store.set_secret(
        &key,
        "web-staging",
        "token".to_string(),
        "sim-web-staging-token",
    )?;
    store.create_set(
        "database-prod".to_string(),
        Some("Production database".to_string()),
    )?;
    store.set_secret(
        &key,
        "database-prod",
        "breakglass".to_string(),
        "sim-db-breakglass",
    )?;
    Ok(store)
}

#[derive(Debug, Clone, Copy)]
struct DemoIds {
    root: Uuid,
    edge: Uuid,
    prod: Uuid,
    staging: Uuid,
    shared: Uuid,
    bastion: Uuid,
    web: Uuid,
    db: Uuid,
    cache: Uuid,
    staging_web: Uuid,
    metrics: Uuid,
}

impl Default for DemoIds {
    fn default() -> Self {
        Self {
            root: uuid(0x01),
            edge: uuid(0x02),
            prod: uuid(0x03),
            staging: uuid(0x04),
            shared: uuid(0x05),
            bastion: uuid(0x11),
            web: uuid(0x12),
            db: uuid(0x13),
            cache: uuid(0x14),
            staging_web: uuid(0x15),
            metrics: uuid(0x16),
        }
    }
}

fn uuid(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::HostSelector;

    #[test]
    fn demo_workspace_validates_and_unlocks_secrets() {
        let workspace = demo_workspace().unwrap();
        workspace.vault.validate().unwrap();
        workspace.local_config.validate().unwrap();
        let key = workspace
            .secrets_store
            .unlock(SIMULATION_MASTER_PASSWORD)
            .unwrap();
        assert!(
            workspace
                .secrets_store
                .reveal(&key, "web-prod", "token")
                .unwrap()
                .expose_str()
                .unwrap()
                .contains("sim-web-prod")
        );
        assert!(workspace.secrets_store.unlock("wrong").is_err());
    }

    #[test]
    fn demo_host_ids_and_paths_are_stable() {
        let workspace = demo_workspace().unwrap();
        let host = workspace
            .vault
            .resolve_host(HostSelector::Query("Production/web-prod-01"))
            .unwrap();
        assert_eq!(host.id, Uuid::from_u128(0x12));
        assert_eq!(host.hostname, "web01.prod.corp.example");
    }

    #[test]
    fn shell_runs_common_commands_and_exits() {
        let workspace = demo_workspace().unwrap();
        let host = workspace
            .vault
            .resolve_host(HostSelector::Query("web-prod-01"))
            .unwrap();
        let mut shell = SimulatedShell::for_host(&host);
        assert!(shell.banner().contains("stassh simulation mode"));
        assert!(shell.handle_input("pwd\r").data.contains("/home/demo"));
        assert!(
            shell
                .handle_input("cat /etc/motd\r")
                .data
                .contains("demo environment")
        );
        let output = shell.handle_input("exit\r");
        assert!(output.closed);
        assert!(
            output
                .data
                .contains("Connection to web01.prod.corp.example closed")
        );
    }
}
