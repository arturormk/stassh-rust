# QUICKSTART: stassh secrets

This tutorial explains how to create and maintain the optional encrypted
`secrets.json` file from the `stassh` CLI.

The secrets feature is a small fallback store for operational values associated
with SSH hosts. It is useful when you are in the field, do not have your primary
password store available, and need to look up a password, PIN, token, or note
that belongs to the selected host or site.

It is not a full password manager. Ordinary SSH connections still work without
`secrets.json`.

## 1. Files and Paths

A normal default setup uses:

```text
~/.ssh/stassh/vault.json
~/.ssh/stassh/local.json
~/.ssh/stassh/secrets.json
```

`vault.json` stores hosts and folders. A host can optionally reference one
secrets set by name:

```json
{
  "display_name": "web-01",
  "hostname": "web-01.example.com",
  "secrets": "customer-site"
}
```

`secrets.json` stores named sets. Plain fields are stored as ordinary strings.
Fields created with the `secret` command are encrypted.

Path resolution for the secrets file is:

1. `--secrets-file /path/to/secrets.json`
2. `STASSH_SECRETS=/path/to/secrets.json`
3. `~/.ssh/stassh/secrets.json` when using the default home vault
4. `secrets.json` beside the selected vault

For a project-local test vault:

```bash
stassh --vault ./vault.json --secrets-file ./secrets.json secrets manage
```

## 2. Create the Secrets Store

Start the management REPL:

```bash
stassh secrets manage
```

If the file does not exist, `stassh` offers to create it:

```text
No secrets store exists at:
    /home/alice/.ssh/stassh/secrets.json
Create it? [Y/n] y
New master password:
Repeat New master password:
Secrets store created.
stassh-secrets>
```

The master password is not saved. It is used to derive an encryption key for this
management session. Command errors inside the `stassh-secrets>` prompt are
reported without closing the session, so you do not need to unlock the store
again after a typo or validation error.

## 3. Create a Set

Create a set for a site, customer, device family, or other group of hosts:

```text
stassh-secrets> create customer-site "Customer Site"
Created customer-site.
stassh-secrets:customer-site>
```

List sets:

```text
stassh-secrets:customer-site> sets
customer-site    Customer Site
```

Switch to an existing set:

```text
stassh-secrets:customer-site> use customer-site
Using: customer-site
```

Set names are stable IDs used by hosts. Use short names without spaces, such as
`customer-site`, `lab-router`, or `shop-17`.

## 4. Add Plain Metadata

Plain fields are convenient for non-secret reference data:

```text
stassh-secrets:customer-site> set admin_user directorio
Updated admin_user.

stassh-secrets:customer-site> set note "Shared admin credentials for this site"
Updated note.
```

Plain values are visible in `secrets.json`, so do not use `set` for passwords,
tokens, PINs, recovery codes, or anything else that should be encrypted.

## 5. Add an Encrypted Secret

Use `secret <field>` for encrypted values:

```text
stassh-secrets:customer-site> secret password
New secret value:
Repeat New secret value:
Updated password.
```

Do not type secret values as command arguments. This is intentionally rejected:

```text
stassh-secrets:customer-site> secret password ActualPassword
secret values must be entered at the prompt, not as command arguments
```

Add any field name you need:

```text
stassh-secrets:customer-site> secret root_password
stassh-secrets:customer-site> secret vnc_password
stassh-secrets:customer-site> secret enable_password
```

Field names do not decide whether a value is secret. The `secret` command does.

## 6. List, Get, Reveal, and Delete Fields

List fields in the current set:

```text
stassh-secrets:customer-site> list
admin_user    directorio
note          Shared admin credentials for this site
password      [secret]
```

Read a plain field:

```text
stassh-secrets:customer-site> get admin_user
directorio
```

Reveal an encrypted field:

```text
stassh-secrets:customer-site> reveal password
Example-Pronounceable-Password-73
```

Delete a field:

```text
stassh-secrets:customer-site> delete password
Deleted password.
```

Exit the management session:

```text
stassh-secrets:customer-site> exit
Secrets locked.
```

## 7. Link Hosts to the Set

Create or edit hosts with `--secrets SET`:

```bash
stassh host add web-01 web-01.example.com --user directorio --secrets customer-site
stassh host add web-02 web-02.example.com --user directorio --secrets customer-site
```

For an existing host:

```bash
stassh host edit web-01 --secrets customer-site
```

Clear the link:

```bash
stassh host edit web-01 --clear-secrets
```

Check the host:

```bash
stassh show web-01
```

The output includes:

```text
Secrets: customer-site
```

## 8. Rename or Delete Sets

Rename the active set or another set:

```text
stassh-secrets:customer-site> rename-set customer-site customer-a
Renamed customer-site to customer-a.
```

When a set is renamed, host references in `vault.json` are updated after the
renamed `secrets.json` has been saved. The two files are still separate files,
so this is not one fully atomic transaction. If Stassh reports that the secrets
set was renamed but host references could not be updated, hosts may still point
to the old set name. Repair those hosts with:

```text
stassh host edit web-01 --secrets customer-a
```

or rename the set back from inside `stassh secrets manage`.

Delete a set:

```text
stassh-secrets:customer-a> delete-set customer-a
Deleted customer-a.
```

Deleting a set removes the fields from `secrets.json`. It does not delete hosts.

## 9. Use Secrets from the TUI

After a host is linked to a set:

```bash
stassh-tui
```

In the TUI:

- highlight the host
- press `s` to open its secrets set
- plain fields are shown immediately
- encrypted fields are shown as `[secret]`
- select an encrypted field and press `Enter`
- type the secrets master password
- press `h` to hide the revealed value
- press `Esc` to close the secrets view

The TUI asks for the master password for each reveal operation. It does not keep
the secrets store unlocked during normal browsing.

## 10. Security Notes

- `secrets.json` is optional.
- The master password is not stored.
- Encrypted fields use authenticated encryption.
- `stassh secrets manage` keeps the derived key in memory until you exit the
  management session.
- Prompted master passwords and encrypted secret values are cleared from process
  memory on drop where Rust can control the buffer.
- Only fields created with `secret` are encrypted.
- Set names, labels, field names, and plain fields remain visible.
- Secret values should not be passed in command-line arguments.
- `stassh` does not inject passwords into SSH sessions, `sudo`, or `su`.
- If you lose the master password, encrypted fields may be unrecoverable.
