use std::env;
use std::path::Path;
use std::process::Command;

pub fn configure_stassh_version() {
    println!("cargo:rerun-if-env-changed=STASSH_VERSION");
    if let Some(git_dir) = workspace_git_dir() {
        println!("cargo:rerun-if-changed={}", git_dir.join("HEAD").display());
        println!(
            "cargo:rerun-if-changed={}",
            git_dir.join("refs/heads").display()
        );
        println!(
            "cargo:rerun-if-changed={}",
            git_dir.join("packed-refs").display()
        );
    }

    let version = env::var("STASSH_VERSION").unwrap_or_else(|_| computed_version());
    println!("cargo:rustc-env=STASSH_VERSION={version}");
}

fn workspace_git_dir() -> Option<std::path::PathBuf> {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").ok()?;
    Path::new(&manifest_dir)
        .ancestors()
        .map(|path| path.join(".git"))
        .find(|path| path.exists())
}

fn computed_version() -> String {
    let package_version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.1.0".to_string());
    let mut parts = package_version.split('.');
    let Some(major) = parts.next() else {
        return package_version;
    };
    let Some(minor) = parts.next() else {
        return package_version;
    };

    let Ok(minor_number) = minor.parse::<u64>() else {
        return package_version;
    };

    if minor_number % 2 == 0 {
        return format!("{major}.{minor}");
    }

    match git_commit_count() {
        Some(count) => format!("{major}.{minor}.{count}"),
        None => package_version,
    }
}

fn git_commit_count() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-list", "--count", "HEAD"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let count = String::from_utf8(output.stdout).ok()?;
    let count = count.trim();

    if count.is_empty() {
        None
    } else {
        Some(count.to_string())
    }
}
