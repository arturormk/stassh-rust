use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child as ProcessChild, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use stassh_core::{
    ActionDefinition, AddFolder, AddHost, ForwardDefinition, HostSelector, LocalConfig,
    ResolvedActionPlan, ResolvedLocalCommand, SecretField, SecretsStore, SimulatedShell,
    TempOpenSshConfig, UpdateHost, Vault, demo_workspace, ensure_home_stassh_permissions,
    load_local_config, load_secrets, load_vault, local_config_path, parse_prepare_env,
    prepare_openssh_command, resolve_action_local_prepare, resolve_action_plan, save_vault,
    secrets_path, vault_path,
};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;
use zeroize::Zeroize;

fn main() {
    let simulation = std::env::args_os().any(|arg| arg == "--simulation");
    tauri::Builder::default()
        .manage(AppState::new(simulation))
        .invoke_handler(tauri::generate_handler![
            load_workspace,
            reload_workspace,
            search_hosts,
            host_details,
            create_host,
            update_host,
            copy_host,
            delete_host,
            move_hosts,
            create_folder,
            rename_folder,
            move_folder,
            delete_folder,
            assign_identity,
            clear_identity,
            update_jumps,
            update_forwards,
            host_secrets,
            reveal_host_secret,
            preview_ssh_command,
            host_actions,
            preview_action,
            start_ssh_session,
            start_action_session,
            write_terminal,
            resize_terminal,
            close_session,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run stassh-gui");
}

struct AppState {
    simulation: bool,
    workspace: Mutex<Option<Workspace>>,
    sessions: Mutex<HashMap<Uuid, Session>>,
}

impl AppState {
    fn new(simulation: bool) -> Self {
        Self {
            simulation,
            workspace: Mutex::new(None),
            sessions: Mutex::new(HashMap::new()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkspaceSource {
    Files,
    Simulation,
}

struct Workspace {
    source: WorkspaceSource,
    vault_path: PathBuf,
    local_config_path: PathBuf,
    secrets_path: PathBuf,
    vault: Vault,
    local_config: LocalConfig,
    secrets_store: Option<SecretsStore>,
}

enum Session {
    Real(RealSession),
    Simulated(SimulatedSession),
}

struct RealSession {
    master: Box<dyn MasterPty + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    child: Box<dyn Child + Send + Sync>,
    local_child: Option<ProcessChild>,
    cleanup: Vec<ResolvedLocalCommand>,
    _temp_config: Option<TempOpenSshConfig>,
}

struct SimulatedSession {
    shell: SimulatedShell,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceSnapshot {
    vault_path: String,
    local_config_path: String,
    secrets_path: String,
    folders: Vec<FolderView>,
    hosts: Vec<HostView>,
    identities: Vec<IdentityView>,
    secrets_available: bool,
    diagnostics: Vec<DiagnosticView>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct FolderView {
    id: Uuid,
    parent_id: Option<Uuid>,
    name: String,
    path: String,
    host_count: usize,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct HostView {
    id: Uuid,
    folder_id: Uuid,
    path: String,
    display_name: String,
    hostname: String,
    port: u16,
    username: Option<String>,
    identity_fingerprint: Option<String>,
    secrets: Option<String>,
    jump_chain: Vec<Uuid>,
    forwards: Vec<ForwardDefinition>,
    tags: Vec<String>,
    notes: Option<String>,
    action_count: usize,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct IdentityView {
    fingerprint: String,
    path: String,
    preferred_name: Option<String>,
    exists: bool,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DiagnosticView {
    severity: &'static str,
    message: String,
    host_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HostDetailsView {
    host: HostView,
    jumps: Vec<JumpView>,
    ssh_command: String,
    diagnostics: Vec<DiagnosticView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JumpView {
    id: Uuid,
    display_name: String,
    hostname: String,
    port: u16,
    username: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SshPreview {
    command: String,
    uses_temp_config: bool,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ActionView {
    id: Uuid,
    name: String,
    origin: &'static str,
    remote_command: Option<String>,
    has_local_prepare: bool,
    has_local_launch: bool,
    forward_count: usize,
    cleanup_count: usize,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct LocalCommandView {
    program: String,
    args: Vec<String>,
    env: HashMap<String, String>,
    display: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ActionPlanView {
    action_name: String,
    allocated_ports: HashMap<String, u16>,
    ssh_command: String,
    uses_temp_config: bool,
    temp_config_path: Option<String>,
    local_prepare: Option<LocalCommandView>,
    local_launch: Option<LocalCommandView>,
    cleanup: Vec<LocalCommandView>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct HostSecretsView {
    host_id: Uuid,
    host_path: String,
    set_key: String,
    label: Option<String>,
    fields: Vec<SecretFieldView>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct SecretFieldView {
    name: String,
    kind: &'static str,
    plain_value: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SearchResult {
    id: Uuid,
    path: String,
    target: String,
    username: Option<String>,
    tags: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SessionOutput {
    session_id: Uuid,
    data: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SessionExit {
    session_id: Uuid,
    message: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct StartSessionView {
    session_id: Uuid,
    initial_output: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostInput {
    folder_id: Uuid,
    display_name: String,
    hostname: String,
    port: u16,
    username: Option<String>,
    identity_fingerprint: Option<String>,
    secrets: Option<String>,
    jump_chain: Vec<Uuid>,
    forwards: Vec<ForwardDefinition>,
    tags: Vec<String>,
    notes: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FolderInput {
    parent_id: Uuid,
    name: String,
}

fn load_workspace_from_paths(
    vault_path_override: Option<PathBuf>,
    local_path_override: Option<PathBuf>,
    secrets_path_override: Option<PathBuf>,
) -> Result<Workspace, String> {
    let vault_path = vault_path(vault_path_override).map_err(error_message)?;
    let local_config_path = local_config_path(local_path_override, &vault_path);
    let secrets_path = secrets_path(secrets_path_override, &vault_path);
    ensure_home_stassh_permissions(&[&vault_path, &local_config_path, &secrets_path])
        .map_err(error_message)?;

    let vault = load_vault(&vault_path).map_err(error_message)?;
    let local_config = load_local_config(&local_config_path).map_err(error_message)?;
    let secrets_store = if secrets_path.exists() {
        Some(load_secrets(&secrets_path).map_err(error_message)?)
    } else {
        None
    };

    Ok(Workspace {
        source: WorkspaceSource::Files,
        vault_path,
        local_config_path,
        secrets_path,
        vault,
        local_config,
        secrets_store,
    })
}

fn load_simulation_workspace() -> Result<Workspace, String> {
    let workspace = demo_workspace().map_err(error_message)?;
    Ok(Workspace {
        source: WorkspaceSource::Simulation,
        vault_path: PathBuf::from("simulation://vault.json"),
        local_config_path: PathBuf::from("simulation://local.json"),
        secrets_path: PathBuf::from("simulation://secrets.json"),
        vault: workspace.vault,
        local_config: workspace.local_config,
        secrets_store: Some(workspace.secrets_store),
    })
}

fn snapshot(workspace: &Workspace) -> WorkspaceSnapshot {
    let folders = workspace
        .vault
        .folders
        .iter()
        .map(|folder| FolderView {
            id: folder.id,
            parent_id: folder.parent_id,
            name: folder.name.clone(),
            path: workspace.vault.folder_path(folder.id),
            host_count: workspace
                .vault
                .hosts
                .iter()
                .filter(|host| host.folder_id == folder.id)
                .count(),
        })
        .collect();

    WorkspaceSnapshot {
        vault_path: workspace.vault_path.display().to_string(),
        local_config_path: workspace.local_config_path.display().to_string(),
        secrets_path: workspace.secrets_path.display().to_string(),
        folders,
        hosts: host_views(&workspace.vault),
        identities: identity_views(&workspace.local_config, workspace.source),
        secrets_available: workspace.secrets_store.is_some(),
        diagnostics: diagnostics(&workspace.vault, &workspace.local_config, workspace.source),
    }
}

fn host_views(vault: &Vault) -> Vec<HostView> {
    vault
        .hosts
        .iter()
        .map(|host| HostView {
            id: host.id,
            folder_id: host.folder_id,
            path: vault.host_path(host),
            display_name: host.display_name.clone(),
            hostname: host.hostname.clone(),
            port: host.port,
            username: host.username.clone(),
            identity_fingerprint: host.identity_fingerprint.clone(),
            secrets: host.secrets.clone(),
            jump_chain: host.jump_chain.clone(),
            forwards: host.forwards.clone(),
            tags: host.tags.clone(),
            notes: host.notes.clone(),
            action_count: host.actions.len() + vault.actions.len(),
        })
        .collect()
}

fn identity_views(local_config: &LocalConfig, source: WorkspaceSource) -> Vec<IdentityView> {
    local_config
        .identity_mappings
        .iter()
        .map(|mapping| IdentityView {
            fingerprint: mapping.fingerprint.clone(),
            path: mapping.path.display().to_string(),
            preferred_name: mapping.preferred_name.clone(),
            exists: source == WorkspaceSource::Simulation || mapping.path.exists(),
        })
        .collect()
}

fn diagnostics(
    vault: &Vault,
    local_config: &LocalConfig,
    source: WorkspaceSource,
) -> Vec<DiagnosticView> {
    let mut diagnostics = Vec::new();
    for group in vault.duplicate_hosts() {
        diagnostics.push(DiagnosticView {
            severity: "warning",
            message: format!("duplicate {:?}: {}", group.kind, group.key),
            host_id: None,
        });
    }
    for host in &vault.hosts {
        if let Some(fingerprint) = &host.identity_fingerprint {
            if local_config.identity_path(fingerprint).is_none() {
                diagnostics.push(DiagnosticView {
                    severity: "warning",
                    message: format!("missing identity mapping for {}", vault.host_path(host)),
                    host_id: Some(host.id),
                });
            }
        }
        for jump_id in &host.jump_chain {
            if vault.host(*jump_id).is_none() {
                diagnostics.push(DiagnosticView {
                    severity: "error",
                    message: format!(
                        "missing jump target {} on {}",
                        jump_id,
                        vault.host_path(host)
                    ),
                    host_id: Some(host.id),
                });
            }
        }
    }
    for mapping in &local_config.identity_mappings {
        if source != WorkspaceSource::Simulation && !mapping.path.exists() {
            diagnostics.push(DiagnosticView {
                severity: "warning",
                message: format!("identity file missing: {}", mapping.path.display()),
                host_id: None,
            });
        }
    }
    diagnostics
}

fn host_secrets_view(workspace: &Workspace, host_id: Uuid) -> Result<HostSecretsView, String> {
    let host = workspace
        .vault
        .host(host_id)
        .ok_or_else(|| format!("host not found: {host_id}"))?;
    let set_key = host.secrets.clone().ok_or_else(|| {
        format!(
            "host has no secrets set: {}",
            workspace.vault.host_path(host)
        )
    })?;
    let store = workspace.secrets_store.as_ref().ok_or_else(|| {
        format!(
            "secrets store not found: {}",
            workspace.secrets_path.display()
        )
    })?;
    let set = store.set(&set_key).map_err(error_message)?;
    let fields = set
        .fields
        .iter()
        .map(|(name, field)| match field {
            SecretField::Plain(value) => SecretFieldView {
                name: name.clone(),
                kind: "plain",
                plain_value: Some(value.clone()),
            },
            SecretField::Secret(_) => SecretFieldView {
                name: name.clone(),
                kind: "secret",
                plain_value: None,
            },
        })
        .collect();
    Ok(HostSecretsView {
        host_id,
        host_path: workspace.vault.host_path(host),
        set_key,
        label: set.label.clone(),
        fields,
    })
}

fn reveal_host_secret_value(
    workspace: &Workspace,
    host_id: Uuid,
    field: &str,
    master_password: &str,
) -> Result<String, String> {
    let host = workspace
        .vault
        .host(host_id)
        .ok_or_else(|| format!("host not found: {host_id}"))?;
    let set_key = host.secrets.as_deref().ok_or_else(|| {
        format!(
            "host has no secrets set: {}",
            workspace.vault.host_path(host)
        )
    })?;
    let store = workspace.secrets_store.as_ref().ok_or_else(|| {
        format!(
            "secrets store not found: {}",
            workspace.secrets_path.display()
        )
    })?;
    let key = store.unlock(master_password).map_err(error_message)?;
    let plaintext = store.reveal(&key, set_key, field).map_err(error_message)?;
    plaintext
        .expose_str()
        .map(str::to_string)
        .map_err(error_message)
}

fn action_views(workspace: &Workspace, host_id: Uuid) -> Result<Vec<ActionView>, String> {
    let host = workspace
        .vault
        .host(host_id)
        .ok_or_else(|| format!("host not found: {host_id}"))?;
    Ok(workspace
        .vault
        .actions
        .iter()
        .map(|action| action_view(action, "common"))
        .chain(
            host.actions
                .iter()
                .map(|action| action_view(action, "host")),
        )
        .collect())
}

fn action_view(action: &ActionDefinition, origin: &'static str) -> ActionView {
    ActionView {
        id: action.id,
        name: action.name.clone(),
        origin,
        remote_command: action.remote_command.clone(),
        has_local_prepare: action.local_prepare.is_some(),
        has_local_launch: action.local_launch.is_some(),
        forward_count: action.forwards.len(),
        cleanup_count: action.cleanup.len(),
    }
}

fn action_by_id(
    workspace: &Workspace,
    host_id: Uuid,
    action_id: Uuid,
) -> Result<ActionDefinition, String> {
    let resolved = workspace
        .vault
        .resolve_host(HostSelector::Id(host_id))
        .map_err(error_message)?;
    resolved
        .actions
        .into_iter()
        .find(|action| action.id == action_id)
        .ok_or_else(|| format!("action not found: {action_id}"))
}

fn action_plan_view(plan: &ResolvedActionPlan) -> ActionPlanView {
    ActionPlanView {
        action_name: plan.action_name.clone(),
        allocated_ports: plan.allocated_ports.clone(),
        ssh_command: plan.ssh_command.render_for_display(),
        uses_temp_config: plan.temp_config.is_some(),
        temp_config_path: plan
            .temp_config
            .as_ref()
            .map(|config| config.path().display().to_string()),
        local_prepare: plan.local_prepare.as_ref().map(local_command_view),
        local_launch: plan.local_launch.as_ref().map(local_command_view),
        cleanup: plan.cleanup.iter().map(local_command_view).collect(),
    }
}

fn local_command_view(command: &ResolvedLocalCommand) -> LocalCommandView {
    LocalCommandView {
        program: command.program.display().to_string(),
        args: command.args.clone(),
        env: command.env.clone(),
        display: display_local_command(command),
    }
}

fn run_action_prepare(
    command: Option<&ResolvedLocalCommand>,
) -> Result<HashMap<String, String>, String> {
    let Some(command) = command else {
        return Ok(HashMap::new());
    };
    let output = local_command(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("failed to run local prepare: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.trim();
        if detail.is_empty() {
            return Err(format!(
                "local prepare exited with status {}",
                output.status
            ));
        }
        return Err(format!(
            "local prepare exited with status {}: {detail}",
            output.status
        ));
    }
    Ok(parse_prepare_env(&String::from_utf8_lossy(&output.stdout)))
}

fn spawn_local_launch(
    command: Option<&ResolvedLocalCommand>,
) -> Result<Option<ProcessChild>, String> {
    let Some(command) = command else {
        return Ok(None);
    };
    local_command(command).spawn().map(Some).map_err(|error| {
        format!(
            "failed to launch local command {}: {error}",
            display_local_command(command)
        )
    })
}

fn local_command(command: &ResolvedLocalCommand) -> Command {
    let mut process = Command::new(&command.program);
    process.args(&command.args).envs(&command.env);
    process
}

fn display_local_command(command: &ResolvedLocalCommand) -> String {
    let mut parts = vec![command.program.display().to_string()];
    parts.extend(command.args.clone());
    parts.join(" ")
}

fn cleanup_session(session: Session) {
    match session {
        Session::Real(mut session) => {
            let _ = session.child.kill();
            if let Some(mut child) = session.local_child {
                let _ = child.kill();
                let _ = child.wait();
            }
            for command in &session.cleanup {
                let _ = local_command(command).status();
            }
        }
        Session::Simulated(mut session) => {
            let _ = session.shell.close();
        }
    }
}

fn finish_session(state: &AppState, session_id: Uuid) {
    let session = state
        .sessions
        .lock()
        .ok()
        .and_then(|mut sessions| sessions.remove(&session_id));
    if let Some(session) = session {
        cleanup_session(session);
    }
}

fn workspace_with_state<T>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&Workspace) -> Result<T, String>,
) -> Result<T, String> {
    let guard = state.workspace.lock().map_err(error_message)?;
    let workspace = guard
        .as_ref()
        .ok_or_else(|| "workspace is not loaded".to_string())?;
    f(workspace)
}

fn mutate_vault(
    state: &State<'_, AppState>,
    f: impl FnOnce(&mut Vault) -> Result<(), String>,
) -> Result<WorkspaceSnapshot, String> {
    let mut guard = state.workspace.lock().map_err(error_message)?;
    let workspace = guard
        .as_mut()
        .ok_or_else(|| "workspace is not loaded".to_string())?;
    if workspace.source == WorkspaceSource::Simulation {
        f(&mut workspace.vault)?;
        workspace.vault.validate().map_err(error_message)?;
        return Ok(snapshot(workspace));
    }
    let mut vault = load_vault(&workspace.vault_path).map_err(error_message)?;
    f(&mut vault)?;
    save_vault(&workspace.vault_path, &vault).map_err(error_message)?;
    workspace.vault = load_vault(&workspace.vault_path).map_err(error_message)?;
    workspace.local_config =
        load_local_config(&workspace.local_config_path).map_err(error_message)?;
    workspace.secrets_store = if workspace.secrets_path.exists() {
        Some(load_secrets(&workspace.secrets_path).map_err(error_message)?)
    } else {
        None
    };
    Ok(snapshot(workspace))
}

#[tauri::command]
fn load_workspace(state: State<'_, AppState>) -> Result<WorkspaceSnapshot, String> {
    let workspace = if state.simulation {
        load_simulation_workspace()?
    } else {
        load_workspace_from_paths(None, None, None)?
    };
    let snapshot = snapshot(&workspace);
    *state.workspace.lock().map_err(error_message)? = Some(workspace);
    Ok(snapshot)
}

#[tauri::command]
fn reload_workspace(state: State<'_, AppState>) -> Result<WorkspaceSnapshot, String> {
    let mut guard = state.workspace.lock().map_err(error_message)?;
    let Some(current) = guard.as_ref() else {
        drop(guard);
        return load_workspace(state);
    };
    let workspace = if current.source == WorkspaceSource::Simulation {
        load_simulation_workspace()?
    } else {
        load_workspace_from_paths(
            Some(current.vault_path.clone()),
            Some(current.local_config_path.clone()),
            Some(current.secrets_path.clone()),
        )?
    };
    let snapshot = snapshot(&workspace);
    *guard = Some(workspace);
    Ok(snapshot)
}

#[tauri::command]
fn search_hosts(query: String, state: State<'_, AppState>) -> Result<Vec<SearchResult>, String> {
    workspace_with_state(&state, |workspace| {
        Ok(workspace
            .vault
            .search_hosts(&query)
            .into_iter()
            .map(|host| SearchResult {
                id: host.id,
                path: workspace.vault.host_path(host),
                target: format!("{}:{}", host.hostname, host.port),
                username: host.username.clone(),
                tags: host.tags.clone(),
            })
            .collect())
    })
}

#[tauri::command]
fn host_details(host_id: Uuid, state: State<'_, AppState>) -> Result<HostDetailsView, String> {
    workspace_with_state(&state, |workspace| {
        let resolved = workspace
            .vault
            .resolve_host(HostSelector::Id(host_id))
            .map_err(error_message)?;
        let host = workspace
            .vault
            .host(host_id)
            .ok_or_else(|| format!("host not found: {host_id}"))?;
        let (ssh, _temp_config) =
            prepare_openssh_command(&resolved, &workspace.local_config).map_err(error_message)?;
        Ok(HostDetailsView {
            host: HostView {
                id: host.id,
                folder_id: host.folder_id,
                path: workspace.vault.host_path(host),
                display_name: host.display_name.clone(),
                hostname: host.hostname.clone(),
                port: host.port,
                username: host.username.clone(),
                identity_fingerprint: host.identity_fingerprint.clone(),
                secrets: host.secrets.clone(),
                jump_chain: host.jump_chain.clone(),
                forwards: host.forwards.clone(),
                tags: host.tags.clone(),
                notes: host.notes.clone(),
                action_count: host.actions.len() + workspace.vault.actions.len(),
            },
            jumps: resolved
                .jump_chain
                .into_iter()
                .map(|jump| JumpView {
                    id: jump.id,
                    display_name: jump.display_name,
                    hostname: jump.hostname,
                    port: jump.port,
                    username: jump.username,
                })
                .collect(),
            ssh_command: ssh.render_for_display(),
            diagnostics: diagnostics(&workspace.vault, &workspace.local_config, workspace.source)
                .into_iter()
                .filter(|diagnostic| diagnostic.host_id == Some(host_id))
                .collect(),
        })
    })
}

#[tauri::command]
fn create_host(input: HostInput, state: State<'_, AppState>) -> Result<WorkspaceSnapshot, String> {
    mutate_vault(&state, |vault| {
        vault
            .add_host(AddHost {
                folder_id: Some(input.folder_id),
                display_name: input.display_name,
                hostname: input.hostname,
                port: Some(input.port),
                username: empty_to_none(input.username),
                identity_fingerprint: empty_to_none(input.identity_fingerprint),
                secrets: empty_to_none(input.secrets),
                jump_chain: input.jump_chain,
                ssh_options: Vec::new(),
                forwards: input.forwards,
                tags: input.tags,
                notes: empty_to_none(input.notes),
            })
            .map_err(error_message)?;
        Ok(())
    })
}

#[tauri::command]
fn update_host(
    host_id: Uuid,
    input: HostInput,
    state: State<'_, AppState>,
) -> Result<WorkspaceSnapshot, String> {
    mutate_vault(&state, |vault| {
        vault
            .update_host(
                HostSelector::Id(host_id),
                UpdateHost {
                    folder_id: Some(input.folder_id),
                    display_name: Some(input.display_name),
                    hostname: Some(input.hostname),
                    port: Some(input.port),
                    username: Some(empty_to_none(input.username)),
                    identity_fingerprint: Some(empty_to_none(input.identity_fingerprint)),
                    secrets: Some(empty_to_none(input.secrets)),
                    jump_chain: Some(input.jump_chain),
                    forwards: Some(input.forwards),
                    tags: Some(input.tags),
                    notes: Some(empty_to_none(input.notes)),
                    ..UpdateHost::default()
                },
            )
            .map_err(error_message)?;
        Ok(())
    })
}

#[tauri::command]
fn copy_host(host_id: Uuid, state: State<'_, AppState>) -> Result<WorkspaceSnapshot, String> {
    mutate_vault(&state, |vault| {
        let source = vault
            .host(host_id)
            .ok_or_else(|| format!("host not found: {host_id}"))?
            .clone();
        let copied = vault
            .add_host(AddHost {
                folder_id: Some(source.folder_id),
                display_name: format!("{} copy", source.display_name),
                hostname: source.hostname,
                port: Some(source.port),
                username: source.username,
                identity_fingerprint: source.identity_fingerprint,
                secrets: source.secrets,
                jump_chain: source.jump_chain,
                ssh_options: source.ssh_options,
                forwards: source.forwards,
                tags: source.tags,
                notes: source.notes,
            })
            .map_err(error_message)?;
        vault
            .update_host(
                HostSelector::Id(copied.id),
                UpdateHost {
                    actions: Some(source.actions),
                    ..UpdateHost::default()
                },
            )
            .map_err(error_message)?;
        Ok(())
    })
}

#[tauri::command]
fn delete_host(host_id: Uuid, state: State<'_, AppState>) -> Result<WorkspaceSnapshot, String> {
    mutate_vault(&state, |vault| {
        vault
            .delete_host(HostSelector::Id(host_id))
            .map_err(error_message)?;
        Ok(())
    })
}

#[tauri::command]
fn move_hosts(
    host_ids: Vec<Uuid>,
    folder_id: Uuid,
    state: State<'_, AppState>,
) -> Result<WorkspaceSnapshot, String> {
    mutate_vault(&state, |vault| {
        for host_id in host_ids {
            vault
                .update_host(
                    HostSelector::Id(host_id),
                    UpdateHost {
                        folder_id: Some(folder_id),
                        ..UpdateHost::default()
                    },
                )
                .map_err(error_message)?;
        }
        Ok(())
    })
}

#[tauri::command]
fn create_folder(
    input: FolderInput,
    state: State<'_, AppState>,
) -> Result<WorkspaceSnapshot, String> {
    mutate_vault(&state, |vault| {
        vault
            .add_folder(AddFolder {
                parent_id: Some(input.parent_id),
                name: input.name,
            })
            .map_err(error_message)?;
        Ok(())
    })
}

#[tauri::command]
fn rename_folder(
    folder_id: Uuid,
    name: String,
    state: State<'_, AppState>,
) -> Result<WorkspaceSnapshot, String> {
    mutate_vault(&state, |vault| {
        vault
            .rename_folder(folder_id, name)
            .map_err(error_message)?;
        Ok(())
    })
}

#[tauri::command]
fn move_folder(
    folder_id: Uuid,
    parent_id: Uuid,
    state: State<'_, AppState>,
) -> Result<WorkspaceSnapshot, String> {
    mutate_vault(&state, |vault| {
        vault
            .move_folder(folder_id, parent_id)
            .map_err(error_message)?;
        Ok(())
    })
}

#[tauri::command]
fn delete_folder(folder_id: Uuid, state: State<'_, AppState>) -> Result<WorkspaceSnapshot, String> {
    mutate_vault(&state, |vault| {
        vault.delete_folder(folder_id).map_err(error_message)?;
        Ok(())
    })
}

#[tauri::command]
fn assign_identity(
    host_id: Uuid,
    fingerprint: String,
    state: State<'_, AppState>,
) -> Result<WorkspaceSnapshot, String> {
    mutate_vault(&state, |vault| {
        vault
            .update_host(
                HostSelector::Id(host_id),
                UpdateHost {
                    identity_fingerprint: Some(Some(fingerprint)),
                    ..UpdateHost::default()
                },
            )
            .map_err(error_message)?;
        Ok(())
    })
}

#[tauri::command]
fn clear_identity(host_id: Uuid, state: State<'_, AppState>) -> Result<WorkspaceSnapshot, String> {
    mutate_vault(&state, |vault| {
        vault
            .update_host(
                HostSelector::Id(host_id),
                UpdateHost {
                    identity_fingerprint: Some(None),
                    ..UpdateHost::default()
                },
            )
            .map_err(error_message)?;
        Ok(())
    })
}

#[tauri::command]
fn update_jumps(
    host_id: Uuid,
    jump_chain: Vec<Uuid>,
    state: State<'_, AppState>,
) -> Result<WorkspaceSnapshot, String> {
    mutate_vault(&state, |vault| {
        vault
            .update_host(
                HostSelector::Id(host_id),
                UpdateHost {
                    jump_chain: Some(jump_chain),
                    ..UpdateHost::default()
                },
            )
            .map_err(error_message)?;
        Ok(())
    })
}

#[tauri::command]
fn update_forwards(
    host_id: Uuid,
    forwards: Vec<ForwardDefinition>,
    state: State<'_, AppState>,
) -> Result<WorkspaceSnapshot, String> {
    mutate_vault(&state, |vault| {
        vault
            .update_host(
                HostSelector::Id(host_id),
                UpdateHost {
                    forwards: Some(forwards),
                    ..UpdateHost::default()
                },
            )
            .map_err(error_message)?;
        Ok(())
    })
}

#[tauri::command]
fn host_secrets(host_id: Uuid, state: State<'_, AppState>) -> Result<HostSecretsView, String> {
    workspace_with_state(&state, |workspace| host_secrets_view(workspace, host_id))
}

#[tauri::command]
fn reveal_host_secret(
    host_id: Uuid,
    field: String,
    mut master_password: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let result = workspace_with_state(&state, |workspace| {
        reveal_host_secret_value(workspace, host_id, &field, &master_password)
    });
    master_password.zeroize();
    result
}

#[tauri::command]
fn preview_ssh_command(host_id: Uuid, state: State<'_, AppState>) -> Result<SshPreview, String> {
    workspace_with_state(&state, |workspace| {
        let resolved = workspace
            .vault
            .resolve_host(HostSelector::Id(host_id))
            .map_err(error_message)?;
        let (command, temp_config) =
            prepare_openssh_command(&resolved, &workspace.local_config).map_err(error_message)?;
        Ok(SshPreview {
            command: command.render_for_display(),
            uses_temp_config: temp_config.is_some(),
        })
    })
}

#[tauri::command]
fn host_actions(host_id: Uuid, state: State<'_, AppState>) -> Result<Vec<ActionView>, String> {
    workspace_with_state(&state, |workspace| action_views(workspace, host_id))
}

#[tauri::command]
fn preview_action(
    host_id: Uuid,
    action_id: Uuid,
    state: State<'_, AppState>,
) -> Result<ActionPlanView, String> {
    workspace_with_state(&state, |workspace| {
        let resolved = workspace
            .vault
            .resolve_host(HostSelector::Id(host_id))
            .map_err(error_message)?;
        let action = action_by_id(workspace, host_id, action_id)?;
        let plan =
            resolve_action_plan(&resolved, &action, &workspace.local_config, &HashMap::new())
                .map_err(error_message)?;
        Ok(action_plan_view(&plan))
    })
}

#[tauri::command]
fn start_ssh_session(
    host_id: Uuid,
    cols: u16,
    rows: u16,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<StartSessionView, String> {
    if state.simulation {
        let shell = workspace_with_state(&state, |workspace| {
            let resolved = workspace
                .vault
                .resolve_host(HostSelector::Id(host_id))
                .map_err(error_message)?;
            Ok(SimulatedShell::for_host(&resolved))
        })?;
        return start_simulated_session(shell, None, app, &state);
    }

    let (program, args, temp_config) = workspace_with_state(&state, |workspace| {
        let resolved = workspace
            .vault
            .resolve_host(HostSelector::Id(host_id))
            .map_err(error_message)?;
        let (command, temp_config) =
            prepare_openssh_command(&resolved, &workspace.local_config).map_err(error_message)?;
        Ok((command.program, command.args, temp_config))
    })?;

    start_terminal_session(
        program,
        args,
        temp_config,
        None,
        Vec::new(),
        cols,
        rows,
        app,
        &state,
    )
}

#[tauri::command]
fn start_action_session(
    host_id: Uuid,
    action_id: Uuid,
    cols: u16,
    rows: u16,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<StartSessionView, String> {
    if state.simulation {
        let (shell, action_banner) = workspace_with_state(&state, |workspace| {
            let resolved = workspace
                .vault
                .resolve_host(HostSelector::Id(host_id))
                .map_err(error_message)?;
            let action = action_by_id(workspace, host_id, action_id)?;
            let mut banner = format!("Running simulated action: {}\r\n", action.name);
            if let Some(remote_command) = &action.remote_command {
                banner.push_str(&format!("remote command: {remote_command}\r\n"));
                banner.push_str(&simulated_remote_command_output(remote_command));
            }
            if action.local_launch.is_some() {
                banner.push_str("local launch skipped in simulation mode\r\n");
            }
            Ok((SimulatedShell::for_host(&resolved), banner))
        })?;
        return start_simulated_session(shell, Some(action_banner), app, &state);
    }

    let plan = workspace_with_state(&state, |workspace| {
        let resolved = workspace
            .vault
            .resolve_host(HostSelector::Id(host_id))
            .map_err(error_message)?;
        let action = action_by_id(workspace, host_id, action_id)?;
        let local_prepare =
            resolve_action_local_prepare(&resolved, &action, &workspace.local_config)
                .map_err(error_message)?;
        let prepare_env = run_action_prepare(local_prepare.as_ref())?;
        resolve_action_plan(&resolved, &action, &workspace.local_config, &prepare_env)
            .map_err(error_message)
    })?;

    start_terminal_session(
        plan.ssh_command.program,
        plan.ssh_command.args,
        plan.temp_config,
        plan.local_launch,
        plan.cleanup,
        cols,
        rows,
        app,
        &state,
    )
}

fn start_simulated_session(
    shell: SimulatedShell,
    prelude: Option<String>,
    _app: AppHandle,
    state: &State<'_, AppState>,
) -> Result<StartSessionView, String> {
    let session_id = Uuid::new_v4();
    let mut output = String::new();
    if let Some(prelude) = prelude {
        output.push_str(&prelude);
    }
    output.push_str(&shell.banner());
    state
        .sessions
        .lock()
        .map_err(error_message)?
        .insert(session_id, Session::Simulated(SimulatedSession { shell }));
    Ok(StartSessionView {
        session_id,
        initial_output: output,
    })
}

fn simulated_remote_command_output(command: &str) -> String {
    if command.contains("df -h") {
        "Filesystem      Size  Used Avail Use% Mounted on\r\n/dev/sim-root    80G   43G   34G  57% /\r\n/dev/sim-data   250G  121G  118G  51% /srv\r\n"
            .to_string()
    } else if command.contains("tail") && command.contains("nginx") {
        "2026/08/27 09:41:02 [warn] upstream response time exceeded simulation threshold\r\n2026/08/27 09:42:18 [info] worker process recycled cleanly\r\n"
            .to_string()
    } else if command.contains("dashboard") {
        "dashboard tunnel ready\r\n".to_string()
    } else {
        "simulated command completed successfully\r\n".to_string()
    }
}

fn start_terminal_session(
    program: OsString,
    args: Vec<OsString>,
    temp_config: Option<TempOpenSshConfig>,
    local_launch: Option<ResolvedLocalCommand>,
    cleanup: Vec<ResolvedLocalCommand>,
    cols: u16,
    rows: u16,
    app: AppHandle,
    state: &State<'_, AppState>,
) -> Result<StartSessionView, String> {
    let session_id = Uuid::new_v4();
    let pty = native_pty_system();
    let pair = pty
        .openpty(PtySize {
            rows: rows.max(8),
            cols: cols.max(20),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(error_message)?;
    let mut command = CommandBuilder::new(program);
    command.args(args);
    let mut child = pair.slave.spawn_command(command).map_err(error_message)?;
    drop(pair.slave);
    let local_child = match spawn_local_launch(local_launch.as_ref()) {
        Ok(child) => child,
        Err(error) => {
            let _ = child.kill();
            return Err(error);
        }
    };

    let mut reader = pair.master.try_clone_reader().map_err(error_message)?;
    let writer = Arc::new(Mutex::new(
        pair.master.take_writer().map_err(error_message)?,
    ));
    state.sessions.lock().map_err(error_message)?.insert(
        session_id,
        Session::Real(RealSession {
            master: pair.master,
            writer: writer.clone(),
            child,
            local_child,
            cleanup,
            _temp_config: temp_config,
        }),
    );

    thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buffer[..n]).to_string();
                    let _ = app.emit("session-output", SessionOutput { session_id, data });
                }
                Err(error) => {
                    let _ = app.emit(
                        "session-exit",
                        SessionExit {
                            session_id,
                            message: format!("terminal read failed: {error}"),
                        },
                    );
                    finish_session(app.state::<AppState>().inner(), session_id);
                    return;
                }
            }
        }
        finish_session(app.state::<AppState>().inner(), session_id);
        let _ = app.emit(
            "session-exit",
            SessionExit {
                session_id,
                message: "session closed".to_string(),
            },
        );
    });

    Ok(StartSessionView {
        session_id,
        initial_output: String::new(),
    })
}

#[tauri::command]
fn write_terminal(
    session_id: Uuid,
    data: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let (writer, close_after_write) = {
        let mut sessions = state.sessions.lock().map_err(error_message)?;
        let session = sessions
            .get_mut(&session_id)
            .ok_or_else(|| format!("session not found: {session_id}"))?;
        match session {
            Session::Real(session) => (Some(session.writer.clone()), false),
            Session::Simulated(session) => {
                let output = session.shell.handle_input(&data);
                let closed = output.closed;
                if !output.data.is_empty() {
                    app.emit(
                        "session-output",
                        SessionOutput {
                            session_id,
                            data: output.data,
                        },
                    )
                    .map_err(error_message)?;
                }
                (None, closed)
            }
        }
    };
    if let Some(writer) = writer {
        writer
            .lock()
            .map_err(error_message)?
            .write_all(data.as_bytes())
            .map_err(error_message)?;
    }
    if close_after_write {
        state
            .sessions
            .lock()
            .map_err(error_message)?
            .remove(&session_id);
        app.emit(
            "session-exit",
            SessionExit {
                session_id,
                message: "session closed".to_string(),
            },
        )
        .map_err(error_message)?;
    }
    Ok(())
}

#[tauri::command]
fn resize_terminal(
    session_id: Uuid,
    cols: u16,
    rows: u16,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let sessions = state.sessions.lock().map_err(error_message)?;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| format!("session not found: {session_id}"))?;
    match session {
        Session::Real(session) => session
            .master
            .resize(PtySize {
                rows: rows.max(8),
                cols: cols.max(20),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(error_message),
        Session::Simulated(_) => {
            let _ = (cols, rows);
            Ok(())
        }
    }
}

#[tauri::command]
fn close_session(session_id: Uuid, state: State<'_, AppState>) -> Result<(), String> {
    let session = state
        .sessions
        .lock()
        .map_err(error_message)?
        .remove(&session_id)
        .ok_or_else(|| format!("session not found: {session_id}"))?;
    cleanup_session(session);
    Ok(())
}

fn empty_to_none(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim().to_string();
        (!trimmed.is_empty()).then_some(trimmed)
    })
}

fn error_message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use stassh_core::save_secrets;

    fn workspace_with_secrets() -> (Workspace, Uuid) {
        let mut vault = Vault::new();
        let host = vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "web".to_string(),
                hostname: "web.example".to_string(),
                port: Some(22),
                username: None,
                identity_fingerprint: None,
                secrets: Some("web-prod".to_string()),
                jump_chain: Vec::new(),
                ssh_options: Vec::new(),
                forwards: Vec::new(),
                tags: Vec::new(),
                notes: None,
            })
            .unwrap();
        let (mut store, key) = SecretsStore::create("master").unwrap();
        store
            .create_set("web-prod".to_string(), Some("Production web".to_string()))
            .unwrap();
        store
            .set_plain("web-prod", "user".to_string(), "deploy".to_string())
            .unwrap();
        store
            .set_secret(&key, "web-prod", "password".to_string(), "s3cr3t")
            .unwrap();

        (
            Workspace {
                source: WorkspaceSource::Files,
                vault_path: "vault.json".into(),
                local_config_path: "local.json".into(),
                secrets_path: "secrets.json".into(),
                vault,
                local_config: LocalConfig::new(),
                secrets_store: Some(store),
            },
            host.id,
        )
    }

    fn workspace_with_actions() -> (Workspace, Uuid, Uuid, Uuid) {
        let mut vault = Vault::new();
        let common_action_id = Uuid::from_u128(0x11111111111111111111111111111111);
        let host_action_id = Uuid::from_u128(0x22222222222222222222222222222222);
        vault.actions.push(ActionDefinition {
            id: common_action_id,
            name: "Uptime".to_string(),
            local_prepare: None,
            forwards: Vec::new(),
            remote_command: Some("uptime".to_string()),
            local_launch: None,
            cleanup: Vec::new(),
        });
        let host = vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "web".to_string(),
                hostname: "web.example".to_string(),
                port: Some(22),
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
        vault
            .update_host(
                HostSelector::Id(host.id),
                UpdateHost {
                    actions: Some(vec![ActionDefinition {
                        id: host_action_id,
                        name: "Disk".to_string(),
                        local_prepare: None,
                        forwards: Vec::new(),
                        remote_command: Some("df -h".to_string()),
                        local_launch: None,
                        cleanup: Vec::new(),
                    }]),
                    ..UpdateHost::default()
                },
            )
            .unwrap();

        (
            Workspace {
                source: WorkspaceSource::Files,
                vault_path: "vault.json".into(),
                local_config_path: "local.json".into(),
                secrets_path: "secrets.json".into(),
                vault,
                local_config: LocalConfig::new(),
                secrets_store: None,
            },
            host.id,
            common_action_id,
            host_action_id,
        )
    }

    #[test]
    fn action_views_list_common_before_host_actions() {
        let (workspace, host_id, common_action_id, host_action_id) = workspace_with_actions();

        let actions = action_views(&workspace, host_id).unwrap();

        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].id, common_action_id);
        assert_eq!(actions[0].origin, "common");
        assert_eq!(actions[1].id, host_action_id);
        assert_eq!(actions[1].origin, "host");
    }

    #[test]
    fn action_plan_view_reports_resolved_dry_run_command() {
        let (workspace, host_id, _common_action_id, host_action_id) = workspace_with_actions();
        let resolved = workspace
            .vault
            .resolve_host(HostSelector::Id(host_id))
            .unwrap();
        let action = action_by_id(&workspace, host_id, host_action_id).unwrap();
        let plan =
            resolve_action_plan(&resolved, &action, &workspace.local_config, &HashMap::new())
                .unwrap();

        let view = action_plan_view(&plan);

        assert_eq!(view.action_name, "Disk");
        assert!(view.ssh_command.contains("web.example"));
        assert!(view.ssh_command.contains("df -h"));
        assert_eq!(view.local_prepare, None);
        assert_eq!(view.local_launch, None);
    }

    #[test]
    fn simulation_workspace_snapshot_uses_virtual_paths_and_mapped_identities_exist() {
        let workspace = load_simulation_workspace().unwrap();
        let snapshot = snapshot(&workspace);

        assert_eq!(snapshot.vault_path, "simulation://vault.json");
        assert!(
            snapshot
                .hosts
                .iter()
                .any(|host| host.display_name == "web-prod-01")
        );
        assert!(snapshot.identities.iter().all(|identity| identity.exists));
        assert!(
            snapshot
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("missing identity mapping"))
        );
    }

    #[test]
    fn simulated_remote_command_output_is_stable_for_common_actions() {
        assert!(simulated_remote_command_output("df -h").contains("/dev/sim-root"));
        assert!(
            simulated_remote_command_output("tail -n 80 /var/log/nginx/error.log")
                .contains("upstream response time exceeded")
        );
        assert!(
            simulated_remote_command_output("echo dashboard tunnel ready")
                .contains("dashboard tunnel ready")
        );
    }

    #[test]
    fn host_secrets_lists_plain_fields_without_secret_plaintext() {
        let (workspace, host_id) = workspace_with_secrets();

        let view = host_secrets_view(&workspace, host_id).unwrap();

        assert_eq!(view.host_id, host_id);
        assert_eq!(view.set_key, "web-prod");
        assert_eq!(view.label.as_deref(), Some("Production web"));
        assert_eq!(
            view.fields,
            vec![
                SecretFieldView {
                    name: "password".to_string(),
                    kind: "secret",
                    plain_value: None,
                },
                SecretFieldView {
                    name: "user".to_string(),
                    kind: "plain",
                    plain_value: Some("deploy".to_string()),
                },
            ]
        );
    }

    #[test]
    fn host_secrets_errors_when_store_is_missing() {
        let (mut workspace, host_id) = workspace_with_secrets();
        workspace.secrets_store = None;

        let error = host_secrets_view(&workspace, host_id).unwrap_err();

        assert!(error.contains("secrets store not found"));
    }

    #[test]
    fn host_secrets_errors_when_set_is_missing() {
        let (mut workspace, host_id) = workspace_with_secrets();
        workspace.secrets_store = Some(SecretsStore::create("master").unwrap().0);

        let error = host_secrets_view(&workspace, host_id).unwrap_err();

        assert!(error.contains("secrets set not found"));
    }

    #[test]
    fn reveal_host_secret_returns_plaintext_for_correct_password() {
        let (workspace, host_id) = workspace_with_secrets();

        let value = reveal_host_secret_value(&workspace, host_id, "password", "master").unwrap();

        assert_eq!(value, "s3cr3t");
    }

    #[test]
    fn reveal_host_secret_errors_for_wrong_password() {
        let (workspace, host_id) = workspace_with_secrets();

        let error = reveal_host_secret_value(&workspace, host_id, "password", "wrong").unwrap_err();

        assert!(error.contains("wrong master password"));
    }

    #[test]
    fn reveal_host_secret_errors_for_plain_field() {
        let (workspace, host_id) = workspace_with_secrets();

        let error = reveal_host_secret_value(&workspace, host_id, "user", "master").unwrap_err();

        assert!(error.contains("field is not encrypted"));
    }

    #[test]
    fn reveal_host_secret_errors_for_unknown_field() {
        let (workspace, host_id) = workspace_with_secrets();

        let error = reveal_host_secret_value(&workspace, host_id, "missing", "master").unwrap_err();

        assert!(error.contains("secret field not found"));
    }

    #[test]
    fn saved_secret_store_does_not_include_plaintext_secret() {
        let (workspace, _) = workspace_with_secrets();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.json");
        save_secrets(&path, workspace.secrets_store.as_ref().unwrap()).unwrap();

        let saved = std::fs::read_to_string(path).unwrap();

        assert!(!saved.contains("s3cr3t"));
    }
}
