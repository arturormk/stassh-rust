# Android Support Assessment

Research snapshot: 2026-09-14

## Decision summary

Android can reuse the stassh domain core and configuration formats, but it
cannot reuse the current connection backend as-is. A normal Android application
cannot assume that `ssh`, `ssh-keygen`, arbitrary local programs, and a desktop
PTY environment exist. Treating Android as another OpenSSH process target would
produce a fragile application and an ongoing native-binary maintenance burden.

The recommended architecture is a separate Tauri Android application that
shares `stassh-core`, the JSON schemas, and selected React components, while
using a mobile-specific interface and an embedded asynchronous Rust SSH engine.
`russh` is the preferred candidate, conditional on a time-boxed feasibility
spike. SSH channel data should stream directly between the embedded engine and
xterm.js; no local PTY is needed for a remote shell.

This is a major feature project. Estimate 2–3 engineer-weeks for a serious
go/no-go spike, 10–16 engineer-weeks for a useful beta, and several additional
months to approach broad desktop parity. Android also creates substantial
recurring overhead: SDK/NDK and target-API updates, mobile lifecycle and device
testing, signing, distribution policy, and direct responsibility for SSH
protocol security behavior.

## What “the same core and configuration” can mean

The following compatibility goal is realistic:

- use the same `Vault`, host, folder, jump, forward, action, and secrets models;
- read and write the same `vault.json` and `secrets.json` schema versions;
- keep `local.json` schema-compatible while storing Android-specific mappings;
- preserve UUIDs, fingerprints, ordering, validation, encryption parameters,
  and external-change protection;
- reject unsupported behavior explicitly rather than creating an Android fork
  of a record.

It does **not** mean copying one machine's `local.json` unchanged. That file is
machine-local by design: `/home/alice/.ssh/key`,
`C:\Users\Alice\.ssh\key`, and an Android app-private key location cannot be
the same path. Users may synchronize the three files if they choose, but only
the vault and encrypted secrets are expected to be semantically portable.

It also cannot mean exact OpenSSH option compatibility. The vault currently
permits arbitrary `ssh_options`, and actions can launch arbitrary local
programs. An embedded SSH library will support an explicit subset. Android must
show per-host and per-action capability diagnostics and refuse unsupported
options instead of silently ignoring them.

## Why the desktop execution path does not transfer

The current GUI opens a local PTY, spawns the system `ssh` executable inside
it, and streams terminal bytes to xterm.js. The CLI and TUI similarly launch
system processes. Android has neither a standard system SSH client nor a stable
contract for packaging a general-purpose OpenSSH executable and spawning it as
though the app were a Unix workstation. Local action programs such as `scp`,
`rsync`, shell scripts, and VNC viewers are also not generally present.

Android application storage and lifecycle add two further differences:

- user-selected shared files are exposed through Storage Access Framework
  document URIs, which are not ordinary Rust `PathBuf` values and do not
  guarantee desktop-style atomic rename;
- a network session that must survive the UI entering the background needs an
  Android lifecycle component and normally a user-visible foreground-service
  notification.

The reusable boundary is therefore the domain/storage semantics above an
execution abstraction, not the OpenSSH child process below it.

## Architecture options

| Option | Advantages | Problems | Recommendation |
| --- | --- | --- | --- |
| Tauri mobile UI + embedded `russh` engine | Reuses Rust, React, xterm.js, and models; no native SSH binary | Must implement authentication, host-key policy, jumps, forwarding, and lifecycle | **Preferred, after spike** |
| Tauri mobile UI + `libssh2`/`libssh` | Mature native libraries | NDK/FFI builds, native CVE updates, and still requires product-level SSH semantics | Fallback if the Rust-engine spike fails |
| Bundle OpenSSH and a PTY | Closest command-line semantics | Executable packaging, dependencies, process restrictions, PTY behavior, size, and security servicing | Reject for production |
| Launch Termux or another SSH app | Very low connection-engine effort | External dependency, inconsistent intents, no integrated sessions, weak support for keys/jumps/forwards | Optional export/escape hatch only |
| Native Kotlin/Compose UI over Rust core | Best access to Android UI APIs | Duplicates the entire frontend and adds a Rust/Kotlin binding boundary | Use only if Tauri/xterm mobile gates fail |

`russh` is a protocol library rather than a drop-in OpenSSH client. That is an
advantage on Android, but it moves policy into stassh. Before committing, the
spike must prove the required crypto backend builds for all selected Android
ABIs and that the library can support the authentication, nested transport,
forwarding, cancellation, and host-key interfaces the product needs.

Do not choose a library merely because it can open a shell. Host-key handling,
algorithm negotiation, encrypted-key support, jump chaining, port forwarding,
and timely security maintenance are release blockers.

## Proposed application boundary

Create a separate Android application target rather than making every desktop
component responsive through conditionals. It should share:

- `stassh-core` models, validation, secrets cryptography, search, resolution,
  simulation fixtures, and action analysis;
- frontend types and small presentational components where they genuinely fit;
- xterm.js and the existing principle of keeping terminal bytes out of global
  UI state;
- versioning and compatibility tests for the JSON artifacts.

It should own:

- phone/tablet navigation and touch interaction;
- Android workspace and key import flows;
- the embedded SSH session manager;
- Android lifecycle, notifications, and secure-storage integration;
- mobile-specific capability diagnostics.

Refactor the shared core around two narrow boundaries before implementing many
features:

1. a storage interface that loads and conditionally saves validated bytes
   without assuming every location is a filesystem path;
2. a session interface that exposes connect, input, output, resize, exit,
   cancel, remote command, and forwarding capabilities without exposing a
   child process or PTY.

The desktop implementation remains OpenSSH plus PTY. The Android implementation
uses SSH protocol channels. Simulation should implement the same session
interface so lifecycle and UI tests do not need real servers.

## Workspace storage and user-managed synchronization

Android should offer two workspace modes.

### Managed workspace

The default is an app-private working directory. It is reliable, needs no broad
storage permission, and is appropriate for `local.json`, imported private keys,
cached known-host information, and a user's initial vault. Provide explicit
Open/Import and Export/Share operations for the portable JSON files.

### Linked external workspace

Advanced users should be able to select a directory through
`ACTION_OPEN_DOCUMENT_TREE`. Persist the granted URI permission and access
`vault.json` and `secrets.json` through the document provider. This enables
user-managed synchronization with a provider or synchronization tool that
exposes a compatible document tree.

Do not request `MANAGE_EXTERNAL_STORAGE`. It is disproportionate to three JSON
files and creates store-policy burden. Storage Access Framework access is
user-selected and needs no blanket storage permission.

The linked mode needs semantics beyond the current `Path` functions:

- keep the provider URI as machine-local application state, never in the
  portable vault;
- detect that a grant was revoked or that a document was moved;
- read into memory, validate completely, and compute a content digest;
- compare the digest again immediately before saving;
- write a new document or provider-supported temporary sibling, validate it,
  and replace the destination only when supported;
- retain a recoverable backup when the provider cannot guarantee atomic
  replacement;
- report read-only providers and conflicts without overwriting external edits.

Cloud document providers may be eventually consistent. The UI must distinguish
“saved to provider” from “synchronized to another device”; stassh should not
claim to implement synchronization.

## Authentication and key custody

Android cannot derive fingerprints by invoking `ssh-keygen`. Add an in-process
key parser/fingerprinter shared with the embedded engine. Import supported
OpenSSH private-key files through the system document picker, validate them,
show their fingerprint, and copy them into app-private storage. The selected
source URI must not become the long-term key reference.

Use an Android Keystore key to wrap the key that encrypts imported SSH private
keys at rest. This is different from claiming that arbitrary imported OpenSSH
keys become hardware-backed Android Keystore keys; many key formats and
algorithms cannot be imported that way. Support optional device authentication
before unwrapping. Never put private key bytes, passwords, or passphrases in
`vault.json` or `local.json`.

Initial authentication scope should include:

- ephemeral password entry;
- supported imported OpenSSH private keys, including encrypted keys and an
  ephemeral passphrase prompt;
- server-directed keyboard-interactive prompts if the chosen engine exposes
  them safely.

Do not automatically treat a field in `secrets.json` as an SSH password. The
existing secrets store is an operational reference, not an authentication API.
A later explicit user-controlled mapping can be designed separately.

Agent forwarding and integration with arbitrary Android credential providers
are out of v1 unless the spike identifies a secure, portable API.

## Host-key verification

Using an embedded engine makes stassh responsible for server identity policy.
This is non-negotiable release work:

- show the algorithm and SHA-256 fingerprint for an unknown host and require an
  explicit first-use decision;
- store accepted keys in app-private known-host data keyed by effective host
  and port;
- fail closed on a changed key and show both the stored and received
  fingerprints;
- handle jump-host keys independently at every hop;
- never ship an “accept every key” production mode;
- keep simulation keys visibly separate from real known-host state.

Known-host data is a new local artifact, not a change to the portable vault
schema. A future portable known-host design should be cross-platform and
security-reviewed rather than introduced only for Android.

## Sessions, terminal, and lifecycle

For an interactive remote shell, request a remote SSH PTY and stream channel
data directly to xterm.js. The local Android process does not need
`portable-pty`. The bridge must support byte-safe input/output, terminal size,
EOF, exit status/signal, disconnect reason, and cancellation. Keep terminal
traffic on a narrow channel rather than React state.

Mobile terminal acceptance must cover:

- soft keyboards and major IMEs;
- hardware keyboards, Ctrl/Alt/Escape/Tab, and configurable extra-key rows;
- composition text, paste, selection, and Unicode;
- rotation, split-screen, phone and tablet sizes, and font scaling;
- temporary network loss and Wi-Fi/mobile transitions;
- screen lock, process recreation, and app background/foreground transitions;
- several sessions without unbounded memory or battery use.

The first beta may explicitly disconnect sessions when the application leaves
the foreground. If sessions or tunnels are promised to continue in the
background, implement a native Android foreground service through a small
Tauri mobile plugin, display a persistent notification, expose a disconnect
action, and comply with the current foreground-service type and start rules.
Do not use WorkManager as a terminal-session daemon.

Desktop terminal grids should not dictate phone design. A phone v1 needs a host
list, inspector/editor, and one focused terminal with fast tab switching. A
tablet can add split terminals later. Broadcast input should be deferred until
the risks of touch focus and accidental multi-host commands have been assessed.

## Feature compatibility and staging

### First useful beta

- open, validate, search, edit, and save a managed or linked workspace;
- unlock and inspect the existing encrypted secrets format;
- direct interactive SSH sessions;
- ephemeral password, keyboard-interactive, and imported-key authentication;
- strict first-use and changed-host-key handling;
- terminal input, output, resize, disconnect, and useful error reporting;
- diagnostics for every unsupported host option or action.

### Follow-up connection features

Implement in this order, with interoperability tests against OpenSSH servers:

1. remote command execution;
2. one and then multiple jump hosts using nested direct-TCP/IP channels;
3. local forwarding;
4. dynamic SOCKS forwarding;
5. remote forwarding;
6. background persistence for user-visible sessions or tunnels.

The order reflects implementation and security complexity, not a schema
change. Existing records remain readable before their execution feature is
available.

### Actions

Remote-only actions can become usable after remote commands and action forwards
exist. Arbitrary desktop local commands are not portable to Android. For v1,
mark actions containing `local_prepare`, `local_launch`, or `cleanup` as
unsupported unless every command maps to an explicitly implemented Android
capability. Do not invoke a shell, download executable helpers, or reinterpret
desktop paths.

Android capabilities could later include safe platform operations such as
opening a forwarded HTTP URL or sharing an exported file. They should be named,
reviewed operations implemented by the app, not arbitrary executables.

### Arbitrary OpenSSH options

Create a documented support table. Parse options only when their semantics are
implemented by the engine. Unknown or unsupported options must block connection
with a precise diagnostic. Silently ignoring `ProxyCommand`, host-key policy,
authentication, or forwarding directives would make the same vault behave
dangerously differently.

## Feasibility spike and go/no-go gates

Time-box the spike to 2–3 engineer-weeks and avoid production UI polish. It
must demonstrate on at least one physical phone and one emulator:

1. a Tauri Android build for arm64 and x86-64 emulator targets using a current
   Android NDK;
2. a direct `russh` connection with password and encrypted private-key
   authentication;
3. explicit host-key accept/reject/change behavior;
4. a remote PTY streamed to xterm.js with typing, resize, Ctrl-C, and exit;
5. cancellation and cleanup without leaked tasks or sockets;
6. one jump connection or nested direct-TCP/IP proof;
7. one local-forward proof;
8. managed-workspace save and a Storage Access Framework round trip with
   persisted access and conflict detection;
9. rotation, background/foreground, network-loss, and process-recreation
   observations;
10. successful builds of the selected crypto backend with 16 KB page-size
    compatibility for every proposed ABI.

Proceed only if the engine is actively maintained, all security-critical flows
can be implemented without patches that the project cannot sustain, terminal
input is usable with real mobile keyboards, and the storage provider does not
force a lossy copy workflow. If the SSH engine fails, evaluate `libssh2` before
changing the UI architecture. If xterm.js/Tauri mobile fails while the engine
succeeds, prototype Kotlin/Compose over the Rust core.

## Distribution and recurring overhead

Every distributed APK must be signed. Protect and back up the long-lived
signing/upload material from the first beta even if distribution begins through
GitHub Releases. Losing signing continuity can prevent users from installing an
update over an existing app.

| Channel | Benefits | Project overhead | Recommendation |
| --- | --- | --- | --- |
| Signed APK on GitHub Releases | Fast open-source beta, full release control | Users enable sideloading; project owns update communication | Start here |
| F-Droid | Open-source audience and repository updates | Reproducible build/metadata requirements and review lag | Add after builds are reproducible |
| Google Play | Broadest discovery and managed rollout | Account verification, AAB/Play signing, listing, privacy/data-safety forms, review, target-API deadlines | Add after security/lifecycle gates |

For Google Play, new applications use Android App Bundles and Play App Signing.
As of the research date, Google states that new applications and updates must
target Android 16/API 36 from 2026-08-31, subject to its published exceptions
and extension process. This requirement changes over time; the release
checklist must consult the current policy rather than freezing API 36 in code or
long-lived documentation.

The Android toolchain adds Android Studio, JDK, SDK, NDK, Gradle, Rust Android
targets, Tauri-generated mobile files, release keystores, and emulator/device
images. Prefer NDK 28 or newer and verify 16 KB native-library page alignment,
which current Android/Tauri guidance identifies as a submission requirement.

| Overhead | Initial | Recurring |
| --- | --- | --- |
| Embedded SSH engine and security policy | High | Dependency/CVE review and interoperability tests |
| Mobile UX and terminal input | High | Device, WebView, and IME regression testing |
| SAF storage plugin and providers | High | Provider/OS compatibility testing |
| Lifecycle/foreground service | Moderate to high | Android policy and behavior updates |
| SDK/NDK/Gradle/Tauri mobile pipeline | High | At least annual target-API and toolchain work |
| Signing and release automation | Moderate | Permanent key custody and rotation procedures |
| Play publication, if chosen | Moderate to high | Store forms, review, listings, and staged releases |
| SSH test infrastructure | Moderate | Maintain server/algorithm/jump/forward matrix |

CI should build arm64 release artifacts and an x86-64 emulator artifact, run
shared Rust format-compatibility tests, and exercise deterministic simulation
UI tests. Release qualification additionally needs physical phone and tablet
smoke tests, current and oldest-supported Android versions, multiple WebView
versions, and at least two keyboard/IME families. Do not put real SSH endpoints,
keys, or decrypted secrets in CI fixtures or crash reports.

## Delivery sequence

### Phase 0: spike

Complete the go/no-go work above and record exact library/version/ABI results.
No store commitment should precede this gate.

### Phase 1: mobile shell and storage foundation

Add the shared storage/session abstractions, managed and linked workspaces,
key import/custody, known-host policy, direct sessions, and the phone-first UI.

Gate: a beta user can move an existing vault to Android, connect securely to a
direct host, edit it, save without overwriting an external change, and move the
unchanged JSON back to desktop.

### Phase 2: useful network parity

Add remote commands, jump chains, local and dynamic forwarding, tablet polish,
and remote-only actions.

Gate: each supported record has cross-engine integration tests and every
unsupported record has an actionable diagnostic.

### Phase 3: durable sessions and wider distribution

Add remote forwarding where justified, foreground-service session persistence,
release monitoring, F-Droid metadata, and optionally Google Play publication.

Gate: background behavior, notification controls, upgrade/signing, store policy,
and security review are complete on production-signed builds.

## Final recommendation

Do not describe Android as a simple Tauri build target. Begin only after the
Windows port, with a 2–3 week `russh`/Tauri/xterm/SAF feasibility spike. If it
passes, build a separate phone-first Tauri application sharing `stassh-core`
and the existing JSON formats, backed by an embedded Rust SSH engine. Keep
private keys and machine-local state app-private, support user-managed files
through SAF, and expose exact capability differences.

Ship a useful direct-SSH beta before pursuing complete forwarding and action
parity. Start with signed GitHub APKs, add F-Droid after reproducibility is
proven, and take on Google Play only after host-key, key-custody, lifecycle, and
policy gates are satisfied. Do not base the product on bundled OpenSSH, Termux,
or silent degradation of unsupported OpenSSH options.

## References

- Tauri, [Prerequisites and Android toolchain](https://v2.tauri.app/start/prerequisites/)
- Tauri, [Calling Rust from the frontend and mobile entry points](https://v2.tauri.app/develop/calling-rust/)
- Tauri, [Mobile plugin development](https://v2.tauri.app/develop/plugins/develop-mobile/)
- Tauri, [Android code signing](https://v2.tauri.app/distribute/sign/android/)
- Android Developers, [Data and file storage overview](https://developer.android.com/training/data-storage)
- Android Developers, [Storage Access Framework](https://developer.android.com/training/data-storage/shared/documents-files)
- Android Developers, [Android Keystore](https://developer.android.com/privacy-and-security/keystore)
- Android Developers, [Background tasks](https://developer.android.com/develop/background-work/background-tasks)
- Android Developers, [Foreground services](https://developer.android.com/develop/background-work/services/fgs)
- Android Developers, [App signing](https://developer.android.com/studio/publish/app-signing)
- Google Play, [Target API level requirements](https://support.google.com/googleplay/android-developer/answer/11926878)
- `russh`, [upstream repository](https://github.com/Eugeny/russh)
- `russh`, [client API documentation](https://docs.rs/russh/latest/russh/client/)
