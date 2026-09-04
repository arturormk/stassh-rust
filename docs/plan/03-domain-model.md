# Domain Model

## Identity

Portable entities use stable IDs. Names, folder paths, hostnames, and display labels are not identity.

Renaming or moving a host must not change its underlying ID. Multiple host records may legitimately point at the same endpoint with different usernames, identities, jumps, or operational contexts.

Use UUIDs or an equivalent opaque stable identifier for portable entities.

## Initial Entities

The model should start with these concepts:

- `Folder`
- `Host`
- `IdentityReference`
- `JumpDefinition`
- `ForwardDefinition`
- `Action`
- `Tag`
- `Vault`
The implementation does not need all fields at once, but serialized formats should be versioned and extensible.

## Folders

Folders support nesting, creation, deletion, renaming, and movement.

Hosts belong to folders by ID. Folder paths are presentation state derived from IDs and parent relationships.

Avoid turning folders into a full CMDB. They exist to organize access definitions.

## Hosts

A host record is an access profile, not a unique physical machine.

Initial host fields should cover:

- ID
- parent folder ID
- display name
- hostname or address
- port
- username
- identity reference
- jump chain
- SSH option overrides
- tags
- notes
- forwards
- actions

The resolver should produce an explicit effective host configuration that frontends and diagnostics can display.

## Identities

Portable identity references should use cryptographic identity, not machine paths.

The portable record should contain a fingerprint and a human-friendly preferred name. Machine-local state maps that reference to a local private key path, an agent identity, a hardware-backed identity, or another OpenSSH-supported source.

The GUI and web frontend must not handle private key material.

## Jumps And Forwards

Jump hosts are first-class and should map to OpenSSH `ProxyJump` or equivalent generated config.

Forward definitions should model:

- local forwards
- remote forwards
- dynamic SOCKS forwards

Tunnel-only sessions should be represented as normal OpenSSH sessions with no shell, not as a separate proprietary tunnel system.

## Actions

Actions are composable workflows over ordinary tools. They may include remote preparation, SSH forwarding, local command launch, waiting, and cleanup.

Actions should use machine capabilities for local tools instead of storing absolute executable paths in portable records. For example, a portable action can require `vnc-viewer`, while each machine maps that capability to its local viewer executable.

Actions now have an initial implementation in the core model and can be launched
from the CLI and TUI. Editing actions is still expected to mature later.

## Local Overrides

The model should allow machine-local overrides without altering portable records.

Likely override targets:

- hostname/address
- jump path
- identity mapping
- terminal choice
- capability executable
- proxy-related settings

Resolved output must show when a value came from an override.
