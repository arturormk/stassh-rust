use std::collections::HashMap;
use std::io;
use std::net::TcpListener;
use std::path::PathBuf;

use thiserror::Error;

use crate::frontend::prepare_openssh_command;
use crate::local::LocalConfig;
use crate::model::{
    ActionDefinition, ActionForwardDefinition, ActionLocalCommand, ActionPort, ForwardDefinition,
    ResolvedHost,
};
use crate::openssh::{OpenSshCommand, TempOpenSshConfig, config_for_host_with_identity_path};

#[derive(Debug, Error)]
pub enum ActionError {
    #[error("action not found: {0}")]
    ActionNotFound(String),
    #[error("action forward name is empty")]
    EmptyForwardName,
    #[error("duplicate action forward name: {0}")]
    DuplicateForwardName(String),
    #[error("capability is not mapped locally: {0}")]
    MissingCapability(String),
    #[error("local command must set either program or capability")]
    MissingLocalProgram,
    #[error("local command cannot set both program and capability")]
    AmbiguousLocalProgram,
    #[error("unknown template variable: {0}")]
    UnknownTemplateVariable(String),
    #[error("invalid port from {port_source}: {value}")]
    InvalidPort { port_source: String, value: String },
    #[error("failed to allocate local port: {0}")]
    AllocatePort(io::Error),
    #[error("failed to prepare ssh: {0}")]
    PrepareSsh(io::Error),
}

#[derive(Debug)]
pub struct ResolvedActionPlan {
    pub action_name: String,
    pub host: ResolvedHost,
    pub local_prepare: Option<ResolvedLocalCommand>,
    pub ssh_command: OpenSshCommand,
    pub temp_config: Option<TempOpenSshConfig>,
    pub local_launch: Option<ResolvedLocalCommand>,
    pub cleanup: Vec<ResolvedLocalCommand>,
    pub allocated_ports: HashMap<String, u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLocalCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

#[derive(Debug)]
pub struct ResolvedActionPrepare {
    pub command: ResolvedLocalCommand,
    pub temp_config: Option<TempOpenSshConfig>,
}

#[derive(Debug)]
struct SshTemplateContext {
    config_path: String,
    alias: String,
    rsh_command: String,
    command: String,
}

impl ResolvedActionPlan {
    pub fn allocated_port(&self, name: &str) -> Option<u16> {
        self.allocated_ports.get(name).copied()
    }
}

pub fn resolve_action_plan(
    host: &ResolvedHost,
    action: &ActionDefinition,
    local_config: &LocalConfig,
    prepare_env: &HashMap<String, String>,
) -> Result<ResolvedActionPlan, ActionError> {
    validate_action(action)?;
    let mut allocated_ports = HashMap::new();
    let mut forwards = host.forwards.clone();
    for forward in &action.forwards {
        forwards.push(resolve_action_forward(
            forward,
            &mut allocated_ports,
            prepare_env,
        )?);
    }

    let mut action_host = host.clone();
    action_host.forwards = forwards;
    let force_config = action_requires_ssh_template(action, ActionPhase::Plan);
    let (mut ssh_command, temp_config) =
        prepare_action_openssh_command(&action_host, local_config, force_config)
            .map_err(ActionError::PrepareSsh)?;
    let ssh_template = temp_config
        .as_ref()
        .map(|config| ssh_template_context(config, &ssh_command));
    if let Some(remote_command) = &action.remote_command {
        ssh_command.args.push(
            render_template(
                remote_command,
                &action_host,
                &allocated_ports,
                prepare_env,
                ssh_template.as_ref(),
            )?
            .into(),
        );
    }

    let local_prepare = action
        .local_prepare
        .as_ref()
        .map(|command| {
            resolve_local_command(
                command,
                &action_host,
                local_config,
                &allocated_ports,
                prepare_env,
                ssh_template.as_ref(),
            )
        })
        .transpose()?;
    let local_launch = action
        .local_launch
        .as_ref()
        .map(|command| {
            resolve_local_command(
                command,
                &action_host,
                local_config,
                &allocated_ports,
                prepare_env,
                ssh_template.as_ref(),
            )
        })
        .transpose()?;
    let cleanup = action
        .cleanup
        .iter()
        .map(|command| {
            resolve_local_command(
                command,
                &action_host,
                local_config,
                &allocated_ports,
                prepare_env,
                ssh_template.as_ref(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ResolvedActionPlan {
        action_name: action.name.clone(),
        host: action_host,
        local_prepare,
        ssh_command,
        temp_config,
        local_launch,
        cleanup,
        allocated_ports,
    })
}

pub fn resolve_action_local_prepare(
    host: &ResolvedHost,
    action: &ActionDefinition,
    local_config: &LocalConfig,
) -> Result<Option<ResolvedActionPrepare>, ActionError> {
    let force_config = action_requires_ssh_template(action, ActionPhase::Prepare);
    let (ssh_template, temp_config) = if force_config {
        let (ssh_command, temp_config) = prepare_action_openssh_command(host, local_config, true)
            .map_err(ActionError::PrepareSsh)?;
        let temp_config = temp_config.expect("forced OpenSSH config must produce a temp config");
        (
            Some(ssh_template_context(&temp_config, &ssh_command)),
            Some(temp_config),
        )
    } else {
        (None, None)
    };
    action
        .local_prepare
        .as_ref()
        .map(|command| {
            let command = resolve_local_command(
                command,
                host,
                local_config,
                &HashMap::new(),
                &HashMap::new(),
                ssh_template.as_ref(),
            )?;
            Ok(ResolvedActionPrepare {
                command,
                temp_config,
            })
        })
        .transpose()
}

fn prepare_action_openssh_command(
    host: &ResolvedHost,
    local_config: &LocalConfig,
    force_config: bool,
) -> io::Result<(OpenSshCommand, Option<TempOpenSshConfig>)> {
    if !force_config {
        return prepare_openssh_command(host, local_config);
    }

    let identity_path = host
        .identity_fingerprint
        .as_deref()
        .and_then(|fingerprint| local_config.identity_path(fingerprint));
    let config = config_for_host_with_identity_path(host, identity_path);
    let temp_config = TempOpenSshConfig::write(&config)?;
    let command = temp_config.command();
    Ok((command, Some(temp_config)))
}

fn ssh_template_context(
    temp_config: &TempOpenSshConfig,
    command: &OpenSshCommand,
) -> SshTemplateContext {
    let mut rsh_command = command.clone();
    rsh_command.args.pop();
    SshTemplateContext {
        config_path: temp_config.path().display().to_string(),
        alias: temp_config.alias().to_string(),
        rsh_command: rsh_command.render_for_display(),
        command: command.render_for_display(),
    }
}

#[derive(Debug, Clone, Copy)]
enum ActionPhase {
    Prepare,
    Plan,
}

fn action_requires_ssh_template(action: &ActionDefinition, phase: ActionPhase) -> bool {
    match phase {
        ActionPhase::Prepare => action
            .local_prepare
            .as_ref()
            .is_some_and(local_command_requires_ssh_template),
        ActionPhase::Plan => {
            action
                .remote_command
                .as_ref()
                .is_some_and(|command| template_requires_ssh_template(command))
                || action
                    .local_prepare
                    .as_ref()
                    .is_some_and(local_command_requires_ssh_template)
                || action
                    .local_launch
                    .as_ref()
                    .is_some_and(local_command_requires_ssh_template)
                || action
                    .cleanup
                    .iter()
                    .any(local_command_requires_ssh_template)
        }
    }
}

fn local_command_requires_ssh_template(command: &ActionLocalCommand) -> bool {
    command
        .program
        .as_ref()
        .is_some_and(|program| template_requires_ssh_template(program))
        || command
            .args
            .iter()
            .any(|arg| template_requires_ssh_template(arg))
        || command
            .env
            .values()
            .any(|value| template_requires_ssh_template(value))
}

fn template_requires_ssh_template(template: &str) -> bool {
    [
        "{SSH_CONFIG}",
        "{SSH_ALIAS}",
        "{SSH_DEST}",
        "{SSH_RSH}",
        "{SSH_COMMAND}",
    ]
    .iter()
    .any(|variable| template.contains(variable))
}

pub fn parse_prepare_env(output: &str) -> HashMap<String, String> {
    output
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            if key.is_empty()
                || !key
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
            {
                return None;
            }
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}

fn validate_action(action: &ActionDefinition) -> Result<(), ActionError> {
    let mut names = std::collections::HashSet::new();
    for forward in &action.forwards {
        let name = forward.name().trim();
        if name.is_empty() {
            return Err(ActionError::EmptyForwardName);
        }
        if !names.insert(name.to_string()) {
            return Err(ActionError::DuplicateForwardName(name.to_string()));
        }
    }
    Ok(())
}

fn resolve_action_forward(
    forward: &ActionForwardDefinition,
    allocated_ports: &mut HashMap<String, u16>,
    prepare_env: &HashMap<String, String>,
) -> Result<ForwardDefinition, ActionError> {
    match forward {
        ActionForwardDefinition::Local {
            name,
            bind_address,
            local_port,
            destination_host,
            destination_port,
        } => {
            let local_port = resolve_port(name, local_port, allocated_ports, prepare_env)?;
            Ok(ForwardDefinition::Local {
                bind_address: bind_address.clone(),
                local_port,
                destination_host: destination_host.clone(),
                destination_port: *destination_port,
            })
        }
        ActionForwardDefinition::Dynamic {
            name,
            bind_address,
            local_port,
        } => {
            let local_port = resolve_port(name, local_port, allocated_ports, prepare_env)?;
            Ok(ForwardDefinition::Dynamic {
                bind_address: bind_address.clone(),
                local_port,
            })
        }
    }
}

fn resolve_port(
    name: &str,
    port: &ActionPort,
    allocated_ports: &mut HashMap<String, u16>,
    prepare_env: &HashMap<String, String>,
) -> Result<u16, ActionError> {
    let resolved = match port {
        ActionPort::Auto => allocate_local_port()?,
        ActionPort::Fixed(port) => *port,
        ActionPort::Env(variable) => {
            let value = prepare_env
                .get(variable)
                .ok_or_else(|| ActionError::UnknownTemplateVariable(format!("ENV:{variable}")))?;
            value.parse::<u16>().map_err(|_| ActionError::InvalidPort {
                port_source: format!("ENV:{variable}"),
                value: value.clone(),
            })?
        }
    };
    allocated_ports.insert(name.to_string(), resolved);
    Ok(resolved)
}

fn allocate_local_port() -> Result<u16, ActionError> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(ActionError::AllocatePort)?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(ActionError::AllocatePort)
}

fn resolve_local_command(
    command: &ActionLocalCommand,
    host: &ResolvedHost,
    local_config: &LocalConfig,
    allocated_ports: &HashMap<String, u16>,
    prepare_env: &HashMap<String, String>,
    ssh_template: Option<&SshTemplateContext>,
) -> Result<ResolvedLocalCommand, ActionError> {
    let program = match (&command.program, &command.capability) {
        (Some(_), Some(_)) => return Err(ActionError::AmbiguousLocalProgram),
        (None, None) => return Err(ActionError::MissingLocalProgram),
        (Some(program), None) => PathBuf::from(render_template(
            program,
            host,
            allocated_ports,
            prepare_env,
            ssh_template,
        )?),
        (None, Some(capability)) => local_config
            .capability_path(capability)
            .ok_or_else(|| ActionError::MissingCapability(capability.clone()))?
            .to_path_buf(),
    };
    let args = command
        .args
        .iter()
        .map(|arg| render_template(arg, host, allocated_ports, prepare_env, ssh_template))
        .collect::<Result<Vec<_>, _>>()?;
    let env = command
        .env
        .iter()
        .map(|(key, value)| {
            Ok((
                key.clone(),
                render_template(value, host, allocated_ports, prepare_env, ssh_template)?,
            ))
        })
        .collect::<Result<HashMap<_, _>, ActionError>>()?;
    Ok(ResolvedLocalCommand { program, args, env })
}

fn render_template(
    template: &str,
    host: &ResolvedHost,
    allocated_ports: &HashMap<String, u16>,
    prepare_env: &HashMap<String, String>,
    ssh_template: Option<&SshTemplateContext>,
) -> Result<String, ActionError> {
    let mut rendered = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        rendered.push_str(&rest[..start]);
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('}') else {
            rendered.push_str(&rest[start..]);
            return Ok(rendered);
        };
        let variable = &after_start[..end];
        rendered.push_str(&template_value(
            variable,
            host,
            allocated_ports,
            prepare_env,
            ssh_template,
        )?);
        rest = &after_start[end + 1..];
    }
    rendered.push_str(rest);
    Ok(rendered)
}

fn template_value(
    variable: &str,
    host: &ResolvedHost,
    allocated_ports: &HashMap<String, u16>,
    prepare_env: &HashMap<String, String>,
    ssh_template: Option<&SshTemplateContext>,
) -> Result<String, ActionError> {
    if variable == "HOST" {
        return Ok(host.hostname.clone());
    }
    if variable == "USER" {
        return Ok(host.username.clone().unwrap_or_default());
    }
    if variable == "PORT" {
        return Ok(host.port.to_string());
    }
    if variable == "SSH_CONFIG" {
        return ssh_template
            .map(|context| context.config_path.clone())
            .ok_or_else(|| ActionError::UnknownTemplateVariable(variable.to_string()));
    }
    if variable == "SSH_ALIAS" || variable == "SSH_DEST" {
        return ssh_template
            .map(|context| context.alias.clone())
            .ok_or_else(|| ActionError::UnknownTemplateVariable(variable.to_string()));
    }
    if variable == "SSH_RSH" {
        return ssh_template
            .map(|context| context.rsh_command.clone())
            .ok_or_else(|| ActionError::UnknownTemplateVariable(variable.to_string()));
    }
    if variable == "SSH_COMMAND" {
        return ssh_template
            .map(|context| context.command.clone())
            .ok_or_else(|| ActionError::UnknownTemplateVariable(variable.to_string()));
    }
    if let Some(name) = variable.strip_prefix("LOCAL_PORT:") {
        return allocated_ports
            .get(name)
            .map(u16::to_string)
            .ok_or_else(|| ActionError::UnknownTemplateVariable(variable.to_string()));
    }
    if let Some(name) = variable.strip_prefix("ENV:") {
        return prepare_env
            .get(name)
            .cloned()
            .ok_or_else(|| ActionError::UnknownTemplateVariable(variable.to_string()));
    }
    Err(ActionError::UnknownTemplateVariable(variable.to_string()))
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn host() -> ResolvedHost {
        ResolvedHost {
            id: Uuid::new_v4(),
            path: "lab/pi".to_string(),
            display_name: "pi".to_string(),
            hostname: "pi.local".to_string(),
            port: 22,
            username: Some("alice".to_string()),
            identity_fingerprint: None,
            secrets: None,
            jump_chain: Vec::new(),
            ssh_options: Vec::new(),
            forwards: Vec::new(),
            actions: Vec::new(),
            tags: Vec::new(),
            notes: None,
        }
    }

    #[test]
    fn resolves_forwarded_vnc_action_with_auto_port() {
        let mut local_config = LocalConfig::new();
        local_config
            .map_capability("vnc-viewer".to_string(), "/usr/bin/xtightvncviewer".into())
            .unwrap();
        let action = ActionDefinition {
            id: Uuid::new_v4(),
            name: "Desktop".to_string(),
            local_prepare: None,
            forwards: vec![ActionForwardDefinition::Local {
                name: "vnc".to_string(),
                bind_address: "127.0.0.1".to_string(),
                local_port: ActionPort::Auto,
                destination_host: "127.0.0.1".to_string(),
                destination_port: 5900,
            }],
            remote_command: Some("DISPLAY=:0 x11vnc -scale 1/2".to_string()),
            local_launch: Some(ActionLocalCommand {
                capability: Some("vnc-viewer".to_string()),
                program: None,
                args: vec!["127.0.0.1::{LOCAL_PORT:vnc}".to_string()],
                env: HashMap::new(),
            }),
            cleanup: Vec::new(),
        };

        let plan = resolve_action_plan(&host(), &action, &local_config, &HashMap::new()).unwrap();

        let port = plan.allocated_port("vnc").unwrap();
        assert_ne!(port, 0);
        assert_eq!(
            plan.local_launch.unwrap().args,
            vec![format!("127.0.0.1::{port}")]
        );
        assert!(plan.ssh_command.render_for_display().contains("x11vnc"));
    }

    #[test]
    fn resolves_direct_lan_vnc_action_without_forwards() {
        let mut local_config = LocalConfig::new();
        local_config
            .map_capability("vnc-viewer".to_string(), "/usr/bin/xtightvncviewer".into())
            .unwrap();
        let action = ActionDefinition {
            id: Uuid::new_v4(),
            name: "Desktop".to_string(),
            local_prepare: None,
            forwards: Vec::new(),
            remote_command: Some("DISPLAY=:0 x11vnc -scale 1/2".to_string()),
            local_launch: Some(ActionLocalCommand {
                capability: Some("vnc-viewer".to_string()),
                program: None,
                args: vec!["{HOST}::5900".to_string()],
                env: HashMap::new(),
            }),
            cleanup: Vec::new(),
        };

        let plan = resolve_action_plan(&host(), &action, &local_config, &HashMap::new()).unwrap();

        assert!(plan.allocated_ports.is_empty());
        assert_eq!(plan.local_launch.unwrap().args, vec!["pi.local::5900"]);
    }

    #[test]
    fn resolves_forward_port_from_prepare_environment() {
        let action = ActionDefinition {
            id: Uuid::new_v4(),
            name: "Desktop".to_string(),
            local_prepare: Some(ActionLocalCommand {
                capability: None,
                program: Some("/bin/choose-port".to_string()),
                args: Vec::new(),
                env: HashMap::new(),
            }),
            forwards: vec![ActionForwardDefinition::Local {
                name: "vnc".to_string(),
                bind_address: "127.0.0.1".to_string(),
                local_port: ActionPort::Env("PORT".to_string()),
                destination_host: "127.0.0.1".to_string(),
                destination_port: 5900,
            }],
            remote_command: None,
            local_launch: Some(ActionLocalCommand {
                capability: None,
                program: Some("/usr/bin/xtightvncviewer".to_string()),
                args: vec!["127.0.0.1::{LOCAL_PORT:vnc}".to_string()],
                env: HashMap::new(),
            }),
            cleanup: Vec::new(),
        };
        let prepare_env = HashMap::from([("PORT".to_string(), "5951".to_string())]);

        let plan =
            resolve_action_plan(&host(), &action, &LocalConfig::new(), &prepare_env).unwrap();

        assert_eq!(plan.allocated_port("vnc"), Some(5951));
        assert_eq!(plan.local_launch.unwrap().args, vec!["127.0.0.1::5951"]);
    }

    #[test]
    fn parses_prepare_environment_lines() {
        let parsed = parse_prepare_env("PORT=5900\nignored\nBAD-NAME=value\nDISPLAY=:0\n");

        assert_eq!(parsed.get("PORT").map(String::as_str), Some("5900"));
        assert_eq!(parsed.get("DISPLAY").map(String::as_str), Some(":0"));
        assert!(!parsed.contains_key("BAD-NAME"));
    }

    #[test]
    fn renders_host_port_for_local_commands() {
        let command = ActionLocalCommand {
            capability: None,
            program: Some("/bin/echo".to_string()),
            args: vec!["{USER}@{HOST}:{PORT}".to_string()],
            env: HashMap::new(),
        };

        let resolved = resolve_local_command(
            &command,
            &host(),
            &LocalConfig::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
        )
        .unwrap();

        assert_eq!(resolved.args, vec!["alice@pi.local:22"]);
    }

    #[test]
    fn resolves_config_backed_transfer_templates_for_local_prepare() {
        let mut local_config = LocalConfig::new();
        local_config
            .map_identity(
                "SHA256:alice".to_string(),
                "/home/alice/.ssh/id_ed25519".into(),
                None,
            )
            .unwrap();
        let mut host = host();
        host.identity_fingerprint = Some("SHA256:alice".to_string());
        let action = ActionDefinition {
            id: Uuid::new_v4(),
            name: "Send file".to_string(),
            local_prepare: Some(ActionLocalCommand {
                capability: None,
                program: Some("/bin/send".to_string()),
                args: vec![
                    "{SSH_CONFIG}".to_string(),
                    "{SSH_DEST}".to_string(),
                    "{SSH_ALIAS}".to_string(),
                    "{SSH_RSH}".to_string(),
                    "{SSH_COMMAND}".to_string(),
                ],
                env: HashMap::new(),
            }),
            forwards: Vec::new(),
            remote_command: Some("true".to_string()),
            local_launch: None,
            cleanup: Vec::new(),
        };

        let prepare = resolve_action_local_prepare(&host, &action, &local_config)
            .unwrap()
            .unwrap();

        assert!(prepare.temp_config.as_ref().unwrap().path().exists());
        assert_eq!(prepare.command.args[1], prepare.command.args[2]);
        assert!(prepare.command.args[0].contains("stassh-"));
        assert!(prepare.command.args[3].starts_with("ssh -F "));
        assert!(!prepare.command.args[3].ends_with(&prepare.command.args[1]));
        assert!(prepare.command.args[4].starts_with("ssh -F "));
        assert!(prepare.command.args[4].contains(&prepare.command.args[1]));
    }

    #[test]
    fn ssh_templates_force_config_backed_action_plan() {
        let action = ActionDefinition {
            id: Uuid::new_v4(),
            name: "Rsync".to_string(),
            local_prepare: None,
            forwards: Vec::new(),
            remote_command: Some("true".to_string()),
            local_launch: Some(ActionLocalCommand {
                capability: None,
                program: Some("/usr/bin/rsync".to_string()),
                args: vec![
                    "-e".to_string(),
                    "{SSH_RSH}".to_string(),
                    "/tmp/file".to_string(),
                    "{SSH_DEST}:~/".to_string(),
                ],
                env: HashMap::new(),
            }),
            cleanup: Vec::new(),
        };

        let plan =
            resolve_action_plan(&host(), &action, &LocalConfig::new(), &HashMap::new()).unwrap();

        assert!(plan.temp_config.as_ref().unwrap().path().exists());
        let local_launch = plan.local_launch.unwrap();
        assert!(local_launch.args[1].starts_with("ssh -F "));
        assert!(local_launch.args[3].starts_with("stassh-"));
        assert!(plan.ssh_command.render_for_display().starts_with("ssh -F "));
    }
}
