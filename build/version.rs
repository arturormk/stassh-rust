use std::env;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=STASSH_VERSION");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads");
    println!("cargo:rerun-if-changed=../../.git/packed-refs");

    let version = env::var("STASSH_VERSION").unwrap_or_else(|_| computed_version());
    println!("cargo:rustc-env=STASSH_VERSION={version}");
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
