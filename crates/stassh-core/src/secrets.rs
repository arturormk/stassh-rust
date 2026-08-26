use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::storage::{StorageError, save_json_private};

pub const CURRENT_SECRETS_FORMAT_VERSION: u32 = 1;
const KEY_CHECK_PLAINTEXT: &[u8] = b"STASSH-SECRETS-KEY-CHECK-v1";
const KEY_CHECK_FIELD: &str = "__key_check__";

#[derive(Debug, Error)]
pub enum SecretsError {
    #[error("secrets format version {found} is not supported by this build; expected {expected}")]
    UnsupportedFormat { found: u32, expected: u32 },
    #[error("secrets set not found: {0}")]
    SetNotFound(String),
    #[error("secret field not found: {set}.{field}")]
    FieldNotFound { set: String, field: String },
    #[error("field is not encrypted: {set}.{field}")]
    FieldNotSecret { set: String, field: String },
    #[error("invalid value for {field}: {reason}")]
    InvalidValue { field: &'static str, reason: String },
    #[error("wrong master password or corrupted secrets store")]
    AuthenticationFailed,
    #[error("failed to derive secrets key")]
    KeyDerivation,
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("failed to read {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("failed to parse {path}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("failed to decode base64 for {field}: {source}")]
    Base64 {
        field: &'static str,
        source: base64::DecodeError,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretsStore {
    pub format_version: u32,
    pub store_id: Uuid,
    pub crypto: SecretsCrypto,
    pub key_check: EncryptedSecret,
    #[serde(default)]
    pub sets: BTreeMap<String, SecretSet>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretsCrypto {
    pub kdf: String,
    pub salt: String,
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretSet {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub fields: BTreeMap<String, SecretField>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum SecretField {
    Plain(String),
    Secret(EncryptedSecret),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncryptedSecret {
    #[serde(rename = "type")]
    pub kind: SecretRecordKind,
    pub nonce: String,
    pub ciphertext: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecretRecordKind {
    Secret,
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SecretsKey {
    bytes: [u8; 32],
}

impl fmt::Debug for SecretsKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretsKey(<redacted>)")
    }
}

impl SecretsStore {
    pub fn create(master_password: &str) -> Result<(Self, SecretsKey), SecretsError> {
        let crypto = SecretsCrypto::new();
        let key = derive_key(master_password, &crypto)?;
        let store_id = Uuid::new_v4();
        let key_check = encrypt_record(
            &key,
            store_id,
            KEY_CHECK_FIELD,
            KEY_CHECK_FIELD,
            KEY_CHECK_PLAINTEXT,
        )?;
        let store = Self {
            format_version: CURRENT_SECRETS_FORMAT_VERSION,
            store_id,
            crypto,
            key_check,
            sets: BTreeMap::new(),
        };
        Ok((store, key))
    }

    pub fn validate(&self) -> Result<(), SecretsError> {
        if self.format_version != CURRENT_SECRETS_FORMAT_VERSION {
            return Err(SecretsError::UnsupportedFormat {
                found: self.format_version,
                expected: CURRENT_SECRETS_FORMAT_VERSION,
            });
        }
        if self.crypto.kdf != "argon2id" {
            return Err(SecretsError::InvalidValue {
                field: "crypto.kdf",
                reason: "must be argon2id".to_string(),
            });
        }
        decode_base64("crypto.salt", &self.crypto.salt)?;
        if self.crypto.memory_kib < 8192 {
            return Err(SecretsError::InvalidValue {
                field: "crypto.memory_kib",
                reason: "must be at least 8192".to_string(),
            });
        }
        if self.crypto.iterations == 0 || self.crypto.parallelism == 0 {
            return Err(SecretsError::InvalidValue {
                field: "crypto",
                reason: "iterations and parallelism must be positive".to_string(),
            });
        }
        validate_record("key_check", &self.key_check)?;
        for (set_key, set) in &self.sets {
            validate_key("set", set_key)?;
            if set
                .label
                .as_ref()
                .is_some_and(|label| label.trim().is_empty())
            {
                return Err(SecretsError::InvalidValue {
                    field: "set.label",
                    reason: "must not be empty".to_string(),
                });
            }
            for (field_key, field) in &set.fields {
                validate_key("field", field_key)?;
                if let SecretField::Secret(record) = field {
                    validate_record("field", record)?;
                }
            }
        }
        Ok(())
    }

    pub fn unlock(&self, master_password: &str) -> Result<SecretsKey, SecretsError> {
        let key = derive_key(master_password, &self.crypto)?;
        self.verify_key(&key)?;
        Ok(key)
    }

    pub fn verify_key(&self, key: &SecretsKey) -> Result<(), SecretsError> {
        let plaintext = decrypt_record(
            key,
            self.store_id,
            KEY_CHECK_FIELD,
            KEY_CHECK_FIELD,
            &self.key_check,
        )?;
        if plaintext.as_slice() == KEY_CHECK_PLAINTEXT {
            Ok(())
        } else {
            Err(SecretsError::AuthenticationFailed)
        }
    }

    pub fn create_set(&mut self, key: String, label: Option<String>) -> Result<(), SecretsError> {
        validate_key("set", &key)?;
        if self.sets.contains_key(&key) {
            return Err(SecretsError::InvalidValue {
                field: "set",
                reason: format!("already exists: {key}"),
            });
        }
        self.sets.insert(
            key,
            SecretSet {
                label,
                fields: BTreeMap::new(),
            },
        );
        Ok(())
    }

    pub fn delete_set(&mut self, key: &str) -> Result<SecretSet, SecretsError> {
        self.sets
            .remove(key)
            .ok_or_else(|| SecretsError::SetNotFound(key.to_string()))
    }

    pub fn rename_set(&mut self, old: &str, new: String) -> Result<(), SecretsError> {
        validate_key("set", &new)?;
        if self.sets.contains_key(&new) {
            return Err(SecretsError::InvalidValue {
                field: "set",
                reason: format!("already exists: {new}"),
            });
        }
        let set = self
            .sets
            .remove(old)
            .ok_or_else(|| SecretsError::SetNotFound(old.to_string()))?;
        self.sets.insert(new, set);
        Ok(())
    }

    pub fn set_plain(
        &mut self,
        set: &str,
        field: String,
        value: String,
    ) -> Result<(), SecretsError> {
        validate_key("field", &field)?;
        self.set_mut(set)?
            .fields
            .insert(field, SecretField::Plain(value));
        Ok(())
    }

    pub fn set_secret(
        &mut self,
        key: &SecretsKey,
        set: &str,
        field: String,
        value: &str,
    ) -> Result<(), SecretsError> {
        validate_key("field", &field)?;
        self.verify_key(key)?;
        let record = encrypt_record(key, self.store_id, set, &field, value.as_bytes())?;
        self.set_mut(set)?
            .fields
            .insert(field, SecretField::Secret(record));
        Ok(())
    }

    pub fn reveal(
        &self,
        key: &SecretsKey,
        set: &str,
        field: &str,
    ) -> Result<SecretPlaintext, SecretsError> {
        self.verify_key(key)?;
        let fields = &self.set(set)?.fields;
        let Some(SecretField::Secret(record)) = fields.get(field) else {
            if fields.contains_key(field) {
                return Err(SecretsError::FieldNotSecret {
                    set: set.to_string(),
                    field: field.to_string(),
                });
            }
            return Err(SecretsError::FieldNotFound {
                set: set.to_string(),
                field: field.to_string(),
            });
        };
        let bytes = decrypt_record(key, self.store_id, set, field, record)?;
        Ok(SecretPlaintext { bytes })
    }

    pub fn delete_field(&mut self, set: &str, field: &str) -> Result<SecretField, SecretsError> {
        self.set_mut(set)?
            .fields
            .remove(field)
            .ok_or_else(|| SecretsError::FieldNotFound {
                set: set.to_string(),
                field: field.to_string(),
            })
    }

    pub fn set(&self, key: &str) -> Result<&SecretSet, SecretsError> {
        self.sets
            .get(key)
            .ok_or_else(|| SecretsError::SetNotFound(key.to_string()))
    }

    fn set_mut(&mut self, key: &str) -> Result<&mut SecretSet, SecretsError> {
        self.sets
            .get_mut(key)
            .ok_or_else(|| SecretsError::SetNotFound(key.to_string()))
    }
}

impl SecretsCrypto {
    fn new() -> Self {
        let mut salt = [0u8; 16];
        rand_core::RngCore::fill_bytes(&mut OsRng, &mut salt);
        Self {
            kdf: "argon2id".to_string(),
            salt: BASE64.encode(salt),
            memory_kib: 65_536,
            iterations: 3,
            parallelism: 1,
        }
    }
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SecretPlaintext {
    bytes: Vec<u8>,
}

impl fmt::Debug for SecretPlaintext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretPlaintext(<redacted>)")
    }
}

impl SecretPlaintext {
    pub fn expose_str(&self) -> Result<&str, SecretsError> {
        std::str::from_utf8(&self.bytes).map_err(|_| SecretsError::InvalidValue {
            field: "secret",
            reason: "is not valid UTF-8".to_string(),
        })
    }
}

pub fn load_secrets(path: impl AsRef<Path>) -> Result<SecretsStore, SecretsError> {
    let path = path.as_ref();
    let bytes = fs::read(path).map_err(|source| SecretsError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let store: SecretsStore =
        serde_json::from_slice(&bytes).map_err(|source| SecretsError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
    store.validate()?;
    Ok(store)
}

pub fn save_secrets(path: impl AsRef<Path>, store: &SecretsStore) -> Result<(), SecretsError> {
    store.validate()?;
    save_json_private(path, store)?;
    Ok(())
}

fn derive_key(master_password: &str, crypto: &SecretsCrypto) -> Result<SecretsKey, SecretsError> {
    let salt = decode_base64("crypto.salt", &crypto.salt)?;
    let params = Params::new(
        crypto.memory_kib,
        crypto.iterations,
        crypto.parallelism,
        Some(32),
    )
    .map_err(|_| SecretsError::KeyDerivation)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut bytes = [0u8; 32];
    argon2
        .hash_password_into(master_password.as_bytes(), &salt, &mut bytes)
        .map_err(|_| SecretsError::KeyDerivation)?;
    Ok(SecretsKey { bytes })
}

fn encrypt_record(
    key: &SecretsKey,
    store_id: Uuid,
    set: &str,
    field: &str,
    plaintext: &[u8],
) -> Result<EncryptedSecret, SecretsError> {
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&key.bytes));
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
    let aad = associated_data(store_id, set, field);
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| SecretsError::AuthenticationFailed)?;
    Ok(EncryptedSecret {
        kind: SecretRecordKind::Secret,
        nonce: BASE64.encode(nonce),
        ciphertext: BASE64.encode(ciphertext),
    })
}

fn decrypt_record(
    key: &SecretsKey,
    store_id: Uuid,
    set: &str,
    field: &str,
    record: &EncryptedSecret,
) -> Result<Vec<u8>, SecretsError> {
    let nonce = decode_base64("nonce", &record.nonce)?;
    let ciphertext = decode_base64("ciphertext", &record.ciphertext)?;
    let nonce = XNonce::from_slice(&nonce);
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&key.bytes));
    let aad = associated_data(store_id, set, field);
    cipher
        .decrypt(
            nonce,
            Payload {
                msg: &ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| SecretsError::AuthenticationFailed)
}

fn associated_data(store_id: Uuid, set: &str, field: &str) -> Vec<u8> {
    let mut aad = Vec::new();
    aad.extend_from_slice(b"stassh-secrets-v1\0");
    aad.extend_from_slice(store_id.as_bytes());
    aad.extend_from_slice(&(set.len() as u32).to_be_bytes());
    aad.extend_from_slice(set.as_bytes());
    aad.extend_from_slice(&(field.len() as u32).to_be_bytes());
    aad.extend_from_slice(field.as_bytes());
    aad
}

fn validate_record(field: &'static str, record: &EncryptedSecret) -> Result<(), SecretsError> {
    let nonce = decode_base64(field, &record.nonce)?;
    if nonce.len() != 24 {
        return Err(SecretsError::InvalidValue {
            field,
            reason: "nonce must be 24 bytes".to_string(),
        });
    }
    decode_base64(field, &record.ciphertext)?;
    Ok(())
}

fn validate_key(field: &'static str, value: &str) -> Result<(), SecretsError> {
    if value.trim().is_empty() {
        return Err(SecretsError::InvalidValue {
            field,
            reason: "must not be empty".to_string(),
        });
    }
    if value.contains('/') || value.contains('\\') || value.contains(char::is_whitespace) {
        return Err(SecretsError::InvalidValue {
            field,
            reason: "must not contain slashes or whitespace".to_string(),
        });
    }
    Ok(())
}

fn decode_base64(field: &'static str, value: &str) -> Result<Vec<u8>, SecretsError> {
    BASE64
        .decode(value)
        .map_err(|source| SecretsError::Base64 { field, source })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_round_trip_and_wrong_password_fails() {
        let (mut store, key) = SecretsStore::create("correct horse").unwrap();
        store
            .create_set("site".to_string(), Some("Site".to_string()))
            .unwrap();
        store
            .set_plain("site", "admin_user".to_string(), "root".to_string())
            .unwrap();
        store
            .set_secret(&key, "site", "password".to_string(), "secret-value")
            .unwrap();

        assert_eq!(
            store
                .reveal(&key, "site", "password")
                .unwrap()
                .expose_str()
                .unwrap(),
            "secret-value"
        );
        assert!(store.unlock("wrong password").is_err());
        assert!(matches!(
            store.reveal(&key, "site", "admin_user").unwrap_err(),
            SecretsError::FieldNotSecret { .. }
        ));
    }

    #[test]
    fn sensitive_debug_output_is_redacted() {
        let (mut store, key) = SecretsStore::create("correct horse").unwrap();
        store.create_set("site".to_string(), None).unwrap();
        store
            .set_secret(&key, "site", "password".to_string(), "secret-value")
            .unwrap();
        let plaintext = store.reveal(&key, "site", "password").unwrap();

        assert_eq!(format!("{key:?}"), "SecretsKey(<redacted>)");
        assert_eq!(format!("{plaintext:?}"), "SecretPlaintext(<redacted>)");
        assert!(!format!("{plaintext:?}").contains("secret-value"));
    }

    #[test]
    fn associated_data_binds_field_location() {
        let (mut store, key) = SecretsStore::create("master").unwrap();
        store.create_set("one".to_string(), None).unwrap();
        store.create_set("two".to_string(), None).unwrap();
        store
            .set_secret(&key, "one", "password".to_string(), "secret")
            .unwrap();
        let record = match store.set("one").unwrap().fields.get("password").unwrap() {
            SecretField::Secret(record) => record.clone(),
            SecretField::Plain(_) => panic!("expected secret"),
        };
        store
            .set_mut("two")
            .unwrap()
            .fields
            .insert("password".to_string(), SecretField::Secret(record));

        assert!(matches!(
            store.reveal(&key, "two", "password").unwrap_err(),
            SecretsError::AuthenticationFailed
        ));
    }
}
