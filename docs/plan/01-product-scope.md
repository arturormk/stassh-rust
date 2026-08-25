# Product Scope

## Product Identity

`stassh-rust` is a portable, offline-first SSH workspace for users who administer many remote systems. It should organize hosts, identities, jumps, forwards, and actions while continuing to rely on ordinary OpenSSH for SSH behavior.

The product should feel like an enhanced system terminal with excellent organization and orchestration, not like a large remote-administration suite.

## Target Users

Primary users are technically capable people who already understand SSH or are willing to learn it:

- system administrators
- developers
- consultants
- field engineers
- network administrators
- embedded developers
- infrastructure support technicians
- homelab users
- researchers

The application should expose what it is doing instead of hiding SSH concepts behind opaque abstractions.

## Core Principles

- Lightweight startup, navigation, search, and terminal handling are product requirements.
- The application must work offline except for the network access needed to reach SSH hosts.
- There must be no mandatory account, hosted sync service, telemetry dependency, license check, or cloud backend.
- CLI, TUI, and GUI are first-class frontends over the same model and core behavior.
- Feature parity means semantic compatibility, not identical UI.
- Users should always be able to fall back to ordinary OpenSSH, external terminals, standard viewers, and file-copy backup tools.

## MVP Scope

The first useful version should aim for:

- folder hierarchy and host records
- usernames, ports, tags, and notes where practical
- identity references and machine-local identity resolution
- OpenSSH command/config generation
- direct SSH connect
- jump host support
- local storage with explicit format versioning
- CLI list/search/show/connect/diagnose basics
- TUI browse/search/connect basics
- encrypted portable vault support as soon as storage stabilizes

The MVP may ship in smaller slices. A useful CLI-only or CLI-plus-TUI release is acceptable before the GUI is ready.

## Initial Non-Goals

Do not prioritize these in the initial implementation:

- native SSH protocol implementation
- proprietary cloud sync
- team accounts or credential sharing
- integrated SFTP file manager
- monitoring dashboards
- embedded VNC/RDP/browser implementations
- stored passwords
- plugin marketplace
- AI assistant
- infrastructure discovery crawler
- configuration-management database features

Some of these may become useful later, but they should not distort the early architecture.
