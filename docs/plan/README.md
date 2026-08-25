# stassh-rust Planning Documents

This directory turns the broader project concept in `docs/BLUEPRINT.md` into practical guidance for implementation.

The blueprint remains the product inspiration. These planning documents are the working development guide: they define early defaults, boundaries, and sequencing so contributors and coding agents can make consistent decisions without treating every illustrative detail in the blueprint as fixed.

## Project Names

Repository base name:

```text
stassh-rust
```

Executable names:

```text
stassh      CLI
stassh-tui  terminal UI
stassh-GUI  desktop GUI
```

Crate names should use the `stassh-` prefix. A likely starting point is:

```text
stassh-core
stassh-cli
stassh-tui
stassh-gui
```

Additional crates should be introduced only when they create a clear ownership boundary or reduce meaningful complexity.

## Reading Order

1. `01-product-scope.md`
2. `02-architecture.md`
3. `03-domain-model.md`
4. `04-vault-storage-sync.md`
5. `05-security-privacy.md`
6. `06-frontends.md`
7. `07-roadmap.md`
8. `08-testing-performance.md`

## Working Principles

- Prefer ordinary OpenSSH behavior over custom SSH protocol work.
- Keep core logic independent from CLI, TUI, and GUI presentation concerns.
- Make configuration portable, encrypted, versioned, and user-owned.
- Keep machine-local paths and session-local state out of synchronized records.
- Preserve a clear escape hatch to ordinary tools and inspectable commands.
- Build incrementally; avoid plugin systems, proprietary sync, and large platform assumptions early.
