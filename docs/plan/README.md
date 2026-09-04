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
stassh-gui  desktop GUI
```

Crate names should use the `stassh-` prefix. Current workspace/package names:

```text
stassh-core
stassh
stassh-tui
stassh-gui
```

Additional crates should be introduced only when they create a clear ownership boundary or reduce meaningful complexity.

## Reading Order

1. `01-product-scope.md`
2. `02-architecture.md`
3. `03-domain-model.md`
4. `04-vault-storage-portability.md`
5. `05-security-privacy.md`
6. `06-frontends.md`
7. `07-roadmap.md`
8. `08-testing-performance.md`

## Working Principles

- Prefer ordinary OpenSSH behavior over custom SSH protocol work.
- Keep core logic independent from CLI, TUI, and GUI presentation concerns.
- Make configuration portable, versioned, inspectable, and user-owned.
- Encrypt actual secrets in `secrets.json`; keep ordinary host metadata in
  plain JSON.
- Keep machine-local paths and session-local state out of portable records.
- Preserve a clear escape hatch to ordinary tools and inspectable commands.
- Let users sync or back up the JSON files with tools they already trust; avoid
  app-defined synchronization protocols, plugin systems, and large platform
  assumptions early.
