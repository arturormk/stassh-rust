use std::io::{self, BufRead, Write};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::Backend;
use stassh_core::{
    ResolvedActionPlan, ResolvedHost, ResolvedLocalCommand, SimulatedShell, parse_prepare_env,
    prepare_openssh_command, resolve_action_local_prepare, resolve_action_plan,
    simulated_remote_command_output,
};
use uuid::Uuid;

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
    if app.simulation {
        run_simulated_selected(terminal, app, &resolved, None)?;
        return Ok(());
    }
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
    if app.simulation {
        app.status = "tmux disabled in simulation mode".to_string();
        return Ok(());
    }
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

pub(crate) fn run_selected_action<B: Backend + io::Write>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    action_id: Uuid,
) -> Result<()> {
    let Some(host_id) = app.selected_host_id() else {
        app.status = "select a host to run an action".to_string();
        return Ok(());
    };
    let resolved = app
        .vault
        .resolve_host(stassh_core::HostSelector::Id(host_id))?;
    let action = resolved
        .actions
        .iter()
        .find(|action| action.id == action_id)
        .cloned()
        .with_context(|| format!("action not found: {action_id}"))?;
    if app.simulation {
        let mut prelude = format!("running simulated action: {}\r\n", action.name);
        if let Some(remote_command) = &action.remote_command {
            prelude.push_str(&format!("remote command: {remote_command}\r\n"));
            prelude.push_str(&simulated_remote_command_output(remote_command));
        }
        if action.local_launch.is_some() {
            prelude.push_str("local launch skipped in simulation mode\r\n");
        }
        run_simulated_selected(terminal, app, &resolved, Some(prelude))?;
        app.mode = crate::app::Mode::Browse;
        return Ok(());
    }
    let local_prepare = resolve_action_local_prepare(&resolved, &action, &app.local_config)
        .context("failed to prepare action")?;
    let initial_plan = if local_prepare.is_none() {
        Some(
            resolve_action_plan(
                &resolved,
                &action,
                &app.local_config,
                &std::collections::HashMap::new(),
            )
            .context("failed to prepare action")?,
        )
    } else {
        None
    };

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    let result =
        run_action_after_terminal_release(local_prepare, initial_plan, &resolved, &action, app);
    let restore_result = restore_tui_terminal(terminal);

    app.mode = crate::app::Mode::Browse;
    app.status = match result {
        Ok(status) => match status.code() {
            Some(code) if status.success() => format!("action exited successfully: {code}"),
            Some(code) => format!("action exited with status: {code}"),
            None => "action terminated by signal".to_string(),
        },
        Err(error) => format!("action failed: {error}"),
    };
    restore_result?;
    Ok(())
}

fn run_simulated_selected<B: Backend + io::Write>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    resolved: &ResolvedHost,
    prelude: Option<String>,
) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    let result = run_simulated_foreground(resolved, prelude);
    let restore_result = restore_tui_terminal(terminal);

    app.status = match result {
        Ok(()) => format!("simulation session closed: {}", resolved.path),
        Err(error) => format!("simulation session failed: {error}"),
    };
    restore_result?;
    Ok(())
}

fn run_simulated_foreground(resolved: &ResolvedHost, prelude: Option<String>) -> Result<()> {
    let mut shell = SimulatedShell::for_host(resolved);
    let mut stdout = io::stdout();
    if let Some(prelude) = prelude {
        stdout.write_all(prelude.as_bytes())?;
    }
    stdout.write_all(shell.banner().as_bytes())?;
    stdout.flush()?;

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = line?;
        let output = shell.submit_line(&line);
        stdout.write_all(output.data.as_bytes())?;
        stdout.flush()?;
        if output.closed {
            return Ok(());
        }
    }

    let output = shell.close();
    stdout.write_all(output.data.as_bytes())?;
    stdout.flush()?;
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

fn run_action_after_terminal_release(
    local_prepare: Option<ResolvedLocalCommand>,
    initial_plan: Option<ResolvedActionPlan>,
    resolved: &stassh_core::ResolvedHost,
    action: &stassh_core::ActionDefinition,
    app: &App,
) -> Result<ExitStatus> {
    let prepare_env = if let Some(command) = &local_prepare {
        eprintln!("running local prepare: {}", display_local_command(command));
        let output = local_command(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .output()
            .context("failed to run local prepare")?;
        if !output.status.success() {
            anyhow::bail!("local prepare exited with status {}", output.status);
        }
        parse_prepare_env(&String::from_utf8_lossy(&output.stdout))
    } else {
        std::collections::HashMap::new()
    };

    let plan = if let Some(plan) = initial_plan {
        plan
    } else {
        resolve_action_plan(resolved, action, &app.local_config, &prepare_env)
            .context("failed to prepare action with local prepare environment")?
    };

    eprintln!("running action: {}", plan.action_name);
    eprintln!("connecting: {}", plan.ssh_command.render_for_display());
    let status = run_action_foreground(plan)?;
    Ok(status)
}

#[cfg(unix)]
fn run_action_foreground(plan: ResolvedActionPlan) -> Result<ExitStatus> {
    let mut ssh = Command::new(&plan.ssh_command.program);
    ssh.args(&plan.ssh_command.args);
    #[cfg(unix)]
    prepare_foreground_child(&mut ssh);
    let _sigint_guard = SignalGuard::ignore(libc::SIGINT)?;
    let _sigtou_guard = SignalGuard::ignore(libc::SIGTTOU)?;
    let original_pgrp = terminal_foreground_pgrp();
    let mut ssh_child = ssh.spawn().context("failed to launch ssh")?;

    let mut local_child;
    let status = if let Some(original_pgrp) = original_pgrp {
        let child_pgrp = ssh_child.id() as libc::pid_t;
        unsafe {
            libc::setpgid(child_pgrp, child_pgrp);
            libc::tcsetpgrp(libc::STDIN_FILENO, child_pgrp);
        }
        local_child = spawn_local_launch(plan.local_launch.as_ref())?;
        let status = wait_for_child(&mut ssh_child);
        unsafe {
            libc::tcsetpgrp(libc::STDIN_FILENO, original_pgrp);
        }
        status
    } else {
        local_child = spawn_local_launch(plan.local_launch.as_ref())?;
        wait_for_child(&mut ssh_child)
    };

    if let Some(child) = &mut local_child {
        terminate_child_tree(child);
    }
    for command in &plan.cleanup {
        let _ = local_command(command).status();
    }

    status.context("failed while waiting for ssh")
}

#[cfg(not(unix))]
fn run_action_foreground(plan: ResolvedActionPlan) -> Result<ExitStatus> {
    let mut ssh_child = Command::new(&plan.ssh_command.program)
        .args(&plan.ssh_command.args)
        .spawn()
        .context("failed to launch ssh")?;
    let mut local_child = spawn_local_launch(plan.local_launch.as_ref())?;
    let status = ssh_child.wait().context("failed while waiting for ssh")?;
    if let Some(child) = &mut local_child {
        terminate_child_tree(child);
    }
    for command in &plan.cleanup {
        let _ = local_command(command).status();
    }
    Ok(status)
}

fn spawn_local_launch(command: Option<&ResolvedLocalCommand>) -> Result<Option<Child>> {
    let Some(command) = command else {
        return Ok(None);
    };
    eprintln!(
        "launching local command: {}",
        display_local_command(command)
    );
    let mut process = local_command(command);
    #[cfg(unix)]
    prepare_local_child_group(&mut process);
    process
        .spawn()
        .map(Some)
        .context("failed to launch local command")
}

fn local_command(command: &ResolvedLocalCommand) -> Command {
    let mut process = Command::new(&command.program);
    process.args(&command.args).envs(&command.env);
    process
}

fn display_local_command(command: &ResolvedLocalCommand) -> String {
    let mut parts = vec![command.program.display().to_string()];
    parts.extend(command.args.clone());
    parts.join(" ")
}

#[cfg(unix)]
fn prepare_local_child_group(command: &mut Command) {
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(unix)]
fn terminate_child_tree(child: &mut Child) {
    let pgrp = child.id() as libc::pid_t;
    unsafe {
        libc::kill(-pgrp, libc::SIGTERM);
    }
    wait_or_kill(child, Duration::from_secs(2));
}

#[cfg(not(unix))]
fn terminate_child_tree(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn wait_or_kill(child: &mut Child, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
            Err(_) => return,
        }
    }
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
