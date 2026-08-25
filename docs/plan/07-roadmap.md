# Roadmap

## Milestone 0: Project Skeleton

- Initialize Cargo workspace.
- Add core crate and CLI app crate.
- Establish formatting, linting, tests, and CI basics.
- Add initial documentation commands and contribution guidance.

## Milestone 1: Core Model And Plain Local Storage

- Implement folders, hosts, tags, identity references, jumps, and forwards at the model level.
- Use stable IDs independent of names and paths.
- Add validation and resolved host output.
- Add a simple local storage format with explicit versioning.
- Keep storage replaceable so encrypted vaults and operation journals can follow.

## Milestone 2: OpenSSH Orchestration And CLI

- Locate or configure the OpenSSH binary.
- Generate inspectable OpenSSH commands and temporary config files.
- Implement `stassh list`, `search`, `show`, `connect`, and `diagnose`.
- Support jump hosts and basic forwarding generation.
- Clean up session-local temporary files.

## Milestone 3: Identity Resolution

- Model synchronized identity references by fingerprint and preferred name.
- Add machine-local identity mappings.
- Discover public keys from common `~/.ssh` locations, OpenSSH config references, and `ssh-agent` where practical.
- Report missing identities clearly and allow local resolution.

## Milestone 4: TUI

- Implement folder browsing, fuzzy search, host detail display, and connect.
- Suspend the TUI while OpenSSH owns the terminal, then restore it after SSH exits.
- Add basic edit/create/delete once storage semantics are stable.

## Milestone 5: Encrypted Portable Vault

- Add vault manifest and format version.
- Add passphrase-based unlock with memory-hard KDF and wrapped vault key.
- Encrypt synchronized records.
- Support opening a vault from any directory.
- Add lock behavior that clears frontend vault state and removes temporary session files.

## Milestone 6: Sync Semantics

- Add device identity.
- Implement append-only per-device operation journals.
- Add create/update/delete/move operations and tombstones.
- Add deterministic merge behavior and basic conflict reporting.
- Add snapshots only after operation replay behavior is tested.

## Milestone 7: GUI

- Implement a Tauri-based or similarly lightweight desktop app.
- Reuse core APIs for vault, host resolution, diagnostics, and OpenSSH orchestration.
- Add host tree, search, host editor, integrated terminal tabs, and diagnostics.
- Keep terminal byte streaming out of heavyweight frontend state.

## Milestone 8: Actions And Capabilities

- Initial composable actions exist with optional SSH session commands, forwarding, local launch, and cleanup.
- Machine-local capability mappings exist for tools such as `vnc-viewer`, `browser`, and `terminal`.
- Automatic local port allocation exists for action forwards.
- Implement VNC and web-admin workflows as action examples, not special hard-coded subsystems.
- Add action editing commands and TUI/GUI action editors.

## Milestone 9: Hardening And Interop

- Add portable known-host handling if the initial policy is validated.
- Add OpenSSH config import.
- Improve conflict handling and diagnostics.
- Add export, sanitized export, and backup tooling.
- Add optional tmux integration.
- Test and tune on modest hardware and removable storage.
