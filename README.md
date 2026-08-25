![Made with Codex](https://img.shields.io/badge/made%20with-Codex-111111)

# stassh-rust

`stassh-rust` is an early Rust implementation of a portable, offline-first SSH workspace.

The current codebase provides a useful MVP: a `stassh` CLI, a `stassh-tui` terminal UI, and a reusable `stassh-core` crate. It stores host inventory in a local JSON vault and launches the system OpenSSH client.

![stassh-tui browsing a demo SSH vault with folders, hosts, jumps, identity mappings, and forwards](examples/github-screenshot/stassh-tui-screenshot.jpg)

`stassh-tui` gives the vault a fast terminal interface for browsing folders,
searching hosts, inspecting jump chains and forwards, and launching OpenSSH
connections without giving up plain-file portability.

The longer-term project direction is documented in `docs/BLUEPRINT.md` and `docs/plan/`.

The CLI and TUI exist today. `stassh-gui` is future development, as are reusable actions for both `stassh-tui` and `stassh-gui`.

## Current Status

Implemented now:

- Cargo workspace with `stassh-core`, `stassh`, and `stassh-tui`
- plain, unencrypted `vault.json` storage with `format_version`
- duplicate host detection for vault hygiene
- read-only vault health checks
- folders and hosts with stable UUIDs
- host search and resolution by UUID, exact path/name, or search query
- host add/edit/delete
- folder list/add/rename/move/delete
- jump host chains
- local, remote, and dynamic forwards
- OpenSSH command and config generation
- `stassh connect` using the system `ssh`
- temporary OpenSSH config execution for jumps, forwards, and SSH options
- import of a useful subset of existing OpenSSH config files, including nested `Include` files
- export to OpenSSH config format
- machine-local identity fingerprint to key-path mappings
- fingerprint derivation from provided private key paths with `ssh-keygen -lf`
- basic diagnostics
- optional structured JSON output for CLI commands
- `stassh-tui` for browsing, searching, inspecting, connecting, and basic vault editing
- optional `stassh-tui` tmux/byobu window launch for multiple simultaneous SSH sessions

Not implemented yet:

- encrypted vaults
- synchronization journals
- automatic identity discovery by scanning `~/.ssh` or `ssh-agent`
- `stassh-gui`
- reusable actions/VNC workflows for `stassh-tui` and `stassh-gui`

## Build And Test

Requirements:

- Rust/Cargo
- OpenSSH client available as `ssh` for actual connections
- OpenSSH `ssh-keygen` available for identity fingerprint derivation and OpenSSH config import with `IdentityFile`

Build:

```bash
cargo build --workspace
```

Run tests:

```bash
cargo test --workspace
```

Run the CLI from source:

```bash
cargo run -p stassh -- --help
cargo run -p stassh -- --version
```

Run the TUI from source:

```bash
cargo run -p stassh-tui -- --version
cargo run -p stassh-tui
```

## Output Formats

By default, commands print human-readable text.

Use the global output flag for structured JSON:

```bash
stassh --output json vault status
stassh --output json list
stassh --output json diagnose web
```

JSON mode emits one JSON document per command. It is intended for scripts, tests, and future UI integrations.

For `export openssh -`, text mode writes raw OpenSSH config to stdout. JSON mode wraps the exported config in a JSON object:

```bash
stassh export openssh -
stassh --output json export openssh -
```

## Configuration Locations

`stassh` uses two configuration files:

- `vault.json`: portable host, folder, jump, forwarding, tag, note, and identity fingerprint records
- local config: machine-local identity fingerprint to private-key path mappings

The local config does not contain private key material, but it can reveal local
usernames and filesystem paths, so it should still be treated as private.

By default, new setups use:

```text
~/.ssh/stassh/vault.json
~/.ssh/stassh/local.json
```

This makes syncing a personal SSH workspace between machines as simple as copying
`~/.ssh/stassh/`. Existing project-local vaults remain supported.

On Unix-like systems, `stassh` and `stassh-tui` require safe permissions for the
default home configuration directory:

```text
~/.ssh/stassh/            700
~/.ssh/stassh/vault.json  600
~/.ssh/stassh/local.json  600
```

These checks apply only to paths under `~/.ssh/stassh/`. Project-local and
portable vaults outside that directory are not permission-gated. New files written
by `stassh` are saved with `600` permissions on Unix-like systems.

Vault path resolution order:

1. `--vault /path/to/vault.json`
2. `STASSH_VAULT=/path/to/vault.json`
3. `~/.ssh/stassh/vault.json` when it exists
4. `./vault.json` when it exists
5. `~/.ssh/stassh/vault.json` for a new default path

Local config path resolution order:

1. `--local-config /path/to/local.json`
2. `STASSH_LOCAL_CONFIG=/path/to/local.json`
3. `~/.ssh/stassh/local.json` when the selected vault is `~/.ssh/stassh/vault.json`
4. `.stassh-local.json` beside an explicit non-home vault for portable/project-local use

Choose explicit paths with:

```bash
cargo run -p stassh -- --vault /path/to/vault.json --local-config /path/to/local.json vault status
```

Or set environment variables:

```bash
export STASSH_VAULT=/path/to/vault.json
export STASSH_LOCAL_CONFIG=/path/to/local.json
```

Local `vault.json` files are ignored by Git because vaults may contain infrastructure details.

Older project-local machine mappings are still read from:

```text
.stassh-local.json
```

That file is also ignored by Git.

## Terminal UI

`stassh-tui` is a terminal interface over the same vault used by the CLI. It is
currently focused on fast browsing, searching, inspection, connection launching,
and vault editing for folders, hosts, identities, jump chains, and forwards. The
TUI can select or clear a host's identity fingerprint from the local identity
mappings. Creating, renaming, editing, and removing local identity mappings still
happens through the CLI, as does editing raw SSH options.

Launch it with the same configuration selection behavior as `stassh`:

```bash
stassh-tui --vault /path/to/vault.json --local-config /path/to/local.json
```

Or from source:

```bash
cargo run -p stassh-tui -- --vault /path/to/vault.json --local-config /path/to/local.json
```

If explicit flags are omitted, `stassh-tui` uses the same environment variables
and defaults documented above. Its status line shows both the resolved vault path
and the resolved local config path.

Current keys:

- `j` / `Down`: move selection down
- `k` / `Up`: move selection up
- `/`: enter host search
- `Esc`: leave search mode, or clear a status message in browse mode
- `Backspace`: delete a search character
- `Home`: in browse mode, move to the first visible sibling; in search mode, move to the first result
- `End`: in browse mode, move to the last visible sibling; in search mode, move to the last result
- `PageUp`: in browse mode, move to the parent folder
- `PageDown`: in browse mode, move to the last visible sibling
- `Space`: toggle selected hosts
- `u`: clear selected hosts
- `m`: move selected hosts, or the highlighted host if none are selected
- `n`: create a new host
- `C`: copy the selected host with a default `<name> copy` display name
- `f`: create a new folder
- `e`: edit the selected host or folder
- `i`: select or clear the selected host's identity fingerprint
- `J`: edit the selected host's jump chain
- `F`: edit the selected host's port forwards
- `x` / `Delete`: delete the selected host or empty folder after confirmation
- `Enter`: connect to the selected host, or expand/collapse the selected folder
- `t`: open the selected host in a new tmux window when running inside tmux/byobu
- `d`: toggle connection diagnostics in the detail panel
- `F1`: cycle through wrapped status/help lines
- `r`: reload the vault and local identity mappings from disk
- `q`: quit

While typing a search query, printable letters are added to the query instead of
running browse-mode commands such as `n`, `m`, or `u`.

Host selection works in both browse and search modes. On a host row, `Space`
toggles that host. On a folder row, `Space` toggles all descendant hosts under
that folder. Folder rows show `[x]` when all descendant hosts are selected and
`[-]` when only some are selected. Unselected folder rows show `[v]` when
expanded and `[>]` when collapsed. At startup, the root folder is expanded and
all non-root folders are collapsed. Selections persist between browse and search
and are cleared after moving hosts. This selection model is intended to support
future selected-host export workflows.

Mouse selection is supported in the left list panel when the terminal sends mouse
events. A single left-click selects a visible host, folder, search result, or
move-folder target. A double-click connects to a host, expands/collapses a folder,
or confirms a move-folder target.

In move-folder picker mode:

- `j` / `Down`: move destination folder selection down
- `k` / `Up`: move destination folder selection up
- `Home`: move to the first folder
- `End`: move to the last folder
- `Enter`: move the active host set to the selected folder
- `Esc`: cancel without writing

The move picker shows all folders expanded, regardless of the current collapsed
state in the browse tree. Moving hosts reloads the vault from disk, applies all
host folder changes, saves once, and refreshes the tree/details view. Moving a
host to its current folder is allowed and treated as a no-op for that host.

In host create/edit mode:

- `Tab` / `Down`: move to the next field
- `Shift+Tab` / `Up`: move to the previous field
- `Backspace`: delete a character from the current field
- `Ctrl+S`: save
- `Esc`: cancel without writing

The first editor supports host name, hostname, port, username, tags, and notes.
New hosts are created in the selected folder, or in the selected host's folder.
An empty username clears the host-specific username, an empty notes field clears
notes, and tags are entered as comma-separated values. Empty port means `22`.
When saving, the TUI reloads the vault from disk, applies the change by stable host
ID for edits, saves, and refreshes the tree/details view.

In identity selection mode:

- `j` / `Down` / `Tab`: move to the next identity choice
- `k` / `Up` / `Shift+Tab`: move to the previous identity choice
- `Home`: select `(none)` for password/default SSH authentication
- `End`: select the last identity choice
- `Ctrl+S`: save
- `Esc`: cancel without writing

The first identity choice is always `(none)`, which clears the host's
`identity_fingerprint`. The remaining choices come from the machine-local
identity mappings in `local.json`. If a host already references an unmapped
fingerprint, the TUI preserves that current fingerprint as an extra selectable
choice so opening and saving the editor does not accidentally clear it.

In jump editor mode:

- `j` / `Down` / `Tab`: move to the next host choice
- `k` / `Up` / `Shift+Tab`: move to the previous host choice
- `Space`: toggle the highlighted host as a jump target
- `[` / `]`: move a selected jump target earlier or later in the chain
- `Home`: move to the first host choice
- `End`: move to the last host choice
- `Ctrl+S`: save
- `Esc`: cancel without writing

The edited host is never shown as a jump target. Existing jumps are shown first,
in chain order, followed by the remaining hosts sorted by path. Saving replaces
the host's jump chain with the checked choices in their displayed order.

In forward editor mode:

- `a`: add a local forward
- `A`: add a remote forward
- `d`: add a dynamic SOCKS forward
- `x` / `Delete`: remove the highlighted forward
- `Up` / `Down`: move between forward rows
- `Tab` / `Shift+Tab`: move between fields in the highlighted row
- `Home`: clear the current field
- `End`: move to the last forward row
- `Backspace`: delete a character from the current field
- `Ctrl+S`: save
- `Esc`: cancel without writing

Local and remote forwards use bind address, listening port, destination host, and
destination port fields. Dynamic forwards use bind address and local port fields.
New forwards start with placeholder ports that must be replaced before saving.

In folder create/edit mode:

- `Tab` / `Down`: move to the next field
- `Shift+Tab` / `Up`: move to the previous field
- `Backspace`: delete a character from the current field
- `Ctrl+S`: save
- `Esc`: cancel without writing

Folder editing supports the folder name and parent folder UUID. Folder details
show both the folder ID and parent folder ID so a folder can be moved by changing
the parent field. New folders are created in the selected folder, or in the
selected host's folder. The root folder cannot be renamed, moved, or deleted from
the TUI.

In delete confirmation mode:

- `y` / `Enter`: delete the selected host or folder
- `n` / `Esc`: cancel without writing

Deleting a host removes it from the vault and also removes it from other hosts'
jump chains. Deleting a folder requires the folder to be empty. The TUI reloads
the vault from disk before applying the deletion, saves the updated vault, and
refreshes the tree/details view.

`Enter` preserves the simple OpenSSH-first behavior: the TUI leaves the alternate
screen, runs the system `ssh` attached directly to the current terminal, then
restores the TUI after SSH exits. Password prompts, key prompts, host-key checks,
terminal capabilities, and interactive SSH behavior are handled by OpenSSH and the
user's terminal.

When `stassh-tui` is running inside tmux or byobu, `t` opens the selected host in a
new tmux window using the same resolved OpenSSH configuration. This is the current
multi-session workflow. The TUI does not embed terminal tabs or manage PTYs itself.
If `t` is pressed outside tmux/byobu, the TUI shows a status message and leaves the
current session unchanged.

The status line shows `tmux:on` when tmux/byobu window launching is available and
`tmux:off` otherwise.

For connections that require a generated OpenSSH config, such as jump hosts,
forwards, SSH options, or mapped identities, tmux-launched sessions use temporary
config files under the system temp directory. `stassh-tui` cleans stale generated
config files on startup.

When testing a freshly changed TUI from source, rebuild the binary with:

```bash
cargo build -p stassh-tui
```

A useful manual regression check is to connect to an unavailable host outside tmux
and press `Ctrl+C` while SSH is blocked. The expected behavior is that SSH exits
and `stassh-tui` restores the TUI instead of returning to the shell prompt.

## Duplicate Host Reports

Use `vault check` for a read-only health report:

```bash
stassh vault check
stassh --output json vault check
```

The check report includes vault validation, local config validation, duplicate host groups, dedupe plan summary, hosts with missing local identity mappings, mapped identity files whose paths no longer exist, and raw imported `IdentityFile` options that still need review.

Use `vault duplicates` to find duplicate host entries in the selected vault:

```bash
stassh vault duplicates
stassh --output json vault duplicates
```

The report groups duplicates by:

- `path`: multiple hosts resolve to the same vault path, such as two root-level hosts named `web`
- `connection`: multiple hosts have the same effective connection settings: hostname, port, username, identity fingerprint, jump chain, raw SSH options, and forwards

The command only reports duplicates. It does not modify `vault.json`.

Use `vault dedupe` to plan removal of duplicate path entries:

```bash
stassh vault dedupe
stassh --output json vault dedupe
```

This command is a dry run by default. It shows which host will be kept for each duplicate path and which later hosts would be removed.

Apply the plan explicitly:

```bash
stassh vault dedupe --apply
```

Apply mode removes only duplicate `path` entries. It does not remove `connection` duplicates, because those may be intentional aliases. If any jump chains reference a removed duplicate host ID, they are rewritten to the kept host ID before saving.

## Quick Start

Create a test vault:

```bash
cargo run -p stassh -- --vault /tmp/stassh-vault.json vault init
```

Add a host:

```bash
cargo run -p stassh -- --vault /tmp/stassh-vault.json host add myserver example.com --user alice
```

Inspect what will be resolved:

```bash
cargo run -p stassh -- --vault /tmp/stassh-vault.json diagnose myserver
```

Connect:

```bash
cargo run -p stassh -- --vault /tmp/stassh-vault.json connect myserver
```

Password prompts, key prompts, agents, host-key checks, and other interactive SSH behavior are handled by OpenSSH.

## Common Commands

Vault:

```bash
stassh vault init
stassh vault status
stassh vault check
stassh vault duplicates
stassh vault dedupe
```

Folders:

```bash
stassh folder list
stassh folder add Customers
stassh folder rename <folder-id> Clients
stassh folder move <folder-id> --parent <parent-folder-id>
stassh folder delete <folder-id>
```

Hosts:

```bash
stassh host add web web.example.com --user deploy
stassh host edit web --name web-01 --port 2222
stassh host delete web-01
```

Browse and inspect:

```bash
stassh list
stassh search "web production"
stassh show web
stassh diagnose web
```

Connect:

```bash
stassh connect web
```

Identities:

```bash
stassh identity list
stassh identity add ~/.ssh/customer-key --name customer-key
stassh identity map SHA256:example ~/.ssh/customer-key
stassh identity diagnose SHA256:example
stassh identity unmap SHA256:example
```

Import:

```bash
stassh import openssh ~/.ssh/config
```

Export:

```bash
stassh export openssh ./stassh-ssh-config
stassh export openssh -
```

The `-` export target writes to stdout in text mode.

## Jump Hosts

Create a bastion and a target that jumps through it:

```bash
stassh host add bastion bastion.example.com --user admin
stassh host add db 10.0.0.5 --user root --jump bastion
stassh diagnose db
stassh connect db
```

Repeated `--jump` flags create an ordered jump chain:

```bash
stassh host add internal-db 10.0.0.10 --jump public-bastion --jump internal-gateway
```

Clear or replace jumps:

```bash
stassh host edit internal-db --clear-jumps
stassh host edit internal-db --jump public-bastion
```

## Port Forwarding

Local forward:

```bash
stassh host add web-admin web.example.com --local-forward 127.0.0.1:8080:127.0.0.1:80
```

Remote forward:

```bash
stassh host add callback host.example.com --remote-forward 127.0.0.1:9000:127.0.0.1:9000
```

Dynamic SOCKS forward:

```bash
stassh host add proxy proxy.example.com --dynamic-forward 127.0.0.1:1080
```

When forwards, jumps, or raw SSH options are present, `stassh connect` writes a temporary OpenSSH config file and runs:

```text
ssh -F <temporary-config> <generated-alias>
```

The temporary config is removed after the `ssh` process exits.

## SSH Options

Raw OpenSSH config lines can be attached to a host:

```bash
stassh host add slow-link host.example.com --ssh-option ServerAliveInterval=30
stassh diagnose slow-link
```

This is intentionally direct. The application should expose what OpenSSH will do instead of hiding it.

## Identity Fingerprints

Hosts store only an optional identity fingerprint:

```bash
stassh host add server example.com \
  --identity-fingerprint SHA256:example
```

For ordinary private key files, the easier path is to let `stassh` derive the fingerprint with `ssh-keygen -lf <key-file>`:

```bash
stassh identity add ~/.ssh/customer-key --name customer-key
```

That stores a machine-local mapping from the derived fingerprint to the private key path.

You can also attach a key file directly while creating or editing a host:

```bash
stassh host add server example.com \
  --user alice \
  --identity-file ~/.ssh/customer-key \
  --identity-name customer-key
```

```bash
stassh host edit server \
  --identity-file ~/.ssh/customer-key \
  --identity-name customer-key
```

These commands derive the key fingerprint, set the host's portable identity fingerprint, and store the machine-local path mapping. Fingerprinting reads key metadata and should not require the private-key passphrase.

Manual mapping is still available:

```bash
stassh identity map SHA256:example ~/.ssh/customer-key --name customer-key
```

Inspect mappings:

```bash
stassh identity list
stassh identity diagnose SHA256:example
```

Remove a mapping:

```bash
stassh identity unmap SHA256:example
```

When a host has an identity fingerprint and the current machine has a matching local mapping, generated OpenSSH config includes:

```sshconfig
IdentityFile /path/to/key
IdentitiesOnly yes
```

The fingerprint remains in the portable vault. The preferred name and key path
stay machine-local in the resolved local config file, usually
`~/.ssh/stassh/local.json` for the default home setup or `.stassh-local.json`
beside an explicit portable/project vault.

Current limitation: automatic identity discovery is not implemented yet. `stassh` can derive a fingerprint from a key path you provide, but it does not yet scan `~/.ssh` or `ssh-agent`.

## Import OpenSSH Config

Import a useful subset of an existing OpenSSH config:

```bash
stassh import openssh ~/.ssh/config
```

The vault must already exist:

```bash
stassh vault init
stassh import openssh ~/.ssh/config
```

Currently imported:

- top-level and nested `Include` files, including simple `*` and `?` globs
- concrete `Host` aliases, with matching `Host *` defaults applied
- `HostName`
- `User`
- `Port`
- `ProxyJump` when the target alias can be resolved
- `IdentityFile`, deriving a fingerprint with `ssh-keygen -lf <key-file>` and writing the resolved local config when the local key path can be resolved
- simple `LocalForward`, `RemoteForward`, and `DynamicForward` forms

`Include` paths are resolved relative to the file that declares them, with support for `~`, absolute paths, relative paths, and simple glob components. Include matches are imported in sorted order for deterministic results. Include cycles are detected and skipped with a warning.

`Host *` blocks are not imported as hosts. Instead, their options are applied to concrete hosts using OpenSSH-style ordered matching: the first scalar value wins, while list-like values such as `IdentityFile` and forwards are accumulated. This means a `Host *` block at the end of a file fills in missing defaults, while a `Host *` block near the top can intentionally set values before later concrete blocks are read.

Other wildcard or negated host patterns such as `Host prod-*` or `Host !prod-*` are skipped as standalone imports. Unsupported per-host options are preserved as raw SSH config lines where practical.

If an imported `IdentityFile` uses unsupported OpenSSH tokens such as `%h`, points to a missing file, or cannot be fingerprinted by `ssh-keygen`, it is preserved as a raw `IdentityFile` option and the import summary prints a warning. During one import run, each resolved key path is fingerprinted at most once, even if many host blocks reference it.

The import command prints counts and details for imported hosts, skipped patterns, and warnings.

OpenSSH config is a rich language. This importer does not yet evaluate `Match`, wildcard precedence, token expansion, bracket glob forms, or every valid quoting form.

## Export OpenSSH Config

Export the current vault as an OpenSSH config:

```bash
stassh export openssh ./stassh-ssh-config
```

Use `-` to write the exported config to stdout:

```bash
stassh export openssh -
```

This is useful for inspection or shell pipelines:

```bash
stassh export openssh - | sed -n '1,80p'
```

Exported blocks include:

- `Host`
- `HostName`
- `Port` when non-default
- `User`
- `ProxyJump`
- `LocalForward`, `RemoteForward`, and `DynamicForward`
- raw SSH options stored on the host, including imported `IdentityFile` lines

Exported aliases use the host display name when it is safe and unique for OpenSSH. Duplicate or unsafe names fall back to `stassh-<uuid>`. Each block includes `stassh-id` and `stassh-path` comments for traceability.

Export does not currently include machine-local identity mappings from the local
config file. `stassh diagnose` and `stassh connect` use those mappings when
generating temporary config for one host.

Example import/export workflow:

```bash
stassh vault init
stassh import openssh ~/.ssh/config
stassh diagnose my-host
stassh export openssh ./generated-ssh-config
ssh -F ./generated-ssh-config my-host
```

The exported file is meant to be reviewable OpenSSH configuration. It is not a byte-for-byte round trip of the original file.

## Development Notes

The current storage format is an unreleased development format:

```json
{
  "format_version": 0,
  "folders": [],
  "hosts": []
}
```

It is intentionally plain JSON for early development. Do not store sensitive production inventory in it yet.

Before committing changes, run:

```bash
cargo fmt --all -- --check
cargo test --workspace
```
