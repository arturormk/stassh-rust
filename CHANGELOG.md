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
- GUI quickstart `.deb` package installation now installs local artifacts with
  `dpkg` so same-version local builds replace the installed package without
  triggering `_apt` sandbox file-access warnings.
- GUI terminal panes now support `Ctrl+Shift+C` to copy selected terminal text
  and `Ctrl+Shift+V` to paste from the clipboard.
- GUI Broadcast layouts now apply the exited-pane `Enter` close action to every
  exited pane in that layout.
- GUI layout Broadcast mode is now more visible: the Broadcast control sits on
  the right side of the layout toolbar, and active broadcast layouts highlight
  each participating terminal pane header.
- GUI layout tabs now include a per-layout `Min 72` toggle that keeps narrow
  terminal panes at least 72 columns wide and adds horizontal scrolling instead
  of forcing heavy line wrapping.
- GUI folder Inspector details now include a primary Open All action for opening
  every direct host in the selected folder, with Rename and Delete grouped
  beneath it as secondary maintenance actions.
- The GUI empty workspace view can now probe every host in a selected folder and
  its subfolders with noninteractive SSH connection attempts and live
  success/failure counts.
- GUI Ping All results now show when the selected-folder probe last ran, and the
  Workspace panel scrolls when large result sets exceed the visible area.
- GUI Main layout mode now uses pane selection to promote the main terminal and
  only shows the full-screen control on the current main pane.
- The GUI Inspector now follows the last selected host across tree selections,
  terminal tabs, layout panes, and exited terminal panes.
- Actions now support config-aware SSH transfer templates for helpers such as
  `scp` and `rsync`; the examples now include both send-file and
  directory-sync helpers that use stassh's resolved OpenSSH configuration.
