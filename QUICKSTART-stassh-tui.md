# QUICKSTART: stassh-tui

This tutorial gets you from a fresh clone to using `stassh-tui` as your SSH
workspace. It intentionally skips many reference details. Use
[README.md](README.md) for the full command reference, alternate configuration
locations, importer details, and every key binding.

## 1. Build

Requirements:

- Rust and Cargo
- OpenSSH client available as `ssh`
- OpenSSH `ssh-keygen`, useful when importing identities from `~/.ssh/config`

After cloning:

```bash
git clone https://github.com/arturormk/stassh-rust
cd stassh-rust
cargo test --workspace
```

For a local release build on the current machine:

```bash
RUSTFLAGS="-C debuginfo=0 -C strip=symbols" cargo build -p stassh -p stassh-tui --release
```

The binaries will be:

```text
target/release/stassh
target/release/stassh-tui
```

For a more portable Linux binary, build against musl. This produces a mostly
static executable that is much less sensitive to the target machine's glibc
version, which is useful when carrying `stassh-tui` between newer and older Linux
systems:

```bash
rustup target add x86_64-unknown-linux-musl
RUSTFLAGS="-C debuginfo=0 -C strip=symbols" \
  cargo build -p stassh -p stassh-tui --release --target x86_64-unknown-linux-musl
```

The musl binaries will be under:

```text
target/x86_64-unknown-linux-musl/release/stassh
target/x86_64-unknown-linux-musl/release/stassh-tui
```

Other musl targets follow the same pattern when the Rust target and matching
linker are installed.

Cross-compilation needs the Rust target and a matching C linker. On
Debian/Ubuntu-like systems, install the relevant cross compiler package first.
Package names vary by distribution.

amd64 / x86_64 Linux:

```bash
rustup target add x86_64-unknown-linux-gnu
RUSTFLAGS="-C debuginfo=0 -C strip=symbols" \
  cargo build -p stassh -p stassh-tui --release --target x86_64-unknown-linux-gnu
```

x86 / 32-bit Intel Linux:

```bash
sudo apt install gcc-i686-linux-gnu
rustup target add i686-unknown-linux-gnu
CARGO_TARGET_I686_UNKNOWN_LINUX_GNU_LINKER=i686-linux-gnu-gcc \
RUSTFLAGS="-C debuginfo=0 -C strip=symbols" \
  cargo build -p stassh -p stassh-tui --release --target i686-unknown-linux-gnu
```

ARMv6 hard-float Linux, such as Raspberry Pi Zero/1 class systems:

```bash
sudo apt install gcc-arm-linux-gnueabihf
rustup target add arm-unknown-linux-gnueabihf
CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc \
RUSTFLAGS="-C debuginfo=0 -C strip=symbols" \
  cargo build -p stassh -p stassh-tui --release --target arm-unknown-linux-gnueabihf
```

ARMv7 hard-float Linux:

```bash
sudo apt install gcc-arm-linux-gnueabihf
rustup target add armv7-unknown-linux-gnueabihf
CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc \
RUSTFLAGS="-C debuginfo=0 -C strip=symbols" \
  cargo build -p stassh -p stassh-tui --release --target armv7-unknown-linux-gnueabihf
```

For cross builds, the binaries will be under:

```text
target/<target-triple>/release/stassh
target/<target-triple>/release/stassh-tui
```

## 2. Install

`stassh-tui` is the terminal UI. Keep `stassh` too; it initializes vaults,
imports `~/.ssh/config`, maps identities, and handles some less common edits.

Copy both binaries to a directory in your `PATH`:

```bash
sudo cp target/release/stassh /usr/local/bin/
sudo cp target/release/stassh-tui /usr/local/bin/
```

For a cross-compiled build, copy from the target-specific release directory:

```bash
sudo cp target/armv7-unknown-linux-gnueabihf/release/stassh /usr/local/bin/
sudo cp target/armv7-unknown-linux-gnueabihf/release/stassh-tui /usr/local/bin/
```

Check that both commands are available:

```bash
stassh --help
stassh-tui --help
```

## 3. Create or Import Your Vault

Create the default vault:

```bash
stassh vault init
```

If you already have an OpenSSH config, import it:

```bash
stassh import openssh ~/.ssh/config
stassh vault check
```

The importer handles common concrete `Host` entries, nested `Include` files,
`HostName`, `User`, `Port`, `ProxyJump`, simple forwards, and ordinary
`IdentityFile` paths. It prints warnings for things it cannot fully import.

Start the TUI:

```bash
stassh-tui
```

## 4. Understand the Two Config Files

The default setup uses:

```text
~/.ssh/stassh/vault.json
~/.ssh/stassh/local.json
```

Permissions should be:

```text
~/.ssh/stassh/            700
~/.ssh/stassh/vault.json  600
~/.ssh/stassh/local.json  600
```

`vault.json` is the portable SSH workspace. It stores folders, hosts, common
actions, tags, notes, identity fingerprints, jump chains, forwards, and raw SSH
options. It does not store private key material.

A simplified `vault.json` looks like:

```json
{
  "format_version": 0,
  "actions": [
    {
      "id": "00000000-0000-0000-0000-000000000010",
      "name": "VNC direct",
      "remote_command": "DISPLAY=:0 x11vnc -scale 1/2",
      "local_launch": {
        "capability": "vnc-viewer-delay",
        "args": ["{HOST}::5900"]
      }
    }
  ],
  "folders": [
    {
      "id": "00000000-0000-0000-0000-000000000001",
      "parent_id": null,
      "name": "Root"
    }
  ],
  "hosts": [
    {
      "id": "00000000-0000-0000-0000-000000000002",
      "folder_id": "00000000-0000-0000-0000-000000000001",
      "display_name": "web",
      "hostname": "web.example.com",
      "port": 22,
      "username": "deploy",
      "identity_fingerprint": "SHA256:example",
      "jump_chain": [],
      "ssh_options": [],
      "forwards": [],
      "tags": ["prod"],
      "notes": "primary web host"
    }
  ]
}
```

`local.json` is machine-local. It maps portable identity fingerprints to private
key paths and portable capability names to executable paths on this computer. It
may reveal local usernames and paths, so treat it as private too.

A simplified `local.json` looks like:

```json
{
  "format_version": 0,
  "identity_mappings": [
    {
      "fingerprint": "SHA256:example",
      "preferred_name": "customer-key",
      "path": "/home/alice/.ssh/customer-key"
    }
  ],
  "capability_mappings": [
    {
      "name": "vnc-viewer-delay",
      "path": "/home/alice/bin/stassh-vnc-viewer-delay"
    }
  ]
}
```

To add a local key mapping:

```bash
stassh identity add ~/.ssh/customer-key --name customer-key
```

## 5. Use the TUI

Launch:

```bash
stassh-tui
```

Basic movement:

- `j` / `Down`: move down
- `k` / `Up`: move up
- `/`: search hosts
- `Esc`: leave search or cancel a mode
- `Enter`: connect to a host or expand/collapse a folder
- `r`: reload the vault from disk
- `q`: quit

Create, edit, and delete folders:

- `f`: create a folder under the selected folder, or under the selected host's folder
- `e`: edit the selected folder name or parent folder UUID
- `x` / `Delete`: delete the selected empty folder after confirmation

Create, edit, copy, and delete hosts:

- `n`: create a host in the selected folder, or in the selected host's folder
- `e`: edit the selected host's name, hostname, port, username, tags, and notes
- `C`: copy the selected host as `<name> copy`
- `x` / `Delete`: delete the selected host after confirmation
- `Ctrl+S`: save while inside an editor

Move hosts between folders:

1. Highlight a host and press `Space` to select it.
2. Select more hosts if needed.
3. Press `m` to open the folder picker.
4. Highlight the destination folder.
5. Press `Enter` to move the selected hosts.

If no hosts are selected, `m` moves the highlighted host.

Host access settings:

- `i`: choose or clear the selected host's local identity mapping
- `J`: edit the selected host's jump chain
- `F`: edit local, remote, or dynamic forwards
- `a`: choose and run a common action for the selected host

Raw SSH options and actions are still edited outside the TUI:

```bash
stassh host edit web --ssh-option ServerAliveInterval=30
```

## 6. SSH Sessions

Highlight a host and press `Enter`.

`stassh-tui` leaves the alternate-screen interface and runs normal OpenSSH in
your terminal. Password prompts, passphrase prompts, host-key checks, agent use,
terminal behavior, and remote shell behavior all come from OpenSSH and your
terminal.

To stop a normal remote shell:

```bash
exit
```

or press `Ctrl+D`.

For a stuck connection attempt, `Ctrl+C` usually interrupts OpenSSH. When SSH
exits, `stassh-tui` restores its interface.

## 7. Actions

Actions are reusable workflows. Common actions are stored once in `vault.json`
and apply to every host. Machine-specific local programs are mapped in
`local.json` as capabilities.

For example, a forwarded VNC action can allocate a local port, forward it to
remote port `5900`, run `x11vnc` through SSH, then launch a local viewer script:

```bash
stassh action web "VNC forwarded" --dry-run
stassh action web "VNC forwarded"
```

The direct LAN/VPN version skips forwarding and connects the viewer to the
resolved host address:

```bash
stassh action web "VNC direct" --dry-run
stassh action web "VNC direct"
```

In `stassh-tui`, highlight a host, press `a`, choose the action, and press
`Enter`. The TUI leaves the alternate screen while the action runs and restores
itself when SSH exits. If a local viewer or wrapper script exits early, the CLI
prints that status, which is useful when diagnosing forwarded VNC setup.

## 8. tmux and byobu

`stassh-tui` works well inside `tmux` or byobu.

Start one first:

```bash
tmux
stassh-tui
```

or:

```bash
byobu
stassh-tui
```

Inside tmux/byobu:

- `Enter`: open the selected host in the current terminal until SSH exits
- `t`: open the selected host in a new tmux/byobu window or byobu tab

Using `t` is useful because:

- you can keep `stassh-tui` open as your launcher and catalog
- each SSH session gets its own tmux/byobu window or byobu tab
- you can switch between many simultaneous sessions
- OpenSSH still owns the actual terminal session, so interactive behavior stays normal

If you press `t` outside tmux/byobu, `stassh-tui` shows a status message and does
not change the current session.

## 9. Common First-Day Workflow

```bash
git clone https://github.com/arturormk/stassh-rust
cd stassh-rust
cargo test --workspace
RUSTFLAGS="-C debuginfo=0 -C strip=symbols" cargo build -p stassh -p stassh-tui --release
sudo cp target/release/stassh target/release/stassh-tui /usr/local/bin/

stassh vault init
stassh import openssh ~/.ssh/config
stassh vault check
stassh-tui
```

Then use the TUI to browse, search, clean up folders, edit hosts, set identities,
adjust jumps and forwards, run actions, and connect.
