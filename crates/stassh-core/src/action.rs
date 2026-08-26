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
use crate::openssh::{OpenSshCommand, TempOpenSshConfig};

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
    let (mut ssh_command, temp_config) =
        prepare_openssh_command(&action_host, local_config).map_err(ActionError::PrepareSsh)?;
    if let Some(remote_command) = &action.remote_command {
        ssh_command.args.push(
            render_template(remote_command, &action_host, &allocated_ports, prepare_env)?.into(),
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
) -> Result<Option<ResolvedLocalCommand>, ActionError> {
    action
        .local_prepare
        .as_ref()
        .map(|command| {
            resolve_local_command(
                command,
                host,
                local_config,
                &HashMap::new(),
                &HashMap::new(),
            )
        })
        .transpose()
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
) -> Result<ResolvedLocalCommand, ActionError> {
    let program = match (&command.program, &command.capability) {
        (Some(_), Some(_)) => return Err(ActionError::AmbiguousLocalProgram),
        (None, None) => return Err(ActionError::MissingLocalProgram),
        (Some(program), None) => PathBuf::from(render_template(
            program,
            host,
            allocated_ports,
            prepare_env,
        )?),
        (None, Some(capability)) => local_config
            .capability_path(capability)
            .ok_or_else(|| ActionError::MissingCapability(capability.clone()))?
            .to_path_buf(),
    };
    let args = command
        .args
        .iter()
        .map(|arg| render_template(arg, host, allocated_ports, prepare_env))
        .collect::<Result<Vec<_>, _>>()?;
    let env = command
        .env
        .iter()
        .map(|(key, value)| {
            Ok((
                key.clone(),
                render_template(value, host, allocated_ports, prepare_env)?,
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
) -> Result<String, ActionError> {
    if variable == "HOST" {
        return Ok(host.hostname.clone());
    }
    if variable == "USER" {
        return Ok(host.username.clone().unwrap_or_default());
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
            username: Some("arturo".to_string()),
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
}
