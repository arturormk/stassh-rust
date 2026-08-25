use std::io;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::process::Command;

use anyhow::{Context, Result};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::Backend;
use stassh_core::prepare_openssh_command;

use crate::app::App;
use crate::tmux;

pub(crate) fn connect_selected<B: Backend + io::Write>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
    let Some(host_id) = app.selected_host_id() else {
        app.status = "select a host to connect".to_string();
        return Ok(());
    };

    let resolved = app
        .vault
        .resolve_host(stassh_core::HostSelector::Id(host_id))?;
    let (command, _temp_config) =
        prepare_openssh_command(&resolved, &app.local_config).context("failed to prepare ssh")?;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    eprintln!("connecting: {}", command.render_for_display());
    let mut ssh = Command::new(&command.program);
    ssh.args(&command.args);
    let result = run_foreground_command(ssh).context("failed to launch ssh");

    let restore_result = restore_tui_terminal(terminal);

    app.status = match result {
        Ok(status) => match status.code() {
            Some(code) if status.success() => format!("ssh exited successfully: {code}"),
            Some(code) => format!("ssh exited with status: {code}"),
            None => "ssh terminated by signal".to_string(),
        },
        Err(error) => format!("ssh failed: {error}"),
    };
    restore_result?;
    Ok(())
}

pub(crate) fn open_tmux_window(app: &mut App) -> Result<()> {
    if !tmux::is_inside_tmux() {
        app.status = "tmux unavailable: start stassh-tui inside tmux to use t".to_string();
        return Ok(());
    }
    let Some(host_id) = app.selected_host_id() else {
        app.status = "select a host to open in tmux".to_string();
        return Ok(());
    };

    let resolved = app
        .vault
        .resolve_host(stassh_core::HostSelector::Id(host_id))?;
    let command = tmux::prepare_window_command(
        &resolved,
        &app.local_config,
        &tmux::default_temp_config_dir(),
    )
    .context("failed to prepare tmux window command")?;
    let status = tmux::launch_window(&command).context("failed to launch tmux")?;

    app.status = if status.success() {
        format!("opened tmux window: {}", command.title)
    } else {
        format!("tmux exited with status: {status}")
    };
    Ok(())
}

fn restore_tui_terminal<B: Backend + io::Write>(terminal: &mut Terminal<B>) -> Result<()> {
    execute!(
        terminal.backend_mut(),
        EnterAlternateScreen,
        EnableMouseCapture
    )?;
    enable_raw_mode()?;
    terminal.clear()?;
    Ok(())
}

#[cfg(unix)]
fn run_foreground_command(mut command: Command) -> io::Result<std::process::ExitStatus> {
    prepare_foreground_child(&mut command);
    let _sigint_guard = SignalGuard::ignore(libc::SIGINT)?;
    let _sigtou_guard = SignalGuard::ignore(libc::SIGTTOU)?;
    let original_pgrp = terminal_foreground_pgrp();
    let mut child = command.spawn()?;

    if let Some(original_pgrp) = original_pgrp {
        let child_pgrp = child.id() as libc::pid_t;
        unsafe {
            libc::setpgid(child_pgrp, child_pgrp);
            libc::tcsetpgrp(libc::STDIN_FILENO, child_pgrp);
        }
        let status = wait_for_child(&mut child);
        unsafe {
            libc::tcsetpgrp(libc::STDIN_FILENO, original_pgrp);
        }
        status
    } else {
        wait_for_child(&mut child)
    }
}

#[cfg(not(unix))]
fn run_foreground_command(mut command: Command) -> io::Result<std::process::ExitStatus> {
    command.status()
}

#[cfg(unix)]
fn wait_for_child(child: &mut std::process::Child) -> io::Result<std::process::ExitStatus> {
    loop {
        match child.wait() {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => return result,
        }
    }
}

#[cfg(unix)]
fn prepare_foreground_child(command: &mut Command) {
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) != 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::signal(libc::SIGINT, libc::SIG_DFL) == libc::SIG_ERR {
                return Err(io::Error::last_os_error());
            }
            if libc::signal(libc::SIGQUIT, libc::SIG_DFL) == libc::SIG_ERR {
                return Err(io::Error::last_os_error());
            }
            if libc::signal(libc::SIGTSTP, libc::SIG_DFL) == libc::SIG_ERR {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(unix)]
fn terminal_foreground_pgrp() -> Option<libc::pid_t> {
    let pgrp = unsafe { libc::tcgetpgrp(libc::STDIN_FILENO) };
    if pgrp < 0 { None } else { Some(pgrp) }
}

#[cfg(unix)]
struct SignalGuard {
    signal: libc::c_int,
    previous: libc::sighandler_t,
}

#[cfg(unix)]
impl SignalGuard {
    fn ignore(signal: libc::c_int) -> io::Result<Self> {
        let previous = unsafe { libc::signal(signal, libc::SIG_IGN) };
        if previous == libc::SIG_ERR {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self { signal, previous })
        }
    }
}

#[cfg(unix)]
impl Drop for SignalGuard {
    fn drop(&mut self) {
        unsafe {
            libc::signal(self.signal, self.previous);
        }
    }
}
