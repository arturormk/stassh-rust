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

## Included Actions

The examples in `vault.json` define these common workflows:

- `VNC forwarded`: starts `x11vnc` over SSH, creates a temporary local SSH
  forward to remote `127.0.0.1:5900`, then launches a local viewer against the
  forwarded port.
- `VNC direct`: starts `x11vnc` over SSH, then launches a local viewer against
  the remote host's directly reachable VNC port.
- `Send file to home`: opens a lightweight local file picker and copies the
  selected file to the remote host's home directory with `scp`.

The examples in `local.json` show the matching machine-local capability
mapping. Adjust the path for your machine:

```bash
mkdir -p "$HOME/bin"
cp examples/actions/stassh-vnc-viewer-delay "$HOME/bin/"
cp examples/actions/stassh-send-file-scp "$HOME/bin/"
chmod 755 "$HOME/bin/stassh-vnc-viewer-delay"
chmod 755 "$HOME/bin/stassh-send-file-scp"
stassh capability map vnc-viewer-delay "$HOME/bin/stassh-vnc-viewer-delay"
stassh capability map send-file-scp "$HOME/bin/stassh-send-file-scp"
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
```

## Send File Example

`Send file to home` uses `local_prepare` to run the `send-file-scp` capability.
The helper script starts from the user's home directory by default, lets you
select a file with `fzf` or a fallback picker, then runs:

```text
scp -P <port> <selected-file> <user>@<host>:~/
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

This example is intentionally simple. It uses the resolved `{HOST}`, `{PORT}`,
and `{USER}` template values, but it does not automatically apply stassh
jump-chain or identity mappings to `scp`. For hosts that require jump hosts or
specific identity files, adapt the helper script or use an OpenSSH config entry
that `scp` can already resolve.

Dry-run shows the rendered helper command:

```bash
stassh action <host> "Send file to home" --dry-run
```

## Creating New Examples

Action authoring is intentionally JSON-first in v1.0. A good way to create a new
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
