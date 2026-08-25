# BLUEPRINT.md

## Project Blueprint: Portable, Lightweight SSH Workspace with CLI, TUI, and GUI Frontends

### Status of This Document

This document describes the **initial concept, architectural direction, product philosophy, and likely implementation shape** of the project.

It is intentionally **descriptive, not prescriptive**.

The purpose of this blueprint is to give a coding agent enough context to understand what kind of application is being built, who it is for, why the proposed architecture looks the way it does, and which constraints matter most.

The project **may and should diverge from this document** whenever implementation experience, testing, platform constraints, security considerations, usability findings, or better technical ideas suggest a superior approach.

Do not treat individual library suggestions, module boundaries, storage formats, command names, UI sketches, or implementation details below as immutable requirements.

The project's goals matter more than reproducing the initial design literally.

If a change makes the application:

* simpler,
* safer,
* faster,
* easier to maintain,
* more portable,
* more compatible,
* more understandable,
* or more useful,

then that change should be seriously considered even if it differs from this blueprint.

---

# 1. Project Vision

The project is an open-source, lightweight SSH workspace for people who regularly access many remote machines and want to carry their operational environment between computers.

It should combine some of the useful ideas found in applications such as Termius and MobaXterm while deliberately avoiding their tendency toward becoming large, self-contained platforms.

The application should feel much closer to:

> an enhanced system terminal with excellent organization and orchestration

than to:

> a large graphical remote-administration suite.

The core use case is a person who administers dozens, hundreds, or potentially thousands of hosts and wants:

* a familiar hierarchy of systems,
* fast search,
* remembered usernames,
* remembered SSH identities,
* jump-host topology,
* SSH port-forward definitions,
* reusable connection actions,
* portable encrypted configuration,
* and access to the same environment from multiple computers.

The application should remain comfortable on relatively modest hardware.

A user may deliberately carry an inexpensive or older laptop instead of a primary workstation because losing or damaging the machine would be less serious.

The target is therefore not merely "low application size."

The application should actually **feel fast and lightweight during use**.

---

# 2. Product Identity

The project is not primarily intended to be:

* a new SSH protocol implementation,
* a cloud SSH service,
* a password manager,
* a configuration-management database,
* a team collaboration platform,
* an infrastructure monitoring suite,
* a file synchronization service,
* a replacement operating environment,
* or a giant remote desktop package.

Its primary value is:

1. organizing remote systems,
2. remembering how the user accesses them,
3. composing ordinary SSH functionality conveniently,
4. presenting the same configuration through CLI, TUI, and GUI frontends,
5. and allowing that configuration to be portable and synchronizable without requiring a proprietary backend.

A useful concise description is:

> **A portable, offline-first SSH workspace for people who administer many machines, with equal CLI, terminal, and desktop interfaces and no dependency on proprietary synchronization infrastructure.**

Another equally valid description is:

> **A fast, keyboard-friendly organizer and orchestrator for ordinary OpenSSH.**

---

# 3. Target Audience

The primary audience is technically competent users who already understand SSH or are comfortable learning its concepts.

Typical users may include:

* system administrators,
* developers,
* consultants,
* field engineers,
* network administrators,
* embedded developers,
* infrastructure support technicians,
* homelab users,
* researchers,
* people maintaining remote installations,
* people supporting customer systems,
* and anyone with a large collection of SSH-accessible devices.

A representative user may have a hierarchy such as:

```text
Customers
├── Acme
│   ├── Production
│   │   ├── web-01
│   │   ├── web-02
│   │   ├── db-01
│   │   └── router
│   └── Testing
│       └── staging
├── Widget Corp
│   └── signage-controller
└── OtherCo
    └── vpn-gateway

Home
├── nas
├── raspberrypi
└── router

Lab
├── fpga-host
├── serial-gateway
└── test-server
```

The user does not necessarily want the application to hide SSH concepts.

In many cases, the opposite is preferable.

The application should make SSH easier to organize and invoke while still exposing what it is actually doing.

---

# 4. Core Product Principles

## 4.1 Lightweight by design

Performance is a product requirement.

The application should not merely advertise itself as lightweight because the executable file is small.

It should be designed to have:

* fast startup,
* low idle CPU usage,
* reasonable memory use,
* responsive keyboard interaction,
* responsive host-tree navigation,
* efficient fuzzy search,
* low-overhead terminal I/O,
* and acceptable behavior on low-powered computers.

Avoid architectural decisions that assume:

* abundant RAM,
* high-end CPUs,
* modern GPUs,
* or always-on background services.

---

## 4.2 Offline first

The application should work fully without Internet access other than the network connectivity required to reach the target SSH hosts.

There should be no mandatory:

* account,
* central API,
* proprietary cloud,
* login service,
* license check,
* hosted synchronization backend,
* telemetry dependency,
* or external authentication service.

If development of the project stopped permanently, an installed copy should remain useful indefinitely.

---

## 4.3 OpenSSH first

The initial design should prefer the system's OpenSSH implementation rather than recreating SSH functionality unnecessarily.

OpenSSH already provides:

* public-key authentication,
* SSH agents,
* hardware-backed identities,
* certificates,
* host-key verification,
* `known_hosts`,
* jump hosts,
* local forwarding,
* remote forwarding,
* dynamic SOCKS forwarding,
* keyboard-interactive authentication,
* passwords,
* connection multiplexing,
* and extensive platform compatibility.

The project should therefore primarily **orchestrate OpenSSH**.

A future native Rust SSH backend may be considered if it produces real benefits, but the project should not begin by assuming that it must implement SSH itself.

OpenSSH interoperability is more valuable than abstraction purity.

---

## 4.4 Use existing system capabilities rather than reimplementing them

This philosophy should extend beyond SSH.

Examples:

* OpenSSH should perform SSH.
* The user's actual terminal should perform terminal emulation in the TUI path.
* A VNC viewer should perform VNC.
* A browser should display a forwarded web interface.
* `tmux` may perform terminal multiplexing.
* Existing synchronization tools may synchronize vault files.
* Existing SSH agents should manage SSH keys.

The application should specialize in **organization and orchestration**.

---

## 4.5 CLI, TUI, and GUI are first-class citizens

The project should have three frontends:

```text
CLI
TUI
GUI
```

None should be regarded merely as a compatibility fallback.

They should share the same underlying data model and core logic.

A configuration created in one frontend should be understood by the others.

Feature parity should mean **semantic compatibility**, not identical interfaces.

For example:

* the GUI may have a richer host editor,
* the TUI may have a simpler editor,
* the CLI may expose scripting-oriented commands,

but all should understand the same host, action, forwarding, jump, vault, and identity definitions.

---

## 4.6 The core must not depend on a frontend

The business logic should live in reusable Rust crates.

The core should not depend on:

* Tauri,
* WebKit,
* GTK,
* Ratatui,
* xterm.js,
* browser APIs,
* frontend JavaScript frameworks,
* or GUI-specific state models.

The three frontends should consume the same Rust API.

This helps preserve portability and allows future frontend replacement without rewriting the project.

---

# 5. High-Level Architecture

A conceptual architecture may look like:

```text
                         ┌────────────────────┐
                         │    Rust Core       │
                         │                    │
                         │ domain model       │
                         │ vault              │
                         │ encryption         │
                         │ sync semantics     │
                         │ identity discovery │
                         │ SSH generation     │
                         │ actions            │
                         │ forwarding         │
                         └─────────┬──────────┘
                                   │
             ┌─────────────────────┼─────────────────────┐
             │                     │                     │
     ┌───────▼────────┐    ┌───────▼────────┐    ┌──────▼───────┐
     │      CLI       │    │      TUI       │    │      GUI     │
     │                │    │                │    │              │
     │ scripting      │    │ Ratatui-like   │    │ Tauri-like   │
     │ shell use      │    │ interface      │    │ desktop UI   │
     └────────────────┘    └────────────────┘    └──────────────┘
```

A possible Cargo workspace layout might eventually resemble:

```text
workspace/
├── crates/
│   ├── model/
│   ├── vault/
│   ├── crypto/
│   ├── sync/
│   ├── identities/
│   ├── ssh/
│   ├── sessions/
│   ├── actions/
│   └── core/
│
├── apps/
│   ├── cli/
│   ├── tui/
│   └── gui/
│
└── frontend/
    └── gui-web/
```

The exact decomposition is not mandatory.

The important principle is separation between:

* domain logic,
* storage,
* SSH orchestration,
* session mechanics,
* and presentation.

---

# 6. Recommended Development Order

The project may be easier to design correctly if implementation begins below the GUI layer.

A plausible order is:

```text
1. Core domain model
2. OpenSSH orchestration
3. Minimal local storage
4. CLI
5. TUI
6. Vault encryption
7. Synchronization semantics
8. Tauri GUI
9. Actions
10. VNC workflow
11. Advanced import/export and diagnostics
```

The purpose of this ordering is architectural discipline.

If the CLI can already do:

```bash
app connect <host>
```

then the GUI must call an existing core operation rather than embedding connection logic in a Tauri command handler.

The CLI and TUI can therefore serve as useful reference implementations of the core.

---

# 7. Core Domain Model

The synchronized domain model should use stable identifiers.

Names and hierarchy paths should not serve as identity.

A folder should have a UUID.

A host should have a UUID.

Renaming:

```text
Customers/Acme/Router
```

to:

```text
Clients/Acme/Network/Router
```

should not change the underlying host identity.

Likely major entities include:

```text
Folder
Host
IdentityReference
JumpDefinition
ForwardDefinition
Action
Tag
Vault
SyncOperation
```

Other entities may emerge later.

---

# 8. Folder Hierarchy

Hosts should be organizable into nested folders.

Example:

```text
Customers
└── Acme
    └── Production
        └── web-01
```

Folders should support:

* creation,
* deletion,
* renaming,
* movement,
* nesting,
* synchronization,
* and stable identity independent of path.

Host organization should remain simple.

The application should not gradually become a full CMDB.

---

# 9. Search and Navigation

Folder hierarchy is important for organization, but users with many hosts will often navigate by search.

Fast fuzzy search should therefore be considered a core feature rather than an optional enhancement.

A typical workflow:

```text
Ctrl+P
> acme sql

Acme / Production / sql-01
Acme / Production / sql-02
Acme / Testing / sql-test
```

Pressing Enter should connect immediately or expose the default host action.

Tags may complement folders.

Possible tags:

```text
production
customer:acme
role:database
region:madrid
linux
router
```

Search should remain fast even with thousands of entries.

---

# 10. Host Model

A host might initially contain fields conceptually similar to:

```text
id
folder_id
display_name
hostname
port
username
identity_reference
jump_chain
ssh_options
tags
notes
actions
forwards
```

Not every field needs to exist in version 1.

The model should remain extensible.

---

# 11. OpenSSH Integration

## 11.1 OpenSSH should normally be the execution engine

The application should invoke the installed OpenSSH client.

Interactive sessions should ultimately behave like ordinary:

```bash
ssh host
```

rather than like a completely separate protocol implementation.

---

## 11.2 Generate SSH configuration where useful

Complex sessions are often easier to represent using temporary SSH configuration than giant command lines.

For example:

```sshconfig
Host app-jump
    HostName bastion.example.com
    User admin
    IdentityFile /home/user/.ssh/client-a

Host app-target
    HostName 10.20.0.5
    User root
    IdentityFile /home/user/.ssh/client-a
    ProxyJump app-jump
```

The application may then execute:

```bash
ssh -F <generated-file> app-target
```

Generated configuration should be considered ephemeral session state.

It should normally be deleted after use.

---

## 11.3 SSH operations should remain understandable

Advanced users should be able to see what the application resolved.

Possible future functionality:

```bash
app diagnose acme/router
```

could display:

```text
Resolved host:        router.example.com
User:                 admin
Port:                 22
Identity:             SHA256:...
Identity path:        /home/user/.ssh/acme
Jump chain:           bastion.example.com
Known hosts file:     ...
Forwarding:           ...
Generated alias:      ...
```

Optionally the equivalent OpenSSH command or generated configuration may be shown.

The application should help rather than obscure.

---

# 12. Existing `~/.ssh/config`

The project should not require users to abandon existing OpenSSH configuration.

Potential approaches include:

* referencing an existing SSH alias,
* importing hosts,
* reading selected OpenSSH settings,
* or maintaining fully application-managed hosts.

A host may conceptually have different origins:

```text
ManagedHost
OpenSshAlias
ImportedHost
```

The implementation does not need to adopt exactly these types.

The important principle is coexistence.

A user with years of carefully maintained `~/.ssh/config` should not be penalized for using the application.

---

# 13. SSH Identities

## 13.1 Synced identities should not be file paths

A synchronized host definition should not normally contain:

```text
/home/alice/.ssh/customer_a
```

because that path is machine-specific.

Instead, the synchronized configuration should identify the key cryptographically.

Conceptually:

```text
IdentityReference
    fingerprint
    preferred_name
```

Example:

```text
fingerprint = SHA256:abcdefghijklmnop...
preferred_name = acme-production
```

---

## 13.2 Machine-local key mapping

Each machine should resolve the synchronized identity reference to something locally usable.

For example:

```text
SHA256:abc... -> /home/alice/.ssh/acme
```

while another computer may map the same fingerprint to:

```text
C:\Users\Alice\.ssh\acme_ed25519
```

A third system may find the identity through `ssh-agent`.

This mapping should generally be machine-local.

---

## 13.3 Identity discovery

The application may discover identities through:

* public key files under `~/.ssh`,
* files referenced by `~/.ssh/config`,
* `ssh-agent`,
* user-selected private key files,
* hardware-backed SSH identities,
* and potentially other OpenSSH-supported mechanisms.

Private key material should not need to enter the GUI frontend or WebView.

The Rust/backend layer should handle identity paths and process invocation.

---

## 13.4 Missing identities

If a host references a known fingerprint but the current machine cannot resolve it, the UI might say:

```text
This host uses:

acme-production
SHA256:abc...

This identity has not been located on this computer.

[Locate key]
[Use agent identity]
[Cancel]
```

Once resolved, the mapping should apply to all hosts using the same identity.

---

# 14. SSH Agents and Hardware Keys

Agent-based authentication should be treated as normal rather than exceptional.

The architecture should avoid assuming:

```text
SSH identity == private key file
```

OpenSSH-supported identities may include:

* ordinary private key files,
* `ssh-agent`,
* FIDO2/security keys,
* hardware-backed keys,
* certificates,
* or other mechanisms OpenSSH already understands.

The application should avoid narrowing these possibilities through an overly restrictive initial data model.

---

# 15. Password Authentication

Password authentication should be supported because real infrastructure sometimes requires it.

It should not be the preferred workflow.

Initially, allowing OpenSSH itself to prompt for the password may be sufficient.

This has several advantages:

* minimal custom secret-handling code,
* no need to place passwords in frontend state,
* no password persistence,
* and behavior consistent with normal SSH.

Stored passwords may be considered later if there is a compelling use case and an appropriate secret-storage design.

Do not make password-storage complexity a prerequisite for the first usable version.

---

# 16. Jump Hosts

Jump hosts should be first-class.

A host should be able to reference:

* one jump host,
* or possibly a jump chain.

Example:

```text
Laptop
  -> public-bastion
  -> internal-gateway
  -> database
```

The implementation should map this cleanly to OpenSSH semantics.

Existing OpenSSH concepts such as `ProxyJump` should be preferred when possible.

The application should avoid inventing opaque proprietary tunnel semantics.

---

# 17. Port Forwarding

Port forwarding should be presented directly and honestly.

Supported concepts should likely include:

```text
Local forward
Remote forward
Dynamic/SOCKS forward
```

A local forward editor may show:

```text
Type:             Local
Bind address:     127.0.0.1
Local port:       8080
Destination host: 10.0.0.7
Destination port: 80
```

The UI may also show the corresponding OpenSSH representation:

```text
-L 127.0.0.1:8080:10.0.0.7:80
```

Likewise:

```text
-R ...
-D ...
```

This audience is likely to benefit from seeing the underlying operation.

The application should make forwarding convenient without pretending that it is magic.

---

# 18. Tunnel-Only Sessions

The application should support sessions whose purpose is forwarding rather than opening a shell.

Conceptually:

```bash
ssh -N ...
```

Useful options such as failing when the requested forward cannot be established should be considered.

The UI may provide a small active-forward panel:

```text
Active forwards

web-admin
L 127.0.0.1:8080 -> 10.0.0.7:80

postgres
L 127.0.0.1:5433 -> db:5432

proxy
D 127.0.0.1:1080
```

---

# 19. Actions

VNC should probably not become a hard-coded special subsystem.

Instead, the project should eventually support composable **Actions**.

An Action could conceptually perform:

```text
1. optional remote preparation command
2. optional SSH forwarding setup
3. optional local command
4. wait for local command
5. optional cleanup
```

This abstraction can support many workflows.

Examples:

```text
VNC
RDP
Web administration interface
Jupyter
Database client
Debugging service
Remote serial bridge
Custom service console
```

---

# 20. VNC Workflow

A VNC action may look conceptually like:

```text
Action: Desktop

Remote preparation:
    systemctl --user start x11vnc

Forward:
    local 127.0.0.1:{AUTO}
    ->
    remote 127.0.0.1:5900

Local application:
    vncviewer 127.0.0.1::{LOCAL_PORT}

Cleanup:
    systemctl --user stop x11vnc
```

The desired user experience is:

```text
select host
-> choose Desktop
-> application performs SSH setup
-> VNC viewer opens
```

The user should not have to manually establish the forward every time.

However, the application should still expose enough detail that an advanced user can understand what happened.

---

# 21. Automatic Local Ports

Actions such as VNC often require an available local TCP port.

The application should provide a mechanism to allocate one automatically.

A likely strategy:

1. bind local TCP port `0`,
2. ask the OS which port was assigned,
3. release the socket,
4. immediately start OpenSSH using that port,
5. retry if a race caused another process to claim it.

The exact implementation may differ.

Port allocation should be treated as ephemeral session state and should never need synchronization.

---

# 22. Machine Capabilities

Portable actions should not necessarily synchronize absolute executable paths.

For example, a synchronized action should preferably say:

```text
requires capability: vnc-viewer
```

rather than:

```text
/usr/bin/xtigervncviewer
```

Each machine can map capabilities to local programs.

Example:

```text
vnc-viewer -> /usr/bin/xtigervncviewer
browser    -> /usr/bin/firefox
terminal   -> /usr/bin/kitty
```

Another platform may map those capabilities differently.

This improves portability.

---

# 23. Configuration Layers

The project should distinguish several kinds of state.

A useful conceptual model is:

```text
SYNCHRONIZED / PORTABLE
    hosts
    folders
    usernames
    identity fingerprints
    jumps
    forwards
    actions
    tags
    notes
    known-host policy/data where appropriate

VAULT-LOCAL
    vault-specific preferences
    perhaps some display or organization metadata

MACHINE-LOCAL
    identity path mappings
    external program paths
    machine capabilities
    terminal choice
    platform-specific overrides

SESSION-LOCAL
    allocated ports
    temporary SSH configs
    active PTYs
    temporary environment
    one-time overrides
```

The exact placement of individual settings may evolve.

The important principle is to avoid synchronizing machine-specific paths and transient state.

---

# 24. Local Overrides

A device should be able to override selected synchronized properties without altering the shared vault.

Example synchronized host:

```text
Host: router
Address: router.customer.example
User: admin
```

A particular field laptop might use:

```text
Address override: 192.168.41.1
```

Other possible local overrides:

* alternative jump host,
* alternate address,
* local identity mapping,
* preferred terminal,
* local action program,
* local proxy settings.

The design should allow this without corrupting the portable configuration model.

---

# 25. Portable Vaults

Portable removable storage should be a first-class use case.

The application should be able to open a vault from an arbitrary directory.

Example:

```bash
app --vault /media/alice/SSHVAULT
app-tui --vault /media/alice/SSHVAULT
```

The GUI should offer something conceptually like:

```text
Open vault...
Create portable vault...
```

A recognizable vault manifest may allow detection of removable vaults.

Automatic detection should remain optional and unobtrusive.

---

# 26. Shared Trusted Workstation Scenario

A motivating use case is:

* a shared maintenance computer,
* several trusted users,
* each carrying their own removable vault.

Alice inserts a USB drive and opens her vault.

She sees:

* her hosts,
* her hierarchy,
* her usernames,
* her jumps,
* her actions,
* her identity references,
* and her host trust information.

She closes the vault and removes the drive.

Bob inserts his own vault and gets a completely different environment.

The shared application installation itself should not need to store their infrastructure configuration.

This is a major reason portable vault behavior should be considered early.

---

# 27. Portable Mode Privacy

Opening a vault from removable storage may optionally enable a stricter portable mode whose goal is:

> leave as little user-specific state as reasonably practical on the host computer.

Potentially avoid persisting:

* recently accessed hosts,
* search history,
* decrypted cache data,
* vault passphrases,
* generated SSH configuration,
* portable identity paths,
* action execution history.

Temporary session data should be cleaned up when the vault is locked or closed.

Perfect forensic erasure should not be promised.

The goal is practical reduction of residue, not impossible guarantees.

---

# 28. Portable `known_hosts`

A portable SSH environment may need portable host-key trust.

Using the shared machine's:

```text
~/.ssh/known_hosts
```

could cause trust state to leak between users.

The project should therefore consider a vault-specific known-host database.

Generated OpenSSH configuration can point to it.

This allows the user's remembered host fingerprints to travel with the vault.

The precise policy should be carefully designed so the application does not weaken OpenSSH host-key verification.

---

# 29. Optional Portable SSH Keys

The default design should prefer keeping private keys outside synchronized vault configuration.

However, users may intentionally want a fully self-contained portable environment.

A USB vault may optionally contain:

```text
vault configuration
+
SSH private key files
```

This should not be prohibited.

The application should still rely on OpenSSH's own private-key protection mechanisms.

A portable key may itself be encrypted with an OpenSSH passphrase.

An even stronger portable setup may use:

```text
USB drive:
    encrypted host configuration

hardware security key:
    SSH authentication identity
```

This keeps infrastructure metadata and authentication material physically separate.

---

# 30. Portable Mode Is Not Safe on an Untrusted Computer

Documentation must state this clearly.

An encrypted vault does not make an arbitrary compromised computer safe.

A malicious host machine can potentially:

* capture the vault passphrase,
* inspect decrypted configuration,
* intercept keystrokes,
* hijack SSH sessions,
* copy conventional private keys,
* or manipulate launched programs.

Portable vault mode is intended for:

> trusted or reasonably trusted machines

not arbitrary public computers.

Hardware-backed SSH identities reduce some risks but do not solve a compromised terminal environment.

---

# 31. Locking a Vault

The application should provide an explicit:

```text
Lock Vault
```

operation.

Potential behavior:

1. stop or detach managed sessions according to policy,
2. close active forwarding processes where appropriate,
3. remove temporary SSH configuration,
4. release decrypted vault material,
5. forget the passphrase,
6. clear vault-specific frontend state,
7. return to a no-vault-open screen.

Where the operating system permits it, a future:

```text
Lock and unmount
```

operation may be useful for removable storage.

---

# 32. Vault Encryption

The synchronized/portable host configuration should be encryptable using a common passphrase.

A strong conceptual design is:

```text
user passphrase
    ↓
memory-hard password KDF
    ↓
key-encryption key
    ↓
decrypt random vault key
    ↓
vault key encrypts actual records
```

This is preferable to deriving the encryption key directly for every record.

It allows the passphrase to be changed by rewrapping the vault key rather than re-encrypting the entire data set.

A likely password KDF would be something modern and memory-hard, such as Argon2id.

A modern authenticated encryption scheme should be used.

Exact cryptographic crate and format selection should be based on mature implementations and security review.

Do not invent custom cryptography.

---

# 33. Vault Metadata

An unencrypted vault manifest may need to contain non-secret metadata such as:

```text
format identifier
format version
KDF algorithm
KDF parameters
salt
encrypted vault key
```

It should not expose host inventory.

The manifest should allow the program to determine how to unlock and migrate the vault.

---

# 34. Vault Versioning

The vault format must be versioned from the beginning.

Even experimental versions should explicitly identify the format.

Example:

```text
vault_format = 1
```

or equivalent.

The project may remain in active use for many years.

Schema migration should therefore be considered part of the architecture rather than an afterthought.

A good migration system should ideally:

* detect the current format,
* preserve backups,
* migrate safely,
* fail without corrupting the original,
* and report what changed.

---

# 35. Synchronization Philosophy

The project should support synchronization without implementing a proprietary synchronization service.

The application should define **what synchronization means**, not necessarily how bytes reach another machine.

Possible external transports include:

* Syncthing,
* Nextcloud,
* Dropbox,
* rsync,
* Git,
* SMB,
* network storage,
* USB drives,
* scripts,
* or tools not yet invented.

The project should remain agnostic.

---

# 36. Synchronization Semantics Before Transport

Do not begin by creating provider integrations.

First define standardized semantics for:

```text
create
update
delete
move
rename
conflict
tombstone
device identity
operation identity
```

If these semantics are robust, many external tools can move the data without the application needing to know how.

---

# 37. Append-Only Operation Log

A promising synchronization representation is an append-only per-device operation journal.

Conceptually:

```text
vault/
├── manifest
├── snapshot
└── devices/
    ├── device-a/
    │   ├── 00000001.op
    │   ├── 00000002.op
    │   └── 00000003.op
    │
    └── device-b/
        ├── 00000001.op
        └── 00000002.op
```

Each device only appends operations under its own identity.

External synchronization tools then merge files rather than trying to coordinate writes into one database file.

This can dramatically reduce synchronization conflicts.

The exact file layout may differ.

---

# 38. Sync Operation Model

An operation might conceptually contain:

```text
operation_id
device_id
sequence_number
timestamp
entity_type
entity_id
operation_type
payload
```

Possible operation types:

```text
create
update
delete
move
```

The design should avoid relying solely on wall-clock timestamps because computer clocks may differ.

Stable operation IDs and device-local monotonic sequence values are likely useful.

---

# 39. Deletion and Tombstones

Deletion must synchronize correctly.

Physically removing an entity without recording the deletion could allow an old offline device to later reintroduce it.

Deletes should therefore be represented as tombstones or equivalent durable operations.

An old laptop returning after six months should not resurrect hosts that were intentionally deleted.

---

# 40. Conflict Handling

Two devices may edit the same host while offline.

The project does not necessarily require sophisticated CRDT machinery.

However, conflicts should be:

* deterministic,
* detectable,
* recoverable,
* understandable,
* and never silently destructive.

Possible strategies include:

* field-level last-writer rules,
* operation ordering,
* explicit conflict objects,
* preserved previous versions,
* or combinations of these.

The initial strategy should be kept as simple as possible while remaining safe.

Retaining enough history to explain what happened may be more valuable than attempting invisible cleverness.

---

# 41. Snapshots and Compaction

An append-only operation history will eventually grow.

The vault may periodically generate compact snapshots.

Conceptually:

```text
snapshot + recent operation journal
```

Older operations may eventually be compacted after sufficient safety guarantees exist.

Compaction must not break multi-device synchronization.

It is acceptable for early versions to postpone aggressive compaction.

Configuration data is small.

Correctness matters more than saving a few megabytes.

---

# 42. USB Storage Considerations

The vault should perform well on inexpensive USB 2.0 flash storage.

Host configuration data is small, so throughput is unlikely to be the main issue.

More important considerations include:

* reducing unnecessary writes,
* atomic updates,
* crash recovery,
* unexpected device removal,
* filesystem compatibility,
* and partial-write detection.

An append-oriented design is attractive partly because it minimizes rewrites.

---

# 43. Filesystem Portability

Portable vaults may live on filesystems such as exFAT.

Correctness should not depend on:

* Unix permissions,
* symlinks,
* hard links,
* extended attributes,
* case-sensitive names,
* inode behavior,
* or POSIX-only locking semantics.

Opaque UUID-based filenames are preferable to filenames derived from host names.

Application-level validation and locking should be used where needed.

---

# 44. GUI

The desktop GUI is expected to use a lightweight desktop stack, likely based on Tauri.

The precise frontend framework is not fixed.

The GUI should remain visually modest and highly responsive.

A possible layout:

```text
┌──────────────────────┬────────────────────────────────────────┐
│ Search...            │ server-a │ router │ build-box          │
├──────────────────────┼────────────────────────────────────────┤
│ ▼ Customers          │                                        │
│   ▼ Acme             │                                        │
│      production-1    │              TERMINAL                  │
│      production-2    │                                        │
│      router           │                                        │
│   ▶ Widget Corp       │                                        │
│                      │                                        │
│ ▼ Home               │                                        │
│      nas              │                                        │
│      raspberry-pi     │                                        │
├──────────────────────┴────────────────────────────────────────┤
│ root@production-1 · via bastion · key: acme                  │
└───────────────────────────────────────────────────────────────┘
```

The actual design may differ substantially.

---

# 45. GUI Philosophy

The GUI should avoid unnecessary visual weight.

Avoid:

* huge cards,
* oversized icons,
* decorative dashboards,
* splash screens,
* animated backgrounds,
* promotional panels,
* account-centric UI,
* graphical server illustrations,
* heavy transition effects,
* and excessive empty space.

The goal is not ugliness.

The goal is density, clarity, speed, and restraint.

Once an SSH session is open, the application should mostly get out of the user's way.

---

# 46. GUI Terminal

The GUI will need a terminal emulator because a WebView is not a native terminal.

A likely architecture is:

```text
terminal component
    ↕
thin IPC/streaming bridge
    ↕
Rust PTY management
    ↕
system ssh
```

xterm.js is one reasonable candidate.

A native PTY crate may be used where appropriate.

Exact library choice should be evaluated for portability, correctness, maintenance status, and performance.

---

# 47. Terminal I/O Must Have a Narrow Hot Path

Terminal output can be high-volume.

Do not route PTY bytes through heavyweight frontend application state if avoidable.

Keyboard input and terminal output should use the most direct streaming mechanism reasonably available.

Host-tree state and terminal byte streams are fundamentally different workloads.

Do not make each terminal chunk trigger expensive global frontend reactivity.

---

# 48. TUI

The TUI should be a full interface to the same vault.

A likely Rust terminal UI stack may involve tools such as Ratatui and Crossterm, but this is not mandatory.

A conceptual screen:

```text
┌─ Hosts ────────────────────────┬─ Details ─────────────────────┐
│ ▼ Customers                    │ Acme / Production / web-01     │
│   ▼ Acme                       │                                │
│     ● production-1             │ Host: 10.0.0.20                │
│     ● production-2             │ User: root                     │
│     ● router                   │ Key: acme-ed25519              │
│                                │ Jump: bastion                  │
│ ▼ Home                         │                                │
│     ● nas                      │ [Enter] Connect                │
│     ● pi                       │ [a] Actions                    │
├────────────────────────────────┴────────────────────────────────┤
│ / search   e edit   n new   q quit                              │
└─────────────────────────────────────────────────────────────────┘
```

---

# 49. TUI Should Use the Real Terminal for SSH

The TUI does not necessarily need to embed a terminal emulator.

A preferable lightweight workflow may be:

```text
TUI
    ↓ user selects host
suspend alternate-screen UI
    ↓
launch ordinary ssh attached directly to terminal
    ↓
remote shell
    ↓ user exits
restore TUI
```

This allows:

* the user's terminal emulator,
* terminal capabilities,
* mouse support,
* Unicode handling,
* scrollback,
* and existing terminal behavior

to continue working naturally.

It also keeps the TUI extremely lightweight.

---

# 50. tmux Integration

Users who want many simultaneous terminal sessions may already use `tmux`.

The project should consider optional tmux integration rather than implementing a second terminal multiplexer.

Possible connection behaviors:

```text
Replace TUI until SSH exits
Open in current tmux session
Open in new tmux window
Open in external terminal
```

This should remain optional.

The application must not require tmux.

---

# 51. CLI

The CLI should be more than an implementation test.

It should become a useful scripting interface.

Possible commands:

```bash
app list
app search acme
app show acme/router
app connect acme/router
app action acme/server desktop
app forward acme/db postgres
app vault open ...
app vault status
app diagnose acme/router
```

Names are placeholders.

The CLI syntax should evolve based on usability.

---

# 52. Machine-Readable CLI Output

Where useful, CLI commands should support structured output.

Example:

```bash
app list --json
```

This allows integration with:

* shell scripts,
* `jq`,
* automation tools,
* launchers,
* and external utilities.

Human-readable output should remain the default.

---

# 53. Stable CLI Semantics

Because users may script against the CLI, command behavior should eventually become reasonably stable.

Backward compatibility matters more for the CLI than for graphical layout.

When breaking changes become necessary, they should be deliberate and documented.

---

# 54. No Mandatory Daemon

The initial architecture should not require a permanently running background service.

The application should function when invoked normally.

If a daemon later becomes useful for:

* persistent forwards,
* session handoff,
* notifications,
* or another clearly valuable feature,

it can be evaluated then.

Do not assume one from the start.

---

# 55. Sessions

Session state should be separate from synchronized configuration.

A session may include:

```text
host_id
resolved host settings
selected local identity
temporary SSH config
PTY/process handle
allocated ports
active forwards
action state
timestamps
```

Most of this should disappear when the session ends.

---

# 56. Host-Key Verification

The application must not weaken SSH host-key verification merely for convenience.

The default behavior should preserve ordinary OpenSSH security expectations.

If the application manages its own `known_hosts`, it should still expose meaningful first-connect and key-change warnings.

Dangerous bypasses such as ignoring host-key verification should not become casual UI conveniences.

If supported at all, they should be explicit and clearly risky.

---

# 57. Security Boundary Between Rust and Frontend

Sensitive data should remain outside the GUI frontend whenever practical.

Private keys should not be read into WebView JavaScript.

Vault passphrases should spend as little time as possible in frontend state.

Generated private operational data should stay primarily in Rust backend memory.

The GUI should receive only what it needs to present.

---

# 58. Secrets in Memory

The project should make reasonable efforts to minimize lifetime of:

* vault passphrases,
* decrypted vault keys,
* sensitive temporary values,
* and decrypted records.

Do not promise perfect memory erasure across all platforms.

Use established zeroization mechanisms where they provide practical benefit.

Avoid security theater.

---

# 59. Logging

Logs are useful for diagnosis but can leak infrastructure details.

Logging policy should distinguish between:

```text
normal logs
debug logs
sensitive diagnostic output
```

Avoid accidentally logging:

* passwords,
* private keys,
* decrypted secrets,
* or raw sensitive payloads.

Hostnames and usernames may themselves be sensitive in some environments.

Debug verbosity should be deliberate.

---

# 60. Export and Backup

Backup should be simple.

The vault should be ordinary files that can be copied using familiar tools.

Users should not need a proprietary export service to rescue their configuration.

Useful operations may include:

```text
Copy vault
Backup vault
Export decrypted configuration
Export sanitized configuration
```

Any decrypted export must make its security implications obvious.

---

# 61. Import

Import may eventually support:

* OpenSSH config,
* other versions of this application's vault,
* structured JSON/YAML formats,
* possibly common third-party SSH managers.

Import should not be allowed to distort the core model unnecessarily.

Support useful formats when practical.

Avoid spending excessive complexity chasing every proprietary competitor format.

---

# 62. Portability Targets

The Rust core, CLI, and TUI should aim for broad architecture support.

Desirable Linux targets include:

```text
x86-64
ARM64
ARMv7
32-bit x86
```

The exact minimum CPU level and libc requirements should be determined experimentally.

Support for additional architectures is welcome where low-cost.

---

# 63. GUI Architecture Support

The GUI may support a narrower target matrix because graphical dependencies such as the system WebView and toolkit determine what is practical.

A reasonable philosophy is:

```text
Core/CLI/TUI:
    support old or unusual hardware aggressively

GUI:
    support architectures where the native GUI dependency ecosystem is healthy
```

Do not compromise the core design merely to force identical GUI availability everywhere.

---

# 64. "Older Hardware" Means Modest Graphics Environment, Not Ancient Userspace

The target includes systems running modern Linux distributions with intentionally lightweight graphical environments.

Examples might include:

```text
modern Linux
+
Xorg
+
LXDE / LXQt / Openbox / XFCE
```

A machine does not need Wayland to be considered supported.

The project may reasonably require a modern enough kernel, libc, Rust-compatible userspace, and OpenSSH environment.

Supporting obsolete Linux distributions is not a primary goal.

---

# 65. TUI for Minimal Systems

The TUI should work comfortably on:

* X terminal emulators,
* Linux virtual consoles,
* SSH sessions,
* serial consoles where appropriate,
* framebuffer-only systems with a normal TTY,
* low-memory systems,
* Raspberry Pi-class devices.

This is a real product use case rather than merely an emergency fallback.

---

# 66. GUI Framework Restraint

The GUI should avoid frontend framework complexity disproportionate to the task.

A large enterprise web stack is probably unnecessary.

Use the smallest reasonable framework or approach that provides:

* maintainability,
* accessibility,
* keyboard handling,
* reliable state management,
* and good performance.

Do not choose technologies merely because they are fashionable.

---

# 67. Dependency Discipline

Rust and JavaScript projects can accumulate very large dependency graphs.

New dependencies should earn their place.

Especially in:

* the core,
* CLI,
* and TUI,

avoid adding large libraries for trivial functionality.

This does not mean "never depend on libraries."

Security-sensitive and complex functionality should absolutely use mature libraries.

The principle is:

> minimize accidental complexity, not dependencies at all costs.

---

# 68. Performance Measurement

Performance should be benchmarked on deliberately modest hardware.

Useful metrics include:

```text
Cold GUI launch time
Warm GUI launch time
TUI startup time
CLI startup time
Idle CPU
Idle memory
Memory with 1 terminal
Memory with 10 terminals
Host search latency
Render time for 1000+ hosts
Terminal throughput
Typing latency under heavy output
Vault open/unlock time
Vault save operation latency
```

Exact budgets can be established after prototypes exist.

Regression measurement is more important than pretending to know perfect thresholds now.

---

# 69. Fast Startup

The application should not perform unnecessary work before becoming interactive.

Avoid startup sequences involving:

* large migrations on every launch,
* plugin scanning,
* online update calls,
* telemetry initialization,
* unnecessary network requests,
* expensive global state reconstruction,
* or eagerly opening every stored host.

Load lazily where reasonable.

The host tree should appear quickly.

---

# 70. Failure Should Degrade Gracefully

Optional functionality should remain optional.

Examples:

If VNC is unavailable:

```text
SSH still works.
```

If a local identity cannot be resolved:

```text
the user can locate it.
```

If sync is not configured:

```text
the vault works locally.
```

If the GUI cannot run:

```text
the TUI and CLI remain usable.
```

If a fancy action fails:

```text
ordinary SSH remains available.
```

The application should avoid all-or-nothing architecture.

---

# 71. Diagnostics

Troubleshooting tools should be unusually good.

This audience often works on broken systems.

Possible diagnostic commands:

```bash
app diagnose host acme/router
app diagnose identity SHA256:...
app diagnose vault
app diagnose ssh
```

Useful diagnostic information might include:

* resolved host,
* resolved user,
* resolved identity,
* identity source,
* jump chain,
* generated SSH config,
* OpenSSH binary,
* version,
* relevant environment,
* forwarding definitions,
* local capability mappings,
* known-host file,
* operation-log status,
* and sync conflicts.

Secrets should be redacted.

---

# 72. Explainable Behavior

Whenever possible, the user should be able to answer:

> What exactly is the application doing?

Examples:

* show generated SSH options,
* show selected key fingerprint,
* show jump chain,
* show local forwarding port,
* show remote command being executed,
* show local viewer command,
* show synchronization conflict,
* show which local override is active.

This is a tool for technically capable users.

Transparency builds trust.

---

# 73. Keyboard-First Design

Both GUI and TUI should offer strong keyboard navigation.

Potential interactions:

```text
Ctrl+P     host search
Enter      connect/default action
Ctrl+T     new terminal/session
Ctrl+W     close session
e          edit
a          actions
f          forwards
/          search
```

Exact bindings should be chosen later and should respect platform conventions.

Mouse support is welcome.

Keyboard efficiency is essential.

---

# 74. Actions Palette

A selected host may expose:

```text
SSH
Desktop
Open Web UI
Start Tunnel...
Database
Edit Host
Copy Hostname
Diagnostics
```

The set may be defined by:

* built-in operations,
* host actions,
* folder inheritance,
* tags,
* or future extensibility.

Do not overengineer a plugin system prematurely.

---

# 75. Extensibility Without Early Plugin Complexity

It may eventually be desirable to support custom extensions.

However, a generic plugin architecture should not be an early requirement.

Actions may provide enough extensibility for many use cases.

A simple command-based action system can cover a surprising amount of ground without:

* plugin APIs,
* ABI compatibility,
* sandboxing,
* package stores,
* or extension lifecycle complexity.

---

# 76. Folder Defaults and Inheritance

A possible useful feature is folder-level defaults.

Example:

```text
Customers/Acme
    default username: support
    default identity: acme
    default jump: bastion-acme
```

Hosts underneath may inherit those values.

This could greatly reduce repetitive configuration.

However, inheritance can become difficult to reason about.

If implemented, the resolved result must remain inspectable.

The user should be able to see:

```text
username = support
source = inherited from Customers/Acme
```

This feature is promising but not mandatory for the first implementation.

---

# 77. Host Templates

Similarly, reusable templates may eventually help with repetitive host creation.

Example:

```text
Template: Customer Linux Server
    user = support
    port = 22
    action = Desktop
    tags = linux
```

Templates should not be confused with live inheritance unless intentionally designed that way.

This is a later feature.

---

# 78. Multiple Users on the Same Host

A single physical host may need several access profiles.

Example:

```text
database.example.com
    as admin
    as postgres
    as deploy
```

The domain model should not assume that hostname uniquely identifies an entry.

Each host profile is an access definition.

The user may choose whatever presentation makes sense:

```text
db-prod (root)
db-prod (postgres)
```

or separate actions/users within one host.

The architecture should remain flexible.

---

# 79. Aliases and Duplicate Endpoints

Different host records may legitimately refer to the same endpoint.

Examples:

* separate customer roles,
* separate SSH identities,
* alternate routes,
* different jump paths,
* different operational contexts.

Do not globally deduplicate records merely because hostname and port match.

---

# 80. Environment Portability

The broader concept is that the **access environment** should be portable.

A useful portability spectrum:

```text
Primary workstation
    local or synchronized vault
    local keys
    GUI/TUI/CLI

Travel laptop
    synchronized vault
    selected local keys
    GUI or TUI

Shared trusted workstation
    vault on USB
    local keys / USB keys / hardware key
    GUI/TUI/CLI

Minimal rescue machine
    vault on USB
    TUI or CLI
    OpenSSH
```

The architecture should support all of these without requiring separate data formats.

---

# 81. Open Source Philosophy

The project is intended to be published as open source.

Design decisions should therefore favor:

* transparent formats,
* local ownership,
* minimal lock-in,
* inspectable behavior,
* standard protocols,
* standard cryptography,
* and interoperability.

The project should not depend on infrastructure that only one vendor can operate.

---

# 82. Data Ownership

The user's host configuration belongs to the user.

They should be able to:

* back it up,
* copy it,
* move it,
* inspect exported versions,
* synchronize it with arbitrary tools,
* and migrate away from the application.

Encryption should protect ownership, not create lock-in.

---

# 83. Naming

This blueprint intentionally does not choose a final product name.

Temporary executable examples such as:

```text
app
app-tui
app-gui
```

are placeholders.

A final name should ideally communicate:

* SSH,
* portability,
* speed,
* navigation,
* hopping,
* or infrastructure access,

without sounding excessively enterprise-oriented.

Naming should not delay architecture work.

---

# 84. Non-Goals for the Initial Version

The following should probably not be initial priorities:

* integrated SFTP file manager,
* team account system,
* cloud synchronization service,
* collaboration features,
* credential sharing,
* AI assistant,
* remote monitoring dashboard,
* SSH server monitoring,
* metrics collection,
* embedded browser,
* embedded VNC implementation,
* embedded RDP implementation,
* proprietary SSH protocol stack,
* plugin marketplace,
* large snippet management system,
* infrastructure discovery crawler,
* automatic CMDB creation,
* remote package management,
* proprietary VPN functionality.

Some may eventually become useful.

They should not distract from the core.

---

# 85. Suggested Initial MVP

A strong first usable milestone could include:

## Core

* folder hierarchy,
* host records,
* usernames,
* ports,
* identity references,
* local identity resolution,
* OpenSSH execution,
* jump hosts,
* basic local storage.

## CLI

* list hosts,
* search hosts,
* show host,
* connect to host,
* inspect resolved configuration.

## TUI

* browse folders,
* fuzzy search,
* connect,
* basic create/edit/delete,
* return to TUI after SSH exits.

## GUI

* browse host tree,
* fuzzy search,
* edit hosts,
* tabs,
* integrated PTY terminal,
* connect via OpenSSH.

## Port forwarding

* local,
* remote,
* dynamic.

## Vault

* encrypted configuration,
* explicit format version,
* open from arbitrary path,
* portable vault support.

## Synchronization

* standardized create/update/delete operation semantics,
* filesystem-based operation journal,
* no proprietary transport.

This is already a substantial project.

It is acceptable to release useful subsets incrementally.

---

# 86. Suggested Second Phase

Once the fundamentals are stable:

* Actions,
* VNC action,
* automatic local-port allocation,
* machine capabilities,
* portable `known_hosts`,
* improved conflict handling,
* import from OpenSSH config,
* tmux integration,
* hardware-key UX,
* local overrides,
* folder defaults,
* detailed diagnostics,
* export/import tooling.

---

# 87. Testing Strategy

Testing should occur at several layers.

## Unit tests

For:

* model validation,
* path-independent UUID identity,
* vault serialization,
* crypto wrappers,
* operation merging,
* conflict handling,
* OpenSSH config generation,
* forward generation,
* identity fingerprint parsing.

## Integration tests

For:

* launching OpenSSH,
* jump hosts,
* temporary config cleanup,
* forwards,
* portable vault open/close,
* concurrent vault modifications,
* interrupted writes,
* removable-storage disappearance.

## End-to-end tests

For:

* CLI workflows,
* TUI workflows,
* GUI connection workflows.

A local container or lightweight SSH test server may be useful for automated testing.

---

# 88. Test Old and Slow Machines Deliberately

Do not optimize only on the developer workstation.

The project should be periodically run on representative modest systems.

Examples:

* low-power x86 laptop,
* ARM SBC,
* older dual-core laptop,
* system using Xorg and a lightweight desktop,
* terminal-only environment,
* removable USB 2.0 vault.

This should influence real decisions.

---

# 89. Packaging

Packaging should remain uncomplicated where possible.

Potential targets may include:

```text
CLI binary
TUI binary
GUI package
```

Distribution forms may include:

* distro packages,
* AppImage or equivalent where practical,
* archives,
* source builds,
* package repositories,
* release binaries.

No packaging format is mandatory at blueprint stage.

The core and TUI should remain easy to build independently of GUI dependencies.

---

# 90. Configuration Migration and Recovery

Before modifying a vault format, the application should ideally create or preserve a recoverable previous version.

Unexpected shutdown during migration should not destroy the vault.

A possible approach:

```text
vault
vault.backup-before-v2
```

or snapshot-based rollback.

Exact implementation may vary.

---

# 91. Corruption Detection

Encrypted authenticated records naturally provide some corruption detection.

The project should make failures clear.

Examples:

```text
operation 0000137 is truncated
record authentication failed
snapshot is valid through operation 0000136
```

The application should recover as much as safely possible without silently pretending corrupted state is valid.

---

# 92. Concurrent Access

A vault may accidentally be opened by two processes on the same machine.

The project should define behavior.

Possibilities:

* application-level lock,
* per-device append log allowing some concurrency,
* warning and read-only fallback.

Portable filesystems may not provide robust locking semantics.

The design should not depend entirely on POSIX advisory locks.

---

# 93. Device Identity

Synchronization likely requires each installation or writable vault client to have a stable device identifier.

Example:

```text
device UUID
human label: travel-laptop
```

The identifier should not contain personally identifying hardware data.

Users may benefit from seeing which device produced an operation.

Example:

```text
Host modified by:
travel-laptop
2026-08-24 10:32
```

Wall-clock timestamps should be presentation aids, not the sole ordering mechanism.

---

# 94. Device Removal

Users should be able to retire a device from synchronization.

Retirement should not immediately invalidate old operation history.

The system may need to distinguish:

```text
known device
active device
retired device
```

This is a later synchronization concern.

Do not overbuild it initially.

---

# 95. Passphrase Change

Changing a vault passphrase should ideally only re-encrypt the random vault key.

It should not require rewriting every encrypted record.

This is one reason for key wrapping.

The user should be encouraged to keep backups before cryptographic metadata changes.

---

# 96. Forgotten Passphrases

There should be no misleading recovery promise.

If the vault encryption is designed correctly and the passphrase is lost with no cached or alternate key, recovery may be impossible.

Documentation should state this clearly.

The project should not implement insecure recovery mechanisms merely for convenience.

---

# 97. OS Credential Stores

A future convenience feature may allow a device to remember the vault unlock key using the operating system's secure credential store.

This should be optional.

Portable mode should probably avoid persistent unlock storage by default.

The vault format should not depend on OS credential stores.

---

# 98. Cross-Platform Considerations

The core model should avoid unnecessary assumptions about:

* path separators,
* home directory structure,
* terminal emulator,
* executable naming,
* SSH binary path,
* filesystem case behavior.

Linux may be the initial implementation focus.

The design should avoid making future Windows or macOS support artificially difficult.

---

# 99. Windows

If Windows support is added, the project should preferably use standard OpenSSH available on modern Windows rather than immediately adopting a separate SSH implementation.

GUI and CLI may be straightforward.

TUI behavior should be tested carefully with Windows terminals and ConPTY.

Cross-platform support should remain practical rather than ideological.

---

# 100. macOS

macOS already provides OpenSSH and suitable terminal environments.

The portable vault and CLI/TUI architecture should translate naturally.

GUI packaging will require platform-specific work.

No special cloud integration should be necessary.

---

# 101. Accessibility

Keyboard-first design benefits accessibility but is not sufficient by itself.

The GUI should use semantic controls where practical.

Consider:

* screen reader labels,
* visible focus indicators,
* scalable text,
* high contrast,
* reduced-motion preferences.

Accessibility should not require a heavyweight UI.

---

# 102. Documentation

Documentation should explain both:

* what the UI does,
* and what SSH operation it corresponds to.

Examples:

```text
Local Forward
equivalent OpenSSH option:
-L
```

```text
Jump Host
implemented using:
ProxyJump / -J
```

This helps less-experienced users learn SSH rather than becoming dependent on opaque application terminology.

---

# 103. Security Documentation

Security documentation should explain:

* what is encrypted,
* what is not,
* where keys live,
* what synchronization exposes,
* portable-vault risks,
* shared-machine assumptions,
* host-key verification,
* password handling,
* and limitations of memory cleanup.

Avoid exaggerated security claims.

---

# 104. Development Philosophy for Coding Agents

Coding agents working on this project should prefer:

1. clear interfaces,
2. small understandable modules,
3. explicit state,
4. boring file formats,
5. standard protocols,
6. mature cryptography,
7. deterministic behavior,
8. inspectable commands,
9. low dependency overhead,
10. incremental implementation.

Do not prematurely invent:

* distributed systems,
* plugin runtimes,
* proprietary service abstractions,
* custom SSH protocols,
* complex reactive state layers,
* or elaborate frontend architecture.

Build the smallest correct version of each layer and measure it.

---

# 105. Preserve the Escape Hatch

A recurring project principle should be:

> The user should always be able to fall back to ordinary tools.

If the GUI connection fails, show enough information to try OpenSSH manually.

If an Action fails, the underlying forward and command should be understandable.

If synchronization is unavailable, the vault remains usable locally.

If the GUI cannot run, use the TUI.

If the TUI is unavailable, use the CLI.

If the application disappears entirely, exported host information should still be recoverable.

---

# 106. Example User Stories

## Story A: Consultant workstation

A consultant opens the GUI.

They type:

```text
acme db
```

and see:

```text
Customers / Acme / Production / db-01
```

Enter opens an SSH tab.

The application selects:

* username,
* key fingerprint,
* local identity,
* jump host,
* known-host file,

and invokes OpenSSH.

---

## Story B: Travel laptop

The same user carries a cheap ARM laptop.

They install only the TUI.

Their vault is synchronized using Syncthing.

They run:

```bash
app-tui
```

search for the same host and connect.

The host tree is identical to the workstation.

The local SSH key path differs, but the fingerprint resolves automatically.

---

## Story C: Shared maintenance machine

A trusted maintenance laptop has the application installed.

Alice inserts:

```text
ALICE_SSH
```

and opens the vault from USB.

She sees her own environment.

She performs several support operations.

She chooses:

```text
Lock Vault
```

and removes the USB device.

Bob inserts his own vault.

No account switching is required.

---

## Story D: VNC

The user selects:

```text
Customer / signage-controller
```

and chooses:

```text
Desktop
```

The Action:

1. SSHes into the host,
2. launches or prepares the remote VNC service,
3. allocates an available local port,
4. creates an SSH forward,
5. launches the configured VNC viewer,
6. cleans up when the viewer exits.

The operation feels like one action while remaining transparently composed from ordinary tools.

---

## Story E: Web admin

A router exposes an admin page only on:

```text
127.0.0.1:8080
```

The host has an Action:

```text
Open Web UI
```

The application:

1. opens an SSH local forward,
2. allocates a free port,
3. launches the browser,
4. closes the tunnel when appropriate.

No special embedded browser is required.

---

## Story F: Minimal rescue machine

A user has:

* a modern Linux console-only machine,
* OpenSSH,
* the CLI/TUI,
* and a USB vault.

They do not need a desktop environment.

They can still access the full organized host configuration.

---

# 107. Example Conceptual Commands

These are illustrative only.

```bash
app list
```

```bash
app search "acme database"
```

```bash
app connect acme/prod/db01
```

```bash
app connect --id 84f3...
```

```bash
app action acme/prod/db01 desktop
```

```bash
app forward acme/prod/db01 postgres
```

```bash
app show acme/prod/db01
```

```bash
app diagnose acme/prod/db01
```

```bash
app vault open /media/user/SSHVAULT
```

```bash
app vault lock
```

```bash
app identities
```

```bash
app identities resolve
```

---

# 108. Example Conceptual Host Record

This is only illustrative.

```text
Host {
    id: UUID
    folder_id: UUID

    display_name: "db-01"
    hostname: "10.50.10.21"
    port: 22
    username: "postgres"

    identity:
        fingerprint: "SHA256:..."
        preferred_name: "acme-prod"

    jump_chain:
        - host UUID of "acme-bastion"

    tags:
        - "production"
        - "database"

    forwards:
        - UUID

    actions:
        - UUID
}
```

The final serialized representation may be very different.

---

# 109. Example Conceptual Action

```text
Action {
    id: UUID
    name: "Desktop"

    remote_prepare:
        command: "systemctl --user start x11vnc"

    forwards:
        - type: local
          local_host: "127.0.0.1"
          local_port: auto
          remote_host: "127.0.0.1"
          remote_port: 5900

    local_launch:
        capability: "vnc-viewer"
        arguments:
            - "127.0.0.1::{LOCAL_PORT}"

    cleanup:
        command: "systemctl --user stop x11vnc"
}
```

Again, this is explanatory rather than normative.

---

# 110. Example Conceptual Identity Mapping

Synchronized:

```text
IdentityReference {
    id: UUID
    fingerprint: "SHA256:abc..."
    preferred_name: "customer-a"
}
```

Machine local:

```text
IdentityResolution {
    identity_id: UUID
    source: file
    path: "/home/alice/.ssh/customer-a"
}
```

Another machine:

```text
IdentityResolution {
    identity_id: UUID
    source: agent
}
```

---

# 111. Example Conceptual Sync Operation

```text
Operation {
    id: UUID
    device_id: UUID
    sequence: 172
    entity_type: host
    entity_id: UUID
    operation: update

    changes:
        hostname: "10.20.30.44"
}
```

Encrypted serialization may use JSON, CBOR, MessagePack, or another suitable format.

Do not choose based on novelty.

Choose based on:

* robustness,
* implementation quality,
* versioning support,
* debuggability where appropriate,
* and interoperability.

---

# 112. Questions the Implementation Should Answer Experimentally

Several parts of this blueprint should be validated through prototypes rather than debate.

Examples:

1. How quickly can the Tauri GUI cold-start on low-end hardware?
2. How much memory does the system WebView consume?
3. What PTY mechanism performs best across supported systems?
4. How directly can terminal bytes be streamed through Tauri?
5. How gracefully can the TUI suspend and restore around OpenSSH?
6. How well does key fingerprint discovery work across real user setups?
7. How should agent identities be mapped?
8. How reliable is the operation-journal approach under Syncthing?
9. What locking behavior works on exFAT?
10. What happens when a USB vault disappears mid-write?
11. What is the best practical conflict-resolution model?
12. What are realistic minimum glibc and CPU targets?
13. Is 32-bit x86 still practical for the chosen Rust dependency set?
14. Which GUI architectures have healthy WebKit/GTK packaging?
15. How should vault-specific `known_hosts` interact with existing OpenSSH config?

Experiments should be allowed to change the architecture.

---

# 113. Architectural Red Flags

A change should receive extra scrutiny if it introduces any of the following:

```text
mandatory account
mandatory proprietary server
large always-running daemon
frontend-specific business logic
private keys inside JavaScript
custom SSH protocol implementation without clear benefit
custom cryptography
opaque forwarding semantics
large cloud SDK in the core
single monolithic database file that synchronizes poorly
machine-specific paths in synchronized host records
mandatory network access on startup
heavy background indexing
GUI dependency in the core
Electron-like bundled browser stack without overwhelming justification
```

None is absolutely forbidden forever.

Each would require strong justification.

---

# 114. Success Criteria

The project is succeeding if users can truthfully say things such as:

> I keep hundreds of SSH systems organized without maintaining giant command histories.

> My work laptop and my small travel laptop use the same host setup.

> I synchronize it with whatever tool I already trust.

> The application does not care where my sync directory comes from.

> My SSH keys stay where I decide they should stay.

> I understand the tunnels the application creates.

> I can use the same configuration from GUI, terminal UI, or shell scripts.

> If the GUI is too heavy for a machine, the TUI still feels instant.

> If the project vanished tomorrow, I would still own my configuration and could recover it.

> It feels like a better way to use OpenSSH rather than a proprietary replacement for OpenSSH.

---

# 115. Final Guidance

The essential spirit of the project is more important than any specific implementation described above.

Preserve these qualities:

* **fast,**
* **local-first,**
* **portable,**
* **transparent,**
* **OpenSSH-friendly,**
* **keyboard-efficient,**
* **low-overhead,**
* **cross-platform where practical,**
* **usable from modest hardware,**
* **independent of proprietary infrastructure,**
* **and respectful of existing system tools.**

The application should feel like something an experienced Unix user might wish had always existed.

It should make complex SSH environments easier to carry and navigate without taking control away from the user.

Whenever implementation choices conflict with this blueprint, prefer the choice that better serves the actual user and the long-term project.

This document is a starting map, not a contract.

