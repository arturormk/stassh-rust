# BLUEPRINT-secrets.md

## Stassh Host-Associated Secrets

### Status and Intent of This Document

This document describes the initial concept for adding a small encrypted secrets facility to Stassh.

It is intended primarily as guidance for the coding agent implementing the feature.

This document is **descriptive, not prescriptive**.

The implementation may diverge from the structures, command names, file layouts, cryptographic primitives, UI details, or workflows described here whenever development experience suggests a better approach.

Changes are encouraged if they make the feature:

* safer,
* simpler,
* easier to audit,
* more maintainable,
* more portable,
* faster,
* easier to use,
* or better aligned with the rest of Stassh.

However, deviations should continue to respect the security concerns and intended usage model described here.

The important goals are:

1. secrets must be encrypted appropriately at rest;
2. plaintext secrets should exist only when actually needed;
3. the master password should not be persisted;
4. ordinary Stassh operation should not require the secrets store to be unlocked;
5. normal password reveal should require deliberate user action;
6. the CLI and TUI must share the **same Rust implementation** of secrets functionality rather than developing independent implementations that can diverge.

---

# 1. Background

Stassh already provides a portable model for describing SSH hosts and accessing them through multiple frontends.

The project includes or is intended to include:

* `stassh`, the CLI;
* `stassh-tui`, the terminal user interface;
* eventually a graphical frontend.

A representative workflow is:

```text
stassh-tui
    ↓
select host
    ↓
connect
    ↓
ordinary OpenSSH session
```

Stassh already knows information such as:

* the target host,
* SSH username,
* SSH identity,
* possibly folders and other metadata.

There is a real-world maintenance problem that Stassh could solve with a very small additional feature.

A user may SSH into a host using an SSH key and then need to:

```bash
su admin
```

or perform some other administrative action requiring a password.

The user may normally know that password but occasionally forget it.

The password may also be shared among a family of related systems, such as every server or device installed at a particular customer site or shopping centre.

The user wants to be able to leave the SSH session, return to Stassh with the host still selected, inspect secrets associated with that host, explicitly reveal the required password, remember or copy it, and connect again.

---

# 2. This Is a Fallback Facility, Not a Full Password Manager

The intended secrets feature is deliberately modest.

It is **not intended to turn Stassh into a general password manager**.

The user may already maintain authoritative credentials elsewhere, for example:

* a password manager on a phone;
* KeePassXC;
* an encrypted spreadsheet;
* a corporate credential-management system;
* another encrypted notebook or database.

The Stassh secrets facility is primarily a convenient operational fallback.

The intended mental model is:

> "I am working on this host and I need a piece of information associated with it that I do not currently remember."

In the primary motivating case, that information is an administrative password.

The facility may also prove useful for related per-site information, hence the deliberately generic `secrets.json` model.

---

# 3. Expected Human Workflow

The intended normal workflow is:

```text
stassh-tui
    ↓
select tsoviladecans01
    ↓
connect via SSH
    ↓
remote shell
    ↓
su admin
    ↓
Password:
```

The user cannot remember the password.

They then:

```text
exit SSH
    ↓
return to stassh-tui
    ↓
tsoviladecans01 remains selected
    ↓
open Secrets
    ↓
select "password"
    ↓
Reveal
    ↓
enter Stassh secrets master password
    ↓
password is decrypted and displayed
    ↓
user memorizes or copies it
    ↓
password is hidden again
    ↓
reconnect via SSH
    ↓
type the password normally
```

Stassh does **not** need to:

* detect the remote password prompt;
* understand `sudo`;
* understand `su`;
* automatically answer prompts;
* inject passwords into PTYs;
* perform privilege escalation;
* monitor terminal output looking for password requests.

This simplification is intentional.

---

# 4. Human Memory Is Part of the Usage Model

The secrets facility should not be optimized around the assumption that the user will retrieve every password every time.

In the intended use case, passwords may be designed to be reasonably memorable.

For example, the user may use pronounceable random passwords and may normally remember the password associated with a particular customer site after using it recently.

A Reveal operation is therefore expected to be an occasional fallback.

This is one reason why requiring the master password for every Reveal operation is considered acceptable.

The design does **not** need to optimize aggressively for:

```text
unlock once
→ reveal dozens of passwords during normal work
```

The normal operational model is closer to:

```text
forgot one password
→ deliberately reveal it
→ remember it
→ continue working
```

---

# 5. High-Level Data Model

The proposal introduces a separate file:

```text
secrets.json
```

The existing Stassh host configuration remains in:

```text
vault.json
```

A host in `vault.json` may optionally reference a named secrets set.

Conceptually:

```json
{
  "name": "tsoviladecans01",
  "hostname": "tsoviladecans01",
  "user": "directorio",
  "secrets": "tso-viladecans"
}
```

Several hosts may reference the same set:

```text
tsoviladecans01 ─┐
tsoviladecans02 ─┤
tsoviladecans03 ─┼──> tso-viladecans
tsoviladecans04 ─┘
```

This is important because operational passwords are frequently shared across a group of similar hosts.

---

# 6. Named Sets of Arbitrary Key/Value Data

`secrets.json` should contain named sets.

Each set contains arbitrary fields.

Conceptually:

```text
tso-viladecans
    admin_user       = directorio
    password         = <encrypted>
    root_user        = root
    root_password    = <encrypted>
    note             = Common credentials for this site
```

Another set might be:

```text
plaza-estacion
    admin_user       = mantenimiento
    password         = <encrypted>
    vnc_user         = soporte
    vnc_password     = <encrypted>
    note             = VNC is used only on the signage controller
```

The secrets system should therefore not be hard-coded around a rigid structure such as:

```text
username + password
```

A field is simply:

```text
key → value
```

where the value may be either:

* ordinary plaintext metadata;
* or an encrypted secret.

---

# 7. Do Not Infer Secrecy from the Field Name

The initial idea considered treating fields with names such as:

```text
_password
```

or perhaps any key starting with `_` as secret.

This should preferably **not** be done.

Security semantics should not depend on naming conventions.

For example, all of these may legitimately need encryption:

```text
password
root_password
enable_password
vnc_password
pin
api_token
recovery_code
```

A field should be secret because its **stored value says that it is secret**, not because the field happens to have a special spelling.

This also means renaming a field cannot accidentally convert it between encrypted and plaintext storage.

---

# 8. Plain and Secret Field Representations

A plaintext field may serialize simply as:

```json
"admin_user": "directorio"
```

An encrypted field should serialize as an explicit structured value.

Conceptually:

```json
"password": {
  "type": "secret",
  "nonce": "...",
  "ciphertext": "..."
}
```

The exact schema may evolve.

A complete set might therefore resemble:

```json
{
  "label": "TSO Viladecans",
  "fields": {
    "admin_user": "directorio",

    "password": {
      "type": "secret",
      "nonce": "...",
      "ciphertext": "..."
    },

    "root_user": "root",

    "root_password": {
      "type": "secret",
      "nonce": "...",
      "ciphertext": "..."
    },

    "note": "Common credentials for this site"
  }
}
```

The important distinction is semantic:

```text
JSON string
    = ordinary field

structured encrypted value
    = secret field
```

---

# 9. `password` Is Not Intrinsically Special in the File Format

The motivating field will normally be called:

```text
password
```

but the file format should preferably not make that name cryptographically special.

Instead, the CLI may allow any field to be created as a secret.

For example:

```text
secret password
secret root_password
secret api_token
```

This provides a generic representation without requiring Stassh to become a generic password manager.

The normal TUI presentation can still prioritize common names such as `password` if doing so proves useful.

---

# 10. Separation of Metadata and Secrets

Ordinary metadata should remain available without entering the master password.

For example:

```text
admin_user      directorio
note            Common admin account for this location
password        [secret]
```

The first two values can be displayed immediately.

Only the encrypted value requires explicit Reveal.

This minimizes unnecessary decryption and keeps the secrets feature useful as an operational reference.

---

# 11. Proposed `secrets.json` Structure

A conceptual format might look something like:

```json
{
  "version": 1,

  "store_id": "9ee3d2aa-0000-0000-0000-000000000000",

  "crypto": {
    "kdf": "argon2id",
    "salt": "...",
    "memory_kib": 65536,
    "iterations": 3,
    "parallelism": 1
  },

  "key_check": {
    "nonce": "...",
    "ciphertext": "..."
  },

  "sets": {
    "tso-viladecans": {
      "label": "TSO Viladecans",

      "fields": {
        "admin_user": "directorio",

        "password": {
          "type": "secret",
          "nonce": "...",
          "ciphertext": "..."
        },

        "note": "Common credentials for this site"
      }
    }
  }
}
```

This example is explanatory only.

The implementation is free to choose a better serialization format or schema.

The properties that matter are:

* explicit format versioning;
* stable store identity if useful;
* KDF parameters stored with the file;
* encrypted secret values;
* authenticated encryption;
* named sets;
* arbitrary fields;
* clear distinction between plaintext and encrypted values.

---

# 12. Encryption Model

The master password should **not** be used directly as an encryption key.

It should be processed through an appropriate password-based key derivation function.

A likely conceptual model is:

```text
master password
      +
random store salt
      ↓
memory-hard KDF
      ↓
derived encryption key
```

Argon2id is a strong mainstream candidate.

The exact parameters should be configurable in the file and should be selected with real performance measurements on Stassh's target hardware.

Because Stassh intentionally supports modest machines, KDF settings should balance:

* resistance to offline password guessing;
* memory requirements;
* and reasonable interactive latency.

Do not choose weak settings merely to make unlock instantaneous.

Do not choose arbitrarily expensive settings that make the TUI painful on supported low-end hardware.

Benchmark.

---

# 13. Mainstream Rust Cryptography

The implementation should use established mainstream Rust cryptography crates rather than implementing cryptographic primitives directly.

Reasonable implementation candidates include RustCrypto ecosystem crates such as:

* `argon2` for Argon2id password-based key derivation;
* `chacha20poly1305` for authenticated ChaCha20-Poly1305 / XChaCha20-Poly1305 encryption;
* `zeroize` for deliberate clearing of sensitive buffers.

These are implementation candidates, not immutable requirements. The coding agent should evaluate current stable APIs, compatibility with Stassh's MSRV and architecture targets, audit history, maintenance status, and dependency impact before selecting them. The RustCrypto `argon2` crate supports Argon2id, `chacha20poly1305` provides AEAD implementations including XChaCha20-Poly1305, and `zeroize` provides explicit memory-zeroing primitives.

Other well-maintained crates may be preferable if there is a concrete reason.

Security-sensitive functionality should favor mature, auditable implementations over cleverness or minimal dependency count.

---

# 14. Authenticated Encryption Is Required

Secret values should use an authenticated encryption scheme.

Encryption alone is not sufficient.

The implementation should be able to detect:

* incorrect master passwords;
* corrupted ciphertext;
* modified ciphertext;
* invalid authentication tags.

An AEAD construction is therefore desirable.

XChaCha20-Poly1305 is one candidate because it provides authenticated encryption and supports large nonces that are convenient for generating independently random nonces per encrypted record. The current RustCrypto `chacha20poly1305` crate provides XChaCha20-Poly1305 support.

The exact algorithm may change if a superior mainstream choice better fits the project.

Do not invent custom encryption.

---

# 15. Encrypt Secret Fields Independently

Each encrypted field should be encrypted independently.

Do **not** decrypt the entire `secrets.json` contents simply because one password needs to be revealed.

Conceptually:

```text
secrets.json loaded
    ↓
plaintext metadata available
    ↓
all passwords remain ciphertext
    ↓
user chooses one secret
    ↓
derive key
    ↓
decrypt only that field
```

This means the program can keep the full parsed secrets structure in memory without simultaneously holding every plaintext password.

Only the selected secret should need to exist in plaintext during a Reveal operation.

---

# 16. Per-Secret Nonces

Every independently encrypted secret field should use a fresh nonce appropriate for the chosen AEAD.

Conceptually:

```text
derived key + nonce A → secret A
derived key + nonce B → secret B
derived key + nonce C → secret C
```

Nonce generation must use a cryptographically secure randomness source.

Never reuse a nonce where the selected encryption scheme requires uniqueness.

The implementation should rely on the cryptographic crate's intended APIs rather than manually constructing unsafe nonce-management mechanisms.

---

# 17. Authenticated Context / Associated Data

If supported cleanly by the chosen AEAD implementation, encrypted secret fields should be cryptographically bound to their logical context using authenticated associated data.

For example, the associated data might conceptually include:

```text
format version
store UUID
set key
field key
```

Example:

```text
stassh-secrets-v1
9ee3d2aa-...
tso-viladecans
password
```

This means moving ciphertext from:

```text
tso-viladecans/password
```

to:

```text
plaza-estacion/password
```

would cause authentication failure instead of silently producing a valid secret in the wrong location.

The exact encoding of associated data must be deterministic and unambiguous.

Do not use an ambiguous concatenation scheme.

---

# 18. Master Password Verification

`secrets.json` should preferably contain a small encrypted verification record.

Conceptually:

```json
"key_check": {
  "nonce": "...",
  "ciphertext": "..."
}
```

This could be an authenticated encryption of a fixed internal value such as:

```text
STASSH-SECRETS-KEY-CHECK-v1
```

The purpose is to verify that a supplied master password derives the correct key.

Workflow:

```text
master password
    ↓
KDF
    ↓
derived key
    ↓
decrypt/authenticate key_check
    ↓
success → key is valid
failure → incorrect password or corrupted store
```

This is especially important before modifying the secrets file.

Without verification, a mistyped master password could theoretically be used to encrypt a newly added secret under the wrong key, making the store internally inconsistent.

---

# 19. Do Not Store the Master Password

The master password must not be persisted in:

* `secrets.json`;
* `vault.json`;
* `.stassh-local.json`;
* environment files;
* temporary files;
* logs;
* shell history;
* process arguments.

The file may contain:

* a salt;
* KDF parameters;
* encrypted key-check material.

It should not contain the master password itself.

---

# 20. Master Password Strength

Because `secrets.json` is portable and may be stolen, an attacker with a copy of the file can attempt offline guesses against the master password.

No file format can prevent this entirely.

The defenses are:

* a strong master password;
* an expensive memory-hard KDF;
* appropriate KDF parameters.

The UI/documentation should encourage a good master password.

Do not provide insecure "password recovery" mechanisms that undermine encryption.

If the user loses the master password, the encrypted secrets may become unrecoverable.

That is expected behavior for a properly encrypted store.

---

# 21. Normal TUI Reveal Policy

The normal `stassh-tui` workflow should **not keep the secrets store unlocked**.

Every Reveal operation should ask for the master password.

Conceptually:

```text
select encrypted field
    ↓
Reveal
    ↓
Master password: ********
    ↓
derive key
    ↓
verify key_check
    ↓
decrypt selected field
    ↓
display value
```

After the reveal is finished:

* the plaintext should be discarded;
* the derived key should be discarded;
* sensitive buffers should be zeroized where practical.

The next Reveal should ask for the master password again.

This intentionally avoids needing:

* an unlocked/locked secrets state machine;
* idle timers;
* session-key caches;
* auto-lock logic;
* background secret management.

---

# 22. Why Reveal Requires the Master Password Every Time

This is not considered excessive friction for the intended use case.

Reveal is expected to be relatively uncommon.

The user may normally remember the password.

The user may also have a primary password manager elsewhere.

The Stassh secrets facility is a fallback.

Therefore:

```text
Reveal
→ deliberate authentication
→ temporary plaintext
```

is preferable to maintaining long-lived decrypted state merely for convenience.

---

# 23. TUI Presentation

With a host selected, the TUI should provide a Secrets action if that host references a secrets set.

The exact key binding is up to the TUI design.

A conceptual screen:

```text
┌─ Secrets · tsoviladecans01 ─────────────────────┐
│                                                 │
│ TSO Viladecans                                  │
│                                                 │
│ admin_user       directorio                     │
│ password         ••••••••••••••                 │
│ root_user        root                           │
│ root_password    ••••••••••••••                 │
│ note             Common admin credentials       │
│                                                 │
│ Enter reveal   c copy   Esc close               │
└─────────────────────────────────────────────────┘
```

Plain fields may be displayed normally.

Encrypted fields must remain masked until explicitly revealed.

---

# 24. Revealing a Secret

Selecting an encrypted field and choosing Reveal should result in a secure master-password prompt.

Conceptually:

```text
┌─ Reveal Secret ────────────────────────────────┐
│                                               │
│ Master password: ***************               │
│                                               │
│ Enter reveal · Esc cancel                     │
└───────────────────────────────────────────────┘
```

After successful authentication:

```text
password    Example-Pronounceable-Password-73
```

The UI should make it easy to hide the value again.

Leaving the secrets view should always hide plaintext secrets.

A timeout for visible plaintext may be considered later, but is not essential to the initial design.

---

# 25. Copying Secrets

The TUI may optionally support copying a revealed secret to the system clipboard.

This is useful when the user does not intend to memorize it.

However, clipboard handling has security limitations:

* clipboard managers may retain history;
* other applications may observe the clipboard;
* automatic clearing does not guarantee removal from external clipboard histories.

Therefore:

* Reveal should remain a normal first-class workflow;
* Copy should be explicit;
* the application should not claim that clearing the clipboard guarantees erasure.

No clipboard functionality is required for the core cryptographic design.

---

# 26. No Automatic PTY Injection in the Initial Feature

The initial implementation should not automatically send secrets into SSH sessions.

Specifically, avoid adding complexity such as:

* remote prompt detection;
* sudo prompt detection;
* password injection;
* auto-fill;
* terminal-output parsing.

These may be reconsidered later if real-world usage justifies them.

They are not needed to solve the motivating problem.

---

# 27. `stassh secrets manage`

Normal operational reveal and secrets administration have different usage patterns.

For administration, repeatedly asking for the master password would be unnecessarily cumbersome.

The CLI should therefore provide an interactive maintenance mode:

```bash
stassh secrets manage
```

This command should:

1. locate the appropriate `secrets.json`;
2. prompt securely for the master password;
3. derive the encryption key;
4. verify it using the key-check record;
5. retain the derived key in protected/zeroizing memory for the lifetime of this explicit management session;
6. open a small purpose-built secrets management REPL;
7. discard/zeroize the key when the session ends.

This is an intentional exception to the "master password for every reveal" policy.

---

# 28. `manage` Is a Deliberate Trusted Maintenance Session

`stassh secrets manage` should be understood as:

> "I am intentionally maintaining my secrets database in a controlled environment."

This differs from ordinary field use on a maintenance laptop or remote site.

Therefore:

```text
normal TUI operation:
    every Reveal requires master password

explicit CLI management session:
    unlock once
    perform many CRUD operations
    exit
    lock
```

These policies are compatible because they serve different workflows.

---

# 29. Management REPL

A deliberately small interactive interface is preferable.

Conceptually:

```text
$ stassh secrets manage
Master password: ********

Secrets store unlocked.

stassh-secrets>
```

Possible commands:

```text
sets
create <set>
delete-set <set>
rename-set <old> <new>
use <set>

list
get <field>
set <field> <value>
secret <field>
reveal <field>
delete <field>

help
exit
```

These names are illustrative rather than requirements.

The command language should remain small and predictable.

---

# 30. Example Management Session

```text
$ stassh secrets manage
Master password: ********

Secrets store unlocked.

stassh-secrets> sets
plaza-estacion
tso-viladecans

stassh-secrets> use tso-viladecans
Using: tso-viladecans

stassh-secrets:tso-viladecans> list
admin_user       directorio
password         [secret]
root_user        root
root_password    [secret]
note             Common credentials

stassh-secrets:tso-viladecans> set admin_user administrador

stassh-secrets:tso-viladecans> secret password
New secret value:
Repeat secret value:
Updated password.

stassh-secrets:tso-viladecans> reveal password
Example-Pronounceable-Password-73

stassh-secrets:tso-viladecans> delete root_password
Deleted root_password.

stassh-secrets:tso-viladecans> exit
Secrets locked.
```

---

# 31. Secret Values Must Not Be Typed as REPL Arguments

This should preferably be avoided:

```text
stassh-secrets> set password MySecretPassword
```

when the value is intended to be encrypted.

Instead:

```text
stassh-secrets> secret password
New secret value:
Repeat secret value:
```

The secret input should use a no-echo terminal prompt.

This prevents secret values from casually appearing in:

* REPL history;
* terminal scrollback as part of the command;
* debug output;
* accidental command logging.

---

# 32. Plain Fields May Use Ordinary Arguments

Plain metadata can remain convenient.

For example:

```text
stassh-secrets:tso-viladecans> set admin_user directorio
```

or:

```text
stassh-secrets:tso-viladecans> set note "Shared credentials for the local servers"
```

There is no need to treat non-secret strings as confidential.

---

# 33. `manage` Should Not Become a General Shell

The management REPL should remain purpose-built.

Avoid features such as:

```text
!
shell
exec
arbitrary external command execution
```

unless a compelling later requirement exists.

During `manage`, the process holds the derived secrets key.

Reducing unnecessary attack surface and unexpected execution paths is desirable.

The REPL should primarily provide CRUD operations over the secrets store.

---

# 34. Ctrl+C and Ctrl+D Behavior

A reasonable terminal behavior would be:

```text
Ctrl+C
    cancel current input/operation

Ctrl+D
    exit management session
```

On exit:

* secret plaintext buffers should be dropped/zeroized;
* the derived master key should be dropped/zeroized;
* the application should clearly report that the secrets session is locked.

---

# 35. Creating the Initial `secrets.json`

`stassh secrets manage` should preferably handle a missing secrets store gracefully.

Example:

```text
$ stassh secrets manage

No secrets store exists at:
    /path/to/secrets.json

Create it? [Y/n] y

New master password:
Repeat master password:

Secrets store created.

stassh-secrets>
```

Creation should generate:

* file format version;
* store ID if used;
* random KDF salt;
* KDF parameters;
* initial derived key;
* encrypted key-check record;
* empty set collection.

The user may then populate the file interactively.

This removes the need for a separate mandatory initialization workflow.

A separate:

```bash
stassh secrets init
```

command may still be useful if it fits the CLI architecture.

---

# 36. CRUD Through Normal CLI Commands

Although `manage` may become the primary human administration interface, the shared core should also make individual operations available so the `stassh` CLI can expose them if useful.

Possible examples:

```bash
stassh secrets set list
stassh secrets set add tso-viladecans
stassh secrets set delete tso-viladecans
stassh secrets field list tso-viladecans
```

For a secret field:

```bash
stassh secrets field set tso-viladecans password --secret
```

should prompt securely for:

```text
Master password:
Secret value:
Repeat secret value:
```

The exact CLI syntax is not important at blueprint stage.

---

# 37. Never Accept Secret Values as Ordinary CLI Arguments by Default

Avoid:

```bash
stassh secrets field set tso-viladecans password \
    --secret "ActualPassword"
```

because command-line arguments may be exposed through:

* shell history;
* process inspection;
* debugging tools;
* logs;
* scripts.

Secret values should normally be read through an echo-disabled terminal prompt.

If a future automation mode needs another input mechanism, it should be designed explicitly and documented with its security implications.

---

# 38. Host-to-Set Association

A host should have an optional reference such as:

```json
"secrets": "tso-viladecans"
```

The reference should identify the secrets set, not duplicate its fields.

A host without this property simply has no associated Stassh secrets.

The TUI should gracefully omit or disable the Secrets action for such hosts.

---

# 39. Set Identity

A set should have a stable key used for references.

Example:

```text
tso-viladecans
```

It may also have a presentation label:

```text
TSO Viladecans
```

The label may be freely changed without necessarily changing the stable key.

Alternatively, the implementation may use UUIDs internally if that better fits the existing Stassh model.

The critical point is that host associations must remain stable.

---

# 40. Renaming Sets

If set identifiers are user-visible keys and a set is renamed, host references may need updating.

This should be performed transactionally.

If stable UUIDs are used internally, the label/key can be changed more safely.

This is an implementation choice.

Prefer whatever integrates naturally with the existing Stassh host/vault model.

---

# 41. Shared Rust Core Is Mandatory Architecturally

The secrets feature must live in the shared Rust Stassh implementation.

Neither the CLI nor TUI should independently implement:

* `secrets.json` parsing;
* serialization;
* KDF handling;
* encryption;
* decryption;
* key verification;
* associated-data construction;
* secret resolution;
* CRUD semantics;
* rekeying;
* atomic file writing.

These behaviors belong in the reusable Stassh Rust library/core.

Conceptually:

```text
                       ┌────────────────────────┐
                       │    stassh Rust core    │
                       │                        │
                       │ SecretsStore           │
                       │ SecretSet              │
                       │ SecretValue            │
                       │ encryption/decryption  │
                       │ KDF                    │
                       │ CRUD                   │
                       │ rekey                  │
                       │ persistence            │
                       └───────────┬────────────┘
                                   │
                    ┌──────────────┴──────────────┐
                    │                             │
             ┌──────▼───────┐              ┌──────▼────────┐
             │ stassh CLI   │              │ stassh-tui    │
             │              │              │               │
             │ manage REPL  │              │ display       │
             │ CRUD         │              │ reveal        │
             │ rekey        │              │ copy          │
             └──────────────┘              └───────────────┘
```

The frontends should be thin consumers of the core API.

---

# 42. Why Shared Implementation Matters

Crypto and storage logic must not diverge between frontends.

A dangerous architecture would be:

```text
stassh CLI:
    one secrets parser
    one crypto implementation

stassh-tui:
    another parser
    another crypto implementation
```

That can lead to:

* incompatible files;
* different authentication behavior;
* different error handling;
* subtle cryptographic inconsistencies;
* duplicated security bugs.

Instead:

```text
stassh_core::secrets
```

or the equivalent existing shared crate/module should be the authoritative implementation.

---

# 43. Possible Shared Rust API

The exact API is up to the implementation, but conceptually the core may expose functionality similar to:

```rust
SecretsStore::open(...)
SecretsStore::create(...)

SecretsStore::list_sets(...)
SecretsStore::get_set(...)
SecretsStore::create_set(...)
SecretsStore::delete_set(...)
SecretsStore::rename_set(...)

SecretsStore::list_fields(...)
SecretsStore::set_plain_field(...)
SecretsStore::set_secret_field(...)
SecretsStore::delete_field(...)

SecretsStore::reveal_secret(...)
SecretsStore::rekey(...)
```

The crypto details should remain encapsulated.

Frontends should not need to manipulate:

* nonces;
* KDF parameters;
* ciphertext buffers;
* authentication tags.

---

# 44. Sensitive Types

It may be useful to model sensitive values with dedicated Rust types rather than ordinary `String` where practical.

Conceptually:

```rust
SecretString
DerivedKey
PlainSecret
```

These may use `Zeroizing<T>` or another suitable abstraction.

The purpose is to:

* make accidental logging harder;
* make secret handling obvious in code review;
* trigger zeroization on drop where practical.

Do not allow secret types to casually implement or derive:

```rust
Debug
Display
Serialize
Clone
```

unless there is a clear reason and safe behavior.

---

# 45. Avoid Accidental Logging

Secrets must not appear in:

* tracing spans;
* debug output;
* error messages;
* panic messages;
* diagnostic dumps;
* `Debug` derivations;
* serialized frontend events.

For example, avoid:

```rust
#[derive(Debug)]
struct RevealedSecret {
    password: String,
}
```

if debug formatting could leak the value.

Redaction should be the default.

---

# 46. Secret Input Handling

Master passwords and secret values should be read using appropriate terminal input mechanisms that disable echo.

The shared core may expose abstractions for handling secret values, but frontend-specific input belongs to the CLI/TUI layer.

The important boundary is:

```text
frontend obtains secret input securely
    ↓
passes sensitive value to core
    ↓
core performs crypto/storage
```

Do not pass the master password through command-line arguments.

---

# 47. Normal Reveal Core API

The TUI should conceptually do something like:

```text
user requests reveal
    ↓
TUI asks for master password
    ↓
TUI calls shared core reveal operation
    ↓
core:
    derive key
    verify key_check
    decrypt exactly requested field
    return sensitive value
    discard derived key
```

The TUI should not reproduce the KDF or encryption steps itself.

---

# 48. Management Session Core API

The `manage` command can use a long-lived unlocked context.

Conceptually:

```rust
let session = secrets_store.unlock(master_password)?;
```

The resulting context may expose CRUD and reveal methods using the already-derived key.

Example conceptual API:

```rust
UnlockedSecretsStore
    .create_set(...)
    .set_secret(...)
    .reveal(...)
    .delete(...)
```

When the unlocked object is dropped, its key material should be zeroized where practical.

This object should remain internal to the controlled management process.

---

# 49. Changing the Master Password

The CLI should provide a deliberate rekey operation.

Possible command:

```bash
stassh secrets rekey
```

or:

```bash
stassh secrets passwd
```

The exact name is not important.

Workflow:

```text
Current master password:
New master password:
Repeat new master password:
```

The current password must first be verified.

---

# 50. Rekey Semantics

Changing the master password may re-encrypt all secret fields.

A reasonable operation is:

```text
derive OLD key
derive NEW key using new random KDF salt

for each encrypted field:
    decrypt one field with OLD key
    immediately encrypt it with NEW key
    using a fresh nonce
    discard plaintext
```

The implementation should **not** need to hold every plaintext secret simultaneously.

At most one secret should normally need to exist as plaintext during the transformation.

---

# 51. Rekey Should Refresh Cryptographic Metadata

A rekey operation should preferably generate:

* a new random KDF salt;
* a new derived key;
* a new key-check ciphertext;
* fresh nonces for all re-encrypted secret fields.

Rekeying may also be a convenient opportunity to update KDF parameters if the stored parameters are obsolete.

For example:

```text
old store:
    older Argon2id settings

rekey:
    new password
    newer recommended parameters
```

Migration behavior should remain explicit and testable.

---

# 52. Rekey Must Be Transactional

Never modify the only copy of `secrets.json` destructively in place.

A safer conceptual process:

```text
1. read original secrets.json
2. verify old master password
3. construct completely re-encrypted new representation
4. write secrets.json.new
5. flush appropriately
6. reopen and validate new representation
7. optionally verify/decrypt records
8. atomically replace original where supported
```

If anything fails before replacement:

```text
original secrets.json remains valid
```

This matters even if rekey normally occurs on a trusted workstation.

Filesystems fail.

Processes crash.

USB storage can disappear.

Power can be lost.

---

# 53. Normal CRUD Writes Should Also Be Safe

The same philosophy should apply to ordinary modifications.

Prefer:

```text
read
modify in memory
write temporary file
flush
replace
```

over:

```text
truncate original
write new contents
```

The details should account for cross-platform filesystem behavior.

---

# 54. Autosave in `manage`

The management REPL should probably persist every successful mutation transactionally.

Example:

```text
stassh-secrets> set admin_user administrador
Updated.
```

At that point the change should already be safely stored.

Avoid requiring:

```text
save
```

before exit unless there is a compelling design reason.

Autosave reduces the chance that a long maintenance session is accidentally lost.

---

# 55. Backups

Before risky operations such as rekey or format migration, keeping a recoverable backup may be useful.

The exact policy may depend on existing Stassh file-management conventions.

Be mindful that backups of `secrets.json` still contain encrypted passwords and therefore remain sensitive files.

Do not create endless uncontrolled backup copies.

---

# 56. File Permissions

Where the operating system provides meaningful filesystem permissions, Stassh should create `secrets.json` with restrictive user-only permissions where practical.

However, the cryptographic design must **not depend on Unix permissions for security**.

Portable stores may reside on:

* exFAT;
* FAT-derived filesystems;
* network storage;
* USB media.

Encryption remains the primary protection.

---

# 57. Portable Vault Use

The design should work naturally with Stassh's portable-vault concept.

A portable directory may contain:

```text
vault.json
secrets.json
.stassh-local.json        # depending on existing architecture/policy
```

The exact placement should follow current Stassh design.

The important property is that `secrets.json` can be carried independently and opened on another trusted machine.

---

# 58. `secrets.json` Is Optional

Stassh should function normally without a secrets file.

If no `secrets.json` exists:

* SSH functionality still works;
* host browsing still works;
* CLI functionality unrelated to secrets still works;
* TUI functionality unrelated to secrets still works.

The secrets subsystem must remain optional.

---

# 59. Missing Secrets Set

If a host references:

```json
"secrets": "tso-viladecans"
```

but that set does not exist, Stassh should not crash.

The TUI may display something like:

```text
Secrets set "tso-viladecans" not found.
```

Diagnostics should make the broken reference clear.

---

# 60. Missing `secrets.json`

If a host references a secrets set but the secrets file itself is absent, Stassh should likewise fail gracefully.

For example:

```text
This host references secrets set:
    tso-viladecans

No secrets store is available.
```

Ordinary SSH should remain unaffected.

---

# 61. Wrong Master Password

If the user enters the wrong master password:

```text
Reveal
    ↓
derive incorrect key
    ↓
key_check authentication fails
```

The UI should report something concise such as:

```text
Incorrect master password.
```

If the implementation cannot distinguish incorrect password from store corruption with confidence, an error such as:

```text
Unable to unlock secrets store:
incorrect master password or corrupted store.
```

is acceptable.

Do not expose cryptographic internals unnecessarily to normal users.

---

# 62. Corrupted Secret Record

If the master key verifies correctly but a particular secret cannot authenticate, this is likely record corruption or tampering.

The application should distinguish this from a wrong master password if possible.

Example:

```text
Secrets store unlocked, but field
"tso-viladecans/password"
failed authentication.
```

Do not display garbage plaintext.

---

# 63. File Versioning

`secrets.json` should contain an explicit format version from the beginning.

Example:

```json
"version": 1
```

The parser should reject unknown future versions cleanly rather than attempting unsafe interpretation.

Version migration should be explicit.

---

# 64. KDF Parameters Belong in the File

The file should contain the parameters necessary to reproduce the password-derived key.

Conceptually:

```json
"crypto": {
  "kdf": "argon2id",
  "salt": "...",
  "memory_kib": 65536,
  "iterations": 3,
  "parallelism": 1
}
```

These values are not secret.

They allow parameters to evolve over time.

Do not hard-code one forever-unversioned KDF configuration into the executable.

---

# 65. KDF Performance Testing

Stassh targets modest hardware, including machines on which heavyweight applications are undesirable.

Before fixing default KDF parameters, test them on representative systems such as:

* older x86-64 laptops;
* 32-bit x86 if supported;
* ARMv7;
* ARM64 SBCs;
* low-memory systems.

The target should provide meaningful password-guessing resistance while remaining reasonable for an explicit interactive Reveal operation.

Reveal does not need to feel instantaneous.

It should not feel broken either.

---

# 66. Memory Handling

The program should make reasonable attempts to minimize the lifetime of sensitive plaintext.

Potentially sensitive values include:

* master password;
* derived encryption key;
* revealed password;
* newly entered password before encryption.

Use mechanisms such as `zeroize` or equivalent where appropriate. The `zeroize` crate is specifically intended to prevent compiler optimization from removing deliberate memory clearing.

However, do not make unrealistic guarantees.

Application-level zeroization does not prove that a secret never existed in:

* copied stack memory;
* allocator internals;
* kernel buffers;
* terminal buffers;
* swap;
* other application components.

The project should practice useful hardening without making claims it cannot guarantee.

---

# 67. Avoid Unnecessary Copies

Sensitive types should ideally be passed by reference or moved rather than cloned.

Avoid patterns that create many plaintext copies merely for convenience.

For example, prefer an API where a revealed secret has a clearly controlled lifetime.

---

# 68. Error Handling

Errors should be useful but should not reveal sensitive contents.

Reasonable error categories may include:

```text
SecretsFileNotFound
SetNotFound
FieldNotFound
FieldIsNotSecret
IncorrectMasterPassword
AuthenticationFailed
UnsupportedFormatVersion
CorruptStore
WriteFailure
RekeyFailure
```

Exact Rust error types should follow existing project conventions.

---

# 69. No Secrets in Panic Diagnostics

Particular care should be taken with:

```rust
unwrap()
expect()
panic!()
```

in secret-handling paths.

A failure should never produce output containing a plaintext secret.

Use structured error handling.

---

# 70. Testing Requirements

Secrets functionality should have substantial automated tests because it is security-sensitive and shared by all frontends.

Tests should cover at least:

### File parsing

* valid empty store;
* valid plaintext fields;
* valid secret fields;
* malformed JSON;
* unsupported format version;
* missing required crypto metadata.

### Encryption

* encrypt/decrypt round trip;
* different nonces produce different ciphertext;
* wrong master password fails;
* modified ciphertext fails;
* modified nonce fails;
* modified authenticated context fails;
* field moved to another set fails if context binding is implemented.

### CRUD

* create set;
* delete set;
* rename set;
* add plaintext field;
* modify plaintext field;
* add secret field;
* replace secret field;
* delete field.

### Host association

* valid set reference;
* missing set;
* host with no secrets.

### Rekey

* old password works before rekey;
* new password works after rekey;
* old password fails after rekey;
* all secret values survive rekey;
* plaintext fields remain unchanged;
* failed rekey preserves original file.

---

# 71. Property and Fuzz Testing

If practical, serialization/parsing and crypto-envelope handling are good candidates for:

* property tests;
* malformed-input testing;
* fuzz testing.

The secrets parser should treat the file as potentially corrupted or attacker-controlled input.

A stolen and modified secrets file should not cause unsafe behavior.

---

# 72. Transaction Failure Tests

Simulate failure during:

* temporary-file write;
* flush;
* rename;
* rekey;
* malformed replacement file.

The original valid store should remain recoverable whenever possible.

---

# 73. TUI Tests

TUI-level tests should confirm:

* host with secrets exposes Secrets action;
* host without secrets behaves normally;
* plaintext fields display without master password;
* encrypted fields remain masked;
* Reveal asks for master password;
* wrong password does not reveal;
* successful Reveal displays only selected value;
* leaving the view hides the plaintext;
* subsequent Reveal asks for the master password again.

Crypto correctness itself should be tested in the shared core rather than duplicated in TUI tests.

---

# 74. CLI Management Tests

`stassh secrets manage` should be tested for:

* creating a missing store;
* correct master-password verification;
* wrong master-password rejection;
* set creation;
* set selection;
* plain-field CRUD;
* secret-field CRUD;
* reveal;
* Ctrl+D/exit cleanup;
* safe persistence.

Again, crypto implementation belongs to shared core tests.

---

# 75. Possible Future Extensions

The initial design should leave room for later additions without implementing them prematurely.

Possibilities include:

* more than one secrets set associated with a host;
* folder-level inherited secrets sets;
* secret-field copy support;
* integration with external password managers;
* system-keyring-backed secrets;
* hardware-backed vault unlocking;
* secure PTY injection;
* secret expiration metadata;
* per-field notes;
* search;
* a GUI secrets viewer;
* import/export.

These are not part of the initial requirement.

---

# 76. More Than One Secrets Set per Host

The initial model may use:

```json
"secrets": "tso-viladecans"
```

which gives each host zero or one secrets set.

That is likely sufficient for the immediate use case.

If real-world usage later demonstrates a need for:

```text
site-wide credentials
+
customer-wide credentials
+
host-specific credentials
```

the model could evolve toward:

```json
"secrets": [
  "customer-global",
  "tso-viladecans",
  "tsoviladecans01-local"
]
```

Do not add this complexity until it is useful.

---

# 77. Folder-Level Association

Because credentials may apply to every host in a site folder, folder inheritance could eventually be attractive.

Example:

```text
TSO Viladecans
    secrets: tso-viladecans

    tsoviladecans01
    tsoviladecans02
```

Hosts would inherit the set.

This should not be required for the first implementation unless the existing Stassh configuration model already makes inheritance natural.

Explicit host references are simpler.

---

# 78. External Password Managers

The current design intentionally stores encrypted values in `secrets.json`.

A future abstraction might allow a secret field to reference an external provider instead.

Conceptually:

```json
"password": {
  "type": "external",
  "provider": "some-password-manager",
  "reference": "..."
}
```

This is not needed now.

Do not create a provider framework prematurely unless implementation experience indicates it is useful.

---

# 79. Secrets Synchronization

`secrets.json` may be carried or synchronized using the same user-controlled mechanisms as other Stassh state.

Examples:

* USB drive;
* Syncthing;
* rsync;
* cloud filesystem;
* manual copy.

Because the file contains encrypted passwords, synchronization providers see ciphertext rather than plaintext.

However, metadata such as:

* set names;
* plaintext field names;
* plaintext field values;
* possibly labels,

may remain visible.

This should be understood as part of the security model.

If metadata confidentiality later becomes important, the format may evolve.

---

# 80. What Is and Is Not Protected

Under the proposed model:

### Protected by encryption

* fields explicitly created as secrets;
* usually passwords;
* any other sensitive values deliberately marked secret.

### Not necessarily encrypted

* set names;
* set labels;
* field names;
* ordinary field values;
* KDF settings;
* salt;
* crypto metadata;
* ciphertext lengths;
* file version.

This is intentional.

The feature is designed primarily to protect passwords, not necessarily hide the existence of customer/site names.

If threat-model requirements change, reconsider the format.

---

# 81. Threat Model

The implementation should primarily protect against:

### Loss or theft of the secrets file

Someone obtains `secrets.json` but does not know the master password.

They should not be able to recover encrypted secret fields without performing an offline password-guessing attack.

### Casual access on a shared machine

Someone can inspect Stassh files but does not possess the master password.

Encrypted fields remain unavailable.

### Accidental disclosure

Secrets should not casually leak through:

* logs;
* command-line arguments;
* configuration dumps;
* normal TUI views.

### File corruption or tampering

Authenticated encryption should detect modification of encrypted values.

---

# 82. Threats This Feature Does Not Solve

The secrets store does **not** protect against a fully compromised machine while the user reveals a secret.

Malware may:

* capture the master password;
* read process memory;
* record the screen;
* capture the keyboard;
* inspect clipboard contents.

Likewise, revealing a password in a public environment allows shoulder surfing.

The intended use assumes a reasonably trusted machine.

Do not market this as a mechanism for safely using secrets on hostile computers.

---

# 83. Why This Is Still Reasonably Safe for the Use Case

The feature deliberately limits exposure:

```text
password encrypted at rest
    ↓
no persistent unlocked state during ordinary TUI use
    ↓
master password required for each Reveal
    ↓
only requested secret decrypted
    ↓
plaintext exists briefly
    ↓
user returns to ordinary SSH
```

This is a small and understandable security model.

It avoids much of the complexity associated with a full password manager.

---

# 84. Security Philosophy

Prefer boring, established security mechanisms.

Use:

* mainstream password KDFs;
* mainstream AEAD encryption;
* secure random nonce generation;
* established Rust crates;
* short-lived plaintext;
* transactional file writes;
* explicit user actions.

Avoid:

* custom ciphers;
* home-grown password hashing;
* reversible obfuscation;
* secret values in process arguments;
* silent long-lived unlocking;
* frontend-specific crypto;
* magic field-name security conventions.

---

# 85. Suggested Implementation Milestones

A sensible implementation sequence might be:

## Phase 1: Core data structures

Implement shared Rust representations for:

```text
SecretsStore
SecretSet
PlainField
EncryptedField
CryptoMetadata
```

Add parsing and serialization tests.

## Phase 2: Crypto envelope

Implement:

* KDF;
* key-check;
* encryption;
* decryption;
* associated data;
* secure nonce generation;
* sensitive memory wrappers.

Add comprehensive tests.

## Phase 3: Persistence

Implement:

* create;
* load;
* safe writes;
* CRUD;
* error handling.

## Phase 4: CLI management

Implement:

```bash
stassh secrets manage
```

with enough CRUD functionality to populate a real secrets store.

## Phase 5: Host association

Add optional host → secrets-set reference to the existing Stassh model.

## Phase 6: TUI viewer

Add:

* Secrets action;
* plaintext field display;
* masked secret display;
* Reveal workflow;
* master-password prompt.

## Phase 7: Rekey

Implement transactional:

```bash
stassh secrets rekey
```

## Phase 8: Polish

Add:

* optional copy;
* diagnostics;
* improved errors;
* documentation;
* cross-platform testing.

This ordering is illustrative.

---

# 86. Proposed Initial Real-World Example

Suppose `vault.json` contains hosts:

```text
tsoviladecans01
tsoviladecans02
```

Both reference:

```text
tso-viladecans
```

The user runs:

```bash
stassh secrets manage
```

and creates:

```text
Set: tso-viladecans
Label: TSO Viladecans

admin_user       directorio
password         [secret]
note             Shared admin credentials
```

The encrypted password lives only as ciphertext in `secrets.json`.

Later:

```text
stassh-tui
```

shows:

```text
TSO Viladecans
├── tsoviladecans01
└── tsoviladecans02
```

The user connects to `tsoviladecans01`.

After discovering they need the admin password, they exit SSH.

The host remains selected.

They open:

```text
Secrets
```

and see:

```text
admin_user      directorio
password        •••••••••••••
note            Shared admin credentials
```

They select:

```text
password
```

and choose:

```text
Reveal
```

Stassh asks:

```text
Master password:
```

After successful verification:

```text
password        Actual-Pronounceable-Password-42
```

The user remembers or copies it, closes the secrets view, reconnects, and types the password into the remote system normally.

This is the primary experience this feature exists to support.

---

# 87. Architectural Success Criteria

The implementation is successful if all of the following are true:

* `stassh` and `stassh-tui` read exactly the same secrets format;
* all crypto lives in shared Rust code;
* hosts can reference reusable secrets sets;
* multiple hosts can share one set;
* arbitrary plaintext metadata can coexist with encrypted fields;
* secrecy is explicit rather than inferred from names;
* individual secret fields are independently encrypted;
* normal Reveal asks for the master password every time;
* only the requested secret is decrypted;
* `stassh secrets manage` allows convenient unlocked CRUD during deliberate administration;
* `stassh secrets rekey` can safely change the master password;
* secrets do not appear in normal logs or command arguments;
* the feature remains optional;
* ordinary SSH functionality remains completely usable without `secrets.json`.

---

# 88. Architectural Red Flags

During implementation, pause and reconsider if the design starts requiring any of the following without strong justification:

```text
separate crypto code in CLI and TUI
secret values passed through command-line arguments
all passwords decrypted merely to open the secrets screen
master key cached indefinitely during normal TUI use
plaintext passwords written to temporary files
security determined by field-name prefixes
custom cryptography
unauthenticated encryption
passwords printed in debug logs
destructive in-place rekey
mandatory secrets store for ordinary SSH
automatic sudo/password prompt handling
a full general-purpose password-manager subsystem
```

These would indicate that the implementation is drifting away from the intended simplicity.

---

# 89. Final Guidance to the Coding Agent

The goal is not to build a sophisticated password manager.

The goal is to add a **small, secure, host-associated encrypted reference facility** to Stassh.

Its most important use case is:

> "I am maintaining this host. I unexpectedly need a password I normally know but cannot currently remember. Stassh knows which set of operational secrets belongs to this host, so I can deliberately reveal the value and continue working."

Keep that scenario in mind when making tradeoffs.

The project should favor:

* minimal machinery;
* explicit actions;
* strong cryptographic primitives;
* shared Rust implementation;
* short-lived plaintext;
* reusable named sets;
* predictable CLI/TUI behavior;
* safe file updates;
* and easy auditing.

`stassh secrets manage` exists because administrative editing is a deliberate trusted operation and benefits from unlocking once.

`stassh-tui` Reveal asks every time because routine field use should keep secrets locked by default.

Those two workflows intentionally have different convenience/security tradeoffs.

Above all:

> **Implement the security-sensitive behavior once, in the shared Stassh Rust core, and make every frontend use that implementation.**

The blueprint is a starting design, not a contract.

If implementation work reveals a better architecture, use it—provided the resulting design remains understandable, auditable, and at least as protective of secrets as the model described here.

