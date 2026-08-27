# Frontends

## Shared Semantics

`stassh`, `stassh-tui`, and `stassh-gui` are first-class frontends over the same vault and core logic.

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

Optional tmux/byobu integration exists in the TUI. The application must not
require tmux.

## GUI

The GUI should be modest, dense, fast, keyboard-friendly, and visually restrained.

Current MVP features:

- host tree
- search
- host editor
- folder editor
- contextual inspector for selected hosts, selected folders, active terminals,
  and layouts
- diagnostics and generated OpenSSH preview
- linked secrets inspection with explicit reveal
- ordered jump-chain editing through a dedicated inspector pane
- structured local, remote, and dynamic forward editing through a dedicated
  inspector pane and host editor
- terminal tabs
- focused terminal find
- independent terminal layout tabs
- equal-grid and main-pane terminal layouts
- layout-local broadcast input
- internal full-screen terminal panes
- running-session close confirmation
- open-session indicators in the host tree
- integrated PTY terminal through xterm.js
- connect via OpenSSH

The GUI host tree is a persistent navigator. It intentionally does not carry the
TUI batch-selection model; multiple GUI sessions can be opened one by one while
the tree remains visible.

Avoid decorative dashboards, large cards, splash screens, account-centric flows, animated backgrounds, and excessive empty space.

The current GUI uses Tauri, React, xterm.js, and a Rust PTY backend. Any future
framework changes should be justified by maintainability, accessibility,
keyboard handling, state management, and performance rather than fashion.

Near-term GUI polish:

- add richer action-running, dry-run inspection, and JSON-first authoring support
- add screenshot or equivalent visual regression checks for terminal layouts

## GUI Terminal Path

The GUI uses a terminal emulator because a WebView is not a native terminal.

Current structure:

```text
terminal component / layout view
  <-> narrow streaming IPC bridge
  <-> Rust PTY management
  <-> system ssh
```

Terminal bytes are high-volume data and must not flow through heavyweight global frontend state. Host tree data and terminal I/O should have separate paths.

Terminal tabs and layout tabs are GUI runtime state. Layouts are views over
existing terminal sessions, not separate SSH sessions, and they should remain
outside the portable vault unless a future cross-frontend persistence model is
designed.

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
