# Windows Support Assessment

Research snapshot: 2026-09-14

## Decision summary

Windows support is a moderate port, not a new application. The existing Rust
core, JSON files, React interface, xterm.js terminal, Tauri shell, and
`portable-pty` dependency are all suitable foundations. The main work is in
removing a few Unix assumptions, validating the Windows OpenSSH and ConPTY
session path, and establishing a Windows release pipeline.

The recommended first target is native x86-64 Windows 10 version 1809 or newer,
with Windows 11 as the primary test environment. Build the GUI, CLI, and TUI
natively; do not require WSL. Use the Microsoft-provided OpenSSH client and the
system ConPTY implementation. Distribute a signed NSIS installer plus a
portable ZIP. Defer MSI, WinGet, ARM64, and bundling OpenSSH until the x86-64
release is stable.

A realistic order-of-magnitude estimate for one engineer familiar with the
codebase is 3–6 engineer-weeks for a signed x86-64 beta. This includes
portability fixes, GUI terminal validation, CI, packaging, and documentation,
but not the calendar time required to obtain a signing identity or build
SmartScreen reputation.

## What already transfers

The following pieces should work unchanged or with small platform adaptations:

- `stassh-core` domain models, validation, host resolution, encryption, import,
  export, action planning, and simulation data;
- the `vault.json`, `local.json`, and `secrets.json` schemas;
- React, xterm.js, Tauri IPC, host/folder editors, search, secrets inspection,
  and simulation mode;
- ordinary OpenSSH arguments and generated OpenSSH configuration;
- Crossterm and Ratatui for a native Windows console TUI;
- `portable-pty`, which has a Windows backend using ConPTY;
- Tauri's Windows WebView2 frontend and Windows installer support.

The repository is already partly prepared: Unix permission calls and Unix
process-group behavior are conditionally compiled, while non-Unix fallbacks
exist. The portable vault does not contain Unix-only paths. The remaining
issues are concentrated in path discovery, command rendering, terminal process
behavior, file replacement, and packaging.

## Supported product shape

The initial Windows release should contain all three user-facing programs:

| Program | Initial status | Notes |
| --- | --- | --- |
| `stassh-gui.exe` | First-class | Reuse the current GUI and embedded xterm.js terminal. |
| `stassh.exe` | First-class | Important for scripting, diagnostics, import, and export. |
| `stassh-tui.exe` | Supported | Run in Windows Terminal or another modern console. No tmux/byobu feature. |

Windows support means native Windows paths, OpenSSH, consoles, and installers.
WSL may remain a user-selected environment, but it must not be required and it
must not silently mix Windows and WSL key paths or configuration files.

The minimum should be Windows 10 version 1809. Microsoft documents OpenSSH as a
Feature on Demand beginning with that release, and ConPTY was introduced in
the same Windows generation. Supporting older systems would add compatibility
work while weakening the terminal experience. The project should primarily
test current Windows 11 and retain one Windows 10 22H2 smoke-test machine while
that remains practical.

## Execution options

| Option | Reuse and fidelity | Cost and risk | Recommendation |
| --- | --- | --- | --- |
| Tauri + system OpenSSH + ConPTY | Maximum reuse; behavior stays close to Linux | Requires OpenSSH detection and Windows terminal testing | **Use for v1** |
| Bundle OpenSSH with stassh | Predictable availability | Adds security updates, packaging size, provenance, and binary maintenance | Do not use initially |
| Require WSL | Reuses the Linux build | Poor native integration; path and configuration duplication | Reject as the Windows architecture |
| Replace OpenSSH with a Rust SSH engine | Could later share more code with Android | Reimplements authentication, host-key policy, forwarding, and option semantics | Reconsider only after Android proves the engine |

Windows OpenSSH is not guaranteed to be installed. Startup diagnostics should
search the configured machine-local override, `PATH`, and the standard
`%SystemRoot%\System32\OpenSSH` location. If `ssh.exe` or `ssh-keygen.exe` is
missing, the applications should remain usable for browsing and editing and
show exact installation guidance instead of failing at workspace load.

Do not copy OpenSSH into the installer merely to avoid this check. Depending on
the operating-system feature leaves OpenSSH security servicing with Microsoft
and keeps the stassh package smaller. A bundled fallback can be reconsidered
only if real-world installation failure is common enough to justify owning its
update lifecycle.

## Required engineering work

### Paths and configuration

The current default directory is derived only from `HOME`. Native Windows
processes commonly expose `USERPROFILE` instead. Introduce one shared,
platform-aware home-directory resolver and retain the familiar Windows default:

```text
%USERPROFILE%\.ssh\stassh\vault.json
%USERPROFILE%\.ssh\stassh\local.json
%USERPROFILE%\.ssh\stassh\secrets.json
```

Explicit CLI options and the existing `STASSH_*` environment variables must
keep precedence. Portable/project-local vault behavior must remain unchanged.
Unix mode checks remain Unix-only; Windows should validate that expected files
are regular files but should not pretend POSIX modes provide protection.

Keep the JSON schemas compatible. The portable vault stays byte-for-byte
portable. `local.json` is deliberately machine-local, so its Windows paths will
differ from Linux paths even though the format is identical. Reserve normal
capability mappings for OpenSSH tool overrides (for example `openssh.ssh` and
`openssh.ssh-keygen`) instead of adding Windows paths to `vault.json`.

Generated OpenSSH configuration must quote or escape `IdentityFile` and other
generated path values correctly when they contain spaces, non-ASCII text, or a
drive letter. Command previews currently use POSIX shell quoting. Separate the
structured program/argument representation from display rendering and provide
a Windows-aware preview; execution must always pass an argument vector without
round-tripping through a shell.

### Storage correctness

Exercise all save paths on NTFS rather than assuming Unix rename semantics.
Tests must cover replacing an existing vault, antivirus/indexer interference,
an open destination file, cleanup after a failed replacement, and recovery of
the previous valid file. `vault.json`, `local.json`, and `secrets.json` should
all use the same reviewed safe-write primitive where possible.

External-change detection should be tested on Windows' timestamp behavior. If
length plus modification time proves too weak, add a content digest rather
than introducing a platform-specific vault format. Also test CRLF input,
although JSON output may continue using LF for stable cross-platform diffs.

Required path cases are:

- spaces and non-ASCII user names;
- drive-letter paths and mixed slash input;
- long paths within the supported Windows configuration;
- removable drives;
- UNC paths for portable vaults, with clear errors when a provider does not
  offer reliable replacement semantics;
- case-insensitive file lookup without making host or folder names
  case-insensitive in the data model.

### CLI and TUI processes

The non-Unix CLI fallback currently kills only the immediate child. Windows
local actions can start descendant processes, so production cleanup should use
a Windows Job Object or an equivalent reviewed process-tree mechanism. This
matters for action cancellation, GUI session closure, and application exit.

The TUI should suspend its alternate screen and start `ssh.exe` with inherited
console handles. Windows does not have the Unix foreground process-group
handoff used by the Linux implementation; verify Ctrl-C, Ctrl-Break, resize,
and restoration after exit in Windows Terminal. The tmux/byobu command should
be hidden or reported as unavailable, not emulated.

### GUI terminal

Continue using the existing flow:

```text
xterm.js <-> narrow Tauri stream <-> portable-pty/ConPTY <-> ssh.exe
```

Validate rather than rewrite it. Required terminal tests include interactive
password and key-passphrase prompts, full-screen programs, resize, Unicode,
bracketed paste, Ctrl-C, clean EOF, forced close, several simultaneous
sessions, layouts, broadcast input, and action cleanup. ConPTY may emit extra
control sequences such as title changes; xterm.js should receive them without
allowing a remote process to rename the application window unexpectedly.

The present fixed 960-pixel minimum window is reasonable for desktop Windows.
Test 100%, 125%, 150%, and 200% display scaling and high-contrast mode before
calling the GUI supported.

### Actions and feature diagnostics

The action model is already more portable than a shell-script model because it
stores a program, arguments, environment, and optional capability. However,
the example helper scripts and many likely local programs are Unix-specific.

Use capability mappings for platform-specific executables. Before running an
action, diagnose missing capabilities, programs that are not Windows
executables, and templates whose rendered SSH command assumes POSIX shell
quoting. Remote-only commands and ordinary forwards should work normally.
Never reinterpret a `.sh` action through PowerShell or WSL automatically.

## Packaging and project overhead

Tauri supports both NSIS setup executables and MSI packages. Use a per-platform
Tauri configuration so the existing Linux-only `beforeBundleCommand`, DEB/RPM
targets, and sidecar paths are not evaluated on Windows.

Recommended first artifacts:

1. a signed per-user NSIS installer containing the GUI, CLI, and TUI;
2. a signed portable ZIP containing the same executables;
3. checksums attached to the existing draft GitHub Release flow.

NSIS is preferable for the first release because it avoids making MSI/WiX and
the Windows VBSCRIPT optional feature part of the critical path. Add MSI only
when enterprise deployment demand exists. Submit a WinGet manifest after the
installer URL, identity, and unattended upgrade behavior are stable.

Use the system Evergreen WebView2 runtime. Current Windows 10 and Windows 11
normally receive it through the operating system; the installer can bootstrap
it when missing. Do not embed the fixed runtime by default because Tauri notes
that it adds roughly 180 MB and transfers browser-patch responsibility to the
application release.

| Overhead | Initial | Recurring |
| --- | --- | --- |
| Windows CI runner and caches | Moderate | Low |
| Real Windows terminal/device testing | Moderate | Moderate |
| Code-signing identity and protected CI access | Moderate | Renewal and key custody |
| SmartScreen reputation | Calendar-dependent | Sign consistently with the same identity |
| NSIS configuration and portable ZIP | Moderate | Low |
| OpenSSH, ConPTY, and WebView2 compatibility | Moderate | Test on platform updates |
| MSI/WinGet, if added | Deferred | Separate metadata and release maintenance |

Code signing is not technically required to execute a direct-download build,
but an unsigned installer produces an unacceptable trust experience for a
general release. Signing does not instantly eliminate every SmartScreen
warning; stable identity and release reputation still matter.

## Delivery sequence and gates

### Phase 1: compile and core portability

- Add an x86-64 MSVC CI job for core, CLI, and TUI tests.
- Fix home/tool discovery, path emission, safe replacement, and command preview.
- Confirm that existing Linux behavior and file formats do not change.

Gate: core and CLI tests pass on Windows and the same vault opens and saves on
Linux and Windows without semantic changes.

### Phase 2: native frontend validation

- Exercise the TUI in Windows Terminal.
- Build the Tauri GUI and complete the ConPTY terminal matrix.
- Add Windows simulation smoke tests and focused backend integration tests.

Gate: direct SSH, key authentication, jumps, all forwarding types, terminal
resize/close, and compatible actions pass on a clean Windows VM.

### Phase 3: preview and signed beta

- Publish an explicitly unsupported unsigned preview for early testers.
- Establish signing-key custody and CI signing.
- Publish signed NSIS and ZIP artifacts with installation/OpenSSH guidance.

Gate: upgrade and uninstall preserve user configuration, signatures validate,
and a clean non-developer Windows machine passes the install/connect/update
smoke test.

## Final recommendation

Proceed with native Windows support before Android. It preserves the current
architecture, reaches useful parity quickly, and adds manageable ongoing
overhead. Treat x86-64 Windows 10 1809+ as the first compatibility contract,
use system OpenSSH and ConPTY, and ship signed NSIS plus ZIP artifacts. Do not
make WSL, bundled OpenSSH, MSI, WinGet, or ARM64 prerequisites for the first
release.

## References

- Microsoft, [OpenSSH for Windows overview](https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh-overview)
- Microsoft, [OpenSSH Server Configuration for Windows](https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh-server-configuration)
- Tauri, [Prerequisites](https://v2.tauri.app/start/prerequisites/)
- Tauri, [Windows Installer](https://v2.tauri.app/distribute/windows-installer/)
- Tauri, [Windows Code Signing](https://v2.tauri.app/distribute/sign/windows/)
- `portable-pty`, [upstream source and platform implementations](https://github.com/wezterm/wezterm/tree/main/pty)
