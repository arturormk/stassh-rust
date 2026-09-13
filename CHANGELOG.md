# Changelog

All notable changes to this project will be documented in this file.

## Unreleased

### Added

- GUI embedded SSH terminals now launch with `TERM=xterm-256color`, matching the
  xterm.js frontend so OpenSSH can forward a useful terminal type to remote
  sessions.
- The GUI quickstart script now asks which Linux bundle combination to build and
  can offer to install successfully built `.deb` or `.rpm` packages.
- GUI quickstart package installation now passes absolute artifact paths to
  package managers so local `.deb` files are not mistaken for package names.
- GUI terminal panes now support `Ctrl+Shift+C` to copy selected terminal text
  and `Ctrl+Shift+V` to paste from the clipboard.
- GUI Main layout mode now uses pane selection to promote the main terminal and
  only shows the full-screen control on the current main pane.
- The GUI Inspector now follows the last selected host across tree selections,
  terminal tabs, layout panes, and exited terminal panes.
- Actions now support config-aware SSH transfer templates for helpers such as
  `scp` and `rsync`; the examples now include both send-file and
  directory-sync helpers that use stassh's resolved OpenSSH configuration.
