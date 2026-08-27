use std::io;
use std::path::PathBuf;
use std::time::Duration;

mod app;
mod connect;
mod editor;
mod tmux;
mod ui;

use anyhow::{Context, Result};
use app::{App, KeyAction, StatusPageKey};
use clap::Parser;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::layout::Rect;
use stassh_core::{
    SecretsStore, demo_workspace, ensure_home_stassh_permissions, load_local_config, load_secrets,
    load_vault, local_config_path, secrets_path, vault_path,
};

#[derive(Debug, Parser)]
#[command(name = "stassh-tui")]
#[command(version = env!("STASSH_VERSION"))]
#[command(about = "Terminal UI for the stassh SSH workspace")]
struct Cli {
    #[arg(long, global = true, value_name = "PATH")]
    vault: Option<PathBuf>,

    #[arg(long, global = true, value_name = "PATH")]
    local_config: Option<PathBuf>,

    #[arg(long = "secrets-file", global = true, value_name = "PATH")]
    secrets_file: Option<PathBuf>,

    #[arg(long, global = true)]
    simulation: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let status_page_key = if tmux::is_inside_byobu() {
        StatusPageKey::CtrlG
    } else {
        StatusPageKey::F1
    };
    let _ =
        tmux::cleanup_stale_temp_configs(&tmux::default_temp_config_dir(), tmux::STALE_CONFIG_AGE);
    let app = if cli.simulation {
        let workspace = demo_workspace()?;
        App::new(
            PathBuf::from("simulation://vault.json"),
            PathBuf::from("simulation://local.json"),
            PathBuf::from("simulation://secrets.json"),
            workspace.vault,
            workspace.local_config,
            Some(workspace.secrets_store),
            false,
            true,
            status_page_key,
        )
    } else {
        let vault_path = vault_path(cli.vault).context("failed to determine vault path")?;
        let local_config_path = local_config_path(cli.local_config, &vault_path);
        let secrets_path = secrets_path(cli.secrets_file, &vault_path);
        ensure_home_stassh_permissions(&[&vault_path, &local_config_path, &secrets_path])
            .with_context(|| "unsafe ~/.ssh/stassh permissions")?;
        let vault = load_vault(&vault_path)?;
        let local_config = load_local_config(&local_config_path)?;
        let secrets_store = load_optional_secrets(&secrets_path)?;
        App::new(
            vault_path,
            local_config_path,
            secrets_path,
            vault,
            local_config,
            secrets_store,
            tmux::is_inside_tmux(),
            false,
            status_page_key,
        )
    };
    run_tui(app)
}

fn load_optional_secrets(path: &PathBuf) -> Result<Option<SecretsStore>> {
    if path.exists() {
        Ok(Some(load_secrets(path)?))
    } else {
        Ok(None)
    }
}

fn run_tui(mut app: App) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let result = run_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    result
}

fn run_loop<B: Backend + io::Write>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::draw_ui(frame, app))?;

        if app.quit {
            break;
        }

        if event::poll(Duration::from_millis(250))? {
            let action = match event::read()? {
                Event::Key(key) => app.handle_key(key)?,
                Event::Mouse(mouse) => {
                    let size = terminal.size()?;
                    let tree_area = ui::ui_areas(Rect::new(0, 0, size.width, size.height)).tree;
                    app.handle_mouse(mouse, tree_area)?
                }
                _ => KeyAction::None,
            };
            match action {
                KeyAction::None => {}
                KeyAction::Connect => {
                    if let Err(error) = connect::connect_selected(terminal, app) {
                        app.status = format!("connect failed: {error}");
                    }
                }
                KeyAction::RunAction(action_id) => {
                    if let Err(error) = connect::run_selected_action(terminal, app, action_id) {
                        app.status = format!("action failed: {error}");
                    }
                }
                KeyAction::TmuxWindow => {
                    if let Err(error) = connect::open_tmux_window(app) {
                        app.status = format!("tmux failed: {error}");
                    }
                }
            }
        }
    }
    Ok(())
}
