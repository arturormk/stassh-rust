# Action Examples

This directory contains JSON snippets and a local helper script for reusable
`stassh` actions.

These files are examples, not a complete ready-to-use workspace. Copy the
action objects from `vault.json` into the top-level `actions` array of your real
`vault.json`, then map the local helper script in your real `local.json` with
`stassh capability map`.

Read `HOWTO-Actions.md` from the repository root for the full action schema,
template variables, dry-run workflow, and debugging checklist.

## Files

- `vault.json`: portable action snippets. These should not contain
  machine-specific executable paths.
- `local.json`: example machine-local capability mappings. Treat this as a
  pattern, not as a file to copy unchanged.
- `stassh-vnc-viewer-delay`: local wrapper script used by the VNC examples.
- `stassh-send-file-scp`: local wrapper script used by the send-file example.
- `stassh-send-directory-rsync`: local wrapper script used by the directory
  sync example.

## Included Actions

The examples in `vault.json` define these common workflows:

- `VNC forwarded`: starts `x11vnc` over SSH, creates a temporary local SSH
  forward to remote `127.0.0.1:5900`, then launches a local viewer against the
  forwarded port.
- `VNC direct`: starts `x11vnc` over SSH, then launches a local viewer against
  the remote host's directly reachable VNC port.
- `Send file to home`: opens a lightweight local file picker and copies the
  selected file to the remote host's home directory with `scp`.
- `Sync directory to home`: opens a lightweight local directory picker and
  syncs the selected directory to the remote host's home directory with
  `rsync`.

The examples in `local.json` show the matching machine-local capability
mapping. Adjust the path for your machine:

```bash
mkdir -p "$HOME/bin"
cp examples/actions/stassh-vnc-viewer-delay "$HOME/bin/"
cp examples/actions/stassh-send-file-scp "$HOME/bin/"
cp examples/actions/stassh-send-directory-rsync "$HOME/bin/"
chmod 755 "$HOME/bin/stassh-vnc-viewer-delay"
chmod 755 "$HOME/bin/stassh-send-file-scp"
chmod 755 "$HOME/bin/stassh-send-directory-rsync"
stassh capability map vnc-viewer-delay "$HOME/bin/stassh-vnc-viewer-delay"
stassh capability map send-file-scp "$HOME/bin/stassh-send-file-scp"
stassh capability map send-directory-rsync "$HOME/bin/stassh-send-directory-rsync"
```

The helper script expects `xtightvncviewer`. For forwarded VNC targets it uses
`nc` when available to wait until the forwarded local port is open before
launching the viewer.

After copying one or more action objects into your vault, use dry-run mode
before running actions against a real host:

```bash
stassh action <host> "VNC forwarded" --dry-run
stassh action <host> "VNC direct" --dry-run
stassh action <host> "Send file to home" --dry-run
stassh action <host> "Sync directory to home" --dry-run
```

## Send File Example

`Send file to home` uses `local_prepare` to run the `send-file-scp` capability.
The helper script starts from the user's home directory by default, lets you
select a file with `fzf` or a fallback picker, then runs:

```text
scp -F <stassh-temp-config> <selected-file> <stassh-host-alias>:~/
```

If `fzf` is installed, the helper uses it over a precomputed file list rooted
at `$HOME` by default. This is usually faster and clearer than `dialog
--fselect`, but it filters a list rather than browsing directories
interactively. If `fzf` is not available, the helper tries `dialog --fselect`.
If neither is available, it falls back to a numbered menu printed in the
terminal.

Set `STASSH_SEND_FILE_START` to choose a different file-list root:

```bash
STASSH_SEND_FILE_START="$HOME/" stassh action <host> "Send file to home"
```

Hidden dotfiles and files inside dot directories are hidden from the `fzf` and
numbered-menu pickers by default. Include them with:

```bash
STASSH_SEND_FILE_HIDDEN=1 stassh action <host> "Send file to home"
```

You can force a picker with `STASSH_SEND_FILE_PICKER`:

```bash
STASSH_SEND_FILE_PICKER=fzf stassh action <host> "Send file to home"
STASSH_SEND_FILE_PICKER=dialog stassh action <host> "Send file to home"
STASSH_SEND_FILE_PICKER=menu stassh action <host> "Send file to home"
```

The action's `remote_command` is `true`. That means the transfer happens first
as a local preparation step, and stassh only runs a tiny SSH command afterward
if the transfer succeeded.

The helper receives `{SSH_CONFIG}` and `{SSH_DEST}` from stassh, so `scp` uses
the same resolved OpenSSH configuration as the SSH session, including port,
username, jump-chain, identity mapping, forwards, and raw SSH options.

Use the same pattern for `rsync` helpers:

```bash
rsync -e "ssh -F $ssh_config" -- "$source" "$ssh_dest:$remote_dir/"
```

Actions can also pass `{SSH_RSH}` directly when a helper wants the rendered
`ssh -F <stassh-temp-config>` value for `rsync -e`.

Dry-run shows the rendered helper command:

```bash
stassh action <host> "Send file to home" --dry-run
```

## Sync Directory Example

`Sync directory to home` uses `local_prepare` to run the
`send-directory-rsync` capability. The helper script starts from the user's
home directory by default, lets you select a directory with `fzf` or a fallback
picker, then runs:

```text
rsync -az -e <stassh-ssh-rsh> <selected-directory>/ <stassh-host-alias>:~/<selected-directory-name>/
```

The action passes `{SSH_RSH}` and `{SSH_DEST}` to the helper. `{SSH_RSH}`
expands to the remote shell command `rsync -e` expects, while `{SSH_DEST}`
expands to the generated host alias in the temporary stassh OpenSSH config.

Set `STASSH_SEND_DIRECTORY_START` to choose a different directory-list root:

```bash
STASSH_SEND_DIRECTORY_START="$HOME/projects" stassh action <host> "Sync directory to home"
```

Hidden dot directories are hidden from the `fzf` and numbered-menu pickers by
default. Include them with:

```bash
STASSH_SEND_DIRECTORY_HIDDEN=1 stassh action <host> "Sync directory to home"
```

You can force a picker with `STASSH_SEND_DIRECTORY_PICKER`:

```bash
STASSH_SEND_DIRECTORY_PICKER=fzf stassh action <host> "Sync directory to home"
STASSH_SEND_DIRECTORY_PICKER=dialog stassh action <host> "Sync directory to home"
STASSH_SEND_DIRECTORY_PICKER=menu stassh action <host> "Sync directory to home"
```

Dry-run shows the rendered helper command:

```bash
stassh action <host> "Sync directory to home" --dry-run
```

## Creating New Examples

Action authoring is intentionally JSON-first. A good way to create a new
action is to point an AI coding agent or ChatGPT at `HOWTO-Actions.md`, describe
the exact workflow you want, and ask it to produce:

- a portable `vault.json` action snippet,
- any required `local.json` capability mapping names,
- any local helper scripts,
- and a dry-run checklist.

Useful prompt shape:

```text
Read HOWTO-Actions.md and create a stassh action for this workflow:
<describe the remote command, required SSH forwards, local tool to launch,
ports, environment variables, and cleanup behavior>.

Keep machine-specific executable paths out of vault.json. Use local.json
capability mappings for local tools. Include any helper shell scripts needed.
```

Then review the generated JSON before using it. In particular, check that:

- `args` is an array of arguments, not a shell command string.
- every `{LOCAL_PORT:name}` matches an action forward name.
- every `capability` has a matching `stassh capability map ...` command.
- machine-specific paths stay in `local.json` or local helper scripts.
- the action passes `stassh action <host> "<name>" --dry-run`.
