# stassh-rust Presentation Briefing

This document is written for ChatGPT or another discussion partner that needs to
understand `stassh-rust` without reading the source code. It is not intended as
end-user documentation. It explains what the project is, how it is built, what
it can currently do, and which user-facing features matter for public
presentation.

## One-Sentence Description

`stassh-rust` is a portable, offline-first SSH workspace for people who manage
many remote machines, with a shared Rust core and three frontends: CLI, terminal
UI, and desktop GUI.

Another accurate short description is:

> A fast organizer and orchestrator for ordinary OpenSSH, built for users who
> want their SSH workspace to travel between machines without a proprietary
> cloud account.

## What The Project Is For

The project helps technically capable users organize and run SSH workflows across
dozens, hundreds, or potentially thousands of hosts.

The target user already understands SSH or is comfortable with SSH concepts.
Likely audiences include system administrators, developers, consultants, field
engineers, network administrators, embedded developers, infrastructure support
technicians, homelab users, and researchers.

The project is not trying to hide SSH. It is trying to make SSH estates easier
to browse, search, inspect, edit, and invoke. It keeps the user close to the
actual OpenSSH command being run.

The core problem it addresses is: many people have SSH knowledge spread across
`~/.ssh/config`, notes, terminal history, memory, password managers, and ad hoc
scripts. `stassh-rust` gathers the operational structure into a portable local
workspace while still relying on the system OpenSSH client for the actual SSH
transport.

## Product Philosophy

The project is deliberately lightweight and offline-first.

It has no mandatory hosted account, cloud backend, telemetry dependency,
license check, or proprietary synchronization service. Users can copy or sync
the workspace files with tools they already trust.

It is OpenSSH-first. The application does not implement its own SSH protocol
stack. It generates commands and temporary OpenSSH config when needed, then
launches the system `ssh` binary. This keeps compatibility with existing SSH
agents, hardware keys, known-host handling, jump hosts, certificates,
keyboard-interactive authentication, port forwarding, and other OpenSSH
behavior.

The project is closer to an enhanced system terminal with excellent organization
than to a full remote-administration platform. It is not a monitoring suite,
configuration-management database, team credential vault, file manager, cloud
SSH service, or replacement operating environment.

## How It Is Built

The repository is a Rust workspace with these main components:

- `crates/stassh-core`: shared domain model, storage, OpenSSH generation,
  import/export, action resolution, secrets, diagnostics, and simulation data.
- `apps/stassh`: command-line interface.
- `apps/stassh-tui`: terminal UI built with `ratatui` and `crossterm`.
- `apps/stassh-gui`: desktop GUI built with Tauri 2, Rust backend commands,
  React 19, xterm.js, Vite, and TypeScript.

The frontends share the same model and core behavior through `stassh-core`.
This is important: the CLI, TUI, and GUI are not separate products with
separate storage semantics. They are different ways to interact with the same
portable SSH workspace.

The GUI backend uses Rust and `portable-pty` to manage embedded terminal
sessions. The GUI frontend uses React for the desktop interface and xterm.js for
terminal panes.

The TUI is designed for terminal-first or low-resource situations. It uses the
same vault and local config as the CLI and GUI, and can launch connections
directly or use tmux/byobu window launch support for multiple simultaneous SSH
sessions.

The project currently targets a v1.0-style release. The workspace package
version is stored as `1.0.0`, while user-facing release display can appear as
`1.0`. GitHub Actions packaging builds Linux `.deb` and `.rpm` packages that
install all three executables: `stassh`, `stassh-tui`, and `stassh-gui`.

## Storage Model

The default configuration location is:

```text
~/.ssh/stassh/vault.json
~/.ssh/stassh/local.json
~/.ssh/stassh/secrets.json
```

`vault.json` is the portable workspace file. It stores folders, hosts, actions,
tags, notes, jump chains, forwarding definitions, SSH options, host-to-secret
references, and identity fingerprints. It is plain JSON with an explicit format
version.

`local.json` is machine-local. It maps identity fingerprints to private-key
paths and maps capability names to local executable paths. This separation lets
the portable vault refer to an identity by fingerprint while each machine says
where that private key lives locally.

`secrets.json` is optional encrypted storage for host-associated fallback
operational secrets. It is not the primary product identity, and the project is
not a password manager. Secret values can be encrypted with Argon2id-derived
keys and XChaCha20-Poly1305. Metadata such as set names, labels, field names,
and plaintext fields can remain visible, so the user should treat the file as
private even though secret values are encrypted.

Long-running frontends remember the loaded `vault.json` file state and reject
saves if the vault changed externally. The user must reload before saving again.
This avoids silent overwrites when another tool or frontend edits the vault.

On Unix-like systems, the default home configuration directory and files are
expected to have private permissions, such as `700` for the directory and `600`
for JSON files.

## Core Concepts

The project models:

- Folders with stable UUIDs and hierarchical paths.
- Hosts with display name, hostname, port, username, tags, notes, SSH options,
  identity fingerprint reference, optional secrets set reference, forwards, and
  jump chain.
- Jump chains as ordered references to other hosts.
- Local, remote, and dynamic SSH forwards.
- Common reusable actions and host-specific actions.
- Identity mappings from stable fingerprint to local private key path.
- Capability mappings from a logical name, such as `browser`, to a local
  executable path.
- Diagnostics for missing identities, missing capabilities, invalid references,
  duplicate hosts, and other health concerns.

Hosts can be resolved by UUID, exact path/name, or search-style selector.

Search spans host paths, hostnames, usernames, notes, and tags.

## OpenSSH Behavior

The application launches the system `ssh` command.

For simple hosts, it can generate a direct command with `-p`, `-l`, `-i`,
`-J`, forwarding flags, and `-o` options as needed.

For more complex sessions, it can generate a temporary OpenSSH config file and
run `ssh -F <temporary config> <generated alias>`. Temporary configs are useful
for jump chains, identity mapping, forwards, SSH options, and action execution.
The generated config uses stable generated aliases based on host UUIDs.

The project can also import a useful subset of existing OpenSSH config files,
including nested `Include` files, and export vault contents back to OpenSSH
config format.

## Reusable Actions

Actions are one of the important differentiators.

An action can define:

- Extra SSH forwards needed for that workflow.
- Auto-allocated local ports, fixed ports, or ports read from environment output.
- A remote command to run over SSH.
- An optional local preparation command.
- An optional local launch command, such as opening a viewer or browser through
  a forwarded local port.
- Optional cleanup commands.

Actions can use templates for values such as host data and allocated ports.
The action resolver validates the action, allocates ports, prepares the OpenSSH
command, resolves local capability mappings, and produces a plan that can be
previewed or run.

A practical example is a VNC-over-SSH workflow: allocate or define a local
forward, open the SSH tunnel, then launch the local VNC viewer against the
forwarded port. The project can express that workflow without becoming a VNC
client itself.

Another practical example is "Send file to home": a local preparation helper
uses `fzf` or a fallback terminal picker to choose a local file, then invokes
`scp` to copy it to the selected host's home directory before stassh runs a tiny
remote `true` command. This demonstrates that actions can combine interactive
local scripts, selected host template values, and ordinary OpenSSH-adjacent
tools without adding a dedicated file-transfer subsystem.

## CLI Features

The `stassh` CLI is the scripting and inspection interface.

It supports:

- Vault init, status, check, duplicate detection, and path-based dedupe.
- Folder list/add/rename/move/delete.
- Host add/edit/delete.
- Host list, search, show, diagnose, and connect.
- Action execution and diagnosis.
- Interactive secrets management.
- Identity add/list/map/edit/rename/unmap/diagnose.
- Capability list/map/unmap/diagnose for local executable mappings used by
  actions.
- OpenSSH config import and export.
- Text output by default and structured JSON output with `--output json`.

The CLI is useful both for humans and automation. JSON mode emits a single JSON
document per command for scripts, tests, and future integrations.

## TUI Features

The `stassh-tui` frontend is a fast terminal interface for the same workspace.

It supports:

- Browsing hierarchical folders and hosts.
- Expanding and collapsing folders.
- Searching hosts.
- Viewing host and folder details.
- Viewing diagnostics.
- Creating, editing, copying, deleting, and moving hosts.
- Creating, renaming, moving, and deleting folders.
- Selecting multiple hosts and moving them together.
- Assigning or clearing identity fingerprints from local mappings.
- Editing jump chains.
- Editing local, remote, and dynamic forwards.
- Viewing linked secrets and revealing encrypted secret fields when unlocked.
- Opening an action palette.
- Running reusable actions.
- Launching OpenSSH connections.
- Optional tmux/byobu window launch with `t`.
- Mouse selection and double-click activation.
- Simulation mode with demo data and fake terminal behavior.

The TUI is especially important for remote, minimal, or low-resource
environments where a GUI is inconvenient.

## GUI Features

The `stassh-gui` frontend is a Tauri desktop app over the same core workspace.

It currently provides:

- Persistent host/folder tree.
- Search.
- Contextual inspector/editor panes.
- Host and folder create/edit/copy/delete workflows.
- Multi-host move support.
- Identity assignment and clearing.
- Linked secrets view and reveal behavior.
- Ordered jump-chain editing.
- Structured local, remote, and dynamic forward editing.
- Diagnostics display.
- OpenSSH command preview.
- Action list.
- Resolved action dry-run preview.
- Action execution in terminal sessions.
- Embedded xterm.js terminal tabs.
- GUI-managed PTYs for real OpenSSH sessions.
- Terminal layout tabs over existing sessions.
- Equal-grid and main-pane terminal layouts.
- Drag/drop terminal-to-layout composition.
- Layout-local broadcast input.
- Internal full-screen terminal panes.
- Per-terminal find.
- Host-tree indicators for open sessions.
- Exited-state badges for completed sessions.
- Host notes surfaced in terminal headers.
- Simulation mode for screenshot-safe demos and visual regression checks.

The GUI should be presented as a desktop operational workspace, not as a
decorative dashboard. Its value is that browsing, editing, connecting, and
orchestrating many hosts can happen in one calm desktop app while retaining
OpenSSH transparency.

## Simulation And Demo Mode

Both TUI and GUI support simulation mode.

Simulation mode uses deterministic in-memory demo data instead of reading or
writing the user's real `~/.ssh/stassh` files. It includes demo hosts, folders,
identity mappings, capability mappings, fake encrypted secrets, jump chains,
forwards, diagnostics, and scripted terminal sessions.

In GUI simulation, terminal sessions behave like small scripted SSH-like shells.
They print demo connection output and support simple commands such as `help`,
`ls`, `pwd`, `cat`, `uptime`, `clear`, and `exit`.

Fake encrypted demo secrets unlock with the simulation-only master password:

```text
simulation
```

This mode is useful for screenshots, public demos, and visual regression tests
because it avoids exposing real hostnames, usernames, secrets, or SSH sessions.

## Testing And Release Infrastructure

The project has Rust tests across the workspace and CLI integration tests.

The GUI has Playwright visual regression tests using deterministic simulation
data and committed screenshot baselines. This is useful because the GUI has
complex terminal layout behavior that can regress visually even when unit tests
pass.

Build commands:

```bash
cargo build --workspace
cargo test --workspace
```

GUI visual tests:

```bash
cd apps/stassh-gui
npm run test:visual
```

The Linux packaging workflow builds `.deb` and `.rpm` packages for tags such as
`v1.0`. The packages install the CLI, TUI, and GUI binaries together.

## What Is Implemented Now

The project is not just a concept. It has working CLI, TUI, GUI, shared core,
storage, OpenSSH command generation, optional encrypted secrets, import/export,
identity mapping, actions, simulation, and Linux packaging.

Key implemented capabilities include:

- Plain JSON vault with format version.
- Default home config under `~/.ssh/stassh`.
- Safe-permission checks on Unix-like systems.
- Folder and host management.
- Host search and selector resolution.
- Duplicate host detection.
- Read-only vault health checks.
- Jump chains.
- Local, remote, and dynamic forwards.
- Common and host-specific reusable actions.
- OpenSSH command and config generation.
- Direct `stassh connect`.
- `stassh action` workflows.
- `stassh capability` workflows.
- Optional encrypted secrets store.
- Host-to-secrets-set references.
- OpenSSH config import and export.
- Identity fingerprint derivation with `ssh-keygen -lf`.
- Local identity and capability mappings.
- Text and JSON CLI output.
- TUI browsing, editing, connecting, secrets, diagnostics, actions, and tmux.
- GUI host tree, inspector/editor, embedded sessions, action previews, terminal
  layouts, drag/drop, broadcast input, find, full-screen panes, and simulation.

## Not Implemented Yet

Known gaps include:

- Automatic identity discovery by scanning `~/.ssh` or `ssh-agent`.
- GUI-friendly action authoring helpers that preserve the current JSON-first
  action model.
- Broader polish around diagnostics surfaces and advanced GUI workflows.
- Built-in synchronization or multi-device merge logic.
- A native SSH protocol backend.
- Full password-manager behavior.
- Team sharing or cloud collaboration.
- Integrated monitoring, SFTP, VNC, RDP, or browser implementations.

Many of these are intentional non-goals for v1.0 rather than accidental
omissions.

## Public Presentation Angles

Strong angles for discussing the project:

- "Portable SSH workspace, not another SSH client."
- "OpenSSH-first: organize and orchestrate the SSH you already trust."
- "CLI, TUI, and GUI over one shared local vault."
- "Offline-first and file-portable, with no required cloud account."
- "Built for people with many hosts and real operational workflows."
- "Lightweight desktop utility with a serious terminal-first fallback."
- "Reusable SSH actions for workflows like tunnels, forwarded tools, and remote
  commands."

Angles to avoid or handle carefully:

- Do not present it as a password manager. It has optional encrypted secrets,
  but that is not the main identity.
- Do not present it as a replacement for OpenSSH. It deliberately relies on
  OpenSSH.
- Do not present it as a cloud/team product. Offline-first local portability is
  a differentiator.
- Do not overclaim security. The vault metadata is plain JSON, local paths are
  private but not secret, and secrets metadata remains visible.
- Do not imply it has automatic sync, monitoring, SFTP, RDP, VNC, or native SSH
  protocol support.

## Suggested Target Audience Framing

The most natural audience is practical technical operators:

- People who have outgrown hand-edited SSH config but do not want a cloud SSH
  platform.
- Users who move between laptops, field machines, lab systems, and workstations.
- Admins and developers who value transparent OpenSSH commands.
- People who want both keyboard-first and desktop workflows.
- Homelab and infrastructure users who appreciate local files and low overhead.

The tone should be pragmatic and transparent. It should emphasize control,
portability, compatibility, and workflow speed rather than glossy enterprise
claims.

## A Compact Pitch

`stassh-rust` is a portable SSH workspace for people who administer many
machines. It keeps hosts, folders, jump chains, port forwards, identities,
notes, tags, secrets references, and reusable actions in local files under
`~/.ssh/stassh`, then exposes the same workspace through a CLI, a fast TUI, and
a Tauri desktop GUI with embedded terminal sessions. It does not replace
OpenSSH or require a cloud account. It organizes and orchestrates the OpenSSH
workflows users already rely on.
