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
  Initial automated coverage exists. The TUI has deterministic ratatui buffer
  render checks for simulation views, and the GUI has Playwright screenshot
  checks for simulated terminal layout tabs, drag/drop layout composition,
  equal-grid and main-pane modes, broadcast input, internal full-screen panes,
  and focused terminal find. Remaining work is to add explicit tab-reordering
  and scrollback-preservation checks, and optionally a full Tauri
  `./run-stassh-gui-dev.sh --simulation --headless` smoke test if CI has stable
  graphical dependencies. The committed screenshot pair in
  `examples/github-screenshot/` remains the manual baseline for README images;
  refresh it with `./run-stassh-tui-dev.sh --simulation` and
  `./run-stassh-gui-dev.sh --simulation` when the intended visual baseline
  changes.

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

- Action authoring support.
  The GUI can now list common and host-specific actions for the selected host,
  show their origin, preview the resolved dry-run plan, and run them as terminal
  sessions. Full structured action editing is still deferred. Actions remain
  JSON-first because they are programmable workflows with remote commands, local
  tools, templating, forwards, cleanup, and machine-local capability
  assumptions. GUI authoring help should focus on opening `vault.json` and
  `local.json`, copying templates, and validating or previewing JSON rather than
  building a form editor for the whole schema.

- Resource awareness and TUI fallback.
  Add startup or first-run checks for very small screens, insufficient memory,
  unreliable WebView/runtime conditions, rendering problems, high idle CPU, or
  missing graphical session. The GUI should recommend `stassh-tui` or a
  low-resource GUI mode when appropriate.

## Out Of Scope For This File

Secrets management remains important `stassh-gui` work, but it is tracked
separately because it touches broader core, CLI, TUI, security, and process
management behavior.
