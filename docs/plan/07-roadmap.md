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
- Keep storage plain, inspectable, and recoverable.

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

## Milestone 5: Host-Associated Secrets

- Add optional encrypted `secrets.json` storage for fallback operational
  secrets.
- Keep `vault.json` as non-secret portable metadata.
- Support deliberate reveal/hide workflows from the frontends.
- Keep ordinary SSH operation independent from unlocking secrets.

## Milestone 6: Portability And Backup Hygiene

- Treat `vault.json`, `local.json`, and `secrets.json` as user-owned files that
  can be copied, backed up, or synced by external tools.
- Avoid app-defined synchronization protocols, merge journals, and hosted sync.
- Validate files on load and report external-edit conflicts clearly.
- Preserve backups around risky migrations or writes.

## Milestone 6.5: Shared Simulation

- Add deterministic in-memory demo vault, local config, fake encrypted secrets,
  and scripted shell behavior in `stassh-core`.
- Wire `stassh-tui --simulation` to use the shared demo workspace and run
  simulated foreground connect/action sessions.
- Wire `stassh-gui --simulation` to use the same demo workspace and simulated
  terminal tabs for screenshots and visual checks.

## Milestone 7: Desktop GUI

- Implemented a Tauri-based desktop app under `apps/stassh-gui`.
- Reuse core APIs for vault, host resolution, diagnostics, and OpenSSH orchestration.
- Add host tree, search, host/folder editing, integrated terminal tabs, and diagnostics.
- Add terminal layout tabs with equal-grid and main-pane modes.
- Add layout composition by drag/drop, layout-local broadcast input, internal full-screen terminal panes, and host-tree open-session indicators.
- Add contextual Inspector details for selections, terminals, and layouts.
- Add Inspector panes for linked secrets, ordered jump-chain editing, and
  structured local/remote/dynamic forward editing.
- Add focused terminal find and running-session close confirmation.
- Add action listing, resolved dry-run preview, and action terminal-session
  running.
- Add GUI simulation mode with deterministic demo data, fake encrypted secrets,
  and scripted terminal sessions for screenshots and visual checks.
- Add initial automated visual regression coverage for TUI simulation renders
  and GUI terminal layouts.
- Keep the GUI host tree as a persistent navigator; batch host selection remains
  a TUI workflow.
- Keep terminal byte streaming out of heavyweight frontend state.

Future GUI polish:

- Add stronger JSON-first action authoring support.
- Expand automated visual regression coverage with optional full Tauri headless
  smoke checks.
- Keep the README screenshots in `examples/github-screenshot/` captured from
  `./run-stassh-tui-dev.sh --simulation` and
  `./run-stassh-gui-dev.sh --simulation`.

## Milestone 8: Actions And Capabilities

- Initial composable actions exist with optional SSH session commands, forwarding, local launch, and cleanup.
- Machine-local capability mappings exist for tools such as `vnc-viewer`, `browser`, and `terminal`.
- Automatic local port allocation exists for action forwards.
- Implement VNC and web-admin workflows as action examples, not special hard-coded subsystems.
- Add action dry-run/inspection flows and JSON-first authoring helpers.

## Milestone 9: Hardening And Interop

- Add portable known-host handling if the initial policy is validated.
- Add OpenSSH config import.
- Improve conflict handling and diagnostics.
- Add export, sanitized export, and backup tooling.
- Add optional tmux integration.
- Test and tune on modest hardware and removable storage.
