# Optional Embedded VNC Client Assessment

Research snapshot: 2026-09-15

## Decision summary

Stassh should continue to treat VNC as an Action workflow. The normal desktop
path remains an Action that optionally starts a remote VNC server, creates an
SSH local forward, and launches the user's machine-local VNC viewer. That path
has the widest compatibility, lets users keep their preferred viewer, and adds
no remote-desktop implementation to ordinary stassh artifacts.

An embedded VNC surface is nevertheless useful where a separate viewer cannot
be assumed, especially Android and selected managed or restricted Windows
installations. The best first candidate is
[`noVNC`](https://github.com/novnc/noVNC), embedded in the React/Tauri GUI and
fed from a stassh-owned byte transport. It already provides a browser-native
RFB client, canvas rendering, keyboard and pointer input, scaling, clipboard
support, touch gestures, common encodings, and substantially broader field
experience than the available Rust client libraries.

Do not add noVNC to the normal GUI bundle, and do not put a VNC protocol
dependency in `stassh-core`. Publish embedded VNC only in a separately named
GUI build and include it in the future Android GUI after a successful spike.
The CLI and TUI should understand and diagnose a typed Action surface but
should not embed a graphical viewer.

The first supported embedded workflow should be **VNC over an SSH local
forward**. Classic VNC authentication is cryptographically weak and does not
encrypt the session. Direct embedded VNC should remain unavailable until the
selected implementation can provide an end-to-end encrypted transport and a
reviewed server-verification policy. Existing external-viewer Actions,
including direct-LAN workflows, remain unchanged.

The recommended decision is therefore **proceed with a noVNC/Tauri transport
and interoperability spike, then build a GUI-only SSH-forwarded VNC Action
surface if the spike passes**. A proof of concept is small; a production-quality
feature is not. Reliable startup ordering, high-volume binary transport,
credentials, input mapping, session cleanup, packaging isolation, hostile RFB
input, and Android lifecycle behavior all require explicit work.

## Existing stassh VNC model

Stassh already models VNC without owning the protocol. An Action can:

- run a `local_prepare` command;
- add a named local forward with a fixed, automatic, or environment-derived
  local port;
- run a remote command such as `x11vnc` through OpenSSH;
- substitute `{HOST}` and `{LOCAL_PORT:name}` into local launch arguments;
- launch a machine-local viewer through a capability mapping; and
- run cleanup commands when the Action ends.

The examples under `examples/actions` demonstrate forwarded and direct VNC.
The GUI resolves the Action, starts OpenSSH in a PTY, immediately spawns the
optional local viewer, and keeps the SSH child, local child, cleanup commands,
and temporary OpenSSH configuration in one session record. The delay wrapper
used by the example is a practical reminder that process creation does not
mean the SSH forward or remote VNC server is ready yet.

This structure should remain the standard. Embedded VNC changes the final
presentation step, not the meaning of the host, forward, remote command, or
cleanup phases:

```text
Action prepare
    -> OpenSSH or embedded SSH session
    -> remote VNC server and named local forward
    -> resolved loopback VNC endpoint
    -> external viewer OR embedded GUI surface
    -> coordinated close and cleanup
```

Trying to recognize VNC by parsing a viewer executable or arguments such as
`127.0.0.1::5900` would be brittle. Viewer syntaxes differ, arguments can be
wrapped by scripts, and the same local command mechanism intentionally supports
arbitrary programs. The portable Action needs an explicit typed surface when
it wants stassh itself to present VNC.

## Recommended client stack

### noVNC in the Tauri webview

noVNC is both a web application and a reusable JavaScript RFB client. Its
public `RFB` object attaches a remote desktop to an HTML element and accepts a
WebSocket URL, an existing WebSocket, an RTC data channel, or another object
that implements the required raw-channel contract. This is particularly well
matched to the existing React/Tauri GUI:

- the GUI already renders terminal sessions in a webview;
- no second native widget toolkit or child viewer window is required;
- noVNC owns canvas updates, cursor rendering, browser key translation, pointer
  handling, clipping, scaling, and touch gestures;
- the same React surface can be reused on Windows, Linux, and Android;
- compressed RFB bytes can cross the Tauri boundary while decoding and
  framebuffer composition stay in the webview; and
- noVNC is primarily MPL-2.0 licensed, which is compatible with distribution
  alongside this workspace subject to file-level license and notice duties.

At the research snapshot, the packaged noVNC release is 1.6.0. Upstream lists
RFB 3.x interoperability, Raw, CopyRect, RRE, Hextile, Tight, TightPNG, ZRLE,
JPEG, Zlib, and H.264 encodings; classical VNC and several vendor authentication
schemes; desktop resize and local scaling; clipboard text; local cursor; and
mobile-browser support. These are upstream claims, not a stassh compatibility
guarantee. The spike must test the server profiles stassh intends to advertise.

There is one important transport mismatch. A normal VNC server speaks RFB over
a TCP byte stream, while a browser-hosted noVNC client normally expects RFB in
WebSocket messages. Requiring users to install `websockify`, changing the
remote VNC server, or exposing a new listener would defeat the self-contained
goal. Stassh should provide the adaptation in-process.

### Preferred Tauri raw-channel bridge

The first spike should implement a WebSocket-compatible JavaScript channel for
noVNC without changing noVNC itself. Its conceptual interface is small:

```text
open / message / error / close callbacks
binary ready state and protocol metadata
send(bytes)
close()
```

On the Rust side, a session task owns a TCP connection to the resolved
loopback-forward endpoint. Ordered Tauri channels carry server bytes to the
webview, and raw binary command bodies carry client bytes back to Rust. Control
events such as connected, retrying, failed, and closed stay separate from the
high-volume byte path. Do not send framebuffer traffic through Tauri's general
JSON event system or encode it as arrays of JSON numbers.

This approach has several advantages:

- it binds no extra listening socket;
- it does not need WebSocket TLS or a locally trusted certificate;
- it can later wrap an in-memory stream supplied by the embedded `russh`
  backend instead of requiring a TCP forward; and
- session identity and authorization remain inside the existing Tauri command
  boundary.

Its throughput and copying behavior are not proven for this workload. The
spike must measure sustained compressed and Raw updates, backpressure, queue
growth, cancellation, and memory on every target webview. A channel that is
fast for terminal output is not automatically suitable for a changing 4K
framebuffer.

### Fallback loopback WebSocket proxy

If the Tauri channel cannot deliver acceptable throughput, the fallback is a
small in-process WebSocket-to-TCP proxy owned by each VNC Action session. It is
not an external `websockify` process and must:

- bind only to an explicitly resolved loopback address on an ephemeral port;
- require a high-entropy, single-session token in the initial request;
- validate the expected origin where the platform exposes it;
- accept only one intended client unless reconnect is explicitly enabled;
- apply bounded buffers and close both directions together;
- disappear before Action cleanup completes; and
- be allowed by the narrowest platform-specific content-security policy.

This fallback uses noVNC's most exercised transport path, but it adds a local
listener and may encounter mixed-content, certificate, CSP, and Android WebView
differences. It should be selected only from measured spike results, not
introduced alongside the raw-channel bridge as a second permanent path.

## Alternatives considered

### `vnc-rs`

[`vnc-rs`](https://github.com/HsuJv/vnc-rs) is the most relevant reusable Rust
client candidate found in this assessment. Version 0.5.3 is an asynchronous
MIT-or-Apache-2.0 protocol engine that accepts an async stream, consumes input
events, and emits framebuffer, cursor, clipboard, resize, and other events. Its
transport shape could work with either a TCP stream or a later embedded SSH
channel.

It is a credible fallback if embedding noVNC proves impossible, but it moves
substantially more work into stassh. Upstream documents successful use of
Tight, ZRLE, CopyRect, and Raw against a limited set of Windows, Ubuntu, and
macOS servers. Hextile and RRE are intentionally not implemented, and the
documented authentication example covers classical VNC password
authentication rather than the breadth offered by mature viewers.

Stassh would also need to own or adopt:

- a high-performance canvas or GPU renderer and dirty-rectangle composition;
- cursor image and hotspot behavior;
- keysym translation across Windows, Linux, and Android;
- pointer, wheel, multitouch, viewport, and soft-keyboard behavior;
- JPEG and other image decoding paths not fully surfaced by the example;
- clipboard policy and platform bridges; and
- a much larger server/security interoperability program.

Pure Rust is attractive, but it is not enough by itself to justify duplicating
the mature presentation layer noVNC already provides. Keep `vnc-rs` on the
spike matrix and reassess its maintenance and security coverage at that time.

### Emerging Rust viewers

Projects such as
[`DeskVNC`](https://github.com/psmux/DeskVNC),
[`IronVNC`](https://github.com/hkder/ironvnc), and
[`RV`](https://github.com/madeye/rv) demonstrate useful techniques and rapidly
improving Rust protocol support. DeskVNC is especially relevant because it uses
Tauri 2 and reports sending binary dirty rectangles to a WebGL texture rather
than copying entire framebuffers through IPC.

They should be treated as implementation references, not selected as stable
stassh dependencies at this snapshot. DeskVNC declares itself pre-1.0 with a
changing IPC contract, IronVNC is an application rather than a separated
library and documents incomplete server-key verification, and native viewer
UIs do not directly solve Android/Tauri reuse. Re-evaluate them during the
spike because this area is changing quickly.

### LibVNCClient and native viewer code

LibVNCClient has broad encoding and security-type support, Android build
instructions, and long real-world use. It also brings C FFI, CMake/NDK and
Windows native build work, crypto and image-library choices, and continuing
memory-safety and advisory review. More decisively, LibVNCServer/LibVNCClient is
GPL-2.0 licensed and upstream states that linking makes the application a GPL
derivative. TigerVNC viewer code is also GPL licensed.

That licensing model does not fit a linked component in the proposed
MIT-or-Apache-2.0 stassh artifact without deliberately changing the
distribution obligations for the combined application. Continue launching
GPL viewers as independent external programs when users choose them, but do not
link their libraries into the embedded edition under the current licensing
plan.

### Full custom RFB implementation

RFC 6143 makes a minimal Raw-encoding client look approachable. A useful viewer
also needs compression encodings, cursor and resize extensions, vendor security
types, malformed-input defense, clipboard semantics, keyboard mapping, mobile
gestures, and continuous interoperability work. Writing all of this directly
would create more permanent protocol and security ownership than the current
use case justifies. Reject it unless every reusable candidate fails and product
demand supports maintaining another network protocol implementation.

### External viewer intents on Android

Android could export a URI or intent to another installed viewer. That remains
a reasonable escape hatch, but application availability, accepted URI formats,
credential transfer, SSH tunnel reachability, background lifetime, and return
behavior vary. It cannot be the only Android Action implementation and does not
provide the integrated lifecycle requested here.

## Proposed Action surface contract

### Portable typed surface

Add a small dependency-free tagged type to the portable Action model when the
feature is implemented. Conceptually, a forwarded VNC Action becomes:

```json
{
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
  "surface": {
    "type": "vnc",
    "host": "127.0.0.1",
    "port": "{LOCAL_PORT:vnc}"
  },
  "cleanup": []
}
```

The proposed Rust shape is conceptual rather than a commitment to exact names:

```text
ActionDefinition.surface: Option<ActionSurface>

ActionSurface::Vnc {
    host: String,
    port: literal u16 or template string,
}

ResolvedActionSurface::Vnc {
    host: String,
    port: u16,
}
```

`host` supports the existing `{HOST}` context and `port` supports either a
literal or `{LOCAL_PORT:name}`. Resolution must reject unknown variables,
missing forward names, invalid hosts, non-numeric or out-of-range ports, and
ambiguous values before any process starts.

An Action may set `local_launch` or `surface`, not both. This preserves a clear
author choice:

- existing Actions keep launching any external program or mapped capability;
- embedded Actions explicitly request a UI surface;
- a normal build never guesses that an external command is VNC; and
- an embedded build can still run ordinary external-launch Actions.

The first implementation accepts an embedded VNC surface only when its endpoint
is the loopback address and its port is resolved from a named local Action
forward. A direct `{HOST}:5900` embedded surface is rejected by capability
analysis until secure direct transport is implemented. External direct-VNC
Actions remain valid because their viewer owns that security decision.

### Compatibility and format version

The current vault format is version 0 and deserialization does not preserve
unknown JSON fields when an older client saves the model again. Adding
`surface` as an optional field without changing the format version would allow
an older client to silently erase it.

Implementation should therefore introduce a new vault format version and an
explicit version-0-to-version-1 migration. All standard binaries still compile
the lightweight Action surface types even if they cannot render them, so they
can load, display, validate, and preserve the same vault. A binary that cannot
render `vnc` reports that limitation before starting the Action.

Do not store a per-machine viewer choice in `vault.json`. The external versus
embedded distinction is already explicit in the Action, while the availability
of an embedded renderer is an artifact capability. Do not add noVNC types,
Tauri channels, TCP streams, framebuffer buffers, or browser objects to the
public core model.

### Credentials and secrets

The surface descriptor contains no password or username value. For the first
slice, the GUI prompts for credentials requested by the server and holds them
only for the lifetime of the connection attempt or session. Passwords and
server-provided failure text must not appear in Action previews, terminal
output, diagnostics, crash reports, or logs.

Later, an Action may identify fields in the host's already-linked secrets set,
for example `vnc_user` and `vnc_password`, but the reference should name fields
rather than copy their values into the vault. Using stored secrets requires an
explicit unlock flow and the same zeroization and reveal controls used by the
rest of stassh. Do not silently reuse the SSH password as a VNC password.

## Security policy

RFB is a parser for complex, attacker-controlled binary data, including
compressed images and dimensions that influence allocation. The VNC server
must be treated as untrusted even when reached through a trusted SSH host. The
embedded edition needs limits for framebuffer dimensions, rectangle counts,
compressed and decoded sizes, clipboard length, pending input, queue depth,
and reconnect rate. Integer overflow, decompression bombs, invalid rectangles,
and allocation failure must close only the session, not the application.

RFC 6143 describes classical VNC authentication as cryptographically weak. It
truncates or pads the password to eight bytes for a DES challenge-response, and
the subsequent framebuffer and input stream remains unencrypted. It is not an
acceptable substitute for transport security.

For the first embedded release:

- require the VNC connection to traverse a stassh-owned SSH local forward;
- bind automatically allocated forwards to loopback;
- never offer RFB security type `None` as adequate protection by itself;
- expose the negotiated RFB version, security type, encoding, and tunnel state
  in diagnostics;
- fail rather than silently downgrade to an unprotected direct connection;
- cancel VNC before tearing down its SSH transport;
- keep credentials and clipboard contents out of logs; and
- promptly track noVNC, compression, image-decoder, webview, and bridge
  advisories.

Direct embedded VNC can be reconsidered when the stack supports a reviewed
combination such as verified WSS, verified VeNCrypt/X.509, or another
end-to-end encrypted RFB transport. Certificate validation, host naming,
pinning, trust-on-first-use behavior, and downgrade policy must be specified at
that time. A warning dialog alone is not the security boundary.

Clipboard synchronization should be off by default for the first beta or
require a clear per-session opt-in. Remote clipboard content is untrusted and
local clipboard content may contain passwords or tokens. View-only mode should
be available even though it does not replace transport security.

## Action and GUI lifecycle

### Startup

The current GUI launches the local viewer immediately after spawning OpenSSH.
An embedded surface needs explicit readiness handling:

1. validate the complete Action and frontend capabilities;
2. run `local_prepare` and parse its environment;
3. allocate ports and resolve the Action plan and surface;
4. start the SSH process, forward, and optional remote command;
5. attempt the resolved VNC endpoint with bounded exponential backoff;
6. distinguish connection refusal while starting from terminal SSH failure;
7. request VNC credentials only after the server identifies what it needs; and
8. mark the VNC tab connected only after RFB initialization completes.

Retries need a visible deadline and Cancel control. They stop immediately when
OpenSSH exits, the user closes the tab, or the application begins shutdown.
Do not implement readiness with an unconditional sleep or an external wrapper.

### VNC tab

Embedded VNC opens as a normal GUI tab associated with the host and Action. It
should provide:

- connecting, authenticating, connected, reconnecting, exited, and failed
  states;
- fit-to-tab and 1:1 scrollable display modes;
- focus, keyboard capture, pointer capture, wheel, and release-all-input
  behavior;
- view-only mode and an explicit clipboard toggle;
- remote desktop name, dimensions, negotiated security, and encoding details;
- Ctrl+Alt+Delete and a small set of mobile-accessible special keys;
- full-screen behavior consistent with terminal tabs; and
- access to the underlying Action/SSH status and output without mixing terminal
  bytes into RFB data.

The first release does not need to place VNC inside multi-terminal layout panes
or broadcast input. Those behaviors have terminal-specific sizing and safety
semantics. A later layout integration should introduce a generic visual-session
pane contract rather than pretending VNC is a terminal.

### Shutdown and failure

The VNC renderer, bridge task, SSH child, remote command, temporary
configuration, and cleanup commands form one Action session. Closing the VNC
tab must:

1. release held keys and pointer buttons;
2. disconnect noVNC and stop accepting new bytes;
3. close the bridge/TCP connection;
4. terminate the SSH Action session using the platform's normal process-tree
   policy; and
5. run cleanup exactly once.

If noVNC fails to initialize, authentication fails, or the RFB connection
drops, the GUI may offer reconnect while the SSH tunnel is still healthy.
Reconnect must create a fresh RFB connection and credential state. If SSH exits
or cleanup has started, reconnect is unavailable. One failing VNC tab must not
close unrelated terminal or VNC sessions.

## Android implications

Android is the clearest consumer because it cannot assume an external VNC
viewer executable. noVNC already targets mobile browsers, but a browser claim
does not prove correct behavior in the selected Android System WebView, Tauri
mobile bridge, or stassh navigation model.

The future Android application should reuse the same `ActionSurface::Vnc` and
React VNC component. Its data path differs below the surface:

```text
Action surface
    -> embedded russh local-forward/nested channel
    -> stassh byte bridge
    -> noVNC raw channel
    -> Android WebView canvas and input
```

If `russh` exposes the destination as an application-owned stream, Android
does not need to bind a real loopback port merely to feed noVNC. The VNC bridge
should accept an abstract byte stream so desktop OpenSSH/TCP and Android
embedded-SSH transports share the same lifecycle semantics.

The Android spike must additionally cover:

- one-finger pointer movement versus direct-touch modes;
- tap, long-press, drag, right-click, wheel, and pinch behavior;
- soft keyboard invocation, modifier keys, international layouts, and special
  keys;
- device rotation, WebView recreation, background/foreground transitions, and
  process death;
- framebuffer memory on low-memory devices and high-resolution remote screens;
- clipboard permission and privacy behavior;
- keeping the screen awake only while the user requests it; and
- whether a long-lived user-visible session requires integration with the same
  foreground-service policy as SSH sessions.

Do not block the first useful Android SSH beta on VNC. Add embedded VNC after
the Android storage, secrets, session, and `russh` forwarding boundaries pass
their own gates.

## Packaging and dependency policy

The standard Linux and Windows GUI keeps only external viewer support and must
not bundle noVNC modules, decoders, fonts, images, bridge code, or a VNC-specific
networking dependency. The CLI and TUI gain only the core schema and
capability-diagnostic code.

A separately named embedded-VNC GUI artifact contains:

- the ordinary GUI and external Action runner;
- the noVNC library and stassh VNC React surface;
- the selected Tauri byte bridge;
- VNC credential and session controls; and
- embedded-surface integration and tests.

The external Action path remains present in that build. Users may still prefer
TigerVNC, RealVNC Viewer, or another mapped program for compatibility or
workflow reasons. The presence of an embedded renderer must not silently
rewrite or intercept `local_launch`.

The future Android GUI contains embedded VNC by default once it is supported.
Whether a desktop embedded-VNC artifact is later combined with the optional
`russh` desktop edition is a release-packaging decision; do not couple their
code or make one backend a prerequisite for the other. System OpenSSH plus
embedded noVNC is a valid restricted-Windows configuration.

CI must prove both positive and negative packaging properties:

- the normal GUI builds and tests without installing noVNC or the VNC bridge;
- the normal production bundle contains no noVNC assets;
- the embedded GUI builds on Linux and Windows;
- the Android application builds for arm64 and the selected emulator ABI; and
- each artifact reports whether `vnc` surfaces are supported before an Action
  starts.

Measure stripped executable, web assets, installer/package, clean build time,
idle memory, and connected-session memory for standard and embedded builds.
JavaScript package download size is not a substitute for the shipped artifact
measurement.

## Cost estimate

Assuming the existing GUI Action runner and OpenSSH forwarding remain the first
desktop transport:

| Deliverable | Estimated engineering time | Included work |
| --- | ---: | --- |
| Display proof of concept | 3–5 days | One forwarded x11vnc session, noVNC canvas, experimental byte bridge |
| Go/no-go spike | 1–2 weeks | Two bridge candidates, server matrix, throughput/memory, malformed input, packaging check |
| Linux/Windows embedded GUI beta | 4–7 weeks | Schema migration, resolver, readiness, credentials, VNC tab, lifecycle, tests, release variant |
| Android integration after SSH foundation | 3–6 additional weeks | `russh` stream adapter, touch/keyboard, lifecycle, device performance and packaging |
| Broader direct/security compatibility | Separate ongoing track | VeNCrypt/WSS/certificates, vendor auth, additional encodings and server profiles |

These are focused engineering estimates, not calendar promises. The desktop
beta range assumes noVNC works unmodified with one bridge and that VNC remains
an Action surface rather than growing an address book, discovery system, file
transfer, session recording, or general remote-desktop configuration UI.

Recurring maintenance includes noVNC and browser API updates, Windows WebView2
and Android System WebView testing, VNC server interoperability, malformed-input
security review, compression and image-decoder advisories, accessibility/input
regressions, and an additional release artifact.

## Feasibility spike and gates

Use disposable local/container or virtual-machine VNC servers and generated
credentials. Fixtures must not contain private infrastructure, real passwords,
or accepted production server identities.

The spike should prove:

1. an existing forwarded Action can start x11vnc, establish OpenSSH forwarding,
   resolve the typed surface, connect noVNC, and clean up without a wrapper
   sleep;
2. a custom noVNC raw channel can move ordered binary data through Tauri with
   bounded queues on Linux WebKitGTK and Windows WebView2;
3. if that fails, the authenticated loopback WebSocket fallback works without
   exposing a non-loopback listener or weakening CSP more than necessary;
4. Raw, CopyRect, Tight, ZRLE, cursor, resize, bell, and clipboard messages work
   against representative x11vnc, TigerVNC, QEMU, and LibVNCServer fixtures;
5. classical VNC password prompts work only inside the SSH tunnel and no
   credential reaches logs or previews;
6. keyboard modifiers, key release, pointer buttons, wheel input, scaling,
   full-screen, and view-only behavior are reliable;
7. malformed dimensions, rectangles, compressed input, clipboard messages,
   disconnects, and rapid reconnects fail with bounded memory;
8. closing at every startup phase stops retries, VNC, SSH, and cleanup exactly
   once;
9. the normal build graph and production assets remain free of noVNC and bridge
   dependencies; and
10. baseline versus embedded size, build time, CPU, memory, input latency, and
    sustained update measurements are recorded.

### Go criteria

- Typical 1080p administrative workloads remain responsive with no sustained
  queue growth; 4K behavior and limits are documented from measurements.
- The bridge adds no user-visible input lag on a LAN and remains stable during
  sustained updates, resize, reconnect, and application shutdown.
- Required server profiles negotiate supported encodings and produce useful
  errors for unsupported security types or extensions.
- VNC credentials, clipboard content, and framebuffer data stay out of logs and
  global application state.
- Closing or failing a surface never leaves an SSH process, forward, listener,
  held input state, or cleanup command orphaned.
- Windows and Linux embedded packages pass, and the standard artifact contains
  no embedded VNC payload.
- The same surface API has a credible bounded-memory Android transport path.

### No-go criteria

- Acceptable performance requires unbounded queues, repeated whole-framebuffer
  copies, or disabling important webview protections.
- The only working transport exposes an unauthenticated or non-loopback local
  WebSocket listener.
- Embedded startup requires fixed sleeps or commonly races the SSH forward and
  remote server.
- Required input, cursor, resize, or server profiles are unreliable and cannot
  be fixed without maintaining a large noVNC fork.
- The feature requires permitting direct plaintext RFB or silently falling back
  to it.
- Optional packaging cannot keep noVNC and its bridge out of the normal GUI.
- Android System WebView performance, input, or lifecycle behavior is not
  viable on the oldest supported device profile.

## Test plan for implementation

### Core and schema

- deserialize version-0 Actions and migrate them without semantic changes;
- round-trip version-1 external-launch and VNC-surface Actions;
- reject an Action containing both `local_launch` and `surface`;
- resolve literal ports and `{LOCAL_PORT:name}` after allocation;
- reject missing names, invalid ports, unknown templates, non-loopback first
  release targets, and surfaces not associated with a local Action forward;
- report unsupported GUI-only surfaces before running prepare, SSH, or cleanup;
  and
- confirm normal binaries preserve all surface fields.

### Bridge and protocol

- preserve byte ordering in both directions across arbitrary chunk boundaries;
- exercise backpressure, slow consumers, server floods, EOF, half-close,
  cancellation, and reconnect;
- cap buffers and allocation-driving RFB fields;
- test authentication success, failure, cancellation, and retry;
- render incremental rectangles, CopyRect, cursor, resize, and multiple pixel
  formats correctly; and
- fuzz or property-test the stassh-owned framing and session boundary even
  though noVNC owns RFB parsing.

### GUI and lifecycle

- show simulated connecting, authentication, connected, disconnected, and
  failed surfaces without a real server;
- verify fit and 1:1 modes, resize, focus, modifiers, stuck-key release,
  pointer buttons, wheel, view-only, clipboard opt-in, and full-screen;
- close during prepare, SSH startup, endpoint retry, authentication, active
  display, reconnect, and cleanup;
- kill SSH and VNC independently and verify state and recovery choices;
- run multiple VNC and terminal tabs without cross-session events;
- ensure external VNC Actions still launch the mapped program unchanged; and
- visually test representative resolutions, aspect ratios, scaling modes,
  disconnect overlays, and credential prompts.

### Platform and performance

- exercise Linux WebKitGTK and current Windows WebView2 on clean machines;
- test Windows display scaling, international keyboard layouts, clipboard, and
  process-tree cleanup;
- test Android arm64 hardware and the selected emulator ABI across the current
  and oldest supported WebView versions;
- measure 1080p and 4K text, scrolling, video-like change, idle, and reconnect
  workloads; and
- compare release artifacts and dependency inventories for standard and
  embedded builds.

## Final recommendation

Keep VNC external by default. The existing Action and capability model remains
the right standard desktop solution and should continue to support arbitrary
viewers without stassh restricting their behavior.

Add an explicit, dependency-free VNC Action surface to the shared model only
when implementation begins. Make it an alternative to `local_launch`, resolve
it through the existing named-forward mechanism, and require capability
analysis before any process starts. Bump the vault format version so older
clients cannot silently discard the new portable field.

Use noVNC as the preferred embedded renderer, connected to a stassh-owned
binary stream through a measured Tauri raw-channel bridge. Keep a hardened
loopback WebSocket proxy as the spike fallback. Ship the result only in a
separate GUI artifact and, later, the Android GUI. Do not put noVNC in
`stassh-core`, the CLI, the TUI, or normal desktop packages.

Limit the first release to SSH-forwarded VNC. This fits the existing Action
lifecycle, avoids treating weak classical VNC authentication as transport
security, works with system OpenSSH on desktop, and has a direct path to the
future embedded `russh` Android backend. Reconsider direct embedded VNC only
after encrypted transport and server verification are explicit, tested product
features.

## References

- noVNC, [upstream repository and feature list](https://github.com/novnc/noVNC)
- noVNC, [RFB integration API](https://github.com/novnc/noVNC/blob/master/docs/API.md)
- noVNC, [raw channel implementation](https://github.com/novnc/noVNC/blob/master/core/websock.js)
- noVNC, [license details](https://github.com/novnc/noVNC/blob/master/LICENSE.txt)
- RFC Editor, [RFC 6143: The Remote Framebuffer Protocol](https://www.rfc-editor.org/info/rfc6143/)
- RFB community, [extended protocol specification](https://github.com/rfbproto/rfbproto/blob/master/rfbproto.rst)
- `vnc-rs`, [upstream repository](https://github.com/HsuJv/vnc-rs)
- `vnc-rs`, [client API documentation](https://docs.rs/vnc-rs/latest/vnc/)
- LibVNCServer/LibVNCClient, [capabilities, build guidance, and license](https://github.com/LibVNC/libvncserver)
- DeskVNC, [Tauri/Rust implementation reference](https://github.com/psmux/DeskVNC)
- IronVNC, [Rust viewer implementation reference](https://github.com/hkder/ironvnc)
- RV, [native Rust viewer implementation reference](https://github.com/madeye/rv)
- Tauri, [channels for ordered streaming data](https://v2.tauri.app/develop/calling-frontend/)
- Tauri, [raw binary command bodies](https://v2.tauri.app/develop/calling-rust/)
- Tauri, [content security policy](https://v2.tauri.app/security/csp/)
- stassh, [GUI blueprint](BLUEPRINT-gui.md)
- stassh, [Android support assessment](TODO-android.md)
- stassh, [Windows support assessment](TODO-windows.md)
- stassh, [optional `russh` backend assessment](TODO-russh.md)
