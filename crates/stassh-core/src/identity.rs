use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedIdentity {
    pub fingerprint: String,
    pub preferred_name: Option<String>,
}

pub trait IdentityFileResolver {
    fn derive_identity(
        &self,
        path: &Path,
        preferred_name: Option<String>,
    ) -> Result<DerivedIdentity, IdentityDeriveError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct OpenSshIdentityResolver;

impl IdentityFileResolver for OpenSshIdentityResolver {
    fn derive_identity(
        &self,
        path: &Path,
        preferred_name: Option<String>,
    ) -> Result<DerivedIdentity, IdentityDeriveError> {
        derive_identity_from_file(path, preferred_name)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IdentityDeriveError {
    #[error("identity file does not exist: {0}")]
    MissingFile(PathBuf),
    #[error("failed to run {program}: {source}")]
    Spawn {
        program: &'static str,
        source: std::io::Error,
    },
    #[error("{program} failed for {path}: {stderr}")]
    CommandFailed {
        program: &'static str,
        path: PathBuf,
        stderr: String,
    },
    #[error("ssh-keygen fingerprint output did not contain a fingerprint")]
    MissingFingerprint,
    #[error("expected SHA256 fingerprint, got {0}")]
    UnexpectedFingerprint(String),
}

pub fn derive_identity_from_file(
    path: &Path,
    preferred_name: Option<String>,
) -> Result<DerivedIdentity, IdentityDeriveError> {
    if !path.exists() {
        return Err(IdentityDeriveError::MissingFile(path.to_path_buf()));
    }

    let fingerprint_output = Command::new("ssh-keygen")
        .arg("-lf")
        .arg(path)
        .output()
        .map_err(|source| IdentityDeriveError::Spawn {
            program: "ssh-keygen -lf",
            source,
        })?;
    if !fingerprint_output.status.success() {
        return Err(IdentityDeriveError::CommandFailed {
            program: "ssh-keygen -lf",
            path: path.to_path_buf(),
            stderr: String::from_utf8_lossy(&fingerprint_output.stderr)
                .trim()
                .to_string(),
        });
    }

    let stdout = String::from_utf8_lossy(&fingerprint_output.stdout);
    let fingerprint = parse_ssh_keygen_fingerprint_output(&stdout)?;
    let preferred_name = preferred_name.or_else(|| {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .map(ToString::to_string)
    });

    Ok(DerivedIdentity {
        fingerprint,
        preferred_name,
    })
}

pub fn parse_ssh_keygen_fingerprint_output(output: &str) -> Result<String, IdentityDeriveError> {
    let fingerprint = output
        .split_whitespace()
        .nth(1)
        .ok_or(IdentityDeriveError::MissingFingerprint)?;
    if !fingerprint.starts_with("SHA256:") {
        return Err(IdentityDeriveError::UnexpectedFingerprint(
            fingerprint.to_string(),
        ));
    }
    Ok(fingerprint.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sha256_fingerprint_from_ssh_keygen_output() {
        let fingerprint =
            parse_ssh_keygen_fingerprint_output("256 SHA256:abc123 alice@example.com (ED25519)\n")
                .unwrap();

        assert_eq!(fingerprint, "SHA256:abc123");
    }

    #[test]
    fn rejects_unexpected_fingerprint_format() {
        let result =
            parse_ssh_keygen_fingerprint_output("2048 MD5:aa:bb:cc alice@example.com (RSA)\n");

        assert!(matches!(
            result,
            Err(IdentityDeriveError::UnexpectedFingerprint(_))
        ));
    }
}
