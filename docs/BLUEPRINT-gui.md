# BLUEPRINT-gui.md

## Project Blueprint: stassh GUI

### Status Of This Document

This document describes the current direction for `stassh-gui`.

It is intentionally **descriptive, not prescriptive**. The purpose is to give a
developer enough product, architecture, and workflow context to continue building
the GUI without cloning the TUI's design or accidentally weakening the existing
CLI/TUI semantics.

`apps/stassh-gui` exists as a Tauri desktop app with a React/xterm.js frontend
and Rust PTY/session backend. It can browse and edit the vault, search hosts,
show diagnostics and OpenSSH previews in a contextual Inspector, and open
embedded OpenSSH terminal sessions. Its terminal workspace supports independent
terminal tabs, independent `Layout {n}` tabs over existing sessions, equal-grid
and main-pane layouts, drag/drop layout composition, layout-local broadcast
input, host-tree open-session indicators, per-terminal find, exited-session
badges in tabs and terminal headers, host notes in terminal headers, and
internal full-screen terminal panes. It also has a GUI-first simulation mode
with deterministic in-memory demo data, fake encrypted secrets, and scripted
SSH-like terminal sessions for screenshots and future visual regression checks.

The project may and should diverge from this blueprint when implementation
experience, platform constraints, performance testing, security review, or
usability findings suggest a better approach.

Near-term GUI work is mostly polish and peripheral workflow depth: JSON-first
action authoring support, richer diagnostics surfaces, and automated visual
regression coverage for terminal-layout behavior.

---

# 1. Product Direction

`stassh-gui` should be the polished desktop version of the same portable SSH
workspace exposed today through `stassh` and `stassh-tui`.

It should feel like a premium operational utility:

* fast to launch,
* calm and visually refined,
* dense enough for real work,
* keyboard-friendly,
* mouse-friendly,
* comfortable with hundreds or thousands of hosts,
* and clear about what OpenSSH command or action it is running.

It should not feel like a terminal-themed copy of the TUI. It should use the
space, typography, affordances, tabs, panes, menus, search surfaces, drag/drop,
and visual feedback that a good desktop application can provide.

At the same time, it should avoid becoming a decorative dashboard or remote
administration platform. `stassh` remains an organizer and orchestrator for
ordinary SSH workflows.

Useful shorthand:

> A polished desktop SSH workspace for browsing, editing, connecting to, and
> orchestrating many OpenSSH hosts, with embedded terminal sessions and optional
> embedded visual action surfaces such as VNC.

---

# 2. Relationship To Existing Frontends

The GUI is a first-class frontend over the same data and core semantics as the
CLI and TUI.

It should reuse `stassh-core` for:

* vault, local config, and secrets path resolution,
* safe `~/.ssh/stassh` permission checks,
* vault and local config loading and validation,
* host, folder, jump, forward, action, identity, and secrets models,
* host search and path resolution,
* OpenSSH command/config generation,
* reusable action resolution,
* encrypted secret reveal operations,
* and future storage or migration logic.

The GUI must not create vault records that the CLI and TUI cannot safely read,
unless the core format is intentionally migrated for all frontends.

GUI-specific state should remain outside the portable vault unless there is a
clear cross-frontend reason to persist it. Examples of GUI-only state include:

* currently open session tabs,
* pane layout,
* selected sidebar widths,
* terminal scrollback limits,
* action run progress,
* embedded VNC surface state,
* transient decrypted secret display,
* and window geometry.

The TUI remains the preferred interface on terminal-only, low-resource, or
graphically unreliable systems. The GUI should detect when it is unlikely to
perform well and make the TUI path obvious rather than pretending the desktop
experience is always appropriate.

---

# 3. Functional Coverage From The TUI

The GUI should cover the behavior users can currently perform in `stassh-tui`,
then improve the workflow where a graphical application naturally can.

## 3.1 Inventory Navigation

The GUI should provide a persistent host/folder navigation surface.

The current TUI supports:

* hierarchical folders and hosts,
* root folder always present,
* non-root folders collapsed by default,
* expand/collapse,
* host path display,
* sibling and parent navigation,
* mouse selection and double-click activation,
* and status feedback for invalid operations.

The GUI should expose the same hierarchy with a desktop-native tree/list control.
Folder expansion, selection, search result selection, and details should remain
stable while the user edits or reloads data.

For large inventories, the tree should support virtualization or another
efficient rendering strategy. Performance with thousands of hosts matters more
than ornamental presentation.

## 3.2 Search

The TUI search matches terms across host path, hostname, username, notes, and
tags.

The GUI should preserve that broad search behavior and improve presentation:

* global quick search,
* visible matched host path and connection target,
* keyboard selection,
* mouse activation,
* and immediate transition from a result to connect, edit, diagnostics, actions,
  or details.

Search should be fast enough to use as the primary navigation path.

## 3.3 Details And Diagnostics

The TUI details panel shows the selected host or folder, with an optional
diagnostics view.

The GUI should make host details richer and easier to scan:

* host path,
* display name,
* hostname,
* port,
* username or default authentication,
* identity fingerprint and local mapping status,
* secrets set reference,
* jump chain,
* raw SSH options when present,
* local, remote, and dynamic forwards,
* common and host-specific actions,
* tags,
* notes,
* generated OpenSSH command preview,
* and diagnostics/warnings.

Diagnostics should be visible as a dedicated tab, inspector section, or command
palette action. It should identify missing local identity mappings, missing
capability mappings, invalid references, duplicate host concerns, and generated
config needs.

## 3.4 Editing

The TUI currently supports basic vault editing. The GUI should cover the same
surface with form-based editors and validation feedback.

Host editing should support:

* create host in selected folder or selected host's folder,
* edit display name,
* edit hostname,
* edit port,
* edit username,
* edit comma-separated or tokenized tags,
* edit notes,
* copy host with a default `<name> copy` display name,
* delete host after confirmation,
* and preserve host-specific actions when copying.

Folder editing should support:

* create folder under selected folder or selected host's folder,
* rename folder,
* move folder to another valid parent,
* prevent root folder rename/move/delete,
* prevent moving a folder inside itself,
* delete only empty folders,
* and show clear errors for non-empty folder deletion.

The GUI should avoid requiring users to paste UUIDs for ordinary operations.
Folder moves should use a folder picker or drag/drop. UUIDs may still be visible
in advanced details for debugging and compatibility with CLI workflows.

All vault writes should follow the TUI's cautious pattern: reload from disk,
apply the intended stable-ID mutation, save once, reload or refresh UI state,
and surface any conflict or validation error clearly.

## 3.5 Multi-Select And Moves

The TUI supports selecting individual hosts and all descendant hosts under a
folder. Folder rows indicate partial or complete selection, and selected hosts
can be moved together.

The GUI should keep multi-select as a first-class workflow:

* checkbox or selection affordances on hosts,
* folder-level select all descendants,
* partial-selection state on folders,
* selected-host count,
* clear selection,
* move selected hosts to a destination folder,
* and preserve selection across browsing and search until an action clears it.

This selection model is useful beyond moves. Future GUI workflows may use it for
bulk export, tagging, diagnostics, or action runs.

## 3.6 Identity Assignment

The TUI can assign or clear a host's `identity_fingerprint` from local identity
mappings.

The GUI provides a basic identity picker from local identity mappings. It should
evolve toward:

* `(none)` for default OpenSSH/password/agent behavior,
* local identity mappings from `local.json`,
* preferred name,
* fingerprint,
* private key path,
* missing/unmapped current fingerprint preservation,
* and mapping health indicators.

Creating, renaming, editing, and removing local identity mappings currently
happens through the CLI. The GUI may add these workflows if it can do so through
shared core logic without creating incompatible local config behavior.

## 3.7 Jump Chains

The TUI jump editor shows existing jumps first, then remaining hosts sorted by
path. The edited host cannot be its own jump target. Selected jumps can be
reordered.

The GUI currently edits jump chains through the Inspector using host IDs. It
should evolve toward a more visual and precise workflow:

* ordered chain view,
* searchable host picker,
* remove/reorder controls,
* prevention of self-jumps,
* clear display of each jump host's username, hostname, and port,
* and generated `ProxyJump`/temporary config preview when useful.

The stored value remains the ordered list of jump host UUIDs.

## 3.8 Port Forwards

The TUI forward editor supports:

* local forwards,
* remote forwards,
* dynamic SOCKS forwards,
* bind address,
* local or remote listening port,
* destination host,
* destination port,
* add/remove rows,
* and validation that ports are non-zero `u16` values.

The GUI currently shows forwards in compact rows and can add/remove local
forwards in the Inspector. It should evolve toward structured type-specific
editing for local, remote, and dynamic forwards. The direction of traffic should
remain obvious without hiding the exact OpenSSH semantics.

For active sessions, the GUI should distinguish configured forwards from
currently running forwards and should show allocated action ports when relevant.

## 3.9 Actions

Actions are central to the GUI because they can produce workflows richer than a
plain SSH terminal.

The current core action model supports:

* common vault actions applied to every host,
* host-specific actions appended after common actions,
* optional local prepare command,
* action-defined local or dynamic forwards,
* fixed, automatic, or prepare-env-derived local ports,
* optional remote command appended to SSH,
* optional local launch command,
* cleanup commands,
* `{HOST}`,
* `{USER}`,
* `{LOCAL_PORT:name}`,
* and `{ENV:NAME}` template variables.

The GUI exposes actions from the host inspector. It lists common actions before
host-specific actions, shows each action's origin, previews the resolved dry-run
plan, and runs an action as a terminal session. Running an action should keep
showing:

* action name,
* target host,
* lifecycle state,
* local prepare status,
* allocated ports,
* SSH process status,
* local launch status,
* cleanup status,
* and error output or diagnostics when something fails.

The action runner retains the OpenSSH-first model while adding GUI process
management around it.

Action authoring should remain JSON-first for now. The GUI can help by opening
`vault.json` and `local.json`, copying schema-backed templates, validating
actions, and previewing dry-run output. A full structured action form editor is
deferred because actions are programmable workflows rather than simple host
metadata.

## 3.10 Secrets

The TUI can open a selected host's linked secrets set, list fields, show plaintext
fields, and reveal encrypted fields after prompting for the secrets master
password.

The GUI should handle secrets conservatively:

* do not load secrets unless the host references a set and the user opens it,
* display set key, label, and fields,
* distinguish plaintext metadata fields from encrypted fields,
* prompt for the master password only when revealing encrypted fields,
* zeroize password input and plaintext buffers wherever core support allows,
* support hiding a revealed secret again,
* avoid placing secrets in logs, telemetry, crash reports, titles, persistent UI
  state, or terminal scrollback,
* and make copy-to-clipboard behavior explicit and time-limited if added.

Secrets management exists in the CLI today. The GUI may add management later, but
simple, safe reveal of linked host secrets is the first priority.

## 3.11 Reload And External Changes

The TUI can reload vault, local config, and secrets from disk.

The GUI should support explicit reload and may later add file watching. Any file
watching should be conservative: detect changes, notify the user, and reload at a
safe point rather than overwriting external edits silently.

---

# 4. Session Model

The GUI's largest functional difference from the TUI is session ownership.

The TUI leaves the alternate screen and attaches OpenSSH directly to the user's
terminal. It does not manage PTYs or terminal tabs. Multi-session use is handled
with tmux/byobu when available.

The GUI must manage embedded sessions itself.

## 4.1 SSH Terminal Sessions

An SSH connection should open inside the GUI as an embedded terminal tab or pane.

Likely structure:

```text
terminal emulator component
  <-> narrow streaming IPC bridge
  <-> Rust PTY/session manager
  <-> system ssh
```

The terminal byte stream is high-volume data. It should not pass through heavy
global frontend state or general application reducers. Treat terminal I/O as a
dedicated streaming channel.

The Rust side should own:

* PTY creation,
* child process launch,
* resize events,
* stdin/stdout/stderr byte transport,
* process exit status,
* signal/termination behavior,
* temporary OpenSSH config lifetime,
* and cleanup of action child processes.

The frontend should own:

* terminal rendering,
* selection and copy/paste behavior,
* focus,
* tab/pane layout,
* keyboard shortcuts that are not consumed by the terminal,
* session titles,
* and visual status.

Use OpenSSH for the actual connection. Password prompts, key prompts,
keyboard-interactive authentication, host-key verification, agent use, and remote
terminal behavior should happen inside the embedded terminal whenever possible.

Simulation mode intentionally bypasses OpenSSH execution while keeping the same
frontend terminal/session surface. It loads demo vault, local config, and
secrets data in memory, returns virtual `simulation://...` paths, and routes
terminal input to a scripted shell. The simulated shell should print an initial
connection message and prompt automatically, then provide deterministic output
for common commands such as `help`, `ls`, `pwd`, `cat`, `uptime`, `clear`, and
`exit`.

## 4.2 Tabs And Panes

The GUI supports multiple concurrent sessions without requiring tmux.

Current session affordances:

* open selected host in a new terminal tab,
* open multiple selected hosts as terminal tabs,
* keep each terminal session as an individual tab,
* create independent layout tabs over existing terminal sessions,
* view sessions in equal-grid or main-plus-secondary-grid layouts,
* drag a terminal tab onto a layout tab to add it,
* drag one terminal tab onto another terminal tab to create a new layout,
* reorder terminal and layout tabs while keeping terminal scrollback mounted,
* use layout-local broadcast input,
* full-screen the focused terminal pane inside the app window,
* search focused terminal scrollback with optional case sensitivity,
* show host notes in terminal headers when notes are available,
* show connected/running/exited state,
* open screenshot-safe simulated sessions with deterministic startup output,
* remove a session from a layout without closing the SSH session,
* confirm before closing a still-running terminal session,
* close a terminal session,
* and preserve the host browser while sessions are active.

Expected later session affordances:

* open action in a new action/session tab,
* duplicate or reconnect a session when feasible,
* show connected/running/exited/failed state,
* rename session title locally,
* persist layout definitions outside the portable vault when appropriate.

The old tmux behavior remains useful in the TUI but should not be required by
the GUI.

## 4.3 VNC And Visual Actions

VNC support should grow naturally from the existing action system rather than
becoming a separate remote desktop product.

Today, VNC workflows can be represented as actions:

* optionally run a remote command such as `x11vnc`,
* optionally establish an SSH local forward,
* allocate a local port,
* and launch a locally mapped VNC viewer capability.

The GUI should preserve external local launch support because some users will
prefer their established VNC viewer.

In addition, the GUI should support optional embedded VNC when practical:

* an action can resolve to a VNC target through direct host/port or allocated
  forwarded port,
* the user can choose external viewer or embedded viewer,
* embedded VNC opens as a tab or pane associated with the action,
* the SSH/action process remains visible and controllable,
* failure of embedded VNC does not leave SSH forwards or remote commands running
  unintentionally,
* and cleanup runs when the action ends or the user closes it.

Embedded VNC should be treated as an action surface, not the center of the
application. Other future action surfaces may include forwarded web UIs or local
tools.

---

# 5. Design And Interaction

The GUI should be visually polished without sacrificing density.

Current layout:

* left inventory sidebar with folders, hosts, search, and selection state,
* main workspace with terminal and terminal-layout tabs when sessions exist,
* right inspector/editor panel for the selected host, selected folder, active
  terminal host, or active layout,
* status area for vault path, diagnostics count, selected-host count, and
  session feedback.

Recommended future additions:

* action runs and diagnostics as richer first-class surfaces,
* optional VNC action surfaces,
* compact command palette for high-frequency actions,
* richer diagnostics and action surfaces that are easier to scan and edit.

Design goals:

* use calm contrast, excellent spacing, and clear hierarchy,
* keep common workflows one or two interactions away,
* support keyboard operation throughout,
* use icons for common commands where they improve scanning,
* avoid nested cards and decorative dashboards,
* avoid giant hero or welcome screens,
* avoid excessive empty space,
* keep text readable and non-overlapping on small screens,
* preserve terminal focus correctly,
* and show exact operational consequences before destructive actions.

Useful desktop workflows:

* double-click host to connect,
* context-click host or folder for actions,
* drag hosts to folders for move,
* drag folders only when the destination is valid,
* command palette for connect, edit, actions, diagnostics, reload, and reveal,
* quick filter with keyboard-first result activation,
* split detail/editor tabs so browsing is not blocked by modal-heavy UI,
* explicit confirmation for delete and closing active sessions,
* and inline validation for forms.

The GUI should not merely expose key bindings from the TUI. It should provide
discoverable controls, menus, and contextual actions while retaining efficient
keyboard paths for expert users.

---

# 6. Resource Awareness And TUI Fallback

The GUI is for machines that can comfortably run it. If the machine cannot, the
project should make the TUI path easy and respectable.

At startup or first run, the GUI should consider warning or recommending
`stassh-tui` when it detects:

* very small screen resolution,
* insufficient memory for WebView plus terminal scrollback,
* missing or unreliable WebView runtime,
* broken GPU acceleration or rendering,
* high idle CPU from the GUI shell,
* no usable graphical session,
* or terminal/VNC rendering that fails basic health checks.

This should not be a hard gate unless the GUI truly cannot run. Users may still
want to force the GUI on unusual systems.

Useful modes:

* normal GUI mode,
* low-resource GUI mode with reduced animation, smaller scrollback, and simpler
  effects,
* and explicit handoff guidance to `stassh-tui`.

The TUI should remain feature-complete for terminal-only and low-resource use.

---

# 7. Architecture Direction

Tauri is the recommended default direction for `stassh-gui`, because it lets the
project keep Rust in control of core logic and process management while using a
mature web UI stack for terminal and VNC surfaces.

This is a recommendation, not a permanent constraint. A different stack is
acceptable if it proves simpler, faster, more accessible, or more maintainable.

Recommended process boundaries:

```text
GUI frontend
  - tree, search, forms, command palette, layout
  - terminal renderer
  - optional VNC renderer
  - thin session/action controls

Tauri/Rust backend
  - stassh-core calls
  - vault/local/secrets loading and saving
  - permission checks
  - PTY/session manager
  - OpenSSH child processes
  - action lifecycle
  - temporary OpenSSH configs
  - local launch and cleanup processes
  - resource probes

stassh-core
  - durable models and shared behavior
```

Frontend state should separate low-volume application data from high-volume
terminal/VNC streams.

Suggested channels:

* request/response commands for vault reads, edits, diagnostics, and action
  planning,
* event streams for session lifecycle changes,
* byte streams for terminal PTY I/O,
* binary/frame stream for embedded VNC if implemented,
* and redacted structured logs for debugging.

---

# 8. Testing And Acceptance

The first useful GUI release can perform many common TUI workflows plus embedded
SSH sessions. Acceptance coverage should protect both shared vault behavior and
GUI-specific terminal/layout behavior.

Core scenarios:

* open the same default vault/local/secrets paths as CLI and TUI,
* browse folders and hosts,
* search by path, hostname, username, tags, and notes,
* inspect host details and diagnostics,
* create, edit, copy, delete, and move hosts,
* create, rename, move, and delete eligible folders,
* assign and clear host identity fingerprints,
* edit jump chains,
* edit local, remote, and dynamic forwards,
* connect to a simple host in an embedded terminal,
* connect through a jump chain,
* connect with a generated temporary OpenSSH config,
* handle password, key, and host-key prompts inside the terminal,
* run a common action and a host-specific action,
* run an action with automatic local port allocation,
* run an action with local prepare environment,
* launch an external VNC viewer through a capability mapping,
* open embedded VNC when supported,
* reveal and hide an encrypted secret,
* reload after external vault changes,
* and close active sessions without orphaning child processes.

Already-implemented terminal scenarios should also remain covered:

* open multiple embedded terminal tabs,
* open simulated terminal tabs with the initial message and prompt visible
  without user input,
* create and close layout tabs without closing sessions,
* add terminal sessions to layouts by drag/drop,
* create layouts by dragging one terminal tab onto another,
* switch between equal-grid and main-pane layout modes,
* broadcast input to all panes in a layout,
* search focused terminal scrollback,
* confirm before closing a still-running terminal session,
* full-screen and exit the focused terminal pane,
* and preserve terminal contents across tab reordering and layout changes.

Failure scenarios:

* OpenSSH missing,
* missing local identity mapping,
* identity path no longer exists,
* missing capability mapping,
* invalid forward fields,
* invalid jump references,
* unsafe `~/.ssh/stassh` permissions,
* secrets store missing,
* wrong secrets password,
* local prepare failure,
* SSH process exits non-zero,
* VNC embedding fails,
* and GUI resource probe recommends the TUI.

Implementation should include unit tests around session/action planning where
possible, integration tests for backend commands that do not require real remote
hosts, and manual regression scripts for PTY behavior, prompts, resize handling,
and process cleanup.
Simulation mode should be the default data/session source for screenshot
captures and later visual regression automation because it avoids private
infrastructure and real SSH availability.

---

# 9. Development Defaults

Defaults for the current implementation:

* build `apps/stassh-gui` as a new workspace member,
* use `stassh-core` directly rather than shelling out to `stassh` for ordinary
  state operations,
* use system OpenSSH for connections,
* embed SSH sessions through a GUI-managed PTY,
* use `stassh-core` simulation fixtures and scripted shells for GUI simulation
  mode,
* keep terminal tabs and layout tabs as frontend runtime state,
* treat embedded VNC as optional but architecturally anticipated,
* do not add GUI-only vault schema fields,
* keep generated OpenSSH configs temporary and cleaned up,
* keep secrets out of persistent frontend state,
* and keep `stassh-tui` as the fallback interface for constrained environments.

The right GUI will make capable machines pleasant to use without making modest
machines second-class citizens.
