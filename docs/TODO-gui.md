# TODO-gui.md

This file tracks remaining `stassh-gui` work that is not primarily about
Secrets. Keep it in the repo while there are open GUI items.

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

- Jump and forward workflow polish.
  Dedicated Inspector panes now cover ordered jump-chain editing and structured
  local, remote, and dynamic forward editing. Remaining polish includes stronger
  keyboard paths, clearer invalid-reference recovery after external vault edits,
  and distinguishing configured forwards from currently running forwards.

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

- Action running and JSON-first authoring support.
  The GUI should provide a strong action-running surface for the selected host:
  list common and host-specific actions, show their origin, run them through the
  shared core action path, and offer a dry-run/inspect view with resolved SSH
  commands, temporary forwards, allocated ports, local launch commands, cleanup,
  and missing capability diagnostics. Full structured action editing is deferred.
  Actions remain JSON-first because they are programmable workflows with remote
  commands, local tools, templating, forwards, cleanup, and machine-local
  capability assumptions. GUI authoring help should focus on opening
  `vault.json`/`local.json`, copying templates, and validating or previewing
  JSON rather than building a form editor for the whole schema.

- Resource awareness and TUI fallback.
  Add startup or first-run checks for very small screens, insufficient memory,
  unreliable WebView/runtime conditions, rendering problems, high idle CPU, or
  missing graphical session. The GUI should recommend `stassh-tui` or a
  low-resource GUI mode when appropriate.

## Out Of Scope For This File

Secrets management remains important `stassh-gui` work, but it is tracked
separately because it touches broader core, CLI, TUI, security, and process
management behavior.
