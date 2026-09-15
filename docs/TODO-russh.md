# Optional `russh` Backend Assessment

Research snapshot: 2026-09-15

## Decision summary

`russh` could give stassh an SSH implementation that does not depend on an
installed `ssh` executable. That is strategically useful for Android, useful as
an opt-in fallback on Windows, and potentially useful for applications that
want direct access to SSH channels. It is not, however, a drop-in replacement
for the current OpenSSH command.

Do not add `russh` directly to `stassh-core` or to the default desktop builds.
Keep system OpenSSH as the normal desktop execution engine. First introduce a
narrow, engine-neutral session boundary, then put a `russh` implementation in a
separate optional workspace crate. Android can compile that backend in by
default after a successful feasibility spike. If desktop demand justifies it,
publish a separately named embedded/self-contained build that contains both
backends and lets the user force `russh`.

This packaging distinction is unavoidable. Rust statically links used library
code into each executable. A single binary that can select OpenSSH or `russh`
at runtime must contain the async runtime, protocol implementation,
cryptographic algorithms, and key parsers even when the user selects OpenSSH.
A Cargo feature can keep those dependencies out of a build, but it cannot make
them disappear at runtime from an already-built dual-backend executable.

The recommended decision is therefore **proceed with an architecture and
compatibility spike, but do not add `russh` to ordinary desktop artifacts yet**.
The main near-term consumer should be Android. Windows should continue to use
the Microsoft-provided OpenSSH client by default, with an embedded edition
considered only if missing or inconsistent OpenSSH installations prove to be a
real support problem.

## Terminology and candidate

This assessment uses “Rust SSH” to mean the
[`russh`](https://github.com/Eugeny/russh) client/server protocol library, not
the unrelated `rustssh` crate. `russh` is a low-level asynchronous SSH 2.0
library built around Tokio. It exposes authentication, session channels, PTY
requests, remote execution, and forwarding primitives; it does not provide a
complete OpenSSH-compatible command-line client or stassh session manager.

At the research snapshot, upstream advertises `russh` 0.63, requires selecting
either `aws-lc-rs` or `ring` as its cryptographic backend, and declares Rust
1.89 as its minimum supported toolchain. Its default features include
`aws-lc-rs`, compression, and RSA. The crate is Apache-2.0 licensed, which is
compatible with this workspace's MIT-or-Apache-2.0 licensing, subject to normal
third-party notice and dependency-license review.

Calling the result “pure Rust” would be misleading when using the normal crypto
backends: both backend and target details still affect native compilation,
cross-compilation, binary size, and security updates. These details are
especially important for Android ABIs and statically linked desktop builds.

## Why this is more than changing command construction

The current architecture deliberately delegates SSH behavior to OpenSSH:

- `stassh-core::openssh` converts a resolved host into an `ssh` argument vector
  or temporary OpenSSH configuration;
- `prepare_openssh_command` resolves a machine-local identity path and returns
  an `OpenSshCommand` plus an optional temporary file;
- action plans contain an `OpenSshCommand` and expose OpenSSH-specific template
  values such as `{SSH_CONFIG}`, `{SSH_ALIAS}`, `{SSH_RSH}`, and
  `{SSH_COMMAND}`;
- the CLI and TUI run `ssh` as a foreground child and inherit the terminal;
- the GUI opens a local PTY, spawns `ssh` inside it, and streams the PTY to
  xterm.js;
- GUI probes add OpenSSH options such as `BatchMode`, `ConnectTimeout`, and
  `RequestTTY` to the command;
- tmux integration needs a command and a temporary configuration that remain
  valid after the TUI launches a new window;
- key fingerprinting, passphrase prompts, agent use, host-key verification,
  algorithm selection, and most configuration semantics are currently handled
  by OpenSSH.

With `russh`, there is no child process, local PTY, exit status object, OpenSSH
configuration file, or command string. Stassh must create a network transport,
authenticate it, request a remote PTY, route channel bytes, handle resize and
exit messages, and own cancellation. The frontends and action planner therefore
need a session abstraction; returning a different kind of `OpenSshCommand`
cannot express the required behavior.

## Potential advantages

### Android becomes practical

Android cannot assume that a system `ssh` executable, `ssh-keygen`, arbitrary
local commands, or a desktop PTY exists. An embedded protocol engine can open a
remote PTY channel and stream it directly to xterm.js. It also supplies the key
parsing and fingerprinting foundation needed when `ssh-keygen` is unavailable.
This matches the architecture proposed in `TODO-android.md`.

`russh` does not solve Android storage, key custody, foreground-service, input,
or application lifecycle work, but it removes the fragile requirement to
package and spawn a general-purpose OpenSSH executable inside an Android app.

### A self-contained Windows option

A `russh` build can connect when the Windows OpenSSH optional component is not
installed, and it avoids differences in `ssh.exe` discovery. It could also
provide the same protocol behavior on Windows and Android. This is a useful
escape hatch, not a reason to replace the Windows plan: system OpenSSH plus
ConPTY offers much broader compatibility for less stassh-specific code and
leaves OpenSSH servicing with Microsoft.

### Better application-level control

Direct channel access can provide structured authentication prompts,
disconnect reasons, exit signals, forwarding state, and per-hop host keys
without scraping terminal output. The GUI would not need a local PTY merely to
host a remote shell. An engine-neutral simulated session can exercise the same
input, output, resize, and cancellation paths without starting processes or
contacting real servers.

This control may eventually enable more precise diagnostics and lifecycle
management than an opaque child process, provided stassh accepts responsibility
for implementing those policies correctly.

## Costs and risks

### Dependency and executable size

`russh` brings Tokio networking plus a substantial cryptographic and key-format
graph. Its manifest includes multiple AES modes, ChaCha20, elliptic-curve and
Ed25519 implementations, RSA support by default, hashing and MAC algorithms,
post-quantum key exchange support, key encryption formats, compression, and a
required crypto backend. Some of this may overlap conceptually with stassh's
existing secrets cryptography, but Cargo cannot generally deduplicate different
crate families or incompatible versions into one implementation.

The exact release-size increase depends on target, crypto backend, enabled
algorithms, link-time optimization, stripping, and which paths the linker can
discard. It must be measured rather than estimated from downloaded crate size.
The important product fact is that a runtime-selectable dual-backend executable
will be larger even for users who always choose OpenSSH.

The dependencies also increase clean build time, cross-compilation complexity,
the software-bill-of-materials surface, and the number of advisories and
upstream releases that need review. Putting an optional dependency on
`stassh-core` is especially undesirable because Cargo feature unification could
enable it unexpectedly for every binary in a build graph.

### Stassh becomes responsible for SSH security policy

OpenSSH currently owns host-key lookup, first-use prompting, changed-key
failures, algorithm negotiation, authentication sequencing, key loading,
agent communication, and many hardened protocol edge cases. `russh` provides
protocol mechanisms, but the application must choose and persist policy.

An embedded backend must, at minimum:

- display the host-key algorithm and SHA-256 fingerprint for an unknown host;
- require an explicit decision before trusting it;
- store known hosts as machine-local state keyed by effective hostname and
  port;
- fail closed on a changed key and show both fingerprints;
- verify every jump host independently;
- avoid an accept-all-host-keys production mode;
- select modern algorithms by default and make any legacy algorithms an
  explicit, diagnosable opt-in;
- promptly track `russh`, crypto-backend, and transitive security advisories.

This is a permanent maintenance responsibility, not one-time integration work.

### Authentication moves into every frontend

The existing programs inherit OpenSSH's password, key-passphrase,
keyboard-interactive, and agent prompts. An embedded client needs typed prompt
events and secure responses in the CLI, TUI, GUI, and Android UI. It must parse
supported private-key formats, request passphrases without retaining them,
zeroize sensitive buffers where practical, and produce clear errors for
unsupported keys or methods.

Unix agents, Windows Pageant or OpenSSH agents, and Android key custody have
different transports and trust models. Agent forwarding is a separate high-risk
feature and should not be assumed merely because the library exposes protocol
channels for it.

### OpenSSH compatibility is not automatic

The portable host model permits arbitrary `ssh_options`. OpenSSH has a very
large configuration language with precedence rules, token expansion, proxy
commands, certificates, PKCS#11/FIDO providers, agents, multiplexing, QoS,
environment controls, and vendor extensions. `russh` exposes protocol building
blocks, not an implementation of every OpenSSH option.

The embedded backend must define a small supported option set and reject every
unknown or OpenSSH-only option before connecting. Silently ignoring an option
could remove a proxy, identity restriction, host-key rule, or forwarding
behavior the user relies on. Imported OpenSSH records can remain portable data,
but that does not imply that both execution engines can run every imported
record.

### Jumps and forwarding require product code

`russh` exposes the channels needed for direct TCP/IP and remote forwarding,
but stassh must implement listener ownership, bidirectional copying,
backpressure, error propagation, cancellation, and lifecycle reporting.
Dynamic forwarding additionally needs a local SOCKS server. A jump chain needs
each later SSH transport to run over a channel through the previous hop, with
separate host-key and authentication handling at every level.

The features exist at protocol level; that is not equivalent to the
OpenSSH-compatible `-J`, `-L`, `-R`, and `-D` behavior stassh currently gets by
constructing a command.

### Two engines double important test paths

Every supported feature needs interoperability tests against representative
OpenSSH servers for both engines. Error text and behavior will differ. Bugs may
be engine-specific, and users will expect the backend selector to explain why a
record works in one engine but not the other. Maintaining two partial SSH
implementations is worse than keeping one reliable default unless the second
engine unlocks a platform or demonstrated use case.

## Compatibility assessment

| Capability | OpenSSH today | `russh` feasibility | Required stassh work |
| --- | --- | --- | --- |
| Direct interactive shell | Supported | Strong | Remote PTY request, byte streams, resize, EOF, exit and cancellation |
| Remote command | Supported | Strong | Channel execution, stdout/stderr and exit mapping |
| Password authentication | Delegated prompt | Strong | Secure prompt API and frontend implementations |
| Private keys | OpenSSH loads mapped path | Strong for supported formats | In-process loading, encrypted-key prompt, fingerprint checks and custody |
| Keyboard-interactive | Delegated | Available | Multi-prompt event/response flow and cancellation |
| SSH agent/Pageant | Delegated | Platform-dependent support | Discovery, policy, diagnostics and cross-platform tests |
| Host-key verification | OpenSSH known-host policy | Mechanism available | Entire trust UI, persistence and changed-key policy |
| Jump chain | `ProxyJump` | Feasible, non-trivial | Nested transports, per-hop auth/verification and failure reporting |
| Local forwarding | `-L` | Protocol support available | Listener, channel pumps, lifecycle and collision handling |
| Remote forwarding | `-R` | Protocol support available | Registration, incoming channels, cancellation and server-policy errors |
| Dynamic forwarding | `-D` | No complete OpenSSH-style client behavior | SOCKS server plus direct channels and lifecycle management |
| Arbitrary `ssh_options` | Passed to OpenSSH | Not compatible in general | Allowlist and reject unsupported options before connection |
| GUI terminal | Local PTY plus `ssh` | Strong | Direct async stream bridge; no local PTY for remote sessions |
| CLI terminal | Inherited child terminal | Feasible | Raw-mode input/output loop, signal and resize handling |
| TUI terminal | Suspend TUI and run child | Feasible | Embedded terminal/session loop and reliable restoration |
| Ping/probe | Batch OpenSSH command | Strong | Non-interactive connect/auth/command timeout policy |
| tmux window | Launches an `ssh` command | OpenSSH-specific | Keep OpenSSH-only or add a separate helper process |
| SSH action templates | Real config and command strings | Fundamentally OpenSSH-specific | Require OpenSSH or return an explicit incompatibility |
| Local action programs | Ordinary child processes | Unchanged on desktop | Keep separate from the SSH engine; generally unavailable on Android |

## Recommended architecture

### Keep the domain core engine-neutral

The `Vault`, `ResolvedHost`, action definitions, forwards, identity
fingerprints, local mappings, secrets, import, and export formats should not
depend on `russh`, Tokio, or a crypto backend. Do not add an SSH-engine choice to
`vault.json`; it is a machine/runtime preference, not portable host data.

Introduce a narrow session contract that can represent:

- connection progress and authentication prompts;
- host-key decisions;
- byte-safe input, stdout and stderr output;
- remote PTY allocation and resize;
- remote command execution;
- forwarding readiness and failures;
- EOF, exit status or signal, disconnect reason, and cancellation;
- capability analysis before attempting the connection.

Do not expose `std::process::Child`, `portable_pty` types, temporary OpenSSH
files, `russh` handles, or Tokio types through that public boundary. The current
simulation should implement the same contract.

OpenSSH-specific command/config generation remains useful for diagnostics,
export, action templates, tmux, and the default backend. It should sit behind an
OpenSSH adapter rather than define the shape of every session plan.

### Isolate the embedded implementation

Create a separate optional workspace crate, conceptually
`stassh-ssh-russh`, which depends on `russh`, Tokio, and the selected crypto
backend. This is safer than placing an optional `russh` dependency directly on
`stassh-core`: ordinary model, storage, import/export, and OpenSSH-only builds
then cannot acquire the dependency through feature unification.

The crate should initially implement direct interactive sessions and remote
commands. Host-key verification and the intended authentication methods are
part of that first usable slice, not follow-up polish. Add jumps and forwarding
only after the direct path passes interoperability and lifecycle tests.

Choose `aws-lc-rs` versus `ring` only after target builds and size measurements.
Do not disable RSA, compression, or other interoperability features merely to
make a headline binary smaller without measuring the hosts users actually need
to reach. Insecure legacy algorithms remain off by default.

### Make backend selection explicit

For a program compiled with both adapters, expose these conceptual choices:

```text
openssh
russh
```

An invocation/session override should take precedence over a machine-local
default. The GUI can expose the same setting in local application preferences;
the CLI and TUI can expose a global `--ssh-backend` option. A corresponding
environment override may be added for automation. Exact names should remain
consistent across programs.

Desktop defaults to `openssh`. Android, where the OpenSSH adapter is not
available, defaults to `russh`. If a requested backend was not compiled into
that artifact, fail with an actionable message. Do not silently switch engines
after an authentication, host-key, option, or connection failure: automatic
fallback could apply different security policy or ignore required behavior.

Before starting, capability analysis must report whether the selected engine
supports the resolved host and action. OpenSSH-specific action templates and
tmux integration should force OpenSSH or fail clearly. Unsupported
`ssh_options` must name the offending entries.

## Packaging and size policy

There are three realistic packaging models:

| Model | Size effect | User experience | Recommendation |
| --- | --- | --- | --- |
| Add `russh` to every desktop binary | Every user pays the size/dependency cost | Runtime selector is always present | Reject as the default |
| Build OpenSSH-only and dual-backend variants | Standard artifacts stay small | Users deliberately download the embedded edition | Preferred if desktop demand exists |
| Separate helper executable/plugin | Main binary stays small | Extra protocol, installation, lifecycle and versioning complexity | Do not introduce initially |

The standard Linux and Windows CLI, TUI, and GUI artifacts should remain
OpenSSH-only. An Android package necessarily contains `russh`. If an embedded
desktop edition is published, name it clearly, document its different security
and compatibility surface, and do not replace the ordinary download without
evidence that the tradeoff benefits most users.

Cargo features should default off and be enabled by the application package,
not by `stassh-core`. CI must build both configurations so the optional path
does not decay and the OpenSSH-only graph remains free of `russh`, Tokio added
for this purpose, and the selected crypto backend.

The spike must record stripped release sizes for baseline and embedded builds
of every affected executable on representative targets. It should also record
clean build time and package/installer size. Compare identical compiler,
profile, LTO, panic, and strip settings; debug binaries are not meaningful for
this decision.

## Feasibility spike and gates

Time-box a serious spike before committing the public backend selector. It
should prove:

1. `russh` and the chosen crypto backend build for x86-64 Linux GNU and musl,
   x86-64 Windows MSVC, Android arm64, and the selected Android emulator ABI;
2. a direct interactive PTY session supports byte-safe input/output, Unicode,
   resize, EOF, exit status, disconnect, timeout, and prompt cancellation;
3. password, keyboard-interactive, unencrypted and encrypted supported keys,
   and the intended desktop agent integrations work;
4. strict first-use and changed-host-key behavior works without any accept-all
   production path;
5. one two-hop connection, local forwarding, remote forwarding, and dynamic
   forwarding can be implemented with bounded tasks and reliable cleanup;
6. modern OpenSSH servers, at least one older supported server/device profile,
   and deliberately incompatible algorithm configurations produce useful
   results and errors;
7. unsupported `ssh_options`, tmux, and OpenSSH-specific action templates are
   detected before a session starts;
8. baseline versus embedded binary, package, build-time, idle-memory, and
   connection-memory measurements are recorded;
9. dependency licenses, advisories, release cadence, MSRV, and Android native
   build requirements are acceptable.

The spike should use disposable local/container SSH servers and generated test
keys. CI fixtures must not contain production hosts, keys, passwords, or
accepted host records.

### Go criteria

- Direct sessions and required authentication are reliable on all target
  platforms.
- Host-key handling fails closed and has an implementable frontend contract.
- Nested transports and required forwarding can be cancelled without leaked
  listeners or tasks.
- Capability diagnostics prevent silent semantic differences.
- Android build and lifecycle integration are viable.
- Measured size and build overhead are acceptable for the Android package and
  any explicitly opt-in desktop edition.

### No-go criteria

- Any production path requires accepting unknown host keys automatically.
- Required key formats, authentication, jumps, or forwarding cannot be made
  reliable with bounded complexity.
- The crypto backend fails required Android or desktop targets.
- The backend boundary leaks incompatible process and async-runtime assumptions
  throughout the domain core.
- Providing a desktop option would require adding the dependency to all
  ordinary artifacts despite low demonstrated demand.

## Final recommendation

Do not replace OpenSSH and do not make `russh` a default `stassh-core`
dependency. OpenSSH remains the best desktop default because it provides mature
configuration compatibility, authentication and agent integration, host-key
policy, forwarding, terminal behavior, and security servicing without making
stassh implement all of those policies.

Proceed with the engine-neutral session refactor only when Android work begins
or another concrete consumer needs it, and validate `russh` through the spike
above. If successful, keep the implementation in an optional backend crate and
make it the Android engine. Later, a separately packaged dual-backend desktop
edition can allow users to force `russh`, especially on Windows systems without
OpenSSH.

This approach captures the portability advantage without imposing permanent
binary bloat and dependency cost on the default case. It also avoids promising
false parity: `russh` is a capable protocol foundation, but stassh must earn
each advertised OpenSSH-compatible behavior through explicit implementation,
security policy, and interoperability tests.

## References

- `russh`, [upstream repository and supported capabilities](https://github.com/Eugeny/russh)
- `russh`, [client API documentation](https://docs.rs/russh/latest/russh/client/)
- `russh`, [key loading and known-host APIs](https://docs.rs/russh/latest/russh/keys/)
- `russh`, [current crate manifest and feature graph](https://github.com/Eugeny/russh/blob/main/russh/Cargo.toml)
- Tokio, [runtime documentation](https://docs.rs/tokio/latest/tokio/runtime/)
- Cargo Reference, [features and feature unification](https://doc.rust-lang.org/cargo/reference/features.html)
- OpenSSH, [`ssh_config` reference](https://man.openbsd.org/ssh_config)
- stassh, [Android support assessment](TODO-android.md)
- stassh, [Windows support assessment](TODO-windows.md)
