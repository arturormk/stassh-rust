# Architecture

## Shape

Use a Cargo workspace with reusable Rust crates and separate application crates.

A conservative starting layout:

```text
crates/
  stassh-core/
apps/
  stassh/
  stassh-tui/
  stassh-gui/
```

Split additional crates out of `stassh-core` only when the boundary is clear, such as vault storage, crypto wrappers, OpenSSH orchestration, or shared UI-independent session logic.

## Core Boundary

The core crate owns:

- domain model
- validation
- host resolution
- identity references and local resolution interfaces
- OpenSSH command/config generation
- vault loading/saving abstractions
- synchronization semantics
- action and forwarding definitions
- diagnostics data structures

The core must not depend on Tauri, WebKit, GTK, Ratatui, xterm.js, browser APIs, JavaScript frameworks, or frontend state models.

## Frontend Boundary

The CLI, TUI, and GUI consume core APIs. They may format output, present editors, manage input focus, and launch frontend-specific terminal flows, but they should not duplicate host resolution, OpenSSH argument generation, vault semantics, or security policy.

## OpenSSH First

The initial execution engine is the installed OpenSSH client.

Prefer generating temporary SSH configuration for complex sessions instead of building fragile command lines. Generated config is session-local and should be deleted when the session ends.

The application should make resolved behavior inspectable:

- OpenSSH binary and version
- generated config path or rendered content
- resolved host/user/port
- selected identity or agent behavior
- jump chain
- forwarding options
- known-hosts file

## Dependency Discipline

Use mature dependencies for hard problems such as cryptography, serialization, terminal control, PTY handling, and CLI parsing.

Avoid large dependencies for trivial behavior, especially in `stassh-core`, `stassh`, and `stassh-tui`. The GUI can carry a larger platform dependency set, but the core and terminal tools should remain easy to build independently.

## Platform Defaults

Linux is the first implementation target. The core should avoid assumptions that block future macOS or Windows support:

- no Unix-only path model in synchronized records
- no reliance on symlinks, hard links, extended attributes, or POSIX-only locks in portable vaults
- configurable OpenSSH binary path
- no required daemon
- no required graphical environment for CLI/TUI
