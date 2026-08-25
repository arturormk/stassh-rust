# Frontends

## Shared Semantics

`stassh`, `stassh-tui`, and `stassh-GUI` are first-class frontends over the same vault and core logic.

All frontends should understand the same synchronized records and resolved host behavior. They may differ in workflow depth and visual richness, but they must not create incompatible data.

## CLI

The CLI is both a user tool and a scripting interface.

Initial commands should cover:

```text
stassh list
stassh search <query>
stassh show <host>
stassh connect <host>
stassh diagnose <host>
stassh vault status
stassh identities
```

Human-readable output is the default. Commands that expose structured data should support JSON output once their semantics are stable enough to script against.

CLI behavior should become compatibility-conscious earlier than graphical layout, because users may automate it.

## TUI

The TUI should work as a full interface to the same vault on terminal-only and low-resource systems.

Initial features:

- browse folders
- fuzzy search
- inspect host details
- connect
- basic create/edit/delete when the model stabilizes
- return to TUI after SSH exits

Prefer suspending the alternate-screen UI and launching ordinary `ssh` attached directly to the terminal. This preserves the user's terminal emulator behavior and keeps the TUI lightweight.

Optional tmux integration can come later. The application must not require tmux.

## GUI

The GUI should be modest, dense, fast, keyboard-friendly, and visually restrained.

Initial features:

- host tree
- fuzzy search
- host editor
- tabs or sessions
- integrated PTY terminal
- connect via OpenSSH
- diagnostics view

Avoid decorative dashboards, large cards, splash screens, account-centric flows, animated backgrounds, and excessive empty space.

The GUI likely uses Tauri, but the exact frontend framework should be chosen for maintainability, accessibility, keyboard handling, state management, and performance rather than fashion.

## GUI Terminal Path

The GUI needs a terminal emulator because a WebView is not a native terminal.

Likely structure:

```text
terminal component
  <-> narrow streaming IPC bridge
  <-> Rust PTY management
  <-> system ssh
```

Terminal bytes are high-volume data and must not flow through heavyweight global frontend state. Host tree data and terminal I/O should have separate paths.

## Keyboard And Actions

Keyboard-first interaction matters in both TUI and GUI.

Likely shared actions:

- search hosts
- connect/default action
- open action palette
- edit selected host
- open diagnostics
- close session

Exact key bindings should be chosen later and should respect platform conventions.
