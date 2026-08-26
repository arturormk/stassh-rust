use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn stassh() -> Command {
    Command::new(env!("CARGO_BIN_EXE_stassh"))
}

fn temp_dir(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("stassh-cli-{name}-{}-{nonce}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn run(args: &[&str]) -> Output {
    let output = stassh().args(args).output().unwrap();
    if !output.status.success() {
        panic!(
            "command failed: stassh {}\nstdout:\n{}\nstderr:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    output
}

fn run_json(args: &[&str]) -> Value {
    let output = run(args);
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "invalid json: {error}\nstdout:\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn vault_path(dir: &Path) -> String {
    dir.join("vault.json").display().to_string()
}

fn init_vault(dir: &Path) {
    run(&["--vault", &vault_path(dir), "vault", "init"]);
}

fn local_config_path(dir: &Path) -> String {
    dir.join("local.json").display().to_string()
}

fn generate_identity_file(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    let output = Command::new("ssh-keygen")
        .arg("-q")
        .arg("-t")
        .arg("ed25519")
        .arg("-N")
        .arg("")
        .arg("-f")
        .arg(&path)
        .output()
        .unwrap();
    if !output.status.success() {
        panic!(
            "ssh-keygen failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    path
}

fn fingerprint(path: &Path) -> String {
    let output = Command::new("ssh-keygen")
        .arg("-lf")
        .arg(path)
        .output()
        .unwrap();
    if !output.status.success() {
        panic!(
            "ssh-keygen -lf failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8(output.stdout)
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string()
}

#[test]
fn json_vault_status_reports_counts() {
    let dir = temp_dir("json-status");
    init_vault(&dir);

    let value = run_json(&[
        "--vault",
        &vault_path(&dir),
        "--output",
        "json",
        "vault",
        "status",
    ]);

    assert_eq!(value["format_version"], 0);
    assert_eq!(value["folders"], 1);
    assert_eq!(value["hosts"], 0);
}

#[test]
fn json_host_add_and_diagnose_report_host_and_openssh_config() {
    let dir = temp_dir("json-host");
    init_vault(&dir);

    let added = run_json(&[
        "--vault",
        &vault_path(&dir),
        "--output",
        "json",
        "host",
        "add",
        "web",
        "web.example.com",
        "--user",
        "deploy",
        "--port",
        "2222",
        "--tag",
        "prod",
        "--ssh-option",
        "ServerAliveInterval=30",
    ]);

    assert_eq!(added["status"], "added");
    assert_eq!(added["host"]["path"], "web");
    assert_eq!(added["host"]["hostname"], "web.example.com");
    assert_eq!(added["host"]["port"], 2222);
    assert_eq!(added["host"]["tags"][0], "prod");

    let diagnosed = run_json(&[
        "--vault",
        &vault_path(&dir),
        "--output",
        "json",
        "diagnose",
        "web",
    ]);

    assert_eq!(diagnosed["host"]["username"], "deploy");
    assert_eq!(diagnosed["openssh_command"]["program"], "ssh");
    assert_eq!(
        diagnosed["openssh_config"]["alias"].as_str().unwrap().len(),
        39
    );
    assert!(
        diagnosed["openssh_config"]["contents"]
            .as_str()
            .unwrap()
            .contains("ServerAliveInterval=30")
    );
}

#[test]
fn json_host_identity_uses_fingerprint_field() {
    let dir = temp_dir("json-host-identity");
    init_vault(&dir);

    let added = run_json(&[
        "--vault",
        &vault_path(&dir),
        "--output",
        "json",
        "host",
        "add",
        "web",
        "web.example.com",
        "--identity-fingerprint",
        "SHA256:abc",
    ]);

    assert_eq!(added["host"]["identity_fingerprint"], "SHA256:abc");
    assert!(added["host"].get("identity").is_none());
}

#[test]
fn json_host_add_and_edit_manage_secrets_reference() {
    let dir = temp_dir("json-host-secrets");
    init_vault(&dir);

    let added = run_json(&[
        "--vault",
        &vault_path(&dir),
        "--output",
        "json",
        "host",
        "add",
        "web",
        "web.example.com",
        "--secrets",
        "site-a",
    ]);
    assert_eq!(added["host"]["secrets"], "site-a");

    let updated = run_json(&[
        "--vault",
        &vault_path(&dir),
        "--output",
        "json",
        "host",
        "edit",
        "web",
        "--secrets",
        "site-b",
    ]);
    assert_eq!(updated["host"]["secrets"], "site-b");

    let cleared = run_json(&[
        "--vault",
        &vault_path(&dir),
        "--output",
        "json",
        "host",
        "edit",
        "web",
        "--clear-secrets",
    ]);
    assert!(cleared["host"]["secrets"].is_null());
}

#[test]
fn action_dry_run_reports_forwarded_vnc_plan() {
    let dir = temp_dir("action-dry-run");
    init_vault(&dir);
    run(&[
        "--vault",
        &vault_path(&dir),
        "host",
        "add",
        "pi",
        "pi.local",
    ]);
    let vault_file = dir.join("vault.json");
    let mut vault: Value = serde_json::from_slice(&fs::read(&vault_file).unwrap()).unwrap();
    vault["actions"] = serde_json::json!([
        {
            "id": "11111111-1111-1111-1111-111111111111",
            "name": "VNC forwarded",
            "forwards": [
                {
                    "type": "local",
                    "name": "vnc",
                    "bind_address": "127.0.0.1",
                    "local_port": "auto",
                    "destination_host": "127.0.0.1",
                    "destination_port": 5900
                }
            ],
            "remote_command": "DISPLAY=:0 x11vnc -scale 1/2",
            "local_launch": {
                "program": "/bin/echo",
                "args": ["127.0.0.1::{LOCAL_PORT:vnc}"]
            }
        }
    ]);
    fs::write(&vault_file, serde_json::to_vec_pretty(&vault).unwrap()).unwrap();

    let value = run_json(&[
        "--vault",
        &vault_path(&dir),
        "--output",
        "json",
        "action",
        "pi",
        "VNC forwarded",
        "--dry-run",
    ]);

    let port = value["plan"]["allocated_ports"]["vnc"].as_u64().unwrap();
    assert!(port > 0);
    assert!(
        value["plan"]["ssh_command"]["display"]
            .as_str()
            .unwrap()
            .contains("x11vnc")
    );
    assert_eq!(
        value["plan"]["local_launch"]["args"][0],
        format!("127.0.0.1::{port}")
    );
}

#[test]
fn vault_duplicates_reports_path_and_connection_groups() {
    let dir = temp_dir("duplicates");
    init_vault(&dir);
    run(&[
        "--vault",
        &vault_path(&dir),
        "host",
        "add",
        "web",
        "web-a.example",
    ]);
    run(&[
        "--vault",
        &vault_path(&dir),
        "host",
        "add",
        "web",
        "web-b.example",
    ]);
    run(&[
        "--vault",
        &vault_path(&dir),
        "host",
        "add",
        "app-a",
        "app.example",
        "--user",
        "deploy",
    ]);
    run(&[
        "--vault",
        &vault_path(&dir),
        "host",
        "add",
        "app-b",
        "app.example",
        "--user",
        "deploy",
    ]);

    let value = run_json(&[
        "--vault",
        &vault_path(&dir),
        "--output",
        "json",
        "vault",
        "duplicates",
    ]);
    let groups = value["duplicate_groups"].as_array().unwrap();

    assert_eq!(value["duplicate_group_count"], 2);
    assert!(
        groups
            .iter()
            .any(|group| group["kind"] == "path" && group["key"] == "web")
    );
    assert!(groups.iter().any(|group| {
        group["kind"] == "connection"
            && group["hosts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|host| host["path"] == "app-a")
    }));
}

#[test]
fn vault_dedupe_dry_run_and_apply_remove_path_duplicates() {
    let dir = temp_dir("dedupe");
    init_vault(&dir);
    run(&[
        "--vault",
        &vault_path(&dir),
        "host",
        "add",
        "web",
        "web-a.example",
    ]);
    run(&[
        "--vault",
        &vault_path(&dir),
        "host",
        "add",
        "web",
        "web-b.example",
    ]);

    let dry_run = run_json(&[
        "--vault",
        &vault_path(&dir),
        "--output",
        "json",
        "vault",
        "dedupe",
    ]);
    assert_eq!(dry_run["mode"], "dry_run");
    assert_eq!(dry_run["plan"]["remove_count"], 1);

    let applied = run_json(&[
        "--vault",
        &vault_path(&dir),
        "--output",
        "json",
        "vault",
        "dedupe",
        "--apply",
    ]);
    assert_eq!(applied["mode"], "apply");
    assert_eq!(applied["result"]["removed_count"], 1);

    let duplicate_report = run_json(&[
        "--vault",
        &vault_path(&dir),
        "--output",
        "json",
        "vault",
        "duplicates",
    ]);
    assert_eq!(duplicate_report["duplicate_group_count"], 0);
}

#[test]
fn vault_check_reports_identity_and_raw_option_findings() {
    let dir = temp_dir("check");
    init_vault(&dir);
    run(&[
        "--vault",
        &vault_path(&dir),
        "host",
        "add",
        "web",
        "web.example",
        "--identity-fingerprint",
        "SHA256:missing",
        "--ssh-option",
        "IdentityFile ~/.ssh/raw",
    ]);
    run(&[
        "--vault",
        &vault_path(&dir),
        "identity",
        "map",
        "SHA256:stale",
        &dir.join("missing-key").display().to_string(),
        "--name",
        "stale",
    ]);

    let value = run_json(&[
        "--vault",
        &vault_path(&dir),
        "--output",
        "json",
        "vault",
        "check",
    ]);

    assert_eq!(value["vault_validation"]["ok"], true);
    assert_eq!(value["local_config_validation"]["ok"], true);
    assert_eq!(value["missing_identity_mapping_count"], 1);
    assert_eq!(value["missing_identity_file_count"], 1);
    assert_eq!(value["raw_identity_file_option_count"], 1);
    assert_eq!(
        value["missing_identity_mappings"][0]["fingerprint"],
        "SHA256:missing"
    );
}

#[test]
fn identity_rename_and_edit_update_local_mapping() {
    let dir = temp_dir("identity-edit");
    init_vault(&dir);
    let key_path = generate_identity_file(&dir, "id_one");
    let replacement_path = generate_identity_file(&dir, "id_two");
    let replacement_fingerprint = fingerprint(&replacement_path);
    let local_config = local_config_path(&dir);

    run(&[
        "--vault",
        &vault_path(&dir),
        "--local-config",
        &local_config,
        "identity",
        "map",
        &replacement_fingerprint,
        &key_path.display().to_string(),
        "--name",
        "old-name",
    ]);

    let renamed = run_json(&[
        "--vault",
        &vault_path(&dir),
        "--local-config",
        &local_config,
        "--output",
        "json",
        "identity",
        "rename",
        &replacement_fingerprint,
        "renamed",
    ]);
    assert_eq!(renamed["status"], "renamed");
    assert_eq!(renamed["identity"]["preferred_name"], "renamed");
    assert_eq!(renamed["identity"]["path"], key_path.display().to_string());

    let edited = run_json(&[
        "--vault",
        &vault_path(&dir),
        "--local-config",
        &local_config,
        "--output",
        "json",
        "identity",
        "edit",
        &replacement_fingerprint,
        "--path",
        &replacement_path.display().to_string(),
        "--name",
        "updated",
    ]);
    assert_eq!(edited["status"], "updated");
    assert_eq!(edited["identity"]["preferred_name"], "updated");
    assert_eq!(
        edited["identity"]["path"],
        replacement_path.display().to_string()
    );

    let cleared = run_json(&[
        "--vault",
        &vault_path(&dir),
        "--local-config",
        &local_config,
        "--output",
        "json",
        "identity",
        "edit",
        &replacement_fingerprint,
        "--clear-name",
    ]);
    assert_eq!(cleared["status"], "updated");
    assert!(cleared["identity"]["preferred_name"].is_null());
}

#[test]
fn identity_edit_rejects_mismatched_path() {
    let dir = temp_dir("identity-edit-mismatch");
    init_vault(&dir);
    let key_path = generate_identity_file(&dir, "id_one");
    let other_path = generate_identity_file(&dir, "id_two");
    let key_fingerprint = fingerprint(&key_path);
    let local_config = local_config_path(&dir);
    run(&[
        "--vault",
        &vault_path(&dir),
        "--local-config",
        &local_config,
        "identity",
        "map",
        &key_fingerprint,
        &key_path.display().to_string(),
    ]);

    let output = stassh()
        .args([
            "--vault",
            &vault_path(&dir),
            "--local-config",
            &local_config,
            "identity",
            "edit",
            &key_fingerprint,
            "--path",
            &other_path.display().to_string(),
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("identity file fingerprint"));
}

#[test]
fn identity_edit_requires_existing_mapping_and_change() {
    let dir = temp_dir("identity-edit-errors");
    init_vault(&dir);
    let local_config = local_config_path(&dir);

    let no_change = stassh()
        .args([
            "--vault",
            &vault_path(&dir),
            "--local-config",
            &local_config,
            "identity",
            "edit",
            "SHA256:missing",
        ])
        .output()
        .unwrap();
    assert!(!no_change.status.success());
    assert!(String::from_utf8_lossy(&no_change.stderr).contains("requires at least one"));

    let missing = stassh()
        .args([
            "--vault",
            &vault_path(&dir),
            "--local-config",
            &local_config,
            "identity",
            "rename",
            "SHA256:missing",
            "name",
        ])
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("identity is not mapped"));
}

#[test]
fn export_openssh_stdout_text_and_json_modes() {
    let dir = temp_dir("export");
    init_vault(&dir);
    run(&[
        "--vault",
        &vault_path(&dir),
        "host",
        "add",
        "web",
        "web.example",
    ]);

    let text = run(&["--vault", &vault_path(&dir), "export", "openssh", "-"]);
    let text = String::from_utf8(text.stdout).unwrap();
    assert!(text.starts_with("# Generated by stassh"));
    assert!(text.contains("Host web"));

    let json = run_json(&[
        "--vault",
        &vault_path(&dir),
        "--output",
        "json",
        "export",
        "openssh",
        "-",
    ]);
    assert_eq!(json["target"], "stdout");
    assert!(json["contents"].as_str().unwrap().contains("Host web"));
}
