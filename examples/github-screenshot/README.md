# GitHub Screenshot Example

This directory contains fake, screenshot-safe `stassh` data. Hostnames, IP
addresses, fingerprints, and key paths are examples only.

Launch the TUI with:

```bash
cargo run -p stassh-tui -- \
  --vault examples/github-screenshot/vault.json \
  --local-config examples/github-screenshot/local.json
```

Useful hosts to highlight:

- `postgres-primary`: nested folder, two-hop jump chain, local port forward
- `api-prod-01`: production app host through `bastion-use1`
- `redis-cache`: dynamic SOCKS forward through `bastion-euw1`
- `kind-control-plane`: staging/lab host with a remote forward
