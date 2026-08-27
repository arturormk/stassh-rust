# TODO-gui.md

This file tracks remaining `stassh-gui` work that is not primarily about
Secrets or Actions. Keep it in the repo while there are open GUI items.

## Remaining Work

- Richer diagnostics UX.
  The GUI already shows diagnostics, but the surface should become easier to
  scan and act on. It should identify missing local identity mappings, missing
  capability mappings, invalid references, duplicate host concerns, generated
  config needs, and other relevant failure states.

- Terminal layout regression coverage.
  Add manual, screenshot, or equivalent visual checks for terminal layout tabs,
  drag/drop layout composition, tab reordering, equal-grid and main-pane modes,
  broadcast input, internal full-screen panes, focused terminal find, and
  scrollback preservation.

- Layout and session persistence.
  Terminal tabs, layout tabs, and terminal state are currently runtime-only.
  Layout definitions may be persisted later outside the portable vault if a
  GUI-local persistence model is useful.

- Better jump-chain editing.
  Replace host-ID-oriented editing with an ordered visual chain, searchable host
  picker, remove/reorder controls, self-jump prevention, clear jump target
  details, and useful `ProxyJump` or temporary config preview.

- Better port-forward editing.
  Move beyond compact forward rows toward structured local, remote, and dynamic
  forward editors. Keep the direction of traffic and exact OpenSSH semantics
  clear, and distinguish configured forwards from currently running forwards.

- Identity UX polish.
  Improve the identity picker with clearer `(none)` behavior, preferred name,
  fingerprint, private key path, missing/unmapped current fingerprint
  preservation, and mapping health indicators. Local identity mapping management
  can be added later if it uses shared core behavior.

- Reload and external-change workflow.
  Explicit reload exists. Conservative file watching may be added later: detect
  vault/local config changes, notify the user, and reload at a safe point
  instead of silently overwriting external edits.

- Desktop interaction polish.
  Add or improve command palette coverage, context menus, valid drag/drop for
  folders, inline validation, and keyboard paths for high-frequency workflows
  such as connect, edit, diagnostics, reload, and move.

- Resource awareness and TUI fallback.
  Add startup or first-run checks for very small screens, insufficient memory,
  unreliable WebView/runtime conditions, rendering problems, high idle CPU, or
  missing graphical session. The GUI should recommend `stassh-tui` or a
  low-resource GUI mode when appropriate.

## Out Of Scope For This File

Secrets and Actions remain important `stassh-gui` work, but they are tracked
separately because they touch broader core, CLI, TUI, security, and process
management behavior.
