# Vault, Storage, And Sync

## State Layers

Separate state by portability:

- synchronized: folders, hosts, usernames, identity fingerprints, jumps, forwards, actions, tags, notes, portable known-host policy/data where applicable
- vault-local: preferences tied to one vault but not part of the shared model
- machine-local: identity path mappings, capability executable paths, preferred terminal, platform overrides
- session-local: allocated ports, temporary SSH configs, PTYs, process handles, environment, one-time overrides

Do not synchronize machine-specific paths or transient session data.

## Portable Vaults

The application must be able to open a vault from an arbitrary directory. Portable removable storage is a first-class use case.

A vault should include a recognizable manifest so all frontends can detect and open it consistently. Portable mode should reduce user-specific residue on the host machine without promising forensic erasure.

## Encryption

Use standard, reviewed cryptographic crates. Do not invent custom cryptography.

Default design:

- user passphrase
- memory-hard KDF, likely Argon2id
- key-encryption key
- encrypted random vault key
- vault key encrypts synchronized records

Changing the passphrase should rewrap the vault key rather than rewriting every record.

Unencrypted manifest metadata may include format identifier, format version, KDF algorithm, KDF parameters, salt, and encrypted vault key. It must not expose host inventory.

## Versioning And Migration

Vault format versioning is required from the beginning.

Migrations should:

- detect current format
- preserve a recoverable previous version or snapshot
- write atomically where practical
- fail without corrupting the original vault
- report what changed

Unexpected shutdown during migration must not destroy the vault.

## Operation Journal

Synchronization should be transport-agnostic. External tools such as Syncthing, rsync, Git, Nextcloud, Dropbox, SMB, or USB copying may move bytes.

The application defines sync semantics, not sync transport.

Use an append-only per-device operation journal as the default design direction. Each device appends only under its own device identity. Operations should include:

- operation ID
- device ID
- device-local sequence number
- timestamp for display
- entity type
- entity ID
- operation type
- payload

Do not rely on wall-clock timestamps as the sole ordering mechanism.

## Deletes, Conflicts, And Snapshots

Deletes must be durable operations, such as tombstones, so old offline devices cannot silently resurrect intentionally deleted hosts.

Conflict handling should be deterministic, detectable, recoverable, understandable, and never silently destructive. Keep the initial strategy simple, but preserve enough history to explain conflicts.

Snapshots may compact state over time. Early versions may postpone aggressive compaction because configuration data is small and correctness matters more than size.

## Filesystem Constraints

Portable vaults may live on exFAT or inexpensive USB storage.

Do not depend on Unix permissions, symlinks, hard links, extended attributes, inode behavior, case-sensitive names, or POSIX-only locking.

Use opaque IDs for filenames instead of user-facing names. Prefer append-oriented writes, atomic replacement where available, validation, and partial-write detection.

Concurrent access by multiple local processes should be detected. Where locking is unreliable, prefer warning or read-only fallback over risking corruption.
