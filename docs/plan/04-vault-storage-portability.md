# Vault, Storage, And Portability

## State Layers

Separate state by portability:

- portable: folders, hosts, usernames, identity fingerprints, jumps, forwards, actions, tags, notes, portable known-host policy/data where applicable
- vault-local: preferences tied to one vault but not part of the shared model
- machine-local: identity path mappings, capability executable paths, preferred terminal, platform overrides
- session-local: allocated ports, temporary SSH configs, PTYs, process handles, environment, one-time overrides

Do not place machine-specific paths or transient session data in portable records.

## Portable Vaults

The application must be able to open a vault from an arbitrary directory. Portable removable storage is a first-class use case.

A vault should include explicit format/version information so all frontends can
detect and open it consistently. Portable mode should reduce user-specific
residue on the host machine without promising forensic erasure.

## Secrets

`vault.json` is host and workflow metadata, not a secrets store. Actual IP
addresses can be handled through DNS or `/etc/hosts`, SSH keys remain
system-installed, and private key material must not be stored in the vault.

The secrets-bearing file is `secrets.json`. It should encrypt fields that are
explicitly stored as secrets and should use standard, reviewed cryptographic
crates. Do not invent custom cryptography.

Whole-vault encryption is intentionally out of scope unless the product goals
change. Keeping ordinary metadata inspectable preserves the project's plain-file
portability and recovery story.

## Versioning And Migration

Vault format versioning is required from the beginning.

Migrations should:

- detect current format
- preserve a recoverable previous version or snapshot
- write atomically where practical
- fail without corrupting the original vault
- report what changed

Unexpected shutdown during migration must not destroy the vault.

## User-Managed Copying And Sync

External tools such as Syncthing, rsync, Git, Nextcloud, Dropbox, SMB, USB
copying, or normal backups may move the JSON files. Stassh should not define a
custom synchronization protocol, device identity system, operation journal, or
merge engine.

The expected unit of portability is the small set of user-owned JSON files:

```text
vault.json
local.json
secrets.json
```

Users who edit the same files concurrently on multiple machines should rely on
their chosen external tool's conflict behavior. Stassh should validate files
when loading and fail clearly rather than silently discard data.

## Conflicts And Backups

Because the application stores ordinary files, conflict recovery should stay
simple and inspectable: preserve backups around risky writes, report validation
errors clearly, and avoid overwriting external edits without an explicit reload
or save flow.

## Filesystem Constraints

Portable vaults may live on exFAT or inexpensive USB storage.

Do not depend on Unix permissions, symlinks, hard links, extended attributes, inode behavior, case-sensitive names, or POSIX-only locking.

Use opaque IDs for filenames instead of user-facing names. Prefer append-oriented writes, atomic replacement where available, validation, and partial-write detection.

Concurrent access by multiple local processes should be detected. Where locking is unreliable, prefer warning or read-only fallback over risking corruption.
