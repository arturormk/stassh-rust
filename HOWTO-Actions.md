# HOWTO: Actions

Actions are reusable SSH workflows stored in `vault.json`. They are meant for
users who are comfortable with SSH, local scripts, and JSON. The CLI, TUI, and
GUI can run actions, but action authoring is currently JSON-first so that
complex workflows stay explicit and inspectable.

An action can:

- run a command over SSH
- add temporary SSH forwards for that run
- launch a local program or local capability
- run a local preparation command before resolving the final action
- run local cleanup commands after SSH exits

Use dry-run mode while developing actions:

```bash
stassh action web "Action name" --dry-run
stassh --output json action web "Action name" --dry-run
```

Dry-run output shows allocated automatic ports, the rendered SSH command, and
the rendered local commands without opening SSH or launching local tools.

In the GUI, select a host, open the inspector's Actions pane, then use Preview
to inspect the resolved dry-run plan or Run to open the action as a terminal
session. GUI action runs use the same vault and local capability resolution as
the CLI and TUI.

## Where Actions Live

Common actions live at the top level of `vault.json` and apply to every host:

```json
{
  "format_version": 0,
  "actions": [
    {
      "id": "11111111-1111-1111-1111-111111111111",
      "name": "Uptime",
      "remote_command": "uptime"
    }
  ],
  "folders": [],
  "hosts": []
}
```

Host-specific actions live inside a host object:

```json
{
  "id": "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
  "folder_id": "00000000-0000-0000-0000-000000000001",
  "display_name": "web",
  "hostname": "web.example.com",
  "port": 22,
  "username": "alice",
  "jump_chain": [],
  "ssh_options": [],
  "forwards": [],
  "actions": [
    {
      "id": "22222222-2222-2222-2222-222222222222",
      "name": "Restart app",
      "remote_command": "sudo systemctl restart example-app"
    }
  ],
  "tags": [],
  "notes": null
}
```

When you select a host, `stassh` resolves both sets. Common actions appear
first, followed by host-specific actions.

Machine-local executable paths belong in `local.json` as capability mappings.
This keeps portable workflow definitions out of machine-specific paths:

```json
{
  "format_version": 0,
  "identity_mappings": [],
  "capability_mappings": [
    {
      "name": "vnc-viewer-delay",
      "path": "/home/alice/bin/stassh-vnc-viewer-delay"
    }
  ]
}
```

## Action Schema

An action object supports these fields:

```json
{
  "id": "11111111-1111-1111-1111-111111111111",
  "name": "Example",
  "local_prepare": {
    "program": "/path/to/program",
    "args": [],
    "env": {}
  },
  "forwards": [],
  "remote_command": "echo hello",
  "local_launch": {
    "capability": "tool-name",
    "args": [],
    "env": {}
  },
  "cleanup": []
}
```

Fields:

- `id`: UUID for this action.
- `name`: human-readable action name.
- `local_prepare`: optional local command that runs before the final action is
  resolved. Its stdout can provide environment variables.
- `forwards`: optional list of temporary action forwards.
- `remote_command`: optional command passed to SSH as the remote command.
- `local_launch`: optional local command launched while SSH is running.
- `cleanup`: optional list of local commands run after SSH exits.

The simplest useful action only needs `id`, `name`, and `remote_command`:

```json
{
  "id": "33333333-3333-3333-3333-333333333333",
  "name": "Disk usage",
  "remote_command": "df -h"
}
```

This is the common case: select a host, run one command through that host's
normal SSH configuration, and return when the command exits.

## Holding The Remote Session Open

Short remote commands may exit before you have time to read their output,
especially when launched from `stassh-tui`, where the terminal UI returns after
SSH exits. For an interactive action, append a shell read command:

```json
{
  "id": "99999999-9999-9999-9999-999999999999",
  "name": "Disk usage pause",
  "remote_command": "df -h; read -n 1"
}
```

This runs `df -h`, then waits for one keypress before the remote shell exits.
It is a practical convenience for commands whose output you want to inspect
before returning to the TUI.

The exact `read` options depend on the remote shell. `read -n 1` works in
shells such as `bash`, but not in every POSIX `/bin/sh`. If the remote account
does not use a compatible shell, call one explicitly:

```json
{
  "id": "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
  "name": "Disk usage bash pause",
  "remote_command": "bash -lc 'df -h; read -n 1'"
}
```

## Local Commands

`local_prepare`, `local_launch`, and each `cleanup` entry use the same local
command shape:

```json
{
  "capability": "vnc-viewer-delay",
  "args": ["127.0.0.1::{LOCAL_PORT:vnc}"],
  "env": {
    "EXAMPLE": "{HOST}"
  }
}
```

Use exactly one of:

- `capability`: a name resolved through `local.json.capability_mappings`.
- `program`: an explicit local executable path.

`args` is an array of command arguments. It is not a shell string. Put each
argument in its own array item.

`env` is an object of environment variables to set for the local command.
Template variables are expanded in `program`, `args`, `env` values, and
`remote_command`.

## Template Variables

Actions can use these template variables:

- `{HOST}`: resolved target hostname.
- `{USER}`: resolved username, or an empty string when no username is set.
- `{LOCAL_PORT:name}`: local port allocated or resolved for the named action
  forward.
- `{ENV:NAME}`: value emitted by `local_prepare` as a `NAME=value` line.

Unknown variables cause action resolution to fail. This is useful because it
catches misspelled forward names and missing preparation output during dry-run.

## Action Forwards

Action forwards are temporary forwards added only for this action run. They are
separate from persistent host forwards.

Each action forward must have a non-empty `name`. The name is used by
`{LOCAL_PORT:name}`.

Local forward:

```json
{
  "type": "local",
  "name": "admin",
  "bind_address": "127.0.0.1",
  "local_port": "auto",
  "destination_host": "127.0.0.1",
  "destination_port": 8080
}
```

Dynamic SOCKS forward:

```json
{
  "type": "dynamic",
  "name": "socks",
  "bind_address": "127.0.0.1",
  "local_port": { "fixed": 1080 }
}
```

`local_port` can be:

- `"auto"`: ask the OS for an available local port.
- `{ "fixed": 1080 }`: use that fixed local port.
- `{ "env": "PORT" }`: read a port number from `local_prepare` output.

Use `"auto"` when possible. Fixed ports are useful for stable bookmarks or
external tools, but they can collide with existing local services.

Action forwards currently support local forwards and dynamic SOCKS forwards.
Remote forwards are available for persistent host forwards, but not for action
forwards.

## Example: Remote Web Admin Through SSH

This action opens a browser to a remote service that listens on
`127.0.0.1:8080` on the target host. The service does not need to be reachable
from your local network.

```json
{
  "id": "44444444-4444-4444-4444-444444444444",
  "name": "Web admin",
  "forwards": [
    {
      "type": "local",
      "name": "admin",
      "bind_address": "127.0.0.1",
      "local_port": "auto",
      "destination_host": "127.0.0.1",
      "destination_port": 8080
    }
  ],
  "local_launch": {
    "program": "/usr/bin/xdg-open",
    "args": ["http://127.0.0.1:{LOCAL_PORT:admin}/"]
  }
}
```

What happens:

1. `stassh` allocates an available local port.
2. SSH forwards that local port to `127.0.0.1:8080` on the remote host.
3. `xdg-open` opens the forwarded URL locally.
4. When SSH exits, the temporary forward goes away.

## Example: VNC With SSH Forwarding

Use this when the remote VNC port is not directly reachable, or when you want
all VNC traffic to travel through SSH.

```json
{
  "id": "55555555-5555-5555-5555-555555555555",
  "name": "VNC forwarded",
  "forwards": [
    {
      "type": "local",
      "name": "vnc",
      "bind_address": "127.0.0.1",
      "local_port": "auto",
      "destination_host": "127.0.0.1",
      "destination_port": 5900
    }
  ],
  "remote_command": "DISPLAY=:0 x11vnc -scale 1/2",
  "local_launch": {
    "capability": "vnc-viewer-delay",
    "args": ["127.0.0.1::{LOCAL_PORT:vnc}"]
  }
}
```

What it does:

1. Allocates an available local TCP port and names it `vnc`.
2. Starts an SSH local forward from that allocated local port to remote
   `127.0.0.1:5900`.
3. Runs `DISPLAY=:0 x11vnc -scale 1/2` as the SSH remote command.
4. Launches the local `vnc-viewer-delay` capability.
5. Expands `127.0.0.1::{LOCAL_PORT:vnc}` so the viewer connects to the local
   forwarded port.

The local viewer should connect to `127.0.0.1`, not to the remote host. SSH is
the thing carrying traffic to the remote VNC service.

## Example: VNC Without SSH Forwarding

Use this on a trusted LAN or VPN where the remote VNC port is directly
reachable from your machine.

```json
{
  "id": "66666666-6666-6666-6666-666666666666",
  "name": "VNC direct",
  "remote_command": "DISPLAY=:0 x11vnc -scale 1/2",
  "local_launch": {
    "capability": "vnc-viewer-delay",
    "args": ["{HOST}::5900"]
  }
}
```

What it does:

1. Runs `DISPLAY=:0 x11vnc -scale 1/2` over SSH.
2. Launches the local viewer against `{HOST}::5900`.
3. Does not create an SSH port forward.

This is simpler than the forwarded version, but it assumes your local machine
can reach the remote host's VNC port directly.

## VNC Viewer Delay Wrapper

VNC servers may take a few seconds to start after SSH begins running `x11vnc`.
A small local wrapper script can wait until the forwarded local port is open
before launching the real viewer.

```sh
#!/bin/sh
target="$1"
case "$target" in
127.0.0.1::*)
  port="${target##*::}"
  for _ in $(seq 1 60); do
    nc -z 127.0.0.1 "$port" && exec xtightvncviewer "$target"
    sleep 1
  done
  echo "VNC port did not open: $target" >&2
  exit 1
  ;;
esac

sleep 3
exec xtightvncviewer "$target"
```

Map it in `local.json`:

```json
{
  "name": "vnc-viewer-delay",
  "path": "/home/alice/bin/stassh-vnc-viewer-delay"
}
```

The wrapper is local and machine-specific, so it belongs behind a capability.
The portable action only needs to know that a `vnc-viewer-delay` capability
exists.

## Local Prepare And Env Ports

`local_prepare` can emit `KEY=value` lines on stdout. Valid keys contain ASCII
letters, digits, and underscores. Other lines are ignored.

Example preparation script output:

```text
PORT=5951
DISPLAY=:1
```

An action can consume those values:

```json
{
  "id": "77777777-7777-7777-7777-777777777777",
  "name": "Prepared VNC",
  "local_prepare": {
    "program": "/home/alice/bin/choose-vnc-port"
  },
  "forwards": [
    {
      "type": "local",
      "name": "vnc",
      "bind_address": "127.0.0.1",
      "local_port": { "env": "PORT" },
      "destination_host": "127.0.0.1",
      "destination_port": 5900
    }
  ],
  "remote_command": "DISPLAY={ENV:DISPLAY} x11vnc -scale 1/2",
  "local_launch": {
    "capability": "vnc-viewer-delay",
    "args": ["127.0.0.1::{LOCAL_PORT:vnc}"]
  }
}
```

This is useful when a local helper needs to reserve or choose values before the
SSH command and local launch command are rendered.

## Cleanup

Cleanup commands run locally after SSH exits. They are best for local temporary
files, local helper processes, or local state created by `local_prepare` or
`local_launch`.

```json
{
  "id": "88888888-8888-8888-8888-888888888888",
  "name": "Command with cleanup",
  "remote_command": "tail -f /var/log/example.log",
  "cleanup": [
    {
      "program": "/home/alice/bin/stassh-cleanup-example",
      "args": ["{HOST}"]
    }
  ]
}
```

Cleanup failures are reported, but they do not change what happened to the SSH
session. Test cleanup behavior explicitly.

## Debugging Checklist

- Run `stassh action <host> <action> --dry-run`.
- Use `stassh --output json action <host> <action> --dry-run` when an AI or
  script needs structured details.
- Confirm generated SSH arguments are what you expected.
- Confirm automatic ports appear in `allocated_ports`.
- Confirm `{LOCAL_PORT:name}` uses a forward name that exists.
- Confirm every `capability` is mapped in `local.json`.
- Put complex local timing, retries, and probing in wrapper scripts.
- Remember that `args` is an argv array, not shell syntax.
- Test fixed ports for collisions before relying on them.
- Keep remote command quoting explicit; `stassh` passes it as the SSH remote
  command, and the remote side decides how to interpret it.
