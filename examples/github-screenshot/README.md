# GitHub Screenshot Example

This directory contains the screenshot-safe images used by the top-level
`README.md`.

The screenshots should be captured from simulation mode. Simulation mode loads
deterministic in-memory demo vault, local config, and secrets data, then routes
terminal sessions through scripted SSH-like shells instead of real hosts.

Launch the TUI from the repository root with:

```bash
./run-stassh-tui-dev.sh --simulation
```

Launch the GUI from the repository root with:

```bash
./run-stassh-gui-dev.sh --simulation
```

The committed images are:

- `stassh-tui-screenshot.jpg`
- `stassh-gui-screenshot.jpg`

Useful simulated hosts to highlight:

- `web-prod-01`: production web host through `bastion-01`, with a local HTTPS
  forward and a host-specific action
- `db-prod-01`: production database host with an intentionally unmapped identity
  for diagnostics screenshots
- `cache-prod-01`: production cache host through `bastion-01`
- `web-staging-01`: safe staging target for demos
- `metrics-01`: shared monitoring host with a dynamic SOCKS forward
