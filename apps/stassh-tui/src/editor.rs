use anyhow::{Result, bail};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use stassh_core::{
    AddFolder, AddHost, Folder, ForwardDefinition, Host, LocalConfig, UpdateHost, Vault,
};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HostEditor {
    pub(crate) mode: HostEditorMode,
    pub(crate) fields: Vec<EditorField>,
    pub(crate) selected: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostEditorMode {
    Edit { host_id: Uuid },
    Create { folder_id: Uuid },
}

impl HostEditor {
    pub(crate) fn from_host(host: &Host) -> Self {
        Self {
            mode: HostEditorMode::Edit { host_id: host.id },
            fields: vec![
                EditorField::new(
                    EditorFieldKind::DisplayName,
                    "Name",
                    host.display_name.clone(),
                ),
                EditorField::new(EditorFieldKind::Hostname, "HostName", host.hostname.clone()),
                EditorField::new(EditorFieldKind::Port, "Port", host.port.to_string()),
                EditorField::new(
                    EditorFieldKind::Username,
                    "User",
                    host.username.clone().unwrap_or_default(),
                ),
                EditorField::new(EditorFieldKind::Tags, "Tags", host.tags.join(", ")),
                EditorField::new(
                    EditorFieldKind::Notes,
                    "Notes",
                    host.notes.clone().unwrap_or_default(),
                ),
            ],
            selected: 0,
        }
    }

    pub(crate) fn new_host(folder_id: Uuid) -> Self {
        Self {
            mode: HostEditorMode::Create { folder_id },
            fields: vec![
                EditorField::new(EditorFieldKind::DisplayName, "Name", String::new()),
                EditorField::new(EditorFieldKind::Hostname, "HostName", String::new()),
                EditorField::new(EditorFieldKind::Port, "Port", "22".to_string()),
                EditorField::new(EditorFieldKind::Username, "User", String::new()),
                EditorField::new(EditorFieldKind::Tags, "Tags", String::new()),
                EditorField::new(EditorFieldKind::Notes, "Notes", String::new()),
            ],
            selected: 0,
        }
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> EditorAction {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            return EditorAction::Save;
        }
        match key.code {
            KeyCode::Esc => EditorAction::Cancel,
            KeyCode::Tab | KeyCode::Down => {
                self.next_field();
                EditorAction::None
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.previous_field();
                EditorAction::None
            }
            KeyCode::Home => {
                self.current_value_mut().clear();
                EditorAction::None
            }
            KeyCode::End => EditorAction::None,
            KeyCode::Backspace => {
                self.current_value_mut().pop();
                EditorAction::None
            }
            KeyCode::Char(value) => {
                self.current_value_mut().push(value);
                EditorAction::None
            }
            _ => EditorAction::None,
        }
    }

    pub(crate) fn to_update(&self) -> Result<UpdateHost> {
        let values = self.parsed_values()?;

        Ok(UpdateHost {
            display_name: Some(values.display_name),
            hostname: Some(values.hostname),
            port: Some(values.port),
            username: Some(values.username),
            tags: Some(values.tags),
            notes: Some(values.notes),
            ..UpdateHost::default()
        })
    }

    pub(crate) fn to_add(&self) -> Result<AddHost> {
        let values = self.parsed_values()?;
        let HostEditorMode::Create { folder_id } = self.mode else {
            bail!("editor is not creating a host");
        };

        Ok(AddHost {
            folder_id: Some(folder_id),
            display_name: values.display_name,
            hostname: values.hostname,
            port: Some(values.port),
            username: values.username,
            identity_fingerprint: None,
            secrets: None,
            jump_chain: Vec::new(),
            ssh_options: Vec::new(),
            forwards: Vec::new(),
            tags: values.tags,
            notes: values.notes,
        })
    }

    fn next_field(&mut self) {
        self.selected = (self.selected + 1).min(self.fields.len().saturating_sub(1));
    }

    fn previous_field(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn current_value_mut(&mut self) -> &mut String {
        &mut self.fields[self.selected].value
    }

    fn value(&self, kind: EditorFieldKind) -> &str {
        self.fields
            .iter()
            .find(|field| field.kind == kind)
            .map(|field| field.value.as_str())
            .unwrap_or("")
    }

    fn parsed_values(&self) -> Result<ParsedHostFields> {
        let display_name = self.value(EditorFieldKind::DisplayName).trim().to_string();
        let hostname = self.value(EditorFieldKind::Hostname).trim().to_string();
        let port_text = self.value(EditorFieldKind::Port).trim();
        let port = if port_text.is_empty() {
            22
        } else {
            port_text
                .parse::<u16>()
                .map_err(|_| anyhow::anyhow!("port must be a number between 1 and 65535"))?
        };
        if port == 0 {
            bail!("port must be between 1 and 65535");
        }

        let username = empty_to_none(self.value(EditorFieldKind::Username));
        let notes = empty_to_none(self.value(EditorFieldKind::Notes));
        let tags = self
            .value(EditorFieldKind::Tags)
            .split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();

        Ok(ParsedHostFields {
            display_name,
            hostname,
            port,
            username,
            tags,
            notes,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedHostFields {
    display_name: String,
    hostname: String,
    port: u16,
    username: Option<String>,
    tags: Vec<String>,
    notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FolderEditor {
    pub(crate) mode: FolderEditorMode,
    pub(crate) fields: Vec<EditorField>,
    pub(crate) selected: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FolderEditorMode {
    Edit { folder_id: Uuid },
    Create { parent_id: Uuid },
}

impl FolderEditor {
    pub(crate) fn from_folder(vault: &Vault, folder: &Folder) -> Self {
        Self {
            mode: FolderEditorMode::Edit {
                folder_id: folder.id,
            },
            fields: vec![
                EditorField::new(EditorFieldKind::FolderName, "Name", folder.name.clone()),
                EditorField::new(
                    EditorFieldKind::ParentId,
                    "Parent",
                    folder
                        .parent_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| vault.root_folder_id().to_string()),
                ),
            ],
            selected: 0,
        }
    }

    pub(crate) fn new_folder(parent_id: Uuid) -> Self {
        Self {
            mode: FolderEditorMode::Create { parent_id },
            fields: vec![
                EditorField::new(EditorFieldKind::FolderName, "Name", String::new()),
                EditorField::new(EditorFieldKind::ParentId, "Parent", parent_id.to_string()),
            ],
            selected: 0,
        }
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> EditorAction {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            return EditorAction::Save;
        }
        match key.code {
            KeyCode::Esc => EditorAction::Cancel,
            KeyCode::Tab | KeyCode::Down => {
                self.next_field();
                EditorAction::None
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.previous_field();
                EditorAction::None
            }
            KeyCode::Home => {
                self.current_value_mut().clear();
                EditorAction::None
            }
            KeyCode::End => EditorAction::None,
            KeyCode::Backspace => {
                self.current_value_mut().pop();
                EditorAction::None
            }
            KeyCode::Char(value) => {
                self.current_value_mut().push(value);
                EditorAction::None
            }
            _ => EditorAction::None,
        }
    }

    pub(crate) fn to_add(&self) -> Result<AddFolder> {
        let name = self.name()?;
        let parent_id = self.parent_id()?;
        let FolderEditorMode::Create { .. } = self.mode else {
            bail!("editor is not creating a folder");
        };

        Ok(AddFolder {
            parent_id: Some(parent_id),
            name,
        })
    }

    pub(crate) fn name(&self) -> Result<String> {
        Ok(self.value(EditorFieldKind::FolderName).trim().to_string())
    }

    pub(crate) fn parent_id(&self) -> Result<Uuid> {
        self.value(EditorFieldKind::ParentId)
            .trim()
            .parse::<Uuid>()
            .map_err(|_| anyhow::anyhow!("parent must be a folder UUID"))
    }

    fn next_field(&mut self) {
        self.selected = (self.selected + 1).min(self.fields.len().saturating_sub(1));
    }

    fn previous_field(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn current_value_mut(&mut self) -> &mut String {
        &mut self.fields[self.selected].value
    }

    fn value(&self, kind: EditorFieldKind) -> &str {
        self.fields
            .iter()
            .find(|field| field.kind == kind)
            .map(|field| field.value.as_str())
            .unwrap_or("")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IdentityEditor {
    pub(crate) host_id: Uuid,
    pub(crate) host_path: String,
    pub(crate) choices: Vec<IdentityChoice>,
    pub(crate) selected: usize,
    pub(crate) original_fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IdentityChoice {
    pub(crate) fingerprint: Option<String>,
    pub(crate) label: String,
    pub(crate) detail: String,
}

impl IdentityEditor {
    pub(crate) fn from_host(vault: &Vault, local_config: &LocalConfig, host: &Host) -> Self {
        let original_fingerprint = host.identity_fingerprint.clone();
        let mut choices = vec![IdentityChoice {
            fingerprint: None,
            label: "(none)".to_string(),
            detail: "Use password/default SSH authentication".to_string(),
        }];
        choices.extend(local_config.identity_mappings.iter().map(|mapping| {
            let label = mapping
                .preferred_name
                .clone()
                .unwrap_or_else(|| mapping.fingerprint.clone());
            IdentityChoice {
                fingerprint: Some(mapping.fingerprint.clone()),
                label,
                detail: mapping.path.display().to_string(),
            }
        }));
        if let Some(fingerprint) = &original_fingerprint
            && !choices
                .iter()
                .any(|choice| choice.fingerprint.as_ref() == Some(fingerprint))
        {
            choices.push(IdentityChoice {
                fingerprint: Some(fingerprint.clone()),
                label: "(unmapped current identity)".to_string(),
                detail: fingerprint.clone(),
            });
        }
        let selected = original_fingerprint
            .as_ref()
            .and_then(|fingerprint| {
                choices
                    .iter()
                    .position(|choice| choice.fingerprint.as_ref() == Some(fingerprint))
            })
            .unwrap_or(0);

        Self {
            host_id: host.id,
            host_path: vault.host_path(host),
            choices,
            selected,
            original_fingerprint,
        }
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> EditorAction {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            return EditorAction::Save;
        }
        match key.code {
            KeyCode::Esc => EditorAction::Cancel,
            KeyCode::Tab | KeyCode::Down => {
                self.next_choice();
                EditorAction::None
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.previous_choice();
                EditorAction::None
            }
            KeyCode::Home => {
                self.selected = 0;
                EditorAction::None
            }
            KeyCode::End => {
                self.selected = self.choices.len().saturating_sub(1);
                EditorAction::None
            }
            _ => EditorAction::None,
        }
    }

    pub(crate) fn selected_fingerprint(&self) -> Option<String> {
        self.choices
            .get(self.selected)
            .and_then(|choice| choice.fingerprint.clone())
    }

    fn next_choice(&mut self) {
        if !self.choices.is_empty() {
            self.selected = (self.selected + 1) % self.choices.len();
        }
    }

    fn previous_choice(&mut self) {
        if !self.choices.is_empty() {
            self.selected = if self.selected == 0 {
                self.choices.len() - 1
            } else {
                self.selected - 1
            };
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JumpEditor {
    pub(crate) host_id: Uuid,
    pub(crate) host_path: String,
    pub(crate) choices: Vec<JumpChoice>,
    pub(crate) selected: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JumpChoice {
    pub(crate) host_id: Uuid,
    pub(crate) label: String,
    pub(crate) detail: String,
    pub(crate) chosen: bool,
}

impl JumpEditor {
    pub(crate) fn from_host(vault: &Vault, host: &Host) -> Self {
        let mut choices = Vec::new();
        for jump_id in &host.jump_chain {
            if let Some(jump) = vault.host(*jump_id) {
                choices.push(JumpChoice::from_host(vault, jump, true));
            }
        }

        let mut remaining = vault
            .hosts
            .iter()
            .filter(|candidate| candidate.id != host.id && !host.jump_chain.contains(&candidate.id))
            .collect::<Vec<_>>();
        remaining.sort_by_key(|candidate| vault.host_path(candidate));
        choices.extend(
            remaining
                .into_iter()
                .map(|candidate| JumpChoice::from_host(vault, candidate, false)),
        );

        Self {
            host_id: host.id,
            host_path: vault.host_path(host),
            choices,
            selected: 0,
        }
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> EditorAction {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            return EditorAction::Save;
        }
        match key.code {
            KeyCode::Esc => EditorAction::Cancel,
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                self.next_choice();
                EditorAction::None
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                self.previous_choice();
                EditorAction::None
            }
            KeyCode::Home => {
                self.selected = 0;
                EditorAction::None
            }
            KeyCode::End => {
                self.selected = self.choices.len().saturating_sub(1);
                EditorAction::None
            }
            KeyCode::Char(' ') => {
                self.toggle_selected();
                EditorAction::None
            }
            KeyCode::Char('[') => {
                self.move_selected_chosen(-1);
                EditorAction::None
            }
            KeyCode::Char(']') => {
                self.move_selected_chosen(1);
                EditorAction::None
            }
            _ => EditorAction::None,
        }
    }

    pub(crate) fn selected_jump_chain(&self) -> Vec<Uuid> {
        self.choices
            .iter()
            .filter(|choice| choice.chosen)
            .map(|choice| choice.host_id)
            .collect()
    }

    fn next_choice(&mut self) {
        if !self.choices.is_empty() {
            self.selected = (self.selected + 1).min(self.choices.len() - 1);
        }
    }

    fn previous_choice(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn toggle_selected(&mut self) {
        let Some(choice) = self.choices.get_mut(self.selected) else {
            return;
        };
        choice.chosen = !choice.chosen;
    }

    fn move_selected_chosen(&mut self, delta: isize) {
        let Some(choice) = self.choices.get(self.selected) else {
            return;
        };
        if !choice.chosen {
            return;
        }
        let chosen_positions = self
            .choices
            .iter()
            .enumerate()
            .filter_map(|(index, choice)| choice.chosen.then_some(index))
            .collect::<Vec<_>>();
        let Some(chosen_index) = chosen_positions
            .iter()
            .position(|position| *position == self.selected)
        else {
            return;
        };
        let next_chosen_index = (chosen_index as isize + delta)
            .clamp(0, chosen_positions.len().saturating_sub(1) as isize)
            as usize;
        let target = chosen_positions[next_chosen_index];
        self.choices.swap(self.selected, target);
        self.selected = target;
    }
}

impl JumpChoice {
    fn from_host(vault: &Vault, host: &Host, chosen: bool) -> Self {
        Self {
            host_id: host.id,
            label: vault.host_path(host),
            detail: format!(
                "{}@{}:{}",
                host.username.as_deref().unwrap_or("(default)"),
                host.hostname,
                host.port
            ),
            chosen,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ForwardEditor {
    pub(crate) host_id: Uuid,
    pub(crate) host_path: String,
    pub(crate) rows: Vec<ForwardRow>,
    pub(crate) selected_row: usize,
    pub(crate) selected_field: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ForwardRow {
    pub(crate) kind: ForwardRowKind,
    pub(crate) fields: Vec<ForwardField>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ForwardRowKind {
    Local,
    Remote,
    Dynamic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ForwardField {
    pub(crate) label: &'static str,
    pub(crate) value: String,
}

impl ForwardEditor {
    pub(crate) fn from_host(vault: &Vault, host: &Host) -> Self {
        Self {
            host_id: host.id,
            host_path: vault.host_path(host),
            rows: host.forwards.iter().map(ForwardRow::from_forward).collect(),
            selected_row: 0,
            selected_field: 0,
        }
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> EditorAction {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            return EditorAction::Save;
        }
        match key.code {
            KeyCode::Esc => EditorAction::Cancel,
            KeyCode::Char('a') => {
                self.add_row(ForwardRowKind::Local);
                EditorAction::None
            }
            KeyCode::Char('A') => {
                self.add_row(ForwardRowKind::Remote);
                EditorAction::None
            }
            KeyCode::Char('d') => {
                self.add_row(ForwardRowKind::Dynamic);
                EditorAction::None
            }
            KeyCode::Char('x') | KeyCode::Delete => {
                self.remove_selected_row();
                EditorAction::None
            }
            KeyCode::Down => {
                self.next_row();
                EditorAction::None
            }
            KeyCode::Up => {
                self.previous_row();
                EditorAction::None
            }
            KeyCode::Tab => {
                self.next_field();
                EditorAction::None
            }
            KeyCode::BackTab => {
                self.previous_field();
                EditorAction::None
            }
            KeyCode::Home => {
                if self.rows.is_empty() {
                    self.selected_row = 0;
                } else {
                    self.current_value_mut().clear();
                }
                EditorAction::None
            }
            KeyCode::End => {
                self.selected_row = self.rows.len().saturating_sub(1);
                self.clamp_selected_field();
                EditorAction::None
            }
            KeyCode::Backspace => {
                if !self.rows.is_empty() {
                    self.current_value_mut().pop();
                }
                EditorAction::None
            }
            KeyCode::Char(value) => {
                if !self.rows.is_empty() {
                    self.current_value_mut().push(value);
                }
                EditorAction::None
            }
            _ => EditorAction::None,
        }
    }

    pub(crate) fn to_forwards(&self) -> Result<Vec<ForwardDefinition>> {
        self.rows.iter().map(ForwardRow::to_forward).collect()
    }

    fn add_row(&mut self, kind: ForwardRowKind) {
        self.rows.push(ForwardRow::new(kind));
        self.selected_row = self.rows.len().saturating_sub(1);
        self.selected_field = 0;
    }

    fn remove_selected_row(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        self.rows.remove(self.selected_row);
        self.selected_row = self.selected_row.min(self.rows.len().saturating_sub(1));
        self.clamp_selected_field();
    }

    fn next_row(&mut self) {
        if !self.rows.is_empty() {
            self.selected_row = (self.selected_row + 1).min(self.rows.len() - 1);
            self.clamp_selected_field();
        }
    }

    fn previous_row(&mut self) {
        self.selected_row = self.selected_row.saturating_sub(1);
        self.clamp_selected_field();
    }

    fn next_field(&mut self) {
        let Some(row) = self.rows.get(self.selected_row) else {
            return;
        };
        self.selected_field = (self.selected_field + 1).min(row.fields.len().saturating_sub(1));
    }

    fn previous_field(&mut self) {
        self.selected_field = self.selected_field.saturating_sub(1);
    }

    fn clamp_selected_field(&mut self) {
        let len = self
            .rows
            .get(self.selected_row)
            .map(|row| row.fields.len())
            .unwrap_or(0);
        self.selected_field = self.selected_field.min(len.saturating_sub(1));
    }

    fn current_value_mut(&mut self) -> &mut String {
        &mut self.rows[self.selected_row].fields[self.selected_field].value
    }
}

impl ForwardRow {
    fn new(kind: ForwardRowKind) -> Self {
        let fields = match kind {
            ForwardRowKind::Local => vec![
                ForwardField::new("Bind", "127.0.0.1"),
                ForwardField::new("LocalPort", "0"),
                ForwardField::new("DestHost", ""),
                ForwardField::new("DestPort", "0"),
            ],
            ForwardRowKind::Remote => vec![
                ForwardField::new("Bind", "127.0.0.1"),
                ForwardField::new("RemotePort", "0"),
                ForwardField::new("DestHost", ""),
                ForwardField::new("DestPort", "0"),
            ],
            ForwardRowKind::Dynamic => vec![
                ForwardField::new("Bind", "127.0.0.1"),
                ForwardField::new("LocalPort", "0"),
            ],
        };
        Self { kind, fields }
    }

    fn from_forward(forward: &ForwardDefinition) -> Self {
        match forward {
            ForwardDefinition::Local {
                bind_address,
                local_port,
                destination_host,
                destination_port,
            } => Self {
                kind: ForwardRowKind::Local,
                fields: vec![
                    ForwardField::new("Bind", bind_address),
                    ForwardField::new("LocalPort", &local_port.to_string()),
                    ForwardField::new("DestHost", destination_host),
                    ForwardField::new("DestPort", &destination_port.to_string()),
                ],
            },
            ForwardDefinition::Remote {
                bind_address,
                remote_port,
                destination_host,
                destination_port,
            } => Self {
                kind: ForwardRowKind::Remote,
                fields: vec![
                    ForwardField::new("Bind", bind_address),
                    ForwardField::new("RemotePort", &remote_port.to_string()),
                    ForwardField::new("DestHost", destination_host),
                    ForwardField::new("DestPort", &destination_port.to_string()),
                ],
            },
            ForwardDefinition::Dynamic {
                bind_address,
                local_port,
            } => Self {
                kind: ForwardRowKind::Dynamic,
                fields: vec![
                    ForwardField::new("Bind", bind_address),
                    ForwardField::new("LocalPort", &local_port.to_string()),
                ],
            },
        }
    }

    fn to_forward(&self) -> Result<ForwardDefinition> {
        match self.kind {
            ForwardRowKind::Local => Ok(ForwardDefinition::Local {
                bind_address: self.required_text(0, "local forward bind address")?,
                local_port: self.required_port(1, "local forward local port")?,
                destination_host: self.required_text(2, "local forward destination host")?,
                destination_port: self.required_port(3, "local forward destination port")?,
            }),
            ForwardRowKind::Remote => Ok(ForwardDefinition::Remote {
                bind_address: self.required_text(0, "remote forward bind address")?,
                remote_port: self.required_port(1, "remote forward remote port")?,
                destination_host: self.required_text(2, "remote forward destination host")?,
                destination_port: self.required_port(3, "remote forward destination port")?,
            }),
            ForwardRowKind::Dynamic => Ok(ForwardDefinition::Dynamic {
                bind_address: self.required_text(0, "dynamic forward bind address")?,
                local_port: self.required_port(1, "dynamic forward local port")?,
            }),
        }
    }

    fn required_text(&self, index: usize, label: &'static str) -> Result<String> {
        let value = self
            .fields
            .get(index)
            .map(|field| field.value.trim())
            .unwrap_or("");
        if value.is_empty() {
            bail!("{label} must not be empty");
        }
        Ok(value.to_string())
    }

    fn required_port(&self, index: usize, label: &'static str) -> Result<u16> {
        let value = self.required_text(index, label)?;
        let port = value
            .parse::<u16>()
            .map_err(|_| anyhow::anyhow!("{label} must be a number between 1 and 65535"))?;
        if port == 0 {
            bail!("{label} must be between 1 and 65535");
        }
        Ok(port)
    }
}

impl ForwardField {
    fn new(label: &'static str, value: &str) -> Self {
        Self {
            label,
            value: value.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EditorField {
    pub(crate) kind: EditorFieldKind,
    pub(crate) label: &'static str,
    pub(crate) value: String,
}

impl EditorField {
    fn new(kind: EditorFieldKind, label: &'static str, value: String) -> Self {
        Self { kind, label, value }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorFieldKind {
    DisplayName,
    Hostname,
    Port,
    Username,
    Tags,
    Notes,
    FolderName,
    ParentId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorAction {
    None,
    Save,
    Cancel,
}

fn empty_to_none(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use stassh_core::{AddHost, Host, LocalConfig};
    use uuid::Uuid;

    use super::*;

    fn host() -> Host {
        Host {
            id: Uuid::new_v4(),
            folder_id: Uuid::new_v4(),
            display_name: "web".to_string(),
            hostname: "web.example".to_string(),
            port: 2222,
            username: Some("deploy".to_string()),
            identity_fingerprint: None,
            secrets: None,
            jump_chain: Vec::new(),
            ssh_options: Vec::new(),
            forwards: Vec::new(),
            actions: Vec::new(),
            tags: vec!["prod".to_string(), "web".to_string()],
            notes: Some("note".to_string()),
        }
    }

    fn host_with_identity() -> Host {
        Host {
            identity_fingerprint: Some("SHA256:abc".to_string()),
            ..host()
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn vault_with_jump_hosts() -> (Vault, Uuid, Uuid, Uuid) {
        let mut vault = Vault::new();
        let bastion = vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "bastion".to_string(),
                hostname: "bastion.example".to_string(),
                port: None,
                username: Some("admin".to_string()),
                identity_fingerprint: None,
                secrets: None,
                jump_chain: Vec::new(),
                ssh_options: Vec::new(),
                forwards: Vec::new(),
                tags: Vec::new(),
                notes: None,
            })
            .unwrap();
        let gateway = vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "gateway".to_string(),
                hostname: "gateway.example".to_string(),
                port: None,
                username: None,
                identity_fingerprint: None,
                secrets: None,
                jump_chain: Vec::new(),
                ssh_options: Vec::new(),
                forwards: Vec::new(),
                tags: Vec::new(),
                notes: None,
            })
            .unwrap();
        let web = vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "web".to_string(),
                hostname: "web.example".to_string(),
                port: None,
                username: None,
                identity_fingerprint: None,
                secrets: None,
                jump_chain: vec![bastion.id],
                ssh_options: Vec::new(),
                forwards: Vec::new(),
                tags: Vec::new(),
                notes: None,
            })
            .unwrap();
        (vault, web.id, bastion.id, gateway.id)
    }

    #[test]
    fn initializes_fields_from_host() {
        let editor = HostEditor::from_host(&host());

        assert_eq!(editor.value(EditorFieldKind::DisplayName), "web");
        assert_eq!(editor.value(EditorFieldKind::Hostname), "web.example");
        assert_eq!(editor.value(EditorFieldKind::Port), "2222");
        assert_eq!(editor.value(EditorFieldKind::Username), "deploy");
        assert_eq!(editor.value(EditorFieldKind::Tags), "prod, web");
        assert_eq!(editor.value(EditorFieldKind::Notes), "note");
    }

    #[test]
    fn initializes_blank_create_form() {
        let folder_id = Uuid::new_v4();
        let editor = HostEditor::new_host(folder_id);

        assert_eq!(editor.mode, HostEditorMode::Create { folder_id });
        assert_eq!(editor.value(EditorFieldKind::Port), "22");
        assert_eq!(editor.value(EditorFieldKind::DisplayName), "");
        assert_eq!(editor.value(EditorFieldKind::Hostname), "");
    }

    #[test]
    fn navigates_and_edits_text() {
        let mut editor = HostEditor::from_host(&host());

        editor.handle_key(key(KeyCode::Down));
        assert_eq!(editor.selected, 1);
        editor.handle_key(key(KeyCode::Char('x')));
        assert!(editor.value(EditorFieldKind::Hostname).ends_with('x'));
        editor.handle_key(key(KeyCode::Backspace));
        assert_eq!(editor.value(EditorFieldKind::Hostname), "web.example");
        editor.handle_key(key(KeyCode::Up));
        assert_eq!(editor.selected, 0);
    }

    #[test]
    fn parses_update_values() {
        let mut editor = HostEditor::from_host(&host());
        editor.fields[2].value = "".to_string();
        editor.fields[3].value = " ".to_string();
        editor.fields[4].value = " prod, , db ".to_string();
        editor.fields[5].value = "".to_string();

        let update = editor.to_update().unwrap();

        assert_eq!(update.port, Some(22));
        assert_eq!(update.username, Some(None));
        assert_eq!(
            update.tags,
            Some(vec!["prod".to_string(), "db".to_string()])
        );
        assert_eq!(update.notes, Some(None));
    }

    #[test]
    fn parses_add_host_values() {
        let folder_id = Uuid::new_v4();
        let mut editor = HostEditor::new_host(folder_id);
        editor.fields[0].value = "db".to_string();
        editor.fields[1].value = "db.example".to_string();
        editor.fields[2].value = "2222".to_string();
        editor.fields[3].value = "postgres".to_string();
        editor.fields[4].value = "prod, db".to_string();
        editor.fields[5].value = "primary".to_string();

        let add = editor.to_add().unwrap();

        assert_eq!(add.folder_id, Some(folder_id));
        assert_eq!(add.display_name, "db");
        assert_eq!(add.hostname, "db.example");
        assert_eq!(add.port, Some(2222));
        assert_eq!(add.username.as_deref(), Some("postgres"));
        assert_eq!(add.tags, vec!["prod".to_string(), "db".to_string()]);
        assert_eq!(add.notes.as_deref(), Some("primary"));
    }

    #[test]
    fn rejects_invalid_port() {
        let mut editor = HostEditor::from_host(&host());
        editor.fields[2].value = "70000".to_string();

        assert!(editor.to_update().is_err());
    }

    #[test]
    fn identity_editor_initializes_without_identity() {
        let vault = Vault::new();
        let host = host();
        let editor = IdentityEditor::from_host(&vault, &LocalConfig::new(), &host);

        assert_eq!(editor.host_id, host.id);
        assert_eq!(editor.selected_fingerprint(), None);
        assert_eq!(editor.selected, 0);
        assert_eq!(editor.choices.len(), 1);
        assert_eq!(editor.choices[0].label, "(none)");
    }

    #[test]
    fn identity_editor_initializes_with_mapped_identity() {
        let vault = Vault::new();
        let host = host_with_identity();
        let mut local_config = LocalConfig::new();
        local_config
            .map_identity(
                "SHA256:abc".to_string(),
                PathBuf::from("/home/alice/.ssh/acme"),
                Some("local-acme".to_string()),
            )
            .unwrap();

        let editor = IdentityEditor::from_host(&vault, &local_config, &host);

        assert_eq!(editor.selected_fingerprint().as_deref(), Some("SHA256:abc"));
        assert_eq!(editor.selected, 1);
        assert_eq!(editor.choices[1].label, "local-acme");
        assert_eq!(editor.choices[1].detail, "/home/alice/.ssh/acme");
    }

    #[test]
    fn identity_editor_preserves_unmapped_current_identity_choice() {
        let vault = Vault::new();
        let host = host_with_identity();

        let editor = IdentityEditor::from_host(&vault, &LocalConfig::new(), &host);

        assert_eq!(editor.selected_fingerprint().as_deref(), Some("SHA256:abc"));
        assert_eq!(editor.selected, 1);
        assert_eq!(editor.choices[1].label, "(unmapped current identity)");
    }

    #[test]
    fn jump_editor_excludes_edited_host_and_preserves_existing_chain_first() {
        let (vault, web_id, bastion_id, gateway_id) = vault_with_jump_hosts();
        let web = vault.host(web_id).unwrap();

        let editor = JumpEditor::from_host(&vault, web);

        assert_eq!(editor.host_id, web_id);
        assert_eq!(editor.choices[0].host_id, bastion_id);
        assert!(editor.choices[0].chosen);
        assert!(
            editor
                .choices
                .iter()
                .any(|choice| choice.host_id == gateway_id)
        );
        assert!(!editor.choices.iter().any(|choice| choice.host_id == web_id));
    }

    #[test]
    fn jump_editor_toggles_and_reorders_selected_jumps() {
        let (vault, web_id, bastion_id, gateway_id) = vault_with_jump_hosts();
        let web = vault.host(web_id).unwrap();
        let mut editor = JumpEditor::from_host(&vault, web);
        editor.selected = editor
            .choices
            .iter()
            .position(|choice| choice.host_id == gateway_id)
            .unwrap();

        editor.handle_key(key(KeyCode::Char(' ')));
        assert_eq!(editor.selected_jump_chain(), vec![bastion_id, gateway_id]);

        editor.handle_key(key(KeyCode::Char('[')));
        assert_eq!(editor.selected_jump_chain(), vec![gateway_id, bastion_id]);
    }

    #[test]
    fn forward_editor_parses_existing_and_added_forwards() {
        let mut vault = Vault::new();
        let mut host = host();
        host.forwards = vec![ForwardDefinition::Local {
            bind_address: "127.0.0.1".to_string(),
            local_port: 8080,
            destination_host: "10.0.0.7".to_string(),
            destination_port: 80,
        }];
        vault.hosts.push(host.clone());
        let mut editor = ForwardEditor::from_host(&vault, &host);

        editor.handle_key(key(KeyCode::Char('d')));
        let row = editor.rows.last_mut().unwrap();
        row.fields[1].value = "1080".to_string();

        assert_eq!(
            editor.to_forwards().unwrap(),
            vec![
                ForwardDefinition::Local {
                    bind_address: "127.0.0.1".to_string(),
                    local_port: 8080,
                    destination_host: "10.0.0.7".to_string(),
                    destination_port: 80,
                },
                ForwardDefinition::Dynamic {
                    bind_address: "127.0.0.1".to_string(),
                    local_port: 1080,
                },
            ]
        );
    }

    #[test]
    fn forward_editor_rejects_placeholder_ports_and_empty_hosts() {
        let vault = Vault::new();
        let host = host();
        let mut editor = ForwardEditor::from_host(&vault, &host);

        editor.handle_key(key(KeyCode::Char('a')));
        assert!(editor.to_forwards().is_err());

        let row = editor.rows.last_mut().unwrap();
        row.fields[1].value = "8080".to_string();
        row.fields[3].value = "80".to_string();
        assert!(editor.to_forwards().is_err());
    }
}
