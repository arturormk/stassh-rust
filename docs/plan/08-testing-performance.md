# Testing And Performance

## Unit Tests

Cover:

- model validation
- stable ID behavior across rename and move operations
- host resolution
- identity fingerprint parsing
- local identity mapping logic
- OpenSSH config generation
- jump-chain rendering
- forward rendering
- vault manifest parsing
- encryption wrapper behavior
- operation replay and merge ordering
- tombstones and deletion behavior

## Integration Tests

Use a local OpenSSH server or containerized SSH target where practical.

Cover:

- launching OpenSSH
- generated temporary config cleanup
- jump host behavior
- local, remote, and dynamic forwards
- tunnel-only sessions
- identity resolution from file and agent sources
- portable vault open/lock flows
- interrupted writes
- concurrent vault access
- partial operation files
- removable-storage disappearance where it can be simulated

## End-To-End Tests

Cover user workflows:

- CLI list/search/show/connect
- CLI JSON output for stable commands
- TUI browse/search/connect/return
- GUI search/connect/terminal tab behavior
- diagnostics views with redaction
- action execution once actions exist

The GUI should be checked with screenshots or equivalent visual tests where layout regressions are likely.

## Performance Metrics

Track performance over time rather than inventing perfect thresholds before prototypes exist.

Measure:

- CLI startup time
- TUI startup time
- cold GUI launch time
- warm GUI launch time
- idle CPU
- idle memory
- memory with one terminal
- memory with multiple terminals
- host search latency with 1,000+ hosts
- host tree render time with 1,000+ hosts
- terminal throughput
- typing latency during heavy terminal output
- vault unlock time
- vault save latency

## Modest Hardware

Periodically test on representative systems:

- low-power x86 laptop
- ARM64 or ARMv7 single-board computer
- older dual-core laptop
- Xorg with LXDE, LXQt, Openbox, or XFCE
- Linux virtual console or serial-like terminal
- USB 2.0 removable vault storage

Core, CLI, and TUI should remain comfortable on modest systems. GUI support can have a narrower platform matrix if native GUI dependencies require it.

## Security Regression Checks

Add tests or review checks for:

- no passwords or private key material in logs
- diagnostics redact sensitive values
- generated SSH configs are removed after session end
- portable mode avoids writing recent-history state
- host-key verification defaults remain OpenSSH-safe
- vault corruption is detected and reported clearly
