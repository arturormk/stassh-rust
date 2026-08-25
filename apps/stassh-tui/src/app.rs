use std::{
    collections::BTreeSet,
    path::PathBuf,
    time::{Duration, Instant},
};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use stassh_core::{
    ActionDefinition, AddHost, Folder, Host, HostSelector, LocalConfig, UpdateHost, Vault,
    load_local_config, load_vault, save_vault,
};
use uuid::Uuid;

use crate::editor::{
    EditorAction, FolderEditor, FolderEditorMode, ForwardEditor, HostEditor, HostEditorMode,
    IdentityEditor, JumpEditor,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyAction {
    None,
    Connect,
    RunAction(Uuid),
    TmuxWindow,
}

const DOUBLE_CLICK: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MouseClick {
    target: MouseTarget,
    at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MouseTarget {
    Browse(usize),
    Search(usize),
    MoveFolder(usize),
}

#[derive(Debug)]
pub(crate) struct App {
    pub(crate) vault_path: PathBuf,
    pub(crate) local_config_path: PathBuf,
    pub(crate) vault: Vault,
    pub(crate) local_config: LocalConfig,
    pub(crate) tree: Vec<TreeItem>,
    pub(crate) selected: usize,
    pub(crate) search_selected: usize,
    pub(crate) mode: Mode,
    pub(crate) editor: Option<HostEditor>,
    pub(crate) folder_editor: Option<FolderEditor>,
    pub(crate) identity_editor: Option<IdentityEditor>,
    pub(crate) jump_editor: Option<JumpEditor>,
    pub(crate) forward_editor: Option<ForwardEditor>,
    pub(crate) pending_delete: Option<DeleteConfirmation>,
    pub(crate) selected_hosts: BTreeSet<Uuid>,
    pub(crate) collapsed_folders: BTreeSet<Uuid>,
    pub(crate) pending_move_hosts: Vec<Uuid>,
    pub(crate) move_folder_selected: usize,
    pub(crate) action_selected: usize,
    pub(crate) search: String,
    pub(crate) status_page: usize,
    pub(crate) show_diagnostics: bool,
    pub(crate) status: String,
    pub(crate) quit: bool,
    pub(crate) tmux_available: bool,
    last_mouse_click: Option<MouseClick>,
}

impl App {
    pub(crate) fn new(
        vault_path: PathBuf,
        local_config_path: PathBuf,
        vault: Vault,
        local_config: LocalConfig,
        tmux_available: bool,
    ) -> Self {
        let collapsed_folders = default_collapsed_folders(&vault);
        let tree = build_tree(&vault, &collapsed_folders);
        Self {
            vault_path,
            local_config_path,
            vault,
            local_config,
            tree,
            selected: 0,
            search_selected: 0,
            mode: Mode::Browse,
            editor: None,
            folder_editor: None,
            identity_editor: None,
            jump_editor: None,
            forward_editor: None,
            pending_delete: None,
            selected_hosts: BTreeSet::new(),
            collapsed_folders,
            pending_move_hosts: Vec::new(),
            move_folder_selected: 0,
            action_selected: 0,
            search: String::new(),
            status_page: 0,
            show_diagnostics: false,
            status: String::new(),
            quit: false,
            tmux_available,
            last_mouse_click: None,
        }
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> Result<KeyAction> {
        if key.code == KeyCode::F(1) {
            self.status_page = self.status_page.saturating_add(1);
            return Ok(KeyAction::None);
        }
        match self.mode {
            Mode::Browse => self.handle_browse_key(key),
            Mode::Search => Ok(self.handle_search_key(key)),
            Mode::EditHost => self.handle_editor_key(key),
            Mode::EditFolder => self.handle_folder_editor_key(key),
            Mode::EditIdentity => self.handle_identity_editor_key(key),
            Mode::EditJumps => self.handle_jump_editor_key(key),
            Mode::EditForwards => self.handle_forward_editor_key(key),
            Mode::ActionPalette => Ok(self.handle_action_palette_key(key)),
            Mode::ConfirmDelete => self.handle_delete_key(key),
            Mode::PickMoveFolder => self.handle_move_folder_key(key),
        }
    }

    pub(crate) fn handle_mouse(&mut self, mouse: MouseEvent, tree_area: Rect) -> Result<KeyAction> {
        let MouseEventKind::Down(MouseButton::Left) = mouse.kind else {
            return Ok(KeyAction::None);
        };
        let Some(target) = self.mouse_target(mouse, tree_area) else {
            return Ok(KeyAction::None);
        };
        let now = Instant::now();
        let double_click = self.last_mouse_click.as_ref().is_some_and(|last| {
            last.target == target && now.duration_since(last.at) <= DOUBLE_CLICK
        });
        self.select_mouse_target(target);
        if double_click {
            self.last_mouse_click = None;
            self.activate_mouse_target(target)
        } else {
            self.last_mouse_click = Some(MouseClick { target, at: now });
            Ok(KeyAction::None)
        }
    }

    fn handle_browse_key(&mut self, key: KeyEvent) -> Result<KeyAction> {
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('/') => {
                self.mode = Mode::Search;
                self.search.clear();
                self.search_selected = 0;
                self.status.clear();
            }
            KeyCode::Char('d') => self.show_diagnostics = !self.show_diagnostics,
            KeyCode::Char('e') => self.start_edit_selected_item(),
            KeyCode::Char('i') => self.start_edit_selected_identity(),
            KeyCode::Char('J') => self.start_edit_selected_jumps(),
            KeyCode::Char('F') => self.start_edit_selected_forwards(),
            KeyCode::Char('a') => self.start_action_palette(),
            KeyCode::Char('C') => self.copy_selected_host()?,
            KeyCode::Char('n') => self.start_create_host(),
            KeyCode::Char('f') => self.start_create_folder(),
            KeyCode::Char('m') => self.start_move_hosts(),
            KeyCode::Char('u') => self.clear_selected_hosts(),
            KeyCode::Char('x') | KeyCode::Delete => self.start_delete_selected_host(),
            KeyCode::Char('r') => self.reload()?,
            KeyCode::Char('t') => return Ok(KeyAction::TmuxWindow),
            KeyCode::Enter => {
                if self.toggle_selected_folder_expansion() {
                    return Ok(KeyAction::None);
                }
                return Ok(KeyAction::Connect);
            }
            KeyCode::Char(' ') => self.toggle_current_selection(),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Home => self.move_to_first_sibling(),
            KeyCode::End => self.move_to_last_sibling(),
            KeyCode::PageUp => self.move_to_parent(),
            KeyCode::PageDown => self.move_to_last_sibling(),
            KeyCode::Esc => self.status.clear(),
            _ => {}
        }
        Ok(KeyAction::None)
    }

    fn mouse_target(&self, mouse: MouseEvent, tree_area: Rect) -> Option<MouseTarget> {
        let visible_height = tree_area.height.saturating_sub(2) as usize;
        if visible_height == 0
            || tree_area.width < 2
            || mouse.column <= tree_area.x
            || mouse.column
                >= tree_area
                    .x
                    .saturating_add(tree_area.width)
                    .saturating_sub(1)
            || mouse.row <= tree_area.y
            || mouse.row
                >= tree_area
                    .y
                    .saturating_add(tree_area.height)
                    .saturating_sub(1)
        {
            return None;
        }
        let row = (mouse.row - tree_area.y - 1) as usize;
        match self.mode {
            Mode::Browse => {
                let offset = list_scroll_offset(self.selected, visible_height);
                let index = offset + row;
                (index < self.tree.len()).then_some(MouseTarget::Browse(index))
            }
            Mode::Search => {
                let len = self.search_matches().len();
                let offset = list_scroll_offset(self.search_selected, visible_height);
                let index = offset + row;
                (index < len).then_some(MouseTarget::Search(index))
            }
            Mode::PickMoveFolder => {
                let len = self.folder_picker_items().len();
                let offset = list_scroll_offset(self.move_folder_selected, visible_height);
                let index = offset + row;
                (index < len).then_some(MouseTarget::MoveFolder(index))
            }
            Mode::EditHost
            | Mode::EditFolder
            | Mode::EditIdentity
            | Mode::EditJumps
            | Mode::EditForwards
            | Mode::ActionPalette
            | Mode::ConfirmDelete => None,
        }
    }

    fn select_mouse_target(&mut self, target: MouseTarget) {
        match target {
            MouseTarget::Browse(index) => {
                self.selected = index.min(self.tree.len().saturating_sub(1))
            }
            MouseTarget::Search(index) => {
                self.search_selected = index.min(self.search_matches().len().saturating_sub(1))
            }
            MouseTarget::MoveFolder(index) => {
                self.move_folder_selected =
                    index.min(self.folder_picker_items().len().saturating_sub(1))
            }
        }
    }

    fn activate_mouse_target(&mut self, target: MouseTarget) -> Result<KeyAction> {
        match target {
            MouseTarget::Browse(index) => {
                self.selected = index.min(self.tree.len().saturating_sub(1));
                if self.toggle_selected_folder_expansion() {
                    Ok(KeyAction::None)
                } else {
                    Ok(KeyAction::Connect)
                }
            }
            MouseTarget::Search(index) => {
                self.search_selected = index.min(self.search_matches().len().saturating_sub(1));
                Ok(KeyAction::Connect)
            }
            MouseTarget::MoveFolder(index) => {
                self.move_folder_selected =
                    index.min(self.folder_picker_items().len().saturating_sub(1));
                self.confirm_move_hosts()?;
                Ok(KeyAction::None)
            }
        }
    }

    fn handle_editor_key(&mut self, key: KeyEvent) -> Result<KeyAction> {
        let Some(editor) = &mut self.editor else {
            self.mode = Mode::Browse;
            return Ok(KeyAction::None);
        };
        match editor.handle_key(key) {
            EditorAction::None => {}
            EditorAction::Cancel => {
                self.editor = None;
                self.mode = Mode::Browse;
                self.status = "edit cancelled".to_string();
            }
            EditorAction::Save => self.save_editor()?,
        }
        Ok(KeyAction::None)
    }

    fn handle_folder_editor_key(&mut self, key: KeyEvent) -> Result<KeyAction> {
        let Some(editor) = &mut self.folder_editor else {
            self.mode = Mode::Browse;
            return Ok(KeyAction::None);
        };
        match editor.handle_key(key) {
            EditorAction::None => {}
            EditorAction::Cancel => {
                self.folder_editor = None;
                self.mode = Mode::Browse;
                self.status = "edit cancelled".to_string();
            }
            EditorAction::Save => self.save_folder_editor()?,
        }
        Ok(KeyAction::None)
    }

    fn handle_identity_editor_key(&mut self, key: KeyEvent) -> Result<KeyAction> {
        let Some(editor) = &mut self.identity_editor else {
            self.mode = Mode::Browse;
            return Ok(KeyAction::None);
        };
        match editor.handle_key(key) {
            EditorAction::None => {}
            EditorAction::Cancel => {
                self.identity_editor = None;
                self.mode = Mode::Browse;
                self.status = "identity edit cancelled".to_string();
            }
            EditorAction::Save => self.save_identity_editor()?,
        }
        Ok(KeyAction::None)
    }

    fn handle_jump_editor_key(&mut self, key: KeyEvent) -> Result<KeyAction> {
        let Some(editor) = &mut self.jump_editor else {
            self.mode = Mode::Browse;
            return Ok(KeyAction::None);
        };
        match editor.handle_key(key) {
            EditorAction::None => {}
            EditorAction::Cancel => {
                self.jump_editor = None;
                self.mode = Mode::Browse;
                self.status = "jumps edit cancelled".to_string();
            }
            EditorAction::Save => self.save_jump_editor()?,
        }
        Ok(KeyAction::None)
    }

    fn handle_forward_editor_key(&mut self, key: KeyEvent) -> Result<KeyAction> {
        let Some(editor) = &mut self.forward_editor else {
            self.mode = Mode::Browse;
            return Ok(KeyAction::None);
        };
        match editor.handle_key(key) {
            EditorAction::None => {}
            EditorAction::Cancel => {
                self.forward_editor = None;
                self.mode = Mode::Browse;
                self.status = "forwards edit cancelled".to_string();
            }
            EditorAction::Save => self.save_forward_editor()?,
        }
        Ok(KeyAction::None)
    }

    fn handle_search_key(&mut self, key: KeyEvent) -> KeyAction {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Browse;
                self.search.clear();
                self.search_selected = 0;
            }
            KeyCode::Delete => {
                self.start_delete_selected_host();
            }
            KeyCode::Char('i') => {
                self.start_edit_selected_identity();
            }
            KeyCode::Char('J') => {
                self.start_edit_selected_jumps();
            }
            KeyCode::Char('F') => {
                self.start_edit_selected_forwards();
            }
            KeyCode::Char('C') => {
                if let Err(error) = self.copy_selected_host() {
                    self.status = format!("copy error: {error}");
                }
            }
            KeyCode::Enter => return KeyAction::Connect,
            KeyCode::Char(' ') => self.toggle_current_selection(),
            KeyCode::Backspace => {
                self.search.pop();
                self.clamp_search_selection();
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_search_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_search_selection(-1),
            KeyCode::Home => self.search_selected = 0,
            KeyCode::End => self.search_selected = self.search_matches().len().saturating_sub(1),
            KeyCode::Char(value) => {
                self.search.push(value);
                self.clamp_search_selection();
            }
            _ => {}
        }
        KeyAction::None
    }

    fn handle_action_palette_key(&mut self, key: KeyEvent) -> KeyAction {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Browse;
                self.status.clear();
            }
            KeyCode::Enter => {
                if let Some(action) = self.selected_action() {
                    return KeyAction::RunAction(action.id);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_action_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_action_selection(-1),
            KeyCode::Home => self.action_selected = 0,
            KeyCode::End => {
                self.action_selected = self.selected_actions().len().saturating_sub(1);
            }
            _ => {}
        }
        KeyAction::None
    }

    fn handle_delete_key(&mut self, key: KeyEvent) -> Result<KeyAction> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => self.confirm_delete_selected_host()?,
            KeyCode::Char('n') | KeyCode::Esc => {
                self.pending_delete = None;
                self.mode = Mode::Browse;
                self.status = "delete cancelled".to_string();
            }
            _ => {}
        }
        Ok(KeyAction::None)
    }

    fn handle_move_folder_key(&mut self, key: KeyEvent) -> Result<KeyAction> {
        match key.code {
            KeyCode::Esc => {
                self.pending_move_hosts.clear();
                self.mode = Mode::Browse;
                self.status = "move cancelled".to_string();
            }
            KeyCode::Enter => self.confirm_move_hosts()?,
            KeyCode::Down | KeyCode::Char('j') => self.move_folder_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_folder_selection(-1),
            KeyCode::Home => self.move_folder_selected = 0,
            KeyCode::End => {
                self.move_folder_selected = self.folder_picker_items().len().saturating_sub(1)
            }
            _ => {}
        }
        Ok(KeyAction::None)
    }

    fn reload(&mut self) -> Result<()> {
        self.vault = load_vault(&self.vault_path)?;
        self.local_config = load_local_config(&self.local_config_path)?;
        self.prune_collapsed_folders();
        self.rebuild_tree();
        self.selected = self.selected.min(self.tree.len().saturating_sub(1));
        self.search_selected = self
            .search_selected
            .min(self.search_matches().len().saturating_sub(1));
        self.prune_selected_hosts();
        self.pending_delete = None;
        self.pending_move_hosts.clear();
        self.action_selected = 0;
        self.editor = None;
        self.folder_editor = None;
        self.identity_editor = None;
        self.jump_editor = None;
        self.forward_editor = None;
        self.status = "vault reloaded".to_string();
        Ok(())
    }

    fn start_action_palette(&mut self) {
        if self.selected_host().is_none() {
            self.status = "select a host to choose actions".to_string();
            return;
        };
        let action_count = self.selected_actions().len();
        if action_count == 0 {
            self.status = "selected host has no actions".to_string();
            return;
        }
        self.action_selected = self.action_selected.min(action_count.saturating_sub(1));
        self.mode = Mode::ActionPalette;
        self.status.clear();
    }

    fn start_edit_selected_item(&mut self) {
        if self.selected_folder_id().is_some() {
            self.start_edit_selected_folder();
        } else {
            self.start_edit_selected_host();
        }
    }

    fn start_edit_selected_host(&mut self) {
        let Some(host) = self.selected_host() else {
            self.status = "select a host to edit".to_string();
            return;
        };
        self.editor = Some(HostEditor::from_host(host));
        self.mode = Mode::EditHost;
        self.status.clear();
    }

    fn start_edit_selected_identity(&mut self) {
        let Some(host) = self.selected_host() else {
            self.status = "select a host to edit identity".to_string();
            return;
        };
        self.identity_editor = Some(IdentityEditor::from_host(
            &self.vault,
            &self.local_config,
            host,
        ));
        self.mode = Mode::EditIdentity;
        self.status.clear();
    }

    fn start_edit_selected_jumps(&mut self) {
        let Some(host) = self.selected_host().cloned() else {
            self.status = "select a host to edit jumps".to_string();
            return;
        };
        self.forward_editor = None;
        self.jump_editor = Some(JumpEditor::from_host(&self.vault, &host));
        self.mode = Mode::EditJumps;
        self.status.clear();
    }

    fn start_edit_selected_forwards(&mut self) {
        let Some(host) = self.selected_host().cloned() else {
            self.status = "select a host to edit forwards".to_string();
            return;
        };
        self.jump_editor = None;
        self.forward_editor = Some(ForwardEditor::from_host(&self.vault, &host));
        self.mode = Mode::EditForwards;
        self.status.clear();
    }

    fn start_edit_selected_folder(&mut self) {
        let Some(folder) = self.selected_folder() else {
            self.status = "select a folder to edit".to_string();
            return;
        };
        if folder.parent_id.is_none() {
            self.status = "root folder cannot be edited".to_string();
            return;
        }
        self.folder_editor = Some(FolderEditor::from_folder(&self.vault, folder));
        self.mode = Mode::EditFolder;
        self.status.clear();
    }

    fn start_create_host(&mut self) {
        let folder_id = self.selected_target_folder_id();
        self.editor = Some(HostEditor::new_host(folder_id));
        self.mode = Mode::EditHost;
        self.status.clear();
    }

    fn start_create_folder(&mut self) {
        let parent_id = self.selected_target_folder_id();
        self.folder_editor = Some(FolderEditor::new_folder(parent_id));
        self.mode = Mode::EditFolder;
        self.status.clear();
    }

    fn copy_selected_host(&mut self) -> Result<()> {
        let Some(source_id) = self.selected_host_id() else {
            self.status = "select a host to copy".to_string();
            return Ok(());
        };
        let mut vault = load_vault(&self.vault_path)?;
        let Some(source) = vault.host(source_id).cloned() else {
            self.status = "copy error: selected host not found".to_string();
            return Ok(());
        };
        let source_actions = source.actions.clone();

        let copied = match vault.add_host(AddHost {
            folder_id: Some(source.folder_id),
            display_name: format!("{} copy", source.display_name),
            hostname: source.hostname,
            port: Some(source.port),
            username: source.username,
            identity_fingerprint: source.identity_fingerprint,
            jump_chain: source.jump_chain,
            ssh_options: source.ssh_options,
            forwards: source.forwards,
            tags: source.tags,
            notes: source.notes,
        }) {
            Ok(host) => host,
            Err(error) => {
                self.status = format!("copy error: {error}");
                return Ok(());
            }
        };
        if let Some(copied_host) = vault.hosts.iter_mut().find(|host| host.id == copied.id) {
            copied_host.actions = source_actions;
        }
        let copied_id = copied.id;
        let copied_path = vault.host_path(&copied);
        save_vault(&self.vault_path, &vault)?;
        self.vault = vault;
        self.local_config = load_local_config(&self.local_config_path)?;
        self.prune_collapsed_folders();
        self.rebuild_tree();
        self.select_host(copied_id).unwrap_or_else(|| {
            self.selected = self.selected.min(self.tree.len().saturating_sub(1))
        });
        self.search_selected = self
            .search_selected
            .min(self.search_matches().len().saturating_sub(1));
        self.pending_delete = None;
        self.mode = Mode::Browse;
        self.status = format!("host copied: {copied_path}");
        Ok(())
    }

    fn start_delete_selected_host(&mut self) {
        if let Some(host) = self.selected_host() {
            self.pending_delete = Some(DeleteConfirmation::Host {
                id: host.id,
                path: self.vault.host_path(host),
                hostname: host.hostname.clone(),
            });
            self.mode = Mode::ConfirmDelete;
            self.status.clear();
            return;
        }

        let Some(folder) = self.selected_folder() else {
            self.status = "select a host or folder to delete".to_string();
            return;
        };
        if folder.parent_id.is_none() {
            self.status = "root folder cannot be deleted".to_string();
            return;
        }
        self.pending_delete = Some(DeleteConfirmation::Folder {
            id: folder.id,
            path: self.vault.folder_path(folder.id),
        });
        self.mode = Mode::ConfirmDelete;
        self.status.clear();
    }

    fn confirm_delete_selected_host(&mut self) -> Result<()> {
        let Some(pending_delete) = self.pending_delete.clone() else {
            self.mode = Mode::Browse;
            return Ok(());
        };
        let previous_selection = self.selected;
        let mut vault = load_vault(&self.vault_path)?;
        let status = match pending_delete {
            DeleteConfirmation::Host { id, .. } => {
                let deleted = match vault.delete_host(HostSelector::Id(id)) {
                    Ok(host) => host,
                    Err(error) => {
                        self.pending_delete = None;
                        self.mode = Mode::Browse;
                        self.status = format!("delete error: {error}");
                        return Ok(());
                    }
                };
                let deleted_path = vault.host_path(&deleted);
                format!("host deleted: {deleted_path}")
            }
            DeleteConfirmation::Folder { id, .. } => {
                let deleted = match vault.delete_folder(id) {
                    Ok(folder) => folder,
                    Err(error) => {
                        self.pending_delete = None;
                        self.mode = Mode::Browse;
                        self.status = format!("delete error: {error}");
                        return Ok(());
                    }
                };
                format!("folder deleted: {}", deleted.name)
            }
        };
        save_vault(&self.vault_path, &vault)?;
        self.vault = vault;
        self.local_config = load_local_config(&self.local_config_path)?;
        self.prune_collapsed_folders();
        self.rebuild_tree();
        self.selected = previous_selection.min(self.tree.len().saturating_sub(1));
        self.search_selected = self
            .search_selected
            .min(self.search_matches().len().saturating_sub(1));
        if let DeleteConfirmation::Host { id, .. } = pending_delete {
            self.selected_hosts.remove(&id);
        }
        self.pending_delete = None;
        self.mode = Mode::Browse;
        self.status = status;
        Ok(())
    }

    fn save_editor(&mut self) -> Result<()> {
        let Some(editor) = self.editor.clone() else {
            self.mode = Mode::Browse;
            return Ok(());
        };
        let mut vault = load_vault(&self.vault_path)?;
        let saved_id = match editor.mode {
            HostEditorMode::Edit { host_id } => {
                let update = match editor.to_update() {
                    Ok(update) => update,
                    Err(error) => {
                        self.status = format!("edit error: {error}");
                        return Ok(());
                    }
                };
                match vault.update_host(HostSelector::Id(host_id), update) {
                    Ok(host) => host.id,
                    Err(error) => {
                        self.status = format!("edit error: {error}");
                        return Ok(());
                    }
                }
            }
            HostEditorMode::Create { .. } => {
                let add = match editor.to_add() {
                    Ok(add) => add,
                    Err(error) => {
                        self.status = format!("create error: {error}");
                        return Ok(());
                    }
                };
                match vault.add_host(add) {
                    Ok(host) => host.id,
                    Err(error) => {
                        self.status = format!("create error: {error}");
                        return Ok(());
                    }
                }
            }
        };
        save_vault(&self.vault_path, &vault)?;
        self.vault = vault;
        self.local_config = load_local_config(&self.local_config_path)?;
        self.prune_collapsed_folders();
        self.rebuild_tree();
        self.select_host(saved_id).unwrap_or_else(|| {
            self.selected = self.selected.min(self.tree.len().saturating_sub(1))
        });
        self.editor = None;
        self.pending_delete = None;
        self.mode = Mode::Browse;
        self.status = match editor.mode {
            HostEditorMode::Edit { .. } => "host saved".to_string(),
            HostEditorMode::Create { .. } => "host created".to_string(),
        };
        Ok(())
    }

    fn save_folder_editor(&mut self) -> Result<()> {
        let Some(editor) = self.folder_editor.clone() else {
            self.mode = Mode::Browse;
            return Ok(());
        };
        let mut vault = load_vault(&self.vault_path)?;
        let saved_id = match editor.mode {
            FolderEditorMode::Edit { folder_id } => {
                let name = match editor.name() {
                    Ok(name) => name,
                    Err(error) => {
                        self.status = format!("folder edit error: {error}");
                        return Ok(());
                    }
                };
                let parent_id = match editor.parent_id() {
                    Ok(parent_id) => parent_id,
                    Err(error) => {
                        self.status = format!("folder edit error: {error}");
                        return Ok(());
                    }
                };
                if let Err(error) = vault.rename_folder(folder_id, name) {
                    self.status = format!("folder edit error: {error}");
                    return Ok(());
                }
                match vault.move_folder(folder_id, parent_id) {
                    Ok(folder) => folder.id,
                    Err(error) => {
                        self.status = format!("folder edit error: {error}");
                        return Ok(());
                    }
                }
            }
            FolderEditorMode::Create { .. } => {
                let add = match editor.to_add() {
                    Ok(add) => add,
                    Err(error) => {
                        self.status = format!("folder create error: {error}");
                        return Ok(());
                    }
                };
                match vault.add_folder(add) {
                    Ok(folder) => folder.id,
                    Err(error) => {
                        self.status = format!("folder create error: {error}");
                        return Ok(());
                    }
                }
            }
        };
        save_vault(&self.vault_path, &vault)?;
        self.vault = vault;
        self.local_config = load_local_config(&self.local_config_path)?;
        self.prune_collapsed_folders();
        self.rebuild_tree();
        self.select_folder(saved_id).unwrap_or_else(|| {
            self.selected = self.selected.min(self.tree.len().saturating_sub(1))
        });
        self.folder_editor = None;
        self.pending_delete = None;
        self.mode = Mode::Browse;
        self.status = match editor.mode {
            FolderEditorMode::Edit { .. } => "folder saved".to_string(),
            FolderEditorMode::Create { .. } => "folder created".to_string(),
        };
        Ok(())
    }

    fn save_identity_editor(&mut self) -> Result<()> {
        let Some(editor) = self.identity_editor.clone() else {
            self.mode = Mode::Browse;
            return Ok(());
        };
        let mut vault = load_vault(&self.vault_path)?;
        let identity_fingerprint = editor.selected_fingerprint();

        let saved_id = match vault.update_host(
            HostSelector::Id(editor.host_id),
            UpdateHost {
                identity_fingerprint: Some(identity_fingerprint),
                ..UpdateHost::default()
            },
        ) {
            Ok(host) => host.id,
            Err(error) => {
                self.status = format!("identity edit error: {error}");
                return Ok(());
            }
        };
        save_vault(&self.vault_path, &vault)?;
        self.vault = vault;
        self.local_config = load_local_config(&self.local_config_path)?;
        self.prune_collapsed_folders();
        self.rebuild_tree();
        self.select_host(saved_id).unwrap_or_else(|| {
            self.selected = self.selected.min(self.tree.len().saturating_sub(1))
        });
        self.identity_editor = None;
        self.pending_delete = None;
        self.mode = Mode::Browse;
        self.status = "identity saved".to_string();
        Ok(())
    }

    fn save_jump_editor(&mut self) -> Result<()> {
        let Some(editor) = self.jump_editor.clone() else {
            self.mode = Mode::Browse;
            return Ok(());
        };
        let mut vault = load_vault(&self.vault_path)?;
        let saved_id = match vault.update_host(
            HostSelector::Id(editor.host_id),
            UpdateHost {
                jump_chain: Some(editor.selected_jump_chain()),
                ..UpdateHost::default()
            },
        ) {
            Ok(host) => host.id,
            Err(error) => {
                self.status = format!("jumps edit error: {error}");
                return Ok(());
            }
        };
        save_vault(&self.vault_path, &vault)?;
        self.vault = vault;
        self.local_config = load_local_config(&self.local_config_path)?;
        self.prune_collapsed_folders();
        self.rebuild_tree();
        self.select_host(saved_id).unwrap_or_else(|| {
            self.selected = self.selected.min(self.tree.len().saturating_sub(1))
        });
        self.jump_editor = None;
        self.pending_delete = None;
        self.mode = Mode::Browse;
        self.status = "jumps saved".to_string();
        Ok(())
    }

    fn save_forward_editor(&mut self) -> Result<()> {
        let Some(editor) = self.forward_editor.clone() else {
            self.mode = Mode::Browse;
            return Ok(());
        };
        let forwards = match editor.to_forwards() {
            Ok(forwards) => forwards,
            Err(error) => {
                self.status = format!("forwards edit error: {error}");
                return Ok(());
            }
        };
        let mut vault = load_vault(&self.vault_path)?;
        let saved_id = match vault.update_host(
            HostSelector::Id(editor.host_id),
            UpdateHost {
                forwards: Some(forwards),
                ..UpdateHost::default()
            },
        ) {
            Ok(host) => host.id,
            Err(error) => {
                self.status = format!("forwards edit error: {error}");
                return Ok(());
            }
        };
        save_vault(&self.vault_path, &vault)?;
        self.vault = vault;
        self.local_config = load_local_config(&self.local_config_path)?;
        self.prune_collapsed_folders();
        self.rebuild_tree();
        self.select_host(saved_id).unwrap_or_else(|| {
            self.selected = self.selected.min(self.tree.len().saturating_sub(1))
        });
        self.forward_editor = None;
        self.pending_delete = None;
        self.mode = Mode::Browse;
        self.status = "forwards saved".to_string();
        Ok(())
    }

    fn start_move_hosts(&mut self) {
        let host_ids = self.active_move_host_ids();
        if host_ids.is_empty() {
            self.status = "select a host to move".to_string();
            return;
        }
        let target_folder_id = self.selected_target_folder_id();
        self.pending_move_hosts = host_ids;
        self.move_folder_selected = self
            .folder_picker_items()
            .iter()
            .position(|item| item.id == target_folder_id)
            .unwrap_or(0);
        self.mode = Mode::PickMoveFolder;
        self.status.clear();
    }

    fn confirm_move_hosts(&mut self) -> Result<()> {
        let Some(target_folder_id) = self.selected_move_folder_id() else {
            self.status = "select a folder".to_string();
            return Ok(());
        };
        let host_ids = self.pending_move_hosts.clone();
        if host_ids.is_empty() {
            self.mode = Mode::Browse;
            return Ok(());
        }
        let mut vault = load_vault(&self.vault_path)?;
        if vault.folder(target_folder_id).is_none() {
            self.pending_move_hosts.clear();
            self.mode = Mode::Browse;
            self.status = "move error: destination folder not found".to_string();
            return Ok(());
        }
        for host_id in &host_ids {
            if vault.host(*host_id).is_none() {
                self.pending_move_hosts.clear();
                self.mode = Mode::Browse;
                self.status = format!("move error: host not found: {host_id}");
                return Ok(());
            }
        }
        for host_id in &host_ids {
            if let Err(error) = vault.update_host(
                HostSelector::Id(*host_id),
                UpdateHost {
                    folder_id: Some(target_folder_id),
                    ..UpdateHost::default()
                },
            ) {
                self.pending_move_hosts.clear();
                self.mode = Mode::Browse;
                self.status = format!("move error: {error}");
                return Ok(());
            }
        }
        let target_path = vault.folder_path(target_folder_id);
        save_vault(&self.vault_path, &vault)?;
        self.vault = vault;
        self.local_config = load_local_config(&self.local_config_path)?;
        self.prune_collapsed_folders();
        self.rebuild_tree();
        if let Some(first_id) = host_ids.first() {
            self.select_host(*first_id).unwrap_or_else(|| {
                self.selected = self.selected.min(self.tree.len().saturating_sub(1))
            });
        }
        self.search_selected = self
            .search_selected
            .min(self.search_matches().len().saturating_sub(1));
        self.pending_move_hosts.clear();
        self.selected_hosts.clear();
        self.mode = Mode::Browse;
        self.status = format!(
            "moved {} host{} to {target_path}",
            host_ids.len(),
            if host_ids.len() == 1 { "" } else { "s" }
        );
        Ok(())
    }

    pub(crate) fn move_selection(&mut self, delta: isize) {
        self.selected = move_index(self.selected, self.tree.len(), delta);
    }

    pub(crate) fn move_to_first_sibling(&mut self) {
        let Some(current) = self.tree.get(self.selected) else {
            return;
        };
        let current_depth = current.depth;
        let boundary = self
            .tree
            .iter()
            .take(self.selected)
            .rposition(|item| item.depth < current_depth)
            .map(|index| index + 1)
            .unwrap_or(0);
        if let Some(index) = self.tree[boundary..=self.selected]
            .iter()
            .position(|item| item.depth == current_depth)
        {
            self.selected = boundary + index;
        }
    }

    pub(crate) fn move_to_last_sibling(&mut self) {
        let Some(current) = self.tree.get(self.selected) else {
            return;
        };
        let current_depth = current.depth;
        let boundary = self
            .tree
            .iter()
            .enumerate()
            .skip(self.selected + 1)
            .find(|(_, item)| item.depth < current_depth)
            .map(|(index, _)| index)
            .unwrap_or(self.tree.len());
        if let Some(index) = self.tree[self.selected..boundary]
            .iter()
            .rposition(|item| item.depth == current_depth)
        {
            self.selected += index;
        }
    }

    pub(crate) fn move_to_parent(&mut self) {
        let Some(current) = self.tree.get(self.selected) else {
            return;
        };
        if current.depth == 0 {
            return;
        }
        let parent_depth = current.depth - 1;
        if let Some(parent_index) = self
            .tree
            .iter()
            .take(self.selected)
            .rposition(|item| item.depth == parent_depth)
        {
            self.selected = parent_index;
        }
    }

    pub(crate) fn move_search_selection(&mut self, delta: isize) {
        self.search_selected = move_index(self.search_selected, self.search_matches().len(), delta);
    }

    pub(crate) fn move_folder_selection(&mut self, delta: isize) {
        self.move_folder_selected = move_index(
            self.move_folder_selected,
            self.folder_picker_items().len(),
            delta,
        );
    }

    fn clamp_search_selection(&mut self) {
        self.search_selected = self
            .search_selected
            .min(self.search_matches().len().saturating_sub(1));
    }

    pub(crate) fn selected_host_id(&self) -> Option<Uuid> {
        match self.mode {
            Mode::Browse => self.tree.get(self.selected).and_then(|item| item.host_id()),
            Mode::ConfirmDelete => self
                .pending_delete
                .as_ref()
                .and_then(DeleteConfirmation::host_id),
            Mode::EditHost => self.editor.as_ref().and_then(|editor| match editor.mode {
                HostEditorMode::Edit { host_id } => Some(host_id),
                HostEditorMode::Create { .. } => None,
            }),
            Mode::EditIdentity => self.identity_editor.as_ref().map(|editor| editor.host_id),
            Mode::EditJumps => self.jump_editor.as_ref().map(|editor| editor.host_id),
            Mode::EditForwards => self.forward_editor.as_ref().map(|editor| editor.host_id),
            Mode::ActionPalette => self.tree.get(self.selected).and_then(|item| item.host_id()),
            Mode::EditFolder => None,
            Mode::PickMoveFolder => None,
            Mode::Search => self
                .search_matches()
                .get(self.search_selected)
                .map(|host| host.id),
        }
    }

    pub(crate) fn selected_folder_id(&self) -> Option<Uuid> {
        if self.mode != Mode::Browse && self.mode != Mode::ConfirmDelete {
            return None;
        }
        self.tree
            .get(self.selected)
            .and_then(|item| item.folder_id())
    }

    fn selected_target_folder_id(&self) -> Uuid {
        match self.mode {
            Mode::Search => self
                .search_matches()
                .get(self.search_selected)
                .map(|host| host.folder_id)
                .unwrap_or_else(|| self.vault.root_folder_id()),
            _ => self
                .tree
                .get(self.selected)
                .map(|item| match item.kind {
                    TreeItemKind::Folder(folder_id) => folder_id,
                    TreeItemKind::Host(host_id) => self
                        .vault
                        .host(host_id)
                        .map(|host| host.folder_id)
                        .unwrap_or_else(|| self.vault.root_folder_id()),
                })
                .unwrap_or_else(|| self.vault.root_folder_id()),
        }
    }

    pub(crate) fn selected_host(&self) -> Option<&Host> {
        let id = self.selected_host_id()?;
        self.vault.host(id)
    }

    pub(crate) fn selected_folder(&self) -> Option<&Folder> {
        let id = self.selected_folder_id()?;
        self.vault.folder(id)
    }

    pub(crate) fn search_matches(&self) -> Vec<&Host> {
        self.vault.search_hosts(&self.search)
    }

    pub(crate) fn selected_count(&self) -> usize {
        self.selected_hosts.len()
    }

    pub(crate) fn host_is_marked(&self, host_id: Uuid) -> bool {
        self.selected_hosts.contains(&host_id)
    }

    pub(crate) fn folder_selection_state(&self, folder_id: Uuid) -> FolderSelectionState {
        let host_ids = self.descendant_host_ids(folder_id);
        if host_ids.is_empty() {
            FolderSelectionState::Empty
        } else if host_ids
            .iter()
            .all(|host_id| self.selected_hosts.contains(host_id))
        {
            FolderSelectionState::All
        } else if host_ids
            .iter()
            .any(|host_id| self.selected_hosts.contains(host_id))
        {
            FolderSelectionState::Some
        } else {
            FolderSelectionState::None
        }
    }

    pub(crate) fn folder_is_collapsed(&self, folder_id: Uuid) -> bool {
        self.collapsed_folders.contains(&folder_id)
    }

    pub(crate) fn folder_picker_items(&self) -> Vec<FolderPickerItem> {
        build_folder_picker_items(&self.vault)
    }

    pub(crate) fn pending_move_count(&self) -> usize {
        self.pending_move_hosts.len()
    }

    fn clear_selected_hosts(&mut self) {
        self.selected_hosts.clear();
        self.status = "selection cleared".to_string();
    }

    fn toggle_current_selection(&mut self) {
        match self.mode {
            Mode::Browse => match self.tree.get(self.selected).map(|item| item.kind) {
                Some(TreeItemKind::Host(host_id)) => self.toggle_host_selection(host_id),
                Some(TreeItemKind::Folder(folder_id)) => self.toggle_folder_selection(folder_id),
                None => self.status = "nothing to select".to_string(),
            },
            Mode::Search => {
                let Some(host_id) = self
                    .search_matches()
                    .get(self.search_selected)
                    .map(|host| host.id)
                else {
                    self.status = "nothing to select".to_string();
                    return;
                };
                self.toggle_host_selection(host_id);
            }
            _ => {}
        }
    }

    fn toggle_host_selection(&mut self, host_id: Uuid) {
        if !self.selected_hosts.remove(&host_id) {
            self.selected_hosts.insert(host_id);
        }
        self.status = format!("{} selected", self.selected_hosts.len());
    }

    fn toggle_folder_selection(&mut self, folder_id: Uuid) {
        let host_ids = self.descendant_host_ids(folder_id);
        if host_ids.is_empty() {
            self.status = "folder has no hosts".to_string();
            return;
        }
        if host_ids
            .iter()
            .all(|host_id| self.selected_hosts.contains(host_id))
        {
            for host_id in host_ids {
                self.selected_hosts.remove(&host_id);
            }
        } else {
            self.selected_hosts.extend(host_ids);
        }
        self.status = format!("{} selected", self.selected_hosts.len());
    }

    fn active_move_host_ids(&self) -> Vec<Uuid> {
        if !self.selected_hosts.is_empty() {
            self.selected_hosts.iter().copied().collect()
        } else {
            self.selected_host_id().into_iter().collect()
        }
    }

    fn selected_move_folder_id(&self) -> Option<Uuid> {
        self.folder_picker_items()
            .get(self.move_folder_selected)
            .map(|item| item.id)
    }

    fn descendant_host_ids(&self, folder_id: Uuid) -> Vec<Uuid> {
        let mut ids = Vec::new();
        self.push_descendant_host_ids(folder_id, &mut ids);
        ids
    }

    fn push_descendant_host_ids(&self, folder_id: Uuid, ids: &mut Vec<Uuid>) {
        let mut hosts = self
            .vault
            .hosts
            .iter()
            .filter(|host| host.folder_id == folder_id)
            .collect::<Vec<_>>();
        hosts.sort_by(|left, right| left.display_name.cmp(&right.display_name));
        ids.extend(hosts.iter().map(|host| host.id));

        let mut child_folders = self
            .vault
            .folders
            .iter()
            .filter(|folder| folder.parent_id == Some(folder_id))
            .collect::<Vec<_>>();
        child_folders.sort_by(|left, right| left.name.cmp(&right.name));
        for folder in child_folders {
            self.push_descendant_host_ids(folder.id, ids);
        }
    }

    fn prune_selected_hosts(&mut self) {
        let existing = self
            .vault
            .hosts
            .iter()
            .map(|host| host.id)
            .collect::<BTreeSet<_>>();
        self.selected_hosts
            .retain(|host_id| existing.contains(host_id));
    }

    fn toggle_selected_folder_expansion(&mut self) -> bool {
        let Some(folder_id) = self.selected_folder_id() else {
            return false;
        };
        if !self.collapsed_folders.remove(&folder_id) {
            self.collapsed_folders.insert(folder_id);
        }
        self.rebuild_tree();
        self.select_folder(folder_id).unwrap_or_else(|| {
            self.selected = self.selected.min(self.tree.len().saturating_sub(1))
        });
        true
    }

    fn rebuild_tree(&mut self) {
        self.tree = build_tree(&self.vault, &self.collapsed_folders);
    }

    fn prune_collapsed_folders(&mut self) {
        let existing = self
            .vault
            .folders
            .iter()
            .map(|folder| folder.id)
            .collect::<BTreeSet<_>>();
        self.collapsed_folders
            .retain(|folder_id| existing.contains(folder_id));
    }

    fn select_host(&mut self, host_id: Uuid) -> Option<()> {
        let index = self
            .tree
            .iter()
            .position(|item| item.host_id() == Some(host_id))?;
        self.selected = index;
        Some(())
    }

    fn select_folder(&mut self, folder_id: Uuid) -> Option<()> {
        let index = self
            .tree
            .iter()
            .position(|item| item.folder_id() == Some(folder_id))?;
        self.selected = index;
        Some(())
    }

    pub(crate) fn selected_action(&self) -> Option<&ActionDefinition> {
        self.selected_actions().get(self.action_selected).copied()
    }

    pub(crate) fn selected_actions(&self) -> Vec<&ActionDefinition> {
        if self.selected_host().is_none() {
            return Vec::new();
        }
        self.vault
            .actions
            .iter()
            .chain(
                self.selected_host()
                    .into_iter()
                    .flat_map(|host| host.actions.iter()),
            )
            .collect()
    }

    fn move_action_selection(&mut self, delta: isize) {
        self.action_selected =
            move_index(self.action_selected, self.selected_actions().len(), delta);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Browse,
    Search,
    EditHost,
    EditFolder,
    EditIdentity,
    EditJumps,
    EditForwards,
    ActionPalette,
    ConfirmDelete,
    PickMoveFolder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FolderPickerItem {
    pub(crate) id: Uuid,
    pub(crate) depth: usize,
    pub(crate) label: String,
    pub(crate) path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FolderSelectionState {
    Empty,
    None,
    Some,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeleteConfirmation {
    Host {
        id: Uuid,
        path: String,
        hostname: String,
    },
    Folder {
        id: Uuid,
        path: String,
    },
}

impl DeleteConfirmation {
    fn host_id(&self) -> Option<Uuid> {
        match self {
            Self::Host { id, .. } => Some(*id),
            Self::Folder { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TreeItem {
    pub(crate) depth: usize,
    pub(crate) kind: TreeItemKind,
    pub(crate) label: String,
}

impl TreeItem {
    fn folder_id(&self) -> Option<Uuid> {
        match self.kind {
            TreeItemKind::Folder(id) => Some(id),
            TreeItemKind::Host(_) => None,
        }
    }

    fn host_id(&self) -> Option<Uuid> {
        match self.kind {
            TreeItemKind::Host(id) => Some(id),
            TreeItemKind::Folder(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TreeItemKind {
    Folder(Uuid),
    Host(Uuid),
}

pub(crate) fn build_tree(vault: &Vault, collapsed_folders: &BTreeSet<Uuid>) -> Vec<TreeItem> {
    let mut items = Vec::new();
    push_folder(
        vault,
        vault.root_folder_id(),
        0,
        collapsed_folders,
        &mut items,
    );
    items
}

fn default_collapsed_folders(vault: &Vault) -> BTreeSet<Uuid> {
    vault
        .folders
        .iter()
        .filter(|folder| folder.parent_id.is_some())
        .map(|folder| folder.id)
        .collect()
}

fn build_folder_picker_items(vault: &Vault) -> Vec<FolderPickerItem> {
    let mut items = Vec::new();
    push_folder_picker_item(vault, vault.root_folder_id(), 0, &mut items);
    items
}

fn push_folder_picker_item(
    vault: &Vault,
    folder_id: Uuid,
    depth: usize,
    items: &mut Vec<FolderPickerItem>,
) {
    let Some(folder) = vault.folder(folder_id) else {
        return;
    };
    items.push(FolderPickerItem {
        id: folder.id,
        depth,
        label: if folder.parent_id.is_none() {
            "/".to_string()
        } else {
            folder.name.clone()
        },
        path: vault.folder_path(folder.id),
    });

    let mut child_folders = vault
        .folders
        .iter()
        .filter(|child| child.parent_id == Some(folder_id))
        .collect::<Vec<_>>();
    child_folders.sort_by(|left, right| left.name.cmp(&right.name));
    for child in child_folders {
        push_folder_picker_item(vault, child.id, depth + 1, items);
    }
}

fn push_folder(
    vault: &Vault,
    folder_id: Uuid,
    depth: usize,
    collapsed_folders: &BTreeSet<Uuid>,
    items: &mut Vec<TreeItem>,
) {
    let Some(folder) = vault.folder(folder_id) else {
        return;
    };
    items.push(TreeItem {
        depth,
        kind: TreeItemKind::Folder(folder.id),
        label: if folder.parent_id.is_none() {
            "/".to_string()
        } else {
            folder.name.clone()
        },
    });

    if collapsed_folders.contains(&folder_id) {
        return;
    }

    let mut child_folders = vault
        .folders
        .iter()
        .filter(|child| child.parent_id == Some(folder_id))
        .collect::<Vec<_>>();
    child_folders.sort_by(|left, right| left.name.cmp(&right.name));
    for child in child_folders {
        push_folder(vault, child.id, depth + 1, collapsed_folders, items);
    }

    let mut hosts = vault
        .hosts
        .iter()
        .filter(|host| host.folder_id == folder_id)
        .collect::<Vec<_>>();
    hosts.sort_by(|left, right| left.display_name.cmp(&right.display_name));
    for host in hosts {
        items.push(TreeItem {
            depth: depth + 1,
            kind: TreeItemKind::Host(host.id),
            label: host.display_name.clone(),
        });
    }
}

fn move_index(current: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    let next = current as isize + delta;
    next.clamp(0, len as isize - 1) as usize
}

fn list_scroll_offset(selected: usize, visible_height: usize) -> usize {
    if visible_height == 0 {
        0
    } else {
        selected.saturating_sub(visible_height - 1)
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs, path::PathBuf};

    use crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use ratatui::layout::Rect;
    use stassh_core::{
        ActionDefinition, AddFolder, AddHost, ForwardDefinition, load_local_config, load_vault,
        save_local_config,
    };

    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn click(row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 1,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn tree_area() -> Rect {
        Rect::new(0, 0, 40, 20)
    }

    fn sample_app() -> App {
        let mut vault = Vault::new();
        let customers = vault
            .add_folder(AddFolder {
                parent_id: None,
                name: "Customers".to_string(),
            })
            .unwrap();
        vault
            .add_host(AddHost {
                folder_id: Some(customers.id),
                display_name: "web".to_string(),
                hostname: "web.example".to_string(),
                port: None,
                username: Some("deploy".to_string()),
                identity_fingerprint: None,
                jump_chain: Vec::new(),
                ssh_options: Vec::new(),
                forwards: Vec::new(),
                tags: vec!["prod".to_string()],
                notes: None,
            })
            .unwrap();
        vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "lab".to_string(),
                hostname: "lab.example".to_string(),
                port: None,
                username: None,
                identity_fingerprint: None,
                jump_chain: Vec::new(),
                ssh_options: Vec::new(),
                forwards: Vec::new(),
                tags: Vec::new(),
                notes: None,
            })
            .unwrap();
        App::new(
            PathBuf::from("vault.json"),
            PathBuf::from(".stassh-local.json"),
            vault,
            LocalConfig::default(),
            false,
        )
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("stassh-tui-{name}-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn select_label(app: &mut App, label: &str) {
        if !app.tree.iter().any(|item| item.label == label) {
            app.collapsed_folders.clear();
            app.rebuild_tree();
        }
        app.selected = app
            .tree
            .iter()
            .position(|item| item.label == label)
            .unwrap();
    }

    fn app_with_persisted_vault(name: &str) -> (App, PathBuf) {
        let dir = temp_dir(name);
        let vault_path = dir.join("vault.json");
        let local_config_path = dir.join("local.json");
        let mut app = sample_app();
        save_vault(&vault_path, &app.vault).unwrap();
        app.vault_path = vault_path;
        app.local_config_path = local_config_path;
        (app, dir)
    }

    #[test]
    fn startup_tree_collapses_non_root_folders() {
        let app = sample_app();
        let labels = app
            .tree
            .iter()
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>();

        assert_eq!(labels, vec!["/", "Customers", "lab"]);
        let folder_id = app
            .vault
            .folders
            .iter()
            .find(|folder| folder.name == "Customers")
            .unwrap()
            .id;
        assert!(app.collapsed_folders.contains(&folder_id));
    }

    #[test]
    fn enter_toggles_folder_expansion_without_connecting() {
        let mut app = sample_app();
        select_label(&mut app, "Customers");
        let folder_id = app.selected_folder_id().unwrap();
        assert!(app.collapsed_folders.contains(&folder_id));

        let action = app.handle_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(action, KeyAction::None);
        assert!(!app.collapsed_folders.contains(&folder_id));
        let labels = app
            .tree
            .iter()
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["/", "Customers", "web", "lab"]);

        let action = app.handle_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(action, KeyAction::None);
        assert!(app.collapsed_folders.contains(&folder_id));
        let labels = app
            .tree
            .iter()
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["/", "Customers", "lab"]);
    }

    #[test]
    fn enter_on_host_still_connects() {
        let mut app = sample_app();
        select_label(&mut app, "web");

        let action = app.handle_key(key(KeyCode::Enter)).unwrap();

        assert_eq!(action, KeyAction::Connect);
    }

    #[test]
    fn action_key_opens_palette_for_common_actions() {
        let mut app = sample_app();
        app.vault.actions = vec![ActionDefinition {
            id: Uuid::new_v4(),
            name: "Desktop".to_string(),
            local_prepare: None,
            forwards: Vec::new(),
            remote_command: Some("DISPLAY=:0 x11vnc -scale 1/2".to_string()),
            local_launch: None,
            cleanup: Vec::new(),
        }];
        select_label(&mut app, "web");

        let action = app.handle_key(key(KeyCode::Char('a'))).unwrap();

        assert_eq!(action, KeyAction::None);
        assert_eq!(app.mode, Mode::ActionPalette);
        assert_eq!(app.selected_action().unwrap().name, "Desktop");
    }

    #[test]
    fn enter_in_action_palette_returns_run_action() {
        let mut app = sample_app();
        let host_id = app.vault.search_hosts("web")[0].id;
        let action_id = Uuid::new_v4();
        app.vault
            .hosts
            .iter_mut()
            .find(|host| host.id == host_id)
            .unwrap()
            .actions = vec![ActionDefinition {
            id: action_id,
            name: "Desktop".to_string(),
            local_prepare: None,
            forwards: Vec::new(),
            remote_command: Some("DISPLAY=:0 x11vnc -scale 1/2".to_string()),
            local_launch: None,
            cleanup: Vec::new(),
        }];
        select_label(&mut app, "web");
        app.handle_key(key(KeyCode::Char('a'))).unwrap();

        let action = app.handle_key(key(KeyCode::Enter)).unwrap();

        assert_eq!(action, KeyAction::RunAction(action_id));
    }

    #[test]
    fn movement_clamps_to_available_items() {
        let mut app = sample_app();

        app.move_selection(99);
        assert_eq!(app.selected, app.tree.len() - 1);
        app.move_selection(-99);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn home_moves_to_first_visible_sibling() {
        let mut app = sample_app();
        app.collapsed_folders.clear();
        app.rebuild_tree();
        select_label(&mut app, "lab");

        app.handle_key(key(KeyCode::Home)).unwrap();

        assert_eq!(app.tree[app.selected].label, "Customers");
    }

    #[test]
    fn home_at_root_stays_on_root() {
        let mut app = sample_app();

        app.handle_key(key(KeyCode::Home)).unwrap();

        assert_eq!(app.selected, 0);
        assert_eq!(app.tree[app.selected].label, "/");
    }

    #[test]
    fn end_moves_to_last_visible_sibling() {
        let mut app = sample_app();
        app.collapsed_folders.clear();
        app.rebuild_tree();
        select_label(&mut app, "Customers");

        app.handle_key(key(KeyCode::End)).unwrap();

        assert_eq!(app.tree[app.selected].label, "lab");
    }

    #[test]
    fn end_on_last_sibling_stays_on_current_row() {
        let mut app = sample_app();
        select_label(&mut app, "lab");

        app.handle_key(key(KeyCode::End)).unwrap();

        assert_eq!(app.tree[app.selected].label, "lab");
    }

    #[test]
    fn home_and_end_handle_nested_siblings() {
        let mut app = sample_app();
        let parent_id = app
            .vault
            .folders
            .iter()
            .find(|folder| folder.name == "Customers")
            .unwrap()
            .id;
        let child = app
            .vault
            .add_folder(AddFolder {
                parent_id: Some(parent_id),
                name: "Child".to_string(),
            })
            .unwrap();
        app.vault
            .add_host(AddHost {
                folder_id: Some(child.id),
                display_name: "db".to_string(),
                hostname: "db.example".to_string(),
                port: None,
                username: None,
                identity_fingerprint: None,
                jump_chain: Vec::new(),
                ssh_options: Vec::new(),
                forwards: Vec::new(),
                tags: Vec::new(),
                notes: None,
            })
            .unwrap();
        app.collapsed_folders.clear();
        app.rebuild_tree();
        select_label(&mut app, "web");

        app.handle_key(key(KeyCode::Home)).unwrap();
        assert_eq!(app.tree[app.selected].label, "Child");

        app.handle_key(key(KeyCode::End)).unwrap();
        assert_eq!(app.tree[app.selected].label, "web");
    }

    #[test]
    fn page_up_moves_to_visible_parent_folder() {
        let mut app = sample_app();
        select_label(&mut app, "web");

        app.handle_key(key(KeyCode::PageUp)).unwrap();

        assert_eq!(app.tree[app.selected].label, "Customers");
    }

    #[test]
    fn page_down_moves_to_last_visible_sibling() {
        let mut app = sample_app();
        let parent_id = app
            .vault
            .folders
            .iter()
            .find(|folder| folder.name == "Customers")
            .unwrap()
            .id;
        app.vault
            .add_host(AddHost {
                folder_id: Some(parent_id),
                display_name: "api".to_string(),
                hostname: "api.example".to_string(),
                port: None,
                username: None,
                identity_fingerprint: None,
                jump_chain: Vec::new(),
                ssh_options: Vec::new(),
                forwards: Vec::new(),
                tags: Vec::new(),
                notes: None,
            })
            .unwrap();
        app.collapsed_folders.clear();
        app.rebuild_tree();
        select_label(&mut app, "api");

        app.handle_key(key(KeyCode::PageDown)).unwrap();

        assert_eq!(app.tree[app.selected].label, "web");
    }

    #[test]
    fn search_uses_core_host_search() {
        let mut app = sample_app();
        app.mode = Mode::Search;
        app.search = "prod deploy".to_string();

        let matches = app.search_matches();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].display_name, "web");
    }

    #[test]
    fn search_mode_treats_command_letters_as_query_text() {
        let mut app = sample_app();

        app.handle_key(key(KeyCode::Char('/'))).unwrap();
        for value in "ccarenas-mu".chars() {
            app.handle_key(key(KeyCode::Char(value))).unwrap();
        }

        assert_eq!(app.mode, Mode::Search);
        assert_eq!(app.search, "ccarenas-mu");
        assert!(app.editor.is_none());
        assert!(app.pending_move_hosts.is_empty());
        assert!(app.selected_hosts.is_empty());
    }

    #[test]
    fn f1_cycles_status_page_without_mode_action() {
        let mut app = sample_app();

        let action = app.handle_key(key(KeyCode::F(1))).unwrap();

        assert_eq!(action, KeyAction::None);
        assert_eq!(app.status_page, 1);
        assert_eq!(app.mode, Mode::Browse);
    }

    #[test]
    fn repeated_f1_keeps_advancing_status_page_without_mode_action() {
        let mut app = sample_app();

        for _ in 0..7 {
            let action = app.handle_key(key(KeyCode::F(1))).unwrap();
            assert_eq!(action, KeyAction::None);
        }

        assert_eq!(app.status_page, 7);
        assert_eq!(app.mode, Mode::Browse);
    }

    #[test]
    fn f1_cycles_status_page_while_editing_without_changing_text() {
        let mut app = sample_app();
        select_label(&mut app, "web");
        app.handle_key(key(KeyCode::Char('e'))).unwrap();
        let before = app.editor.as_ref().unwrap().fields[0].value.clone();

        let action = app.handle_key(key(KeyCode::F(1))).unwrap();

        assert_eq!(action, KeyAction::None);
        assert_eq!(app.status_page, 1);
        assert_eq!(app.mode, Mode::EditHost);
        assert_eq!(app.editor.as_ref().unwrap().fields[0].value, before);
    }

    #[test]
    fn search_home_and_end_move_to_match_boundaries() {
        let mut app = sample_app();
        app.mode = Mode::Search;
        app.search_selected = 1;

        app.handle_key(key(KeyCode::Home)).unwrap();
        assert_eq!(app.search_selected, 0);
        app.handle_key(key(KeyCode::End)).unwrap();
        assert_eq!(app.search_selected, app.search_matches().len() - 1);
    }

    #[test]
    fn toggles_single_host_selection_from_browse() {
        let mut app = sample_app();
        select_label(&mut app, "web");
        let host_id = app.selected_host().unwrap().id;

        app.handle_key(key(KeyCode::Char(' '))).unwrap();
        assert!(app.selected_hosts.contains(&host_id));

        app.handle_key(key(KeyCode::Char(' '))).unwrap();
        assert!(!app.selected_hosts.contains(&host_id));
    }

    #[test]
    fn mouse_click_selects_browse_tree_row() {
        let mut app = sample_app();

        let action = app.handle_mouse(click(2), tree_area()).unwrap();

        assert_eq!(action, KeyAction::None);
        assert_eq!(app.tree[app.selected].label, "Customers");
    }

    #[test]
    fn mouse_click_outside_tree_is_ignored() {
        let mut app = sample_app();
        let mouse = MouseEvent {
            column: 0,
            ..click(2)
        };

        let action = app.handle_mouse(mouse, tree_area()).unwrap();

        assert_eq!(action, KeyAction::None);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn mouse_click_selects_search_result() {
        let mut app = sample_app();
        app.mode = Mode::Search;
        app.search = "lab".to_string();

        let action = app.handle_mouse(click(1), tree_area()).unwrap();

        assert_eq!(action, KeyAction::None);
        assert_eq!(
            app.search_matches()[app.search_selected].display_name,
            "lab"
        );
    }

    #[test]
    fn mouse_click_selects_move_folder_target() {
        let mut app = sample_app();
        select_label(&mut app, "lab");
        app.handle_key(key(KeyCode::Char('m'))).unwrap();

        let action = app.handle_mouse(click(2), tree_area()).unwrap();

        assert_eq!(action, KeyAction::None);
        assert_eq!(
            app.folder_picker_items()[app.move_folder_selected].label,
            "Customers"
        );
    }

    #[test]
    fn mouse_double_click_on_host_connects() {
        let mut app = sample_app();
        select_label(&mut app, "web");
        let row = app.selected as u16 + 1;

        assert_eq!(
            app.handle_mouse(click(row), tree_area()).unwrap(),
            KeyAction::None
        );
        assert_eq!(
            app.handle_mouse(click(row), tree_area()).unwrap(),
            KeyAction::Connect
        );
    }

    #[test]
    fn mouse_double_click_on_folder_toggles_expansion() {
        let mut app = sample_app();
        select_label(&mut app, "Customers");
        let folder_id = app.selected_folder_id().unwrap();
        let row = app.selected as u16 + 1;
        assert!(app.collapsed_folders.contains(&folder_id));

        app.handle_mouse(click(row), tree_area()).unwrap();
        app.handle_mouse(click(row), tree_area()).unwrap();

        assert!(!app.collapsed_folders.contains(&folder_id));
    }

    #[test]
    fn mouse_double_click_confirms_move_folder_target() {
        let (mut app, dir) = app_with_persisted_vault("mouse-move");
        let vault_path = app.vault_path.clone();
        let target_folder_id = app
            .vault
            .folders
            .iter()
            .find(|folder| folder.name == "Customers")
            .unwrap()
            .id;
        select_label(&mut app, "lab");
        let host_id = app.selected_host().unwrap().id;
        app.handle_key(key(KeyCode::Char('m'))).unwrap();
        let row = app
            .folder_picker_items()
            .iter()
            .position(|folder| folder.id == target_folder_id)
            .unwrap() as u16
            + 1;

        app.handle_mouse(click(row), tree_area()).unwrap();
        app.handle_mouse(click(row), tree_area()).unwrap();

        assert_eq!(app.mode, Mode::Browse);
        assert_eq!(app.vault.host(host_id).unwrap().folder_id, target_folder_id);
        assert_eq!(
            load_vault(&vault_path)
                .unwrap()
                .host(host_id)
                .unwrap()
                .folder_id,
            target_folder_id
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn toggles_descendant_hosts_from_folder() {
        let mut app = sample_app();
        let parent_id = app
            .vault
            .folders
            .iter()
            .find(|folder| folder.name == "Customers")
            .unwrap()
            .id;
        let child = app
            .vault
            .add_folder(AddFolder {
                parent_id: Some(parent_id),
                name: "Child".to_string(),
            })
            .unwrap();
        let child_host = app
            .vault
            .add_host(AddHost {
                folder_id: Some(child.id),
                display_name: "db".to_string(),
                hostname: "db.example".to_string(),
                port: None,
                username: None,
                identity_fingerprint: None,
                jump_chain: Vec::new(),
                ssh_options: Vec::new(),
                forwards: Vec::new(),
                tags: Vec::new(),
                notes: None,
            })
            .unwrap();
        app.rebuild_tree();
        select_label(&mut app, "Customers");
        let web_id = app
            .vault
            .hosts
            .iter()
            .find(|host| host.display_name == "web")
            .unwrap()
            .id;

        app.handle_key(key(KeyCode::Char(' '))).unwrap();
        assert!(app.selected_hosts.contains(&web_id));
        assert!(app.selected_hosts.contains(&child_host.id));
        assert_eq!(
            app.folder_selection_state(parent_id),
            FolderSelectionState::All
        );

        app.handle_key(key(KeyCode::Char(' '))).unwrap();
        assert!(app.selected_hosts.is_empty());
        assert_eq!(
            app.folder_selection_state(parent_id),
            FolderSelectionState::None
        );
    }

    #[test]
    fn toggles_host_selection_from_search_and_clears_it() {
        let mut app = sample_app();
        app.mode = Mode::Search;
        app.search = "web".to_string();
        let host_id = app.search_matches()[0].id;

        app.handle_key(key(KeyCode::Char(' '))).unwrap();
        assert!(app.selected_hosts.contains(&host_id));

        app.handle_key(key(KeyCode::Esc)).unwrap();
        assert_eq!(app.mode, Mode::Browse);
        assert!(app.selected_hosts.contains(&host_id));

        app.handle_key(key(KeyCode::Char('u'))).unwrap();
        assert!(app.selected_hosts.is_empty());
    }

    #[test]
    fn starts_and_cancels_host_edit() {
        let mut app = sample_app();
        select_label(&mut app, "web");

        app.handle_key(key(KeyCode::Char('e'))).unwrap();
        assert_eq!(app.mode, Mode::EditHost);
        assert!(app.editor.is_some());

        app.handle_key(key(KeyCode::Esc)).unwrap();
        assert_eq!(app.mode, Mode::Browse);
        assert!(app.editor.is_none());
    }

    #[test]
    fn starts_host_create_in_selected_hosts_folder() {
        let mut app = sample_app();
        select_label(&mut app, "web");
        let folder_id = app.selected_host().unwrap().folder_id;

        app.handle_key(key(KeyCode::Char('n'))).unwrap();

        assert_eq!(app.mode, Mode::EditHost);
        assert_eq!(
            app.editor.as_ref().unwrap().mode,
            HostEditorMode::Create { folder_id }
        );
    }

    #[test]
    fn copies_selected_host_and_persists_vault() {
        let (mut app, dir) = app_with_persisted_vault("copy-host");
        let vault_path = app.vault_path.clone();
        select_label(&mut app, "web");
        let source = app.selected_host().unwrap().clone();

        app.handle_key(key(KeyCode::Char('C'))).unwrap();

        assert_eq!(app.mode, Mode::Browse);
        let copied = app.selected_host().unwrap();
        assert_ne!(copied.id, source.id);
        assert_eq!(copied.folder_id, source.folder_id);
        assert_eq!(copied.display_name, "web copy");
        assert_eq!(copied.hostname, source.hostname);
        assert_eq!(copied.port, source.port);
        assert_eq!(copied.username, source.username);
        assert_eq!(copied.tags, source.tags);
        assert_eq!(copied.notes, source.notes);
        assert!(app.status.contains("host copied"));

        let persisted = load_vault(&vault_path).unwrap();
        let persisted_copy = persisted.host(copied.id).unwrap();
        assert_eq!(persisted_copy.display_name, "web copy");
        assert_eq!(persisted_copy.hostname, source.hostname);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn copies_selected_search_result_with_uppercase_c() {
        let (mut app, dir) = app_with_persisted_vault("copy-search-host");
        let vault_path = app.vault_path.clone();
        app.mode = Mode::Search;
        app.search = "lab".to_string();
        let source_id = app.search_matches()[0].id;

        app.handle_key(key(KeyCode::Char('C'))).unwrap();

        assert_eq!(app.mode, Mode::Browse);
        let copied = app.selected_host().unwrap();
        assert_ne!(copied.id, source_id);
        assert_eq!(copied.display_name, "lab copy");
        assert_eq!(copied.hostname, "lab.example");
        assert!(
            load_vault(&vault_path)
                .unwrap()
                .hosts
                .iter()
                .any(|host| host.id == copied.id && host.display_name == "lab copy")
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn copy_on_folder_reports_select_host() {
        let mut app = sample_app();
        select_label(&mut app, "Customers");

        app.handle_key(key(KeyCode::Char('C'))).unwrap();

        assert_eq!(app.mode, Mode::Browse);
        assert_eq!(app.status, "select a host to copy");
    }

    #[test]
    fn starts_and_cancels_host_delete() {
        let mut app = sample_app();
        select_label(&mut app, "web");
        let host_id = app.selected_host().unwrap().id;

        app.handle_key(key(KeyCode::Char('x'))).unwrap();
        assert_eq!(app.mode, Mode::ConfirmDelete);
        assert_eq!(
            app.pending_delete.as_ref().unwrap().host_id(),
            Some(host_id)
        );

        app.handle_key(key(KeyCode::Esc)).unwrap();
        assert_eq!(app.mode, Mode::Browse);
        assert!(app.pending_delete.is_none());
        assert!(app.vault.host(host_id).is_some());
    }

    #[test]
    fn confirms_host_delete_and_persists_vault() {
        let dir = temp_dir("delete");
        let vault_path = dir.join("vault.json");
        let local_config_path = dir.join("local.json");
        let mut app = sample_app();
        save_vault(&vault_path, &app.vault).unwrap();
        app.vault_path = vault_path.clone();
        app.local_config_path = local_config_path;
        select_label(&mut app, "web");
        let host_id = app.selected_host().unwrap().id;

        app.handle_key(key(KeyCode::Char('x'))).unwrap();
        app.handle_key(key(KeyCode::Char('y'))).unwrap();

        assert_eq!(app.mode, Mode::Browse);
        assert!(app.pending_delete.is_none());
        assert!(app.vault.host(host_id).is_none());
        let persisted = load_vault(&vault_path).unwrap();
        assert!(persisted.host(host_id).is_none());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn starts_folder_create_in_selected_folder() {
        let mut app = sample_app();
        select_label(&mut app, "Customers");
        let parent_id = app.selected_folder_id().unwrap();

        app.handle_key(key(KeyCode::Char('f'))).unwrap();

        assert_eq!(app.mode, Mode::EditFolder);
        assert_eq!(
            app.folder_editor.as_ref().unwrap().mode,
            FolderEditorMode::Create { parent_id }
        );
    }

    #[test]
    fn starts_and_cancels_folder_edit() {
        let mut app = sample_app();
        select_label(&mut app, "Customers");

        app.handle_key(key(KeyCode::Char('e'))).unwrap();
        assert_eq!(app.mode, Mode::EditFolder);
        assert!(app.folder_editor.is_some());

        app.handle_key(key(KeyCode::Esc)).unwrap();
        assert_eq!(app.mode, Mode::Browse);
        assert!(app.folder_editor.is_none());
    }

    #[test]
    fn creates_folder_and_persists_vault() {
        let dir = temp_dir("folder-create");
        let vault_path = dir.join("vault.json");
        let local_config_path = dir.join("local.json");
        let mut app = sample_app();
        save_vault(&vault_path, &app.vault).unwrap();
        app.vault_path = vault_path.clone();
        app.local_config_path = local_config_path;
        select_label(&mut app, "Customers");

        app.handle_key(key(KeyCode::Char('f'))).unwrap();
        app.folder_editor.as_mut().unwrap().fields[0].value = "Staging".to_string();
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))
            .unwrap();

        assert_eq!(app.mode, Mode::Browse);
        assert!(
            app.vault
                .folders
                .iter()
                .any(|folder| folder.name == "Staging")
        );
        let persisted = load_vault(&vault_path).unwrap();
        assert!(
            persisted
                .folders
                .iter()
                .any(|folder| folder.name == "Staging")
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn renames_and_moves_folder_and_persists_vault() {
        let dir = temp_dir("folder-edit");
        let vault_path = dir.join("vault.json");
        let local_config_path = dir.join("local.json");
        let mut app = sample_app();
        let root_id = app.vault.root_folder_id();
        save_vault(&vault_path, &app.vault).unwrap();
        app.vault_path = vault_path.clone();
        app.local_config_path = local_config_path;
        select_label(&mut app, "Customers");
        let folder_id = app.selected_folder_id().unwrap();

        app.handle_key(key(KeyCode::Char('e'))).unwrap();
        let editor = app.folder_editor.as_mut().unwrap();
        editor.fields[0].value = "Clients".to_string();
        editor.fields[1].value = root_id.to_string();
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))
            .unwrap();

        assert_eq!(app.mode, Mode::Browse);
        let folder = app.vault.folder(folder_id).unwrap();
        assert_eq!(folder.name, "Clients");
        assert_eq!(folder.parent_id, Some(root_id));
        let persisted = load_vault(&vault_path).unwrap();
        let persisted_folder = persisted.folder(folder_id).unwrap();
        assert_eq!(persisted_folder.name, "Clients");
        assert_eq!(persisted_folder.parent_id, Some(root_id));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn confirms_empty_folder_delete_and_persists_vault() {
        let dir = temp_dir("folder-delete");
        let vault_path = dir.join("vault.json");
        let local_config_path = dir.join("local.json");
        let mut app = sample_app();
        let folder = app
            .vault
            .add_folder(AddFolder {
                parent_id: None,
                name: "Empty".to_string(),
            })
            .unwrap();
        app.rebuild_tree();
        save_vault(&vault_path, &app.vault).unwrap();
        app.vault_path = vault_path.clone();
        app.local_config_path = local_config_path;
        select_label(&mut app, "Empty");

        app.handle_key(key(KeyCode::Char('x'))).unwrap();
        assert_eq!(app.mode, Mode::ConfirmDelete);
        app.handle_key(key(KeyCode::Enter)).unwrap();

        assert_eq!(app.mode, Mode::Browse);
        assert!(app.vault.folder(folder.id).is_none());
        let persisted = load_vault(&vault_path).unwrap();
        assert!(persisted.folder(folder.id).is_none());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn starts_move_picker_for_selected_hosts() {
        let mut app = sample_app();
        select_label(&mut app, "web");
        let host_id = app.selected_host().unwrap().id;
        app.handle_key(key(KeyCode::Char(' '))).unwrap();

        app.handle_key(key(KeyCode::Char('m'))).unwrap();

        assert_eq!(app.mode, Mode::PickMoveFolder);
        assert_eq!(app.pending_move_hosts, vec![host_id]);
    }

    #[test]
    fn move_picker_falls_back_to_highlighted_host() {
        let mut app = sample_app();
        select_label(&mut app, "web");
        let host_id = app.selected_host().unwrap().id;

        app.handle_key(key(KeyCode::Char('m'))).unwrap();

        assert_eq!(app.mode, Mode::PickMoveFolder);
        assert_eq!(app.pending_move_hosts, vec![host_id]);
    }

    #[test]
    fn move_picker_shows_subfolders_when_browse_tree_is_collapsed() {
        let mut app = sample_app();
        let parent_id = app
            .vault
            .folders
            .iter()
            .find(|folder| folder.name == "Customers")
            .unwrap()
            .id;
        let child = app
            .vault
            .add_folder(AddFolder {
                parent_id: Some(parent_id),
                name: "Child".to_string(),
            })
            .unwrap();
        app.collapsed_folders.insert(parent_id);
        app.rebuild_tree();
        assert!(!app.tree.iter().any(|item| item.label == "Child"));

        let picker_items = app.folder_picker_items();

        assert!(picker_items.iter().any(|item| item.id == child.id));
    }

    #[test]
    fn cancels_move_picker_without_changing_vault() {
        let (mut app, dir) = app_with_persisted_vault("move-cancel");
        let vault_path = app.vault_path.clone();
        select_label(&mut app, "web");
        let host_id = app.selected_host().unwrap().id;
        let original_folder_id = app.selected_host().unwrap().folder_id;

        app.handle_key(key(KeyCode::Char('m'))).unwrap();
        app.handle_key(key(KeyCode::Esc)).unwrap();

        assert_eq!(app.mode, Mode::Browse);
        assert!(app.pending_move_hosts.is_empty());
        assert_eq!(
            load_vault(&vault_path)
                .unwrap()
                .host(host_id)
                .unwrap()
                .folder_id,
            original_folder_id
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn moves_selected_hosts_and_persists_vault() {
        let (mut app, dir) = app_with_persisted_vault("move-hosts");
        let vault_path = app.vault_path.clone();
        let target_folder_id = app
            .vault
            .folders
            .iter()
            .find(|folder| folder.name == "Customers")
            .unwrap()
            .id;
        select_label(&mut app, "lab");
        let lab_id = app.selected_host().unwrap().id;
        app.handle_key(key(KeyCode::Char(' '))).unwrap();
        select_label(&mut app, "web");
        let web_id = app.selected_host().unwrap().id;
        app.handle_key(key(KeyCode::Char(' '))).unwrap();

        app.handle_key(key(KeyCode::Char('m'))).unwrap();
        app.move_folder_selected = app
            .folder_picker_items()
            .iter()
            .position(|folder| folder.id == target_folder_id)
            .unwrap();
        app.handle_key(key(KeyCode::Enter)).unwrap();

        assert_eq!(app.mode, Mode::Browse);
        assert!(app.selected_hosts.is_empty());
        assert_eq!(app.vault.host(lab_id).unwrap().folder_id, target_folder_id);
        assert_eq!(app.vault.host(web_id).unwrap().folder_id, target_folder_id);
        let persisted = load_vault(&vault_path).unwrap();
        assert_eq!(persisted.host(lab_id).unwrap().folder_id, target_folder_id);
        assert_eq!(persisted.host(web_id).unwrap().folder_id, target_folder_id);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn moving_to_current_folder_succeeds_as_noop() {
        let (mut app, dir) = app_with_persisted_vault("move-noop");
        let vault_path = app.vault_path.clone();
        select_label(&mut app, "web");
        let host_id = app.selected_host().unwrap().id;
        let folder_id = app.selected_host().unwrap().folder_id;

        app.handle_key(key(KeyCode::Char(' '))).unwrap();
        app.handle_key(key(KeyCode::Char('m'))).unwrap();
        app.move_folder_selected = app
            .folder_picker_items()
            .iter()
            .position(|folder| folder.id == folder_id)
            .unwrap();
        app.handle_key(key(KeyCode::Enter)).unwrap();

        assert_eq!(app.mode, Mode::Browse);
        assert!(app.selected_hosts.is_empty());
        assert_eq!(app.vault.host(host_id).unwrap().folder_id, folder_id);
        assert_eq!(
            load_vault(&vault_path)
                .unwrap()
                .host(host_id)
                .unwrap()
                .folder_id,
            folder_id
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn starts_and_cancels_identity_edit_from_browse() {
        let mut app = sample_app();
        select_label(&mut app, "web");

        app.handle_key(key(KeyCode::Char('i'))).unwrap();
        assert_eq!(app.mode, Mode::EditIdentity);
        assert!(app.identity_editor.is_some());

        app.handle_key(key(KeyCode::Esc)).unwrap();
        assert_eq!(app.mode, Mode::Browse);
        assert!(app.identity_editor.is_none());
    }

    #[test]
    fn starts_identity_edit_from_search() {
        let mut app = sample_app();
        app.mode = Mode::Search;
        app.search = "web".to_string();

        app.handle_key(key(KeyCode::Char('i'))).unwrap();

        assert_eq!(app.mode, Mode::EditIdentity);
        assert_eq!(
            app.identity_editor.as_ref().unwrap().host_path,
            "Customers/web"
        );
    }

    #[test]
    fn starts_jumps_and_forwards_edit_from_search() {
        let mut app = sample_app();
        app.mode = Mode::Search;
        app.search = "web".to_string();

        app.handle_key(key(KeyCode::Char('J'))).unwrap();
        assert_eq!(app.mode, Mode::EditJumps);
        assert_eq!(app.jump_editor.as_ref().unwrap().host_path, "Customers/web");

        app.mode = Mode::Search;
        app.search = "web".to_string();
        app.handle_key(key(KeyCode::Char('F'))).unwrap();
        assert_eq!(app.mode, Mode::EditForwards);
        assert_eq!(
            app.forward_editor.as_ref().unwrap().host_path,
            "Customers/web"
        );
    }

    #[test]
    fn saves_jump_chain_to_host_and_persists_vault() {
        let (mut app, dir) = app_with_persisted_vault("jumps-save");
        let vault_path = app.vault_path.clone();
        select_label(&mut app, "web");
        let web_id = app.selected_host().unwrap().id;
        let lab_id = app
            .vault
            .hosts
            .iter()
            .find(|host| host.display_name == "lab")
            .unwrap()
            .id;

        app.handle_key(key(KeyCode::Char('J'))).unwrap();
        assert_eq!(app.mode, Mode::EditJumps);
        assert!(
            !app.jump_editor
                .as_ref()
                .unwrap()
                .choices
                .iter()
                .any(|choice| choice.host_id == web_id)
        );
        app.jump_editor.as_mut().unwrap().selected = app
            .jump_editor
            .as_ref()
            .unwrap()
            .choices
            .iter()
            .position(|choice| choice.host_id == lab_id)
            .unwrap();
        app.handle_key(key(KeyCode::Char(' '))).unwrap();
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))
            .unwrap();

        assert_eq!(app.mode, Mode::Browse);
        assert_eq!(app.status, "jumps saved");
        assert_eq!(app.vault.host(web_id).unwrap().jump_chain, vec![lab_id]);
        assert_eq!(
            load_vault(&vault_path)
                .unwrap()
                .host(web_id)
                .unwrap()
                .jump_chain,
            vec![lab_id]
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn saves_forwards_to_host_and_persists_vault() {
        let (mut app, dir) = app_with_persisted_vault("forwards-save");
        let vault_path = app.vault_path.clone();
        select_label(&mut app, "web");
        let web_id = app.selected_host().unwrap().id;

        app.handle_key(key(KeyCode::Char('F'))).unwrap();
        assert_eq!(app.mode, Mode::EditForwards);
        app.handle_key(key(KeyCode::Char('d'))).unwrap();
        app.forward_editor.as_mut().unwrap().rows[0].fields[1].value = "1080".to_string();
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))
            .unwrap();

        let expected = vec![ForwardDefinition::Dynamic {
            bind_address: "127.0.0.1".to_string(),
            local_port: 1080,
        }];
        assert_eq!(app.mode, Mode::Browse);
        assert_eq!(app.status, "forwards saved");
        assert_eq!(app.vault.host(web_id).unwrap().forwards, expected);
        assert_eq!(
            load_vault(&vault_path)
                .unwrap()
                .host(web_id)
                .unwrap()
                .forwards,
            expected
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_forward_save_keeps_editor_open_and_does_not_persist() {
        let (mut app, dir) = app_with_persisted_vault("forwards-invalid");
        let vault_path = app.vault_path.clone();
        select_label(&mut app, "web");
        let web_id = app.selected_host().unwrap().id;

        app.handle_key(key(KeyCode::Char('F'))).unwrap();
        app.handle_key(key(KeyCode::Char('a'))).unwrap();
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))
            .unwrap();

        assert_eq!(app.mode, Mode::EditForwards);
        assert!(app.status.starts_with("forwards edit error:"));
        assert!(app.vault.host(web_id).unwrap().forwards.is_empty());
        assert!(
            load_vault(&vault_path)
                .unwrap()
                .host(web_id)
                .unwrap()
                .forwards
                .is_empty()
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn saves_selected_local_identity_to_host() {
        let (mut app, dir) = app_with_persisted_vault("identity-select");
        let vault_path = app.vault_path.clone();
        let local_config_path = app.local_config_path.clone();
        app.local_config
            .map_identity(
                "SHA256:abc".to_string(),
                PathBuf::from("/home/alice/.ssh/acme"),
                Some("acme".to_string()),
            )
            .unwrap();
        save_local_config(&local_config_path, &app.local_config).unwrap();
        select_label(&mut app, "web");
        let host_id = app.selected_host().unwrap().id;

        app.handle_key(key(KeyCode::Char('i'))).unwrap();
        app.handle_key(key(KeyCode::Down)).unwrap();
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))
            .unwrap();

        assert_eq!(app.mode, Mode::Browse);
        assert_eq!(
            app.vault
                .host(host_id)
                .unwrap()
                .identity_fingerprint
                .as_deref(),
            Some("SHA256:abc")
        );
        let persisted = load_vault(&vault_path).unwrap();
        assert_eq!(
            persisted
                .host(host_id)
                .unwrap()
                .identity_fingerprint
                .as_deref(),
            Some("SHA256:abc")
        );
        assert!(
            load_local_config(&local_config_path)
                .unwrap()
                .identity_path("SHA256:abc")
                .is_some()
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn clears_host_identity_without_removing_local_mapping() {
        let (mut app, dir) = app_with_persisted_vault("identity-clear");
        let vault_path = app.vault_path.clone();
        let local_config_path = app.local_config_path.clone();
        let host_id = app
            .vault
            .hosts
            .iter()
            .find(|host| host.display_name == "web")
            .unwrap()
            .id;
        app.vault
            .hosts
            .iter_mut()
            .find(|host| host.id == host_id)
            .unwrap()
            .identity_fingerprint = Some("SHA256:abc".to_string());
        save_vault(&vault_path, &app.vault).unwrap();
        app.local_config
            .map_identity(
                "SHA256:abc".to_string(),
                PathBuf::from("/home/alice/.ssh/acme"),
                Some("acme".to_string()),
            )
            .unwrap();
        save_local_config(&local_config_path, &app.local_config).unwrap();
        select_label(&mut app, "web");

        app.handle_key(key(KeyCode::Char('i'))).unwrap();
        app.handle_key(key(KeyCode::Home)).unwrap();
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))
            .unwrap();

        assert!(
            app.vault
                .host(host_id)
                .unwrap()
                .identity_fingerprint
                .is_none()
        );
        assert!(
            load_vault(&vault_path)
                .unwrap()
                .host(host_id)
                .unwrap()
                .identity_fingerprint
                .is_none()
        );
        assert!(
            load_local_config(&local_config_path)
                .unwrap()
                .identity_path("SHA256:abc")
                .is_some()
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn preserves_unmapped_current_identity_when_saved() {
        let (mut app, dir) = app_with_persisted_vault("identity-unmapped-current");
        let vault_path = app.vault_path.clone();
        let host_id = app
            .vault
            .hosts
            .iter()
            .find(|host| host.display_name == "web")
            .unwrap()
            .id;
        app.vault
            .hosts
            .iter_mut()
            .find(|host| host.id == host_id)
            .unwrap()
            .identity_fingerprint = Some("SHA256:abc".to_string());
        save_vault(&vault_path, &app.vault).unwrap();
        select_label(&mut app, "web");

        app.handle_key(key(KeyCode::Char('i'))).unwrap();
        assert_eq!(
            app.identity_editor
                .as_ref()
                .unwrap()
                .selected_fingerprint()
                .as_deref(),
            Some("SHA256:abc")
        );
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))
            .unwrap();

        assert_eq!(app.mode, Mode::Browse);
        assert!(
            app.vault
                .host(host_id)
                .unwrap()
                .identity_fingerprint
                .is_some()
        );
        assert!(
            load_vault(&vault_path)
                .unwrap()
                .host(host_id)
                .unwrap()
                .identity_fingerprint
                .is_some()
        );

        fs::remove_dir_all(dir).unwrap();
    }
}
