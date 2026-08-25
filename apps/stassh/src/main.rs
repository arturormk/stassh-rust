use std::collections::HashMap;
use std::io;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use std::{env, fs};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use stassh_core::model::{ActionDefinition, ResolvedHost};
use stassh_core::openssh::{
    command_for_config, command_for_host, config_for_host_with_identity_path,
};
use stassh_core::{
    AddFolder, AddHost, ForwardDefinition, IdentityImportContext, OpenSshIdentityResolver,
    ResolvedActionPlan, ResolvedLocalCommand, UpdateHost, Vault, derive_identity_from_file,
    ensure_home_stassh_permissions, export_openssh_config, import_openssh_config_with_identities,
    load_local_config, load_vault, local_config_path, parse_prepare_env, prepare_openssh_command,
    read_openssh_config_with_includes, resolve_action_local_prepare, resolve_action_plan,
    save_local_config, save_vault, selector, vault_path,
};
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(name = "stassh")]
#[command(version = env!("STASSH_VERSION"))]
#[command(about = "Portable, offline-first SSH workspace")]
struct Cli {
    #[arg(long, global = true, value_name = "PATH")]
    vault: Option<PathBuf>,

    #[arg(long, global = true, value_name = "PATH")]
    local_config: Option<PathBuf>,

    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Text)]
    output: OutputFormat,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

impl OutputFormat {
    fn is_json(self) -> bool {
        self == OutputFormat::Json
    }
}

fn print_json(value: Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn folder_json(folder: &stassh_core::Folder, path: &str) -> Value {
    json!({
        "id": folder.id,
        "parent_id": folder.parent_id,
        "name": folder.name,
        "path": path,
    })
}

fn host_json(host: &stassh_core::Host, path: &str) -> Value {
    json!({
        "id": host.id,
        "folder_id": host.folder_id,
        "display_name": host.display_name,
        "path": path,
        "hostname": host.hostname,
        "port": host.port,
        "username": host.username,
        "identity_fingerprint": host.identity_fingerprint,
        "jump_chain": host.jump_chain,
        "ssh_options": host.ssh_options,
        "forwards": host.forwards,
        "actions": host.actions,
        "tags": host.tags,
        "notes": host.notes,
    })
}

fn resolved_host_json(
    host: &ResolvedHost,
    local_config: Option<&stassh_core::LocalConfig>,
) -> Value {
    let identity_mapping = host.identity_fingerprint.as_ref().and_then(|fingerprint| {
        local_config.map(|local_config| identity_mapping_diagnosis_json(fingerprint, local_config))
    });

    json!({
        "id": host.id,
        "path": host.path,
        "display_name": host.display_name,
        "hostname": host.hostname,
        "port": host.port,
        "username": host.username,
        "identity_fingerprint": host.identity_fingerprint,
        "identity_mapping": identity_mapping,
        "jump_chain": host.jump_chain.iter().map(resolved_jump_json).collect::<Vec<_>>(),
        "ssh_options": host.ssh_options,
        "forwards": host.forwards,
        "actions": host.actions,
        "tags": host.tags,
        "notes": host.notes,
    })
}

fn resolved_jump_json(jump: &stassh_core::model::ResolvedJump) -> Value {
    json!({
        "id": jump.id,
        "display_name": jump.display_name,
        "hostname": jump.hostname,
        "port": jump.port,
        "username": jump.username,
    })
}

fn identity_mapping_json(mapping: &stassh_core::IdentityMapping) -> Value {
    json!({
        "fingerprint": mapping.fingerprint,
        "preferred_name": mapping.preferred_name,
        "path": mapping.path,
        "exists": mapping.path.exists(),
    })
}

fn identity_mapping_diagnosis_json(
    fingerprint: &str,
    local_config: &stassh_core::LocalConfig,
) -> Value {
    match local_config
        .identity_mappings
        .iter()
        .find(|mapping| mapping.fingerprint == fingerprint)
    {
        Some(mapping) => identity_mapping_json(mapping),
        None => json!({
            "fingerprint": fingerprint,
            "preferred_name": null,
            "path": null,
            "exists": false,
        }),
    }
}

fn command_json(command: &stassh_core::OpenSshCommand) -> Value {
    json!({
        "program": command.program.to_string_lossy(),
        "args": command
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>(),
        "display": command.render_for_display(),
    })
}

fn config_json(config: &stassh_core::OpenSshConfig) -> Value {
    json!({
        "alias": config.alias,
        "contents": config.contents,
    })
}

fn print_duplicate_host_groups(groups: &[stassh_core::DuplicateHostGroup]) {
    println!("duplicate groups: {}", groups.len());
    for group in groups {
        println!("{}: {}", duplicate_kind_label(&group.kind), group.key);
        for host in &group.hosts {
            println!(
                "  - {}\t{}\t{}@{}:{}",
                host.id,
                host.path,
                host.username.as_deref().unwrap_or("(default)"),
                host.hostname,
                host.port
            );
        }
    }
}

fn duplicate_kind_label(kind: &stassh_core::DuplicateHostKind) -> &'static str {
    match kind {
        stassh_core::DuplicateHostKind::Path => "path",
        stassh_core::DuplicateHostKind::Connection => "connection",
    }
}

fn print_host_dedupe_plan(plan: &stassh_core::HostDedupePlan, apply: bool) {
    println!("dedupe strategy: path");
    println!("mode: {}", if apply { "apply" } else { "dry-run" });
    println!("duplicate path groups: {}", plan.groups.len());
    println!("hosts to remove: {}", plan.remove_count);
    for group in &plan.groups {
        println!("path: {}", group.path);
        println!(
            "  keep: {}\t{}\t{}@{}:{}",
            group.keep.id,
            group.keep.path,
            group.keep.username.as_deref().unwrap_or("(default)"),
            group.keep.hostname,
            group.keep.port
        );
        for host in &group.remove {
            println!(
                "  remove: {}\t{}\t{}@{}:{}",
                host.id,
                host.path,
                host.username.as_deref().unwrap_or("(default)"),
                host.hostname,
                host.port
            );
        }
    }
}

fn print_host_dedupe_result(result: &stassh_core::HostDedupeResult) {
    println!("removed hosts: {}", result.removed_count);
    println!(
        "rewritten jump references: {}",
        result.rewritten_jump_references
    );
    for host in &result.removed {
        println!(
            "  - {}\t{}\t{}@{}:{}",
            host.id,
            host.path,
            host.username.as_deref().unwrap_or("(default)"),
            host.hostname,
            host.port
        );
    }
}

fn find_action<'a>(host: &'a ResolvedHost, selector: &str) -> Result<&'a ActionDefinition> {
    if let Ok(id) = Uuid::parse_str(selector) {
        return host
            .actions
            .iter()
            .find(|action| action.id == id)
            .with_context(|| format!("action not found: {selector}"));
    }

    let exact = host
        .actions
        .iter()
        .filter(|action| action.name == selector)
        .collect::<Vec<_>>();
    match exact.as_slice() {
        [action] => Ok(action),
        [] => {
            let matches = host
                .actions
                .iter()
                .filter(|action| action.name.eq_ignore_ascii_case(selector))
                .collect::<Vec<_>>();
            match matches.as_slice() {
                [action] => Ok(action),
                [] => bail!("action not found: {selector}"),
                many => bail!(
                    "more than one action matched {selector}: {}",
                    many.iter()
                        .map(|action| action.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            }
        }
        many => bail!(
            "more than one action matched {selector}: {}",
            many.iter()
                .map(|action| action.id.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn action_plan_json(plan: &ResolvedActionPlan) -> Value {
    json!({
        "action_name": plan.action_name,
        "allocated_ports": plan.allocated_ports,
        "ssh_command": command_json(&plan.ssh_command),
        "temp_config_path": plan.temp_config.as_ref().map(|config| config.path()),
        "local_prepare": plan.local_prepare.as_ref().map(local_command_json),
        "local_launch": plan.local_launch.as_ref().map(local_command_json),
        "cleanup": plan.cleanup.iter().map(local_command_json).collect::<Vec<_>>(),
    })
}

fn local_command_json(command: &ResolvedLocalCommand) -> Value {
    json!({
        "program": command.program,
        "args": command.args,
        "env": command.env,
        "display": display_local_command(command),
    })
}

fn print_action_plan(plan: &ResolvedActionPlan) {
    println!("Action: {}", plan.action_name);
    if plan.allocated_ports.is_empty() {
        println!("Allocated ports: (none)");
    } else {
        println!("Allocated ports:");
        let mut ports = plan.allocated_ports.iter().collect::<Vec<_>>();
        ports.sort_by(|left, right| left.0.cmp(right.0));
        for (name, port) in ports {
            println!("  {name}: {port}");
        }
    }
    if let Some(command) = &plan.local_prepare {
        println!("Local prepare:");
        println!("  {}", display_local_command(command));
    }
    println!("SSH command:");
    println!("  {}", plan.ssh_command.render_for_display());
    if let Some(config) = &plan.temp_config {
        println!("Temporary SSH config: {}", config.path().display());
    }
    if let Some(command) = &plan.local_launch {
        println!("Local launch:");
        println!("  {}", display_local_command(command));
    }
    if !plan.cleanup.is_empty() {
        println!("Cleanup:");
        for command in &plan.cleanup {
            println!("  {}", display_local_command(command));
        }
    }
}

struct ActionRunResult {
    status: ExitStatus,
    local_exit: Option<ExitStatus>,
}

fn run_action_prepare(command: Option<&ResolvedLocalCommand>) -> Result<HashMap<String, String>> {
    let Some(command) = command else {
        return Ok(HashMap::new());
    };
    eprintln!("running local prepare: {}", display_local_command(command));
    let output = local_command(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .context("failed to run local prepare")?;
    if !output.status.success() {
        bail!("local prepare exited with status {}", output.status);
    }
    Ok(parse_prepare_env(&String::from_utf8_lossy(&output.stdout)))
}

fn run_action_plan(plan: ResolvedActionPlan) -> Result<ActionRunResult> {
    let mut ssh_child = Command::new(&plan.ssh_command.program)
        .args(&plan.ssh_command.args)
        .spawn()
        .context("failed to launch ssh")?;
    let mut local_child = spawn_local_launch(plan.local_launch.as_ref())?;
    let mut local_exit = None;

    let status = loop {
        if let Some(child) = &mut local_child
            && local_exit.is_none()
            && let Some(status) = child.try_wait().context("failed to poll local command")?
        {
            eprintln!("local command exited early with status {status}");
            local_exit = Some(status);
        }
        if let Some(status) = ssh_child.try_wait().context("failed to poll ssh")? {
            break status;
        }
        thread::sleep(Duration::from_millis(100));
    };

    if let Some(child) = &mut local_child
        && local_exit.is_none()
    {
        terminate_child_tree(child);
        local_exit = child.try_wait().ok().flatten();
    }
    for command in &plan.cleanup {
        let cleanup_status = local_command(command).status().with_context(|| {
            format!("failed to run cleanup: {}", display_local_command(command))
        })?;
        if !cleanup_status.success() {
            eprintln!("cleanup exited with status {cleanup_status}");
        }
    }

    Ok(ActionRunResult { status, local_exit })
}

fn spawn_local_launch(command: Option<&ResolvedLocalCommand>) -> Result<Option<Child>> {
    let Some(command) = command else {
        return Ok(None);
    };
    eprintln!(
        "launching local command: {}",
        display_local_command(command)
    );
    let mut process = local_command(command);
    #[cfg(unix)]
    prepare_local_child_group(&mut process);
    process
        .spawn()
        .map(Some)
        .context("failed to launch local command")
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

#[cfg(unix)]
fn prepare_local_child_group(command: &mut Command) {
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(unix)]
fn terminate_child_tree(child: &mut Child) {
    let pgrp = child.id() as libc::pid_t;
    unsafe {
        libc::kill(-pgrp, libc::SIGTERM);
    }
    wait_or_kill(child, Duration::from_secs(2));
}

#[cfg(not(unix))]
fn terminate_child_tree(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn wait_or_kill(child: &mut Child, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
            Err(_) => return,
        }
    }
}

fn missing_identity_mappings(vault: &Vault, local_config: &stassh_core::LocalConfig) -> Vec<Value> {
    vault
        .hosts
        .iter()
        .filter_map(|host| {
            let fingerprint = host.identity_fingerprint.as_ref()?;
            if local_config.identity_path(fingerprint).is_some() {
                return None;
            }
            Some(json!({
                "host_id": host.id,
                "host_path": vault.host_path(host),
                "fingerprint": fingerprint,
            }))
        })
        .collect()
}

fn missing_identity_files(local_config: &stassh_core::LocalConfig) -> Vec<Value> {
    local_config
        .identity_mappings
        .iter()
        .filter(|mapping| !mapping.path.exists())
        .map(|mapping| {
            json!({
                "fingerprint": mapping.fingerprint,
                "preferred_name": mapping.preferred_name,
                "path": mapping.path,
            })
        })
        .collect()
}

fn raw_identity_file_options(vault: &Vault) -> Vec<Value> {
    vault
        .hosts
        .iter()
        .flat_map(|host| {
            host.ssh_options
                .iter()
                .filter(|option| {
                    raw_ssh_option_keyword(option).eq_ignore_ascii_case("IdentityFile")
                })
                .map(|option| {
                    json!({
                        "host_id": host.id,
                        "host_path": vault.host_path(host),
                        "option": option,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn raw_ssh_option_keyword(option: &str) -> &str {
    option
        .trim()
        .split_once(char::is_whitespace)
        .map(|(keyword, _)| keyword)
        .or_else(|| {
            option
                .trim()
                .split_once('=')
                .map(|(keyword, _)| keyword.trim())
        })
        .unwrap_or_else(|| option.trim())
}

fn vault_validation_json(vault: &Vault) -> Value {
    match vault.validate() {
        Ok(()) => json!({
            "ok": true,
            "error": null,
        }),
        Err(error) => json!({
            "ok": false,
            "error": error.to_string(),
        }),
    }
}

fn local_config_validation_json(local_config: &stassh_core::LocalConfig) -> Value {
    match local_config.validate() {
        Ok(()) => json!({
            "ok": true,
            "error": null,
        }),
        Err(error) => json!({
            "ok": false,
            "error": error.to_string(),
        }),
    }
}

fn print_vault_check_report(
    vault_validation: &Value,
    local_config_validation: &Value,
    duplicate_groups: &[stassh_core::DuplicateHostGroup],
    dedupe_plan: &stassh_core::HostDedupePlan,
    missing_identity_mappings: &[Value],
    missing_identity_files: &[Value],
    raw_identity_file_options: &[Value],
) {
    println!("vault validation: {}", check_ok_label(vault_validation));
    println!(
        "local config validation: {}",
        check_ok_label(local_config_validation)
    );
    println!("duplicate groups: {}", duplicate_groups.len());
    println!("dedupe removable hosts: {}", dedupe_plan.remove_count);
    println!(
        "missing identity mappings: {}",
        missing_identity_mappings.len()
    );
    println!("missing identity files: {}", missing_identity_files.len());
    println!(
        "raw IdentityFile options: {}",
        raw_identity_file_options.len()
    );

    if !duplicate_groups.is_empty() {
        println!();
        print_duplicate_host_groups(duplicate_groups);
    }
    if dedupe_plan.remove_count > 0 {
        println!();
        print_host_dedupe_plan(dedupe_plan, false);
    }
    print_value_findings("missing identity mappings", missing_identity_mappings);
    print_value_findings("missing identity files", missing_identity_files);
    print_value_findings("raw IdentityFile options", raw_identity_file_options);
}

fn check_ok_label(validation: &Value) -> String {
    if validation
        .get("ok")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        "ok".to_string()
    } else {
        validation
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("failed")
            .to_string()
    }
}

fn print_value_findings(label: &str, findings: &[Value]) {
    if findings.is_empty() {
        return;
    }
    println!();
    println!("{label}:");
    for finding in findings {
        println!("  - {}", finding);
    }
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(subcommand)]
    Vault(VaultCommands),
    #[command(subcommand)]
    Folder(FolderCommands),
    #[command(subcommand)]
    Host(Box<HostCommands>),
    List,
    Search(SearchArgs),
    Show(HostSelectorArgs),
    Diagnose(HostSelectorArgs),
    Connect(HostSelectorArgs),
    Action(ActionArgs),
    #[command(subcommand)]
    Identity(IdentityCommands),
    #[command(subcommand)]
    Import(ImportCommands),
    #[command(subcommand)]
    Export(ExportCommands),
}

#[derive(Debug, Subcommand)]
enum VaultCommands {
    Init,
    Status,
    Check,
    Duplicates,
    Dedupe(DedupeArgs),
}

#[derive(Debug, Subcommand)]
enum FolderCommands {
    List,
    Add(AddFolderArgs),
    Rename(RenameFolderArgs),
    Move(MoveFolderArgs),
    Delete(DeleteFolderArgs),
}

#[derive(Debug, Subcommand)]
enum HostCommands {
    Add(AddHostArgs),
    Edit(EditHostArgs),
    Delete(HostSelectorArgs),
}

#[derive(Debug, Subcommand)]
enum IdentityCommands {
    Add(AddIdentityArgs),
    List,
    Map(MapIdentityArgs),
    Edit(EditIdentityArgs),
    Rename(RenameIdentityArgs),
    Unmap(UnmapIdentityArgs),
    Diagnose(DiagnoseIdentityArgs),
}

#[derive(Debug, Subcommand)]
enum ImportCommands {
    Openssh(ImportOpenSshArgs),
}

#[derive(Debug, Subcommand)]
enum ExportCommands {
    Openssh(ExportOpenSshArgs),
}

#[derive(Debug, Args)]
struct ImportOpenSshArgs {
    path: PathBuf,
}

#[derive(Debug, Args)]
struct ExportOpenSshArgs {
    path: String,
}

#[derive(Debug, Args)]
struct DedupeArgs {
    #[arg(long)]
    apply: bool,
}

#[derive(Debug, Args)]
struct MapIdentityArgs {
    fingerprint: String,
    path: PathBuf,

    #[arg(long)]
    name: Option<String>,
}

#[derive(Debug, Args)]
struct AddIdentityArgs {
    path: PathBuf,

    #[arg(long)]
    name: Option<String>,
}

#[derive(Debug, Args)]
struct EditIdentityArgs {
    fingerprint: String,

    #[arg(long)]
    path: Option<PathBuf>,

    #[arg(long)]
    name: Option<String>,

    #[arg(long)]
    clear_name: bool,
}

#[derive(Debug, Args)]
struct RenameIdentityArgs {
    fingerprint: String,
    name: String,
}

#[derive(Debug, Args)]
struct UnmapIdentityArgs {
    fingerprint: String,
}

#[derive(Debug, Args)]
struct DiagnoseIdentityArgs {
    fingerprint: String,
}

#[derive(Debug, Args)]
struct AddFolderArgs {
    name: String,

    #[arg(long)]
    parent: Option<Uuid>,
}

#[derive(Debug, Args)]
struct AddHostArgs {
    display_name: String,
    hostname: String,

    #[arg(long)]
    folder: Option<Uuid>,

    #[arg(long)]
    port: Option<u16>,

    #[arg(long)]
    user: Option<String>,

    #[arg(long)]
    identity_fingerprint: Option<String>,

    #[arg(long)]
    identity_name: Option<String>,

    #[arg(long)]
    identity_file: Option<PathBuf>,

    #[arg(long = "tag")]
    tags: Vec<String>,

    #[arg(long)]
    notes: Option<String>,

    #[arg(long = "ssh-option")]
    ssh_options: Vec<String>,

    #[arg(long = "jump")]
    jumps: Vec<String>,

    #[command(flatten)]
    forwards: ForwardCliArgs,
}

#[derive(Debug, Args)]
struct EditHostArgs {
    selector: String,

    #[arg(long)]
    name: Option<String>,

    #[arg(long)]
    hostname: Option<String>,

    #[arg(long)]
    folder: Option<Uuid>,

    #[arg(long)]
    port: Option<u16>,

    #[arg(long)]
    user: Option<String>,

    #[arg(long)]
    clear_user: bool,

    #[arg(long)]
    identity_fingerprint: Option<String>,

    #[arg(long)]
    identity_name: Option<String>,

    #[arg(long)]
    identity_file: Option<PathBuf>,

    #[arg(long)]
    clear_identity: bool,

    #[arg(long = "jump")]
    jumps: Vec<String>,

    #[arg(long)]
    clear_jumps: bool,

    #[arg(long = "ssh-option")]
    ssh_options: Vec<String>,

    #[arg(long)]
    clear_ssh_options: bool,

    #[arg(long = "tag")]
    tags: Vec<String>,

    #[arg(long)]
    clear_tags: bool,

    #[arg(long)]
    notes: Option<String>,

    #[arg(long)]
    clear_notes: bool,

    #[command(flatten)]
    forwards: ForwardCliArgs,

    #[arg(long)]
    clear_forwards: bool,
}

#[derive(Debug, Args)]
struct RenameFolderArgs {
    folder_id: Uuid,
    name: String,
}

#[derive(Debug, Args)]
struct MoveFolderArgs {
    folder_id: Uuid,

    #[arg(long)]
    parent: Uuid,
}

#[derive(Debug, Args)]
struct DeleteFolderArgs {
    folder_id: Uuid,
}

#[derive(Debug, Args, Default)]
struct ForwardCliArgs {
    #[arg(
        long = "local-forward",
        value_name = "BIND:LOCAL_PORT:DEST_HOST:DEST_PORT"
    )]
    local_forwards: Vec<String>,

    #[arg(
        long = "remote-forward",
        value_name = "BIND:REMOTE_PORT:DEST_HOST:DEST_PORT"
    )]
    remote_forwards: Vec<String>,

    #[arg(long = "dynamic-forward", value_name = "BIND:LOCAL_PORT")]
    dynamic_forwards: Vec<String>,
}

#[derive(Debug, Args)]
struct SearchArgs {
    query: String,
}

#[derive(Debug, Args)]
struct HostSelectorArgs {
    selector: String,
}

#[derive(Debug, Args)]
struct ActionArgs {
    host: String,
    action: String,

    #[arg(long)]
    dry_run: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let vault_path = vault_path(cli.vault)?;
    let local_config_path = local_config_path(cli.local_config, &vault_path);
    ensure_home_stassh_permissions(&[&vault_path, &local_config_path])
        .with_context(|| "unsafe ~/.ssh/stassh permissions")?;
    let output = cli.output;

    match cli.command {
        Commands::Vault(VaultCommands::Init) => {
            if vault_path.exists() {
                bail!("vault already exists at {}", vault_path.display());
            }
            let vault = Vault::new();
            save_vault(&vault_path, &vault)?;
            if output.is_json() {
                print_json(json!({
                    "status": "initialized",
                    "vault_path": vault_path,
                    "local_config_path": local_config_path,
                    "vault": vault,
                }))?;
            } else {
                println!("initialized vault: {}", vault_path.display());
                println!("local config: {}", local_config_path.display());
            }
        }
        Commands::Vault(VaultCommands::Status) => {
            let vault = load_vault(&vault_path)?;
            if output.is_json() {
                print_json(json!({
                    "vault_path": vault_path,
                    "local_config_path": local_config_path,
                    "format_version": vault.format_version,
                    "actions": vault.actions,
                    "actions_count": vault.actions.len(),
                    "folders": vault.folders.len(),
                    "hosts": vault.hosts.len(),
                }))?;
            } else {
                println!("vault: {}", vault_path.display());
                println!("local config: {}", local_config_path.display());
                println!("format_version: {}", vault.format_version);
                println!("actions: {}", vault.actions.len());
                println!("folders: {}", vault.folders.len());
                println!("hosts: {}", vault.hosts.len());
            }
        }
        Commands::Vault(VaultCommands::Check) => {
            let vault = load_vault(&vault_path)?;
            let local_path = local_config_path.clone();
            let local_config = load_local_config(&local_path)?;
            let vault_validation = vault_validation_json(&vault);
            let local_config_validation = local_config_validation_json(&local_config);
            let duplicate_groups = vault.duplicate_hosts();
            let dedupe_plan = vault.host_dedupe_plan();
            let missing_identity_mappings = missing_identity_mappings(&vault, &local_config);
            let missing_identity_files = missing_identity_files(&local_config);
            let raw_identity_file_options = raw_identity_file_options(&vault);

            if output.is_json() {
                print_json(json!({
                    "vault_path": vault_path,
                    "local_config_path": local_path,
                    "vault_validation": vault_validation,
                    "local_config_validation": local_config_validation,
                    "duplicate_groups": duplicate_groups,
                    "duplicate_group_count": duplicate_groups.len(),
                    "dedupe_plan": dedupe_plan,
                    "missing_identity_mappings": missing_identity_mappings,
                    "missing_identity_mapping_count": missing_identity_mappings.len(),
                    "missing_identity_files": missing_identity_files,
                    "missing_identity_file_count": missing_identity_files.len(),
                    "raw_identity_file_options": raw_identity_file_options,
                    "raw_identity_file_option_count": raw_identity_file_options.len(),
                }))?;
            } else {
                println!("vault: {}", vault_path.display());
                println!("local config: {}", local_path.display());
                print_vault_check_report(
                    &vault_validation,
                    &local_config_validation,
                    &duplicate_groups,
                    &dedupe_plan,
                    &missing_identity_mappings,
                    &missing_identity_files,
                    &raw_identity_file_options,
                );
            }
        }
        Commands::Vault(VaultCommands::Duplicates) => {
            let vault = load_vault(&vault_path)?;
            let duplicate_groups = vault.duplicate_hosts();
            if output.is_json() {
                print_json(json!({
                    "duplicate_groups": duplicate_groups,
                    "duplicate_group_count": duplicate_groups.len(),
                }))?;
            } else {
                print_duplicate_host_groups(&duplicate_groups);
            }
        }
        Commands::Vault(VaultCommands::Dedupe(args)) => {
            let mut vault = load_vault(&vault_path)?;
            let plan = vault.host_dedupe_plan();
            if args.apply {
                let result = vault.apply_host_dedupe_plan(&plan);
                save_vault(&vault_path, &vault)?;
                if output.is_json() {
                    print_json(json!({
                        "mode": "apply",
                        "plan": plan,
                        "result": result,
                    }))?;
                } else {
                    print_host_dedupe_plan(&plan, true);
                    print_host_dedupe_result(&result);
                }
            } else if output.is_json() {
                print_json(json!({
                    "mode": "dry_run",
                    "plan": plan,
                }))?;
            } else {
                print_host_dedupe_plan(&plan, false);
                println!("rerun with --apply to remove the planned path duplicates");
            }
        }
        Commands::Folder(FolderCommands::Add(args)) => {
            let mut vault = load_vault(&vault_path)?;
            let folder = vault.add_folder(AddFolder {
                parent_id: args.parent,
                name: args.name,
            })?;
            let path = vault.folder_path(folder.id);
            save_vault(&vault_path, &vault)?;
            if output.is_json() {
                print_json(json!({
                    "status": "added",
                    "folder": folder_json(&folder, &path),
                }))?;
            } else {
                println!("added folder: {} {}", folder.id, folder.name);
            }
        }
        Commands::Folder(FolderCommands::List) => {
            let vault = load_vault(&vault_path)?;
            if output.is_json() {
                let folders = vault
                    .folders
                    .iter()
                    .map(|folder| folder_json(folder, &vault.folder_path(folder.id)))
                    .collect::<Vec<_>>();
                print_json(json!({ "folders": folders }))?;
            } else {
                for folder in &vault.folders {
                    println!(
                        "{}\t{}\t{}",
                        folder.id,
                        folder
                            .parent_id
                            .map(|id| id.to_string())
                            .unwrap_or_else(|| "(none)".to_string()),
                        vault.folder_path(folder.id)
                    );
                }
            }
        }
        Commands::Folder(FolderCommands::Rename(args)) => {
            let mut vault = load_vault(&vault_path)?;
            let folder = vault.rename_folder(args.folder_id, args.name)?;
            let path = vault.folder_path(folder.id);
            save_vault(&vault_path, &vault)?;
            if output.is_json() {
                print_json(json!({
                    "status": "renamed",
                    "folder": folder_json(&folder, &path),
                }))?;
            } else {
                println!("renamed folder: {} {}", folder.id, path);
            }
        }
        Commands::Folder(FolderCommands::Move(args)) => {
            let mut vault = load_vault(&vault_path)?;
            let folder = vault.move_folder(args.folder_id, args.parent)?;
            let path = vault.folder_path(folder.id);
            save_vault(&vault_path, &vault)?;
            if output.is_json() {
                print_json(json!({
                    "status": "moved",
                    "folder": folder_json(&folder, &path),
                }))?;
            } else {
                println!("moved folder: {} {}", folder.id, path);
            }
        }
        Commands::Folder(FolderCommands::Delete(args)) => {
            let mut vault = load_vault(&vault_path)?;
            let folder = vault.delete_folder(args.folder_id)?;
            save_vault(&vault_path, &vault)?;
            if output.is_json() {
                print_json(json!({
                    "status": "deleted",
                    "folder": folder,
                }))?;
            } else {
                println!("deleted folder: {} {}", folder.id, folder.name);
            }
        }
        Commands::Host(command) => match *command {
            HostCommands::Add(args) => {
                let mut vault = load_vault(&vault_path)?;
                let mut local_config = load_local_config(&local_config_path)?;
                let identity_fingerprint = identity_from_host_args(
                    args.identity_fingerprint,
                    args.identity_name,
                    args.identity_file,
                    &mut local_config,
                )?;
                let jump_chain = resolve_jump_ids(&vault, &args.jumps)?;
                let forwards = parse_forward_args(&args.forwards)?;
                let host = vault.add_host(AddHost {
                    folder_id: args.folder,
                    display_name: args.display_name,
                    hostname: args.hostname,
                    port: args.port,
                    username: args.user,
                    identity_fingerprint,
                    jump_chain,
                    ssh_options: args.ssh_options,
                    forwards,
                    tags: args.tags,
                    notes: args.notes,
                })?;
                let path = vault.host_path(&host);
                save_vault(&vault_path, &vault)?;
                save_local_config(&local_config_path, &local_config)?;
                if output.is_json() {
                    print_json(json!({
                        "status": "added",
                        "host": host_json(&host, &path),
                    }))?;
                } else {
                    println!("added host: {} {}", host.id, path);
                }
            }
            HostCommands::Edit(args) => {
                let mut vault = load_vault(&vault_path)?;
                let jump_chain = if args.clear_jumps {
                    if !args.jumps.is_empty() {
                        bail!("--clear-jumps cannot be combined with --jump");
                    }
                    Some(Vec::new())
                } else if args.jumps.is_empty() {
                    None
                } else {
                    Some(resolve_jump_ids(&vault, &args.jumps)?)
                };
                let forwards = parse_forward_args(&args.forwards)?;
                let forwards = if args.clear_forwards {
                    if !forwards.is_empty() {
                        bail!(
                            "--clear-forwards cannot be combined with --local-forward, --remote-forward, or --dynamic-forward"
                        );
                    }
                    Some(Vec::new())
                } else if forwards.is_empty() {
                    None
                } else {
                    Some(forwards)
                };
                let mut local_config = load_local_config(&local_config_path)?;
                let identity_fingerprint = identity_update_from_args(
                    args.identity_fingerprint,
                    args.identity_name,
                    args.identity_file,
                    args.clear_identity,
                    &mut local_config,
                )?;
                let username =
                    clearable_value(args.user, args.clear_user, "--user", "--clear-user")?;
                let notes =
                    clearable_value(args.notes, args.clear_notes, "--notes", "--clear-notes")?;
                let ssh_options = clearable_vec(
                    args.ssh_options,
                    args.clear_ssh_options,
                    "--ssh-option",
                    "--clear-ssh-options",
                )?;
                let tags = clearable_vec(args.tags, args.clear_tags, "--tag", "--clear-tags")?;

                let host = vault.update_host(
                    selector(&args.selector),
                    UpdateHost {
                        folder_id: args.folder,
                        display_name: args.name,
                        hostname: args.hostname,
                        port: args.port,
                        username,
                        identity_fingerprint,
                        jump_chain,
                        ssh_options,
                        forwards,
                        actions: None,
                        tags,
                        notes,
                    },
                )?;
                let path = vault.host_path(&host);
                save_vault(&vault_path, &vault)?;
                save_local_config(&local_config_path, &local_config)?;
                if output.is_json() {
                    print_json(json!({
                        "status": "updated",
                        "host": host_json(&host, &path),
                    }))?;
                } else {
                    println!("updated host: {} {}", host.id, path);
                }
            }
            HostCommands::Delete(args) => {
                let mut vault = load_vault(&vault_path)?;
                let host = vault.delete_host(selector(&args.selector))?;
                save_vault(&vault_path, &vault)?;
                if output.is_json() {
                    print_json(json!({
                        "status": "deleted",
                        "host": host,
                    }))?;
                } else {
                    println!("deleted host: {} {}", host.id, host.display_name);
                }
            }
        },
        Commands::List => {
            let vault = load_vault(&vault_path)?;
            if output.is_json() {
                let hosts = vault
                    .hosts
                    .iter()
                    .map(|host| host_json(host, &vault.host_path(host)))
                    .collect::<Vec<_>>();
                print_json(json!({ "hosts": hosts }))?;
            } else {
                for host in &vault.hosts {
                    println!("{}\t{}\t{}", host.id, vault.host_path(host), host.hostname);
                }
            }
        }
        Commands::Search(args) => {
            let vault = load_vault(&vault_path)?;
            let matches = vault.search_hosts(&args.query);
            if output.is_json() {
                let hosts = matches
                    .iter()
                    .map(|host| host_json(host, &vault.host_path(host)))
                    .collect::<Vec<_>>();
                print_json(json!({
                    "query": args.query,
                    "hosts": hosts,
                }))?;
            } else {
                for host in matches {
                    println!("{}\t{}\t{}", host.id, vault.host_path(host), host.hostname);
                }
            }
        }
        Commands::Show(args) => {
            let vault = load_vault(&vault_path)?;
            let resolved = vault.resolve_host(selector(&args.selector))?;
            if output.is_json() {
                print_json(json!({ "host": resolved_host_json(&resolved, None) }))?;
            } else {
                print_resolved(&resolved, None);
            }
        }
        Commands::Diagnose(args) => {
            let vault = load_vault(&vault_path)?;
            let local_config = load_local_config(&local_config_path)?;
            let resolved = vault.resolve_host(selector(&args.selector))?;
            let command = command_for_host(&resolved);
            let identity_path = resolved
                .identity_fingerprint
                .as_deref()
                .and_then(|fingerprint| local_config.identity_path(fingerprint));
            let config = config_for_host_with_identity_path(&resolved, identity_path);
            if output.is_json() {
                let config_command = command_for_config("<temporary-config>", &config.alias);
                print_json(json!({
                    "host": resolved_host_json(&resolved, Some(&local_config)),
                    "openssh_command": command_json(&command),
                    "openssh_config_command": command_json(&config_command),
                    "openssh_config": config_json(&config),
                }))?;
            } else {
                print_resolved(&resolved, Some(&local_config));
                println!();
                println!("OpenSSH command:");
                println!("  {}", command.render_for_display());
                println!();
                println!("OpenSSH config command:");
                println!(
                    "  {}",
                    command_for_config("<temporary-config>", &config.alias).render_for_display()
                );
                println!();
                println!("Generated OpenSSH config:");
                println!("{}", config.contents);
            }
        }
        Commands::Connect(args) => {
            let vault = load_vault(&vault_path)?;
            let local_config = load_local_config(&local_config_path)?;
            let resolved = vault.resolve_host(selector(&args.selector))?;
            let (command, _temp_config) = prepare_openssh_command(&resolved, &local_config)
                .context("failed to prepare OpenSSH command")?;
            if !output.is_json() {
                eprintln!("connecting: {}", command.render_for_display());
            }
            let status = Command::new(&command.program)
                .args(&command.args)
                .status()
                .context("failed to launch ssh")?;
            if !status.success() {
                bail!("ssh exited with status {status}");
            }
            if output.is_json() {
                print_json(json!({
                    "status": "connected",
                    "host": resolved_host_json(&resolved, Some(&local_config)),
                    "openssh_command": command_json(&command),
                    "exit_code": status.code(),
                }))?;
            }
        }
        Commands::Action(args) => {
            let vault = load_vault(&vault_path)?;
            let local_config = load_local_config(&local_config_path)?;
            let resolved = vault.resolve_host(selector(&args.host))?;
            let action = find_action(&resolved, &args.action)?.clone();
            let local_prepare = resolve_action_local_prepare(&resolved, &action, &local_config)
                .context("failed to prepare action")?;

            if args.dry_run {
                let plan = resolve_action_plan(&resolved, &action, &local_config, &HashMap::new())
                    .context("failed to resolve action")?;
                if output.is_json() {
                    print_json(json!({
                        "mode": "dry_run",
                        "host": resolved_host_json(&resolved, Some(&local_config)),
                        "action": action,
                        "plan": action_plan_json(&plan),
                    }))?;
                } else {
                    print_action_plan(&plan);
                }
                return Ok(());
            }

            let prepare_env = run_action_prepare(local_prepare.as_ref())?;
            let plan = resolve_action_plan(&resolved, &action, &local_config, &prepare_env)
                .context("failed to resolve action")?;
            if !output.is_json() {
                print_action_plan(&plan);
                eprintln!("running action: {}", plan.action_name);
            }
            let result = run_action_plan(plan)?;
            if !result.status.success() {
                bail!("action ssh exited with status {}", result.status);
            }
            if output.is_json() {
                print_json(json!({
                    "status": "completed",
                    "host": resolved_host_json(&resolved, Some(&local_config)),
                    "action": action,
                    "ssh_exit_code": result.status.code(),
                    "local_exit_code": result.local_exit.and_then(|status| status.code()),
                }))?;
            }
        }
        Commands::Identity(IdentityCommands::List) => {
            let local_path = local_config_path.clone();
            let local_config = load_local_config(&local_path)?;
            if output.is_json() {
                let mappings = local_config
                    .identity_mappings
                    .iter()
                    .map(identity_mapping_json)
                    .collect::<Vec<_>>();
                print_json(json!({
                    "local_config_path": local_path,
                    "identity_mappings": mappings,
                }))?;
            } else {
                println!("local config: {}", local_path.display());
                for mapping in local_config.identity_mappings {
                    println!(
                        "{}\t{}\t{}",
                        mapping.fingerprint,
                        mapping.preferred_name.as_deref().unwrap_or("(unnamed)"),
                        mapping.path.display()
                    );
                }
            }
        }
        Commands::Identity(IdentityCommands::Add(args)) => {
            let local_path = local_config_path.clone();
            let mut local_config = load_local_config(&local_path)?;
            let derived = derive_identity_from_file(&args.path, args.name)?;
            local_config.map_identity(
                derived.fingerprint.clone(),
                args.path.clone(),
                derived.preferred_name.clone(),
            )?;
            save_local_config(&local_path, &local_config)?;
            if output.is_json() {
                print_json(json!({
                    "status": "added",
                    "identity": {
                        "fingerprint": derived.fingerprint,
                        "preferred_name": derived.preferred_name,
                        "path": args.path,
                    },
                }))?;
            } else {
                println!(
                    "added identity: {} {} -> {}",
                    derived.fingerprint,
                    derived.preferred_name.as_deref().unwrap_or("(unnamed)"),
                    args.path.display()
                );
            }
        }
        Commands::Identity(IdentityCommands::Map(args)) => {
            let local_path = local_config_path.clone();
            let mut local_config = load_local_config(&local_path)?;
            let preferred_name = args.name.clone();
            local_config.map_identity(args.fingerprint.clone(), args.path.clone(), args.name)?;
            save_local_config(&local_path, &local_config)?;
            if output.is_json() {
                print_json(json!({
                    "status": "mapped",
                    "identity": {
                        "fingerprint": args.fingerprint,
                        "preferred_name": preferred_name,
                        "path": args.path,
                    },
                }))?;
            } else {
                println!(
                    "mapped identity: {} -> {}",
                    args.fingerprint,
                    args.path.display()
                );
            }
        }
        Commands::Identity(IdentityCommands::Edit(args)) => {
            let local_path = local_config_path.clone();
            let mut local_config = load_local_config(&local_path)?;
            if args.path.is_none() && args.name.is_none() && !args.clear_name {
                bail!("identity edit requires at least one of --path, --name, or --clear-name");
            }
            if args.name.is_some() && args.clear_name {
                bail!("--name cannot be combined with --clear-name");
            }
            if let Some(path) = args.path.as_ref() {
                let derived = derive_identity_from_file(path, None)?;
                if derived.fingerprint != args.fingerprint {
                    bail!(
                        "identity file fingerprint {} does not match {}",
                        derived.fingerprint,
                        args.fingerprint
                    );
                }
            }
            let mapping = local_config
                .identity_mapping_mut(&args.fingerprint)
                .ok_or_else(|| anyhow::anyhow!("identity is not mapped: {}", args.fingerprint))?;
            if let Some(path) = args.path {
                mapping.path = path;
            }
            if args.clear_name {
                mapping.preferred_name = None;
            } else if let Some(name) = args.name {
                mapping.preferred_name = Some(name);
            }
            let mapping = mapping.clone();
            save_local_config(&local_path, &local_config)?;
            if output.is_json() {
                print_json(json!({
                    "status": "updated",
                    "identity": identity_mapping_json(&mapping),
                }))?;
            } else {
                println!(
                    "updated identity: {} -> {}",
                    mapping.fingerprint,
                    mapping.path.display()
                );
            }
        }
        Commands::Identity(IdentityCommands::Rename(args)) => {
            let local_path = local_config_path.clone();
            let mut local_config = load_local_config(&local_path)?;
            let mapping = local_config
                .identity_mapping_mut(&args.fingerprint)
                .ok_or_else(|| anyhow::anyhow!("identity is not mapped: {}", args.fingerprint))?;
            mapping.preferred_name = Some(args.name);
            let mapping = mapping.clone();
            save_local_config(&local_path, &local_config)?;
            if output.is_json() {
                print_json(json!({
                    "status": "renamed",
                    "identity": identity_mapping_json(&mapping),
                }))?;
            } else {
                println!(
                    "renamed identity: {} {}",
                    mapping.fingerprint,
                    mapping.preferred_name.as_deref().unwrap_or("(unnamed)")
                );
            }
        }
        Commands::Identity(IdentityCommands::Unmap(args)) => {
            let local_path = local_config_path.clone();
            let mut local_config = load_local_config(&local_path)?;
            match local_config.unmap_identity(&args.fingerprint) {
                Some(mapping) => {
                    save_local_config(&local_path, &local_config)?;
                    if output.is_json() {
                        print_json(json!({
                            "status": "unmapped",
                            "identity": identity_mapping_json(&mapping),
                        }))?;
                    } else {
                        println!(
                            "unmapped identity: {} -> {}",
                            mapping.fingerprint,
                            mapping.path.display()
                        );
                    }
                }
                None => bail!("identity is not mapped: {}", args.fingerprint),
            }
        }
        Commands::Identity(IdentityCommands::Diagnose(args)) => {
            let local_path = local_config_path.clone();
            let local_config = load_local_config(&local_path)?;
            if output.is_json() {
                print_json(json!({
                    "local_config_path": local_path,
                    "identity_mapping": identity_mapping_diagnosis_json(&args.fingerprint, &local_config),
                }))?;
            } else {
                println!("local config: {}", local_path.display());
                print_identity_mapping(&args.fingerprint, &local_config);
            }
        }
        Commands::Import(ImportCommands::Openssh(args)) => {
            let mut vault = load_vault(&vault_path)?;
            let local_path = local_config_path.clone();
            let mut local_config = load_local_config(&local_path)?;
            let home_dir = env::var_os("HOME").map(PathBuf::from);
            let read = read_openssh_config_with_includes(&args.path, home_dir.as_deref())?;
            let resolver = OpenSshIdentityResolver;
            let mut summary = import_openssh_config_with_identities(
                &mut vault,
                &read.contents,
                IdentityImportContext {
                    local_config: &mut local_config,
                    config_path: &args.path,
                    home_dir: home_dir.as_deref(),
                    resolver: &resolver,
                },
            )?;
            summary.warnings.extend(read.warnings);
            save_vault(&vault_path, &vault)?;
            save_local_config(&local_path, &local_config)?;

            if output.is_json() {
                print_json(json!({
                    "summary": {
                        "imported": summary.imported,
                        "skipped": summary.skipped,
                        "warnings": summary.warnings,
                    },
                }))?;
            } else {
                println!("imported: {}", summary.imported.len());
                for alias in summary.imported {
                    println!("  + {alias}");
                }
                println!("skipped: {}", summary.skipped.len());
                for skipped in summary.skipped {
                    println!("  - {skipped}");
                }
                println!("warnings: {}", summary.warnings.len());
                for warning in summary.warnings {
                    println!("  ! {warning}");
                }
            }
        }
        Commands::Export(ExportCommands::Openssh(args)) => {
            let vault = load_vault(&vault_path)?;
            let contents = export_openssh_config(&vault);
            if args.path == "-" {
                if output.is_json() {
                    print_json(json!({
                        "target": "stdout",
                        "contents": contents,
                    }))?;
                } else {
                    print!("{contents}");
                }
            } else {
                fs::write(&args.path, contents)
                    .with_context(|| format!("failed to write {}", args.path))?;
                if output.is_json() {
                    print_json(json!({
                        "status": "exported",
                        "path": args.path,
                    }))?;
                } else {
                    println!("exported OpenSSH config: {}", args.path);
                }
            }
        }
    }

    Ok(())
}

fn print_resolved(host: &ResolvedHost, local_config: Option<&stassh_core::LocalConfig>) {
    println!("ID: {}", host.id);
    println!("Path: {}", host.path);
    println!("HostName: {}", host.hostname);
    println!("Port: {}", host.port);
    println!("User: {}", host.username.as_deref().unwrap_or("(default)"));
    println!("Tags: {}", display_list(&host.tags));
    println!("Notes: {}", host.notes.as_deref().unwrap_or(""));
    println!(
        "Identity: {}",
        host.identity_fingerprint.as_deref().unwrap_or("(default)")
    );
    if let Some(fingerprint) = &host.identity_fingerprint
        && let Some(local_config) = local_config
    {
        print_identity_mapping(fingerprint, local_config);
    }
    println!(
        "Jump chain: {}",
        if host.jump_chain.is_empty() {
            "(none)".to_string()
        } else {
            host.jump_chain
                .iter()
                .map(|jump| jump.display_name.as_str())
                .collect::<Vec<_>>()
                .join(" -> ")
        }
    );
    println!(
        "Forwards: {}",
        if host.forwards.is_empty() {
            "(none)".to_string()
        } else {
            host.forwards
                .iter()
                .map(display_forward)
                .collect::<Vec<_>>()
                .join(", ")
        }
    );
    println!(
        "Actions: {}",
        if host.actions.is_empty() {
            "(none)".to_string()
        } else {
            host.actions
                .iter()
                .map(|action| action.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        }
    );
    println!("SSH options: {}", display_list(&host.ssh_options));
}

fn print_identity_mapping(fingerprint: &str, local_config: &stassh_core::LocalConfig) {
    println!("Identity mapping:");
    println!("  fingerprint: {fingerprint}");
    match local_config.identity_path(fingerprint) {
        Some(path) => {
            if let Some(mapping) = local_config
                .identity_mappings
                .iter()
                .find(|mapping| mapping.fingerprint == fingerprint)
            {
                println!(
                    "  preferred name: {}",
                    mapping.preferred_name.as_deref().unwrap_or("(none)")
                );
            }
            println!("  path: {}", path.display());
            println!("  exists: {}", path.exists());
        }
        None => {
            println!("  path: (unmapped)");
            println!("  exists: false");
        }
    }
}

fn display_list(values: &[String]) -> String {
    if values.is_empty() {
        "(none)".to_string()
    } else {
        values.join(", ")
    }
}

fn identity_from_host_args(
    fingerprint: Option<String>,
    preferred_name: Option<String>,
    identity_file: Option<PathBuf>,
    local_config: &mut stassh_core::LocalConfig,
) -> Result<Option<String>> {
    if let Some(path) = identity_file {
        if fingerprint.is_some() {
            bail!("--identity-file cannot be combined with --identity-fingerprint");
        }
        let derived = derive_identity_from_file(&path, preferred_name)?;
        local_config.map_identity(
            derived.fingerprint.clone(),
            path,
            derived.preferred_name.clone(),
        )?;
        return Ok(Some(derived.fingerprint));
    }

    if preferred_name.is_some() {
        bail!(
            "--identity-name requires --identity-file; use `stassh identity rename` to name an existing mapping"
        );
    }
    Ok(fingerprint)
}

fn identity_update_from_args(
    fingerprint: Option<String>,
    preferred_name: Option<String>,
    identity_file: Option<PathBuf>,
    clear: bool,
    local_config: &mut stassh_core::LocalConfig,
) -> Result<Option<Option<String>>> {
    if clear {
        if fingerprint.is_some() || preferred_name.is_some() || identity_file.is_some() {
            bail!("--clear-identity cannot be combined with identity flags");
        }
        return Ok(Some(None));
    }

    identity_from_host_args(fingerprint, preferred_name, identity_file, local_config)
        .map(|identity| identity.map(Some))
}

fn clearable_value<T>(
    value: Option<T>,
    clear: bool,
    value_flag: &'static str,
    clear_flag: &'static str,
) -> Result<Option<Option<T>>> {
    if clear {
        if value.is_some() {
            bail!("{clear_flag} cannot be combined with {value_flag}");
        }
        Ok(Some(None))
    } else {
        Ok(value.map(Some))
    }
}

fn clearable_vec<T>(
    values: Vec<T>,
    clear: bool,
    value_flag: &'static str,
    clear_flag: &'static str,
) -> Result<Option<Vec<T>>> {
    if clear {
        if !values.is_empty() {
            bail!("{clear_flag} cannot be combined with {value_flag}");
        }
        Ok(Some(Vec::new()))
    } else if values.is_empty() {
        Ok(None)
    } else {
        Ok(Some(values))
    }
}

fn resolve_jump_ids(vault: &Vault, jumps: &[String]) -> Result<Vec<Uuid>> {
    jumps
        .iter()
        .map(|jump| {
            vault
                .resolve_host(selector(jump))
                .map(|host| host.id)
                .map_err(Into::into)
        })
        .collect()
}

fn parse_forward_args(args: &ForwardCliArgs) -> Result<Vec<ForwardDefinition>> {
    let mut forwards = Vec::new();

    for spec in &args.local_forwards {
        let parts = parse_parts(spec, 4, "local forward")?;
        forwards.push(ForwardDefinition::Local {
            bind_address: parts[0].to_string(),
            local_port: parse_port(parts[1], "local forward local port")?,
            destination_host: parts[2].to_string(),
            destination_port: parse_port(parts[3], "local forward destination port")?,
        });
    }

    for spec in &args.remote_forwards {
        let parts = parse_parts(spec, 4, "remote forward")?;
        forwards.push(ForwardDefinition::Remote {
            bind_address: parts[0].to_string(),
            remote_port: parse_port(parts[1], "remote forward remote port")?,
            destination_host: parts[2].to_string(),
            destination_port: parse_port(parts[3], "remote forward destination port")?,
        });
    }

    for spec in &args.dynamic_forwards {
        let parts = parse_parts(spec, 2, "dynamic forward")?;
        forwards.push(ForwardDefinition::Dynamic {
            bind_address: parts[0].to_string(),
            local_port: parse_port(parts[1], "dynamic forward local port")?,
        });
    }

    Ok(forwards)
}

fn parse_parts<'a>(spec: &'a str, expected: usize, label: &str) -> Result<Vec<&'a str>> {
    let parts = spec.split(':').collect::<Vec<_>>();
    if parts.len() != expected || parts.iter().any(|part| part.is_empty()) {
        bail!("{label} must have {expected} colon-separated non-empty fields");
    }
    Ok(parts)
}

fn parse_port(value: &str, label: &str) -> Result<u16> {
    value
        .parse::<u16>()
        .with_context(|| format!("invalid {label}: {value}"))
}

fn display_forward(forward: &ForwardDefinition) -> String {
    match forward {
        ForwardDefinition::Local {
            bind_address,
            local_port,
            destination_host,
            destination_port,
        } => format!("L {bind_address}:{local_port} -> {destination_host}:{destination_port}"),
        ForwardDefinition::Remote {
            bind_address,
            remote_port,
            destination_host,
            destination_port,
        } => format!("R {bind_address}:{remote_port} -> {destination_host}:{destination_port}"),
        ForwardDefinition::Dynamic {
            bind_address,
            local_port,
        } => format!("D {bind_address}:{local_port}"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_sha256_fingerprint_from_ssh_keygen_output() {
        let fingerprint = stassh_core::parse_ssh_keygen_fingerprint_output(
            "256 SHA256:abc123 alice@example.com (ED25519)\n",
        )
        .unwrap();

        assert_eq!(fingerprint, "SHA256:abc123");
    }

    #[test]
    fn rejects_unexpected_fingerprint_format() {
        let result = stassh_core::parse_ssh_keygen_fingerprint_output(
            "2048 MD5:aa:bb:cc alice@example.com (RSA)\n",
        );

        assert!(result.is_err());
    }
}
