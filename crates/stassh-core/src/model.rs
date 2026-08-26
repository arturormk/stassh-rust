use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const CURRENT_FORMAT_VERSION: u32 = 0;

#[derive(Debug, Error)]
pub enum StasshError {
    #[error("vault format version {found} is not supported by this build; expected {expected}")]
    UnsupportedFormat { found: u32, expected: u32 },
    #[error("folder not found: {0}")]
    FolderNotFound(String),
    #[error("host not found: {0}")]
    HostNotFound(String),
    #[error("cannot delete the root folder")]
    CannotDeleteRootFolder,
    #[error("folder is not empty: {0}")]
    FolderNotEmpty(String),
    #[error("more than one host matched {selector}: {matches}")]
    AmbiguousHost { selector: String, matches: String },
    #[error("invalid value for {field}: {reason}")]
    InvalidValue { field: &'static str, reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Vault {
    pub format_version: u32,
    #[serde(default)]
    pub actions: Vec<ActionDefinition>,
    pub folders: Vec<Folder>,
    pub hosts: Vec<Host>,
}

impl Default for Vault {
    fn default() -> Self {
        Self::new()
    }
}

impl Vault {
    pub fn new() -> Self {
        Self {
            format_version: CURRENT_FORMAT_VERSION,
            actions: Vec::new(),
            folders: vec![Folder {
                id: Uuid::new_v4(),
                parent_id: None,
                name: "Root".to_string(),
            }],
            hosts: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), StasshError> {
        if self.format_version != CURRENT_FORMAT_VERSION {
            return Err(StasshError::UnsupportedFormat {
                found: self.format_version,
                expected: CURRENT_FORMAT_VERSION,
            });
        }

        let folder_ids: HashSet<Uuid> = self.folders.iter().map(|folder| folder.id).collect();
        for folder in &self.folders {
            validate_name("folder.name", &folder.name)?;
            if let Some(parent_id) = folder.parent_id
                && !folder_ids.contains(&parent_id)
            {
                return Err(StasshError::FolderNotFound(parent_id.to_string()));
            }
        }

        for host in &self.hosts {
            validate_name("host.display_name", &host.display_name)?;
            if host.hostname.trim().is_empty() {
                return Err(StasshError::InvalidValue {
                    field: "host.hostname",
                    reason: "must not be empty".to_string(),
                });
            }
            if !folder_ids.contains(&host.folder_id) {
                return Err(StasshError::FolderNotFound(host.folder_id.to_string()));
            }
            if host
                .secrets
                .as_ref()
                .is_some_and(|secrets| secrets.trim().is_empty())
            {
                return Err(StasshError::InvalidValue {
                    field: "host.secrets",
                    reason: "must not be empty".to_string(),
                });
            }
            for jump_id in &host.jump_chain {
                if !self.hosts.iter().any(|jump| jump.id == *jump_id) {
                    return Err(StasshError::HostNotFound(jump_id.to_string()));
                }
                if *jump_id == host.id {
                    return Err(StasshError::InvalidValue {
                        field: "host.jump_chain",
                        reason: "host cannot jump through itself".to_string(),
                    });
                }
            }
        }

        Ok(())
    }

    pub fn root_folder_id(&self) -> Uuid {
        self.folders
            .iter()
            .find(|folder| folder.parent_id.is_none())
            .map(|folder| folder.id)
            .expect("Vault::new always creates a root folder")
    }

    pub fn add_folder(&mut self, add: AddFolder) -> Result<Folder, StasshError> {
        validate_name("folder.name", &add.name)?;
        let parent_id = add.parent_id.unwrap_or_else(|| self.root_folder_id());
        if self.folder(parent_id).is_none() {
            return Err(StasshError::FolderNotFound(parent_id.to_string()));
        }

        let folder = Folder {
            id: Uuid::new_v4(),
            parent_id: Some(parent_id),
            name: add.name,
        };
        self.folders.push(folder.clone());
        Ok(folder)
    }

    pub fn rename_folder(&mut self, id: Uuid, name: String) -> Result<Folder, StasshError> {
        validate_name("folder.name", &name)?;
        let folder = self
            .folders
            .iter_mut()
            .find(|folder| folder.id == id)
            .ok_or_else(|| StasshError::FolderNotFound(id.to_string()))?;
        folder.name = name;
        Ok(folder.clone())
    }

    pub fn move_folder(&mut self, id: Uuid, new_parent_id: Uuid) -> Result<Folder, StasshError> {
        if id == self.root_folder_id() {
            return Err(StasshError::CannotDeleteRootFolder);
        }
        if self.folder(new_parent_id).is_none() {
            return Err(StasshError::FolderNotFound(new_parent_id.to_string()));
        }
        if id == new_parent_id || self.is_descendant_folder(new_parent_id, id) {
            return Err(StasshError::InvalidValue {
                field: "folder.parent_id",
                reason: "folder cannot be moved inside itself".to_string(),
            });
        }

        let folder = self
            .folders
            .iter_mut()
            .find(|folder| folder.id == id)
            .ok_or_else(|| StasshError::FolderNotFound(id.to_string()))?;
        folder.parent_id = Some(new_parent_id);
        Ok(folder.clone())
    }

    pub fn delete_folder(&mut self, id: Uuid) -> Result<Folder, StasshError> {
        if id == self.root_folder_id() {
            return Err(StasshError::CannotDeleteRootFolder);
        }
        let folder = self
            .folder(id)
            .ok_or_else(|| StasshError::FolderNotFound(id.to_string()))?;
        if self.folders.iter().any(|child| child.parent_id == Some(id))
            || self.hosts.iter().any(|host| host.folder_id == id)
        {
            return Err(StasshError::FolderNotEmpty(self.folder_path(id)));
        }
        let index = self
            .folders
            .iter()
            .position(|folder| folder.id == id)
            .expect("folder existence checked above");
        let folder = folder.clone();
        self.folders.remove(index);
        Ok(folder)
    }

    pub fn add_host(&mut self, add: AddHost) -> Result<Host, StasshError> {
        validate_name("host.display_name", &add.display_name)?;
        if add.hostname.trim().is_empty() {
            return Err(StasshError::InvalidValue {
                field: "host.hostname",
                reason: "must not be empty".to_string(),
            });
        }

        let folder_id = add.folder_id.unwrap_or_else(|| self.root_folder_id());
        if self.folder(folder_id).is_none() {
            return Err(StasshError::FolderNotFound(folder_id.to_string()));
        }
        for jump_id in &add.jump_chain {
            if self.host(*jump_id).is_none() {
                return Err(StasshError::HostNotFound(jump_id.to_string()));
            }
        }

        let host = Host {
            id: Uuid::new_v4(),
            folder_id,
            display_name: add.display_name,
            hostname: add.hostname,
            port: add.port.unwrap_or(22),
            username: add.username,
            identity_fingerprint: add.identity_fingerprint,
            secrets: add.secrets,
            jump_chain: add.jump_chain,
            ssh_options: add.ssh_options,
            forwards: add.forwards,
            actions: Vec::new(),
            tags: add.tags,
            notes: add.notes,
        };
        self.hosts.push(host.clone());
        Ok(host)
    }

    pub fn update_host(
        &mut self,
        selector: HostSelector<'_>,
        update: UpdateHost,
    ) -> Result<Host, StasshError> {
        let id = self.resolve_host(selector)?.id;

        if let Some(folder_id) = update.folder_id
            && self.folder(folder_id).is_none()
        {
            return Err(StasshError::FolderNotFound(folder_id.to_string()));
        }
        if let Some(display_name) = update.display_name.as_ref() {
            validate_name("host.display_name", display_name)?;
        }
        if let Some(hostname) = update.hostname.as_ref()
            && hostname.trim().is_empty()
        {
            return Err(StasshError::InvalidValue {
                field: "host.hostname",
                reason: "must not be empty".to_string(),
            });
        }
        if update
            .secrets
            .as_ref()
            .and_then(|secrets| secrets.as_ref())
            .is_some_and(|secrets| secrets.trim().is_empty())
        {
            return Err(StasshError::InvalidValue {
                field: "host.secrets",
                reason: "must not be empty".to_string(),
            });
        }
        if let Some(jump_chain) = update.jump_chain.as_ref() {
            for jump_id in jump_chain {
                if *jump_id == id {
                    return Err(StasshError::InvalidValue {
                        field: "host.jump_chain",
                        reason: "host cannot jump through itself".to_string(),
                    });
                }
                if self.host(*jump_id).is_none() {
                    return Err(StasshError::HostNotFound(jump_id.to_string()));
                }
            }
        }

        let host = self
            .hosts
            .iter_mut()
            .find(|host| host.id == id)
            .expect("host ID was resolved immediately above");

        if let Some(folder_id) = update.folder_id {
            host.folder_id = folder_id;
        }
        if let Some(display_name) = update.display_name {
            host.display_name = display_name;
        }
        if let Some(hostname) = update.hostname {
            host.hostname = hostname;
        }
        if let Some(port) = update.port {
            host.port = port;
        }
        if let Some(username) = update.username {
            host.username = username;
        }
        if let Some(identity_fingerprint) = update.identity_fingerprint {
            host.identity_fingerprint = identity_fingerprint;
        }
        if let Some(secrets) = update.secrets {
            host.secrets = secrets;
        }
        if let Some(jump_chain) = update.jump_chain {
            host.jump_chain = jump_chain;
        }
        if let Some(ssh_options) = update.ssh_options {
            host.ssh_options = ssh_options;
        }
        if let Some(forwards) = update.forwards {
            host.forwards = forwards;
        }
        if let Some(actions) = update.actions {
            host.actions = actions;
        }
        if let Some(tags) = update.tags {
            host.tags = tags;
        }
        if let Some(notes) = update.notes {
            host.notes = notes;
        }

        Ok(host.clone())
    }

    pub fn delete_host(&mut self, selector: HostSelector<'_>) -> Result<Host, StasshError> {
        let id = self.resolve_host(selector)?.id;
        let index = self
            .hosts
            .iter()
            .position(|host| host.id == id)
            .expect("host ID was resolved immediately above");
        let host = self.hosts.remove(index);
        for remaining in &mut self.hosts {
            remaining.jump_chain.retain(|jump_id| *jump_id != id);
        }
        Ok(host)
    }

    pub fn folder(&self, id: Uuid) -> Option<&Folder> {
        self.folders.iter().find(|folder| folder.id == id)
    }

    pub fn host(&self, id: Uuid) -> Option<&Host> {
        self.hosts.iter().find(|host| host.id == id)
    }

    pub fn folder_path(&self, folder_id: Uuid) -> String {
        let folders_by_id: HashMap<Uuid, &Folder> = self
            .folders
            .iter()
            .map(|folder| (folder.id, folder))
            .collect();
        let mut names = Vec::new();
        let mut current_id = Some(folder_id);

        while let Some(id) = current_id {
            let Some(folder) = folders_by_id.get(&id) else {
                break;
            };
            if folder.parent_id.is_some() {
                names.push(folder.name.as_str());
            }
            current_id = folder.parent_id;
        }

        names.reverse();
        if names.is_empty() {
            "/".to_string()
        } else {
            names.join("/")
        }
    }

    pub fn host_path(&self, host: &Host) -> String {
        let folder_path = self.folder_path(host.folder_id);
        if folder_path == "/" {
            host.display_name.clone()
        } else {
            format!("{folder_path}/{}", host.display_name)
        }
    }

    pub fn search_hosts(&self, query: &str) -> Vec<&Host> {
        let normalized_terms: Vec<String> = query
            .split_whitespace()
            .map(|term| term.to_lowercase())
            .collect();
        if normalized_terms.is_empty() {
            return self.hosts.iter().collect();
        }

        self.hosts
            .iter()
            .filter(|host| {
                let haystack = self.host_search_text(host).to_lowercase();
                normalized_terms
                    .iter()
                    .all(|term| haystack.contains(term.as_str()))
            })
            .collect()
    }

    pub fn resolve_host(&self, selector: HostSelector<'_>) -> Result<ResolvedHost, StasshError> {
        let host = match selector {
            HostSelector::Id(id) => self
                .host(id)
                .ok_or_else(|| StasshError::HostNotFound(id.to_string()))?,
            HostSelector::Query(query) => self.find_one_host(query)?,
        };

        let jump_chain = host
            .jump_chain
            .iter()
            .map(|jump_id| {
                self.host(*jump_id)
                    .ok_or_else(|| StasshError::HostNotFound(jump_id.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|jump| ResolvedJump {
                id: jump.id,
                display_name: jump.display_name.clone(),
                hostname: jump.hostname.clone(),
                port: jump.port,
                username: jump.username.clone(),
            })
            .collect();

        let mut actions = self.actions.clone();
        actions.extend(host.actions.clone());

        Ok(ResolvedHost {
            id: host.id,
            path: self.host_path(host),
            display_name: host.display_name.clone(),
            hostname: host.hostname.clone(),
            port: host.port,
            username: host.username.clone(),
            identity_fingerprint: host.identity_fingerprint.clone(),
            secrets: host.secrets.clone(),
            jump_chain,
            ssh_options: host.ssh_options.clone(),
            forwards: host.forwards.clone(),
            actions,
            tags: host.tags.clone(),
            notes: host.notes.clone(),
        })
    }

    pub fn duplicate_hosts(&self) -> Vec<DuplicateHostGroup> {
        let mut groups = Vec::new();
        groups.extend(self.duplicate_hosts_by_path());
        groups.extend(self.duplicate_hosts_by_connection());
        groups
    }

    pub fn host_dedupe_plan(&self) -> HostDedupePlan {
        let mut by_path: HashMap<String, Vec<&Host>> = HashMap::new();
        for host in &self.hosts {
            by_path.entry(self.host_path(host)).or_default().push(host);
        }

        let mut groups = by_path
            .into_iter()
            .filter_map(|(path, hosts)| {
                if hosts.len() < 2 {
                    return None;
                }
                let keep = hosts[0];
                let remove = hosts[1..]
                    .iter()
                    .map(|host| DuplicateHostEntry::from_host(host, self.host_path(host)))
                    .collect::<Vec<_>>();
                Some(HostDedupeGroup {
                    path,
                    keep: DuplicateHostEntry::from_host(keep, self.host_path(keep)),
                    remove,
                })
            })
            .collect::<Vec<_>>();
        groups.sort_by(|left, right| left.path.cmp(&right.path));

        let remove_count = groups.iter().map(|group| group.remove.len()).sum();
        HostDedupePlan {
            strategy: HostDedupeStrategy::Path,
            groups,
            remove_count,
        }
    }

    pub fn apply_host_dedupe_plan(&mut self, plan: &HostDedupePlan) -> HostDedupeResult {
        let mut replacement_ids = HashMap::new();
        let mut removed = Vec::new();

        for group in &plan.groups {
            for host in &group.remove {
                replacement_ids.insert(host.id, group.keep.id);
                removed.push(host.clone());
            }
        }

        let mut rewritten_jump_references = 0;
        for host in &mut self.hosts {
            for jump_id in &mut host.jump_chain {
                if let Some(keep_id) = replacement_ids.get(jump_id) {
                    *jump_id = *keep_id;
                    rewritten_jump_references += 1;
                }
            }
            let mut seen = HashSet::new();
            host.jump_chain.retain(|jump_id| seen.insert(*jump_id));
        }

        self.hosts
            .retain(|host| !replacement_ids.contains_key(&host.id));

        HostDedupeResult {
            strategy: plan.strategy.clone(),
            removed,
            removed_count: replacement_ids.len(),
            rewritten_jump_references,
        }
    }

    fn duplicate_hosts_by_path(&self) -> Vec<DuplicateHostGroup> {
        let mut by_path: HashMap<String, Vec<DuplicateHostEntry>> = HashMap::new();
        for host in &self.hosts {
            let path = self.host_path(host);
            by_path
                .entry(path.clone())
                .or_default()
                .push(DuplicateHostEntry::from_host(host, path));
        }

        duplicate_groups(by_path, DuplicateHostKind::Path)
    }

    fn duplicate_hosts_by_connection(&self) -> Vec<DuplicateHostGroup> {
        let mut by_connection: HashMap<String, Vec<DuplicateHostEntry>> = HashMap::new();
        for host in &self.hosts {
            let key = connection_duplicate_key(host);
            by_connection
                .entry(key)
                .or_default()
                .push(DuplicateHostEntry::from_host(host, self.host_path(host)));
        }

        duplicate_groups(by_connection, DuplicateHostKind::Connection)
    }

    fn find_one_host(&self, query: &str) -> Result<&Host, StasshError> {
        if let Ok(id) = Uuid::parse_str(query) {
            return self
                .host(id)
                .ok_or_else(|| StasshError::HostNotFound(query.to_string()));
        }

        let exact_path_matches: Vec<&Host> = self
            .hosts
            .iter()
            .filter(|host| self.host_path(host) == query)
            .collect();
        if exact_path_matches.len() == 1 {
            return Ok(exact_path_matches[0]);
        }

        let exact_name_matches: Vec<&Host> = self
            .hosts
            .iter()
            .filter(|host| host.display_name == query)
            .collect();
        if exact_name_matches.len() == 1 {
            return Ok(exact_name_matches[0]);
        }

        let matches = self.search_hosts(query);
        match matches.as_slice() {
            [host] => Ok(host),
            [] => Err(StasshError::HostNotFound(query.to_string())),
            many => Err(StasshError::AmbiguousHost {
                selector: query.to_string(),
                matches: many
                    .iter()
                    .map(|host| self.host_path(host))
                    .collect::<Vec<_>>()
                    .join(", "),
            }),
        }
    }

    fn host_search_text(&self, host: &Host) -> String {
        let mut parts = vec![
            self.host_path(host),
            host.hostname.clone(),
            host.username.clone().unwrap_or_default(),
            host.notes.clone().unwrap_or_default(),
        ];
        parts.extend(host.tags.clone());
        parts.join(" ")
    }

    fn is_descendant_folder(&self, candidate_id: Uuid, ancestor_id: Uuid) -> bool {
        let folders_by_id: HashMap<Uuid, &Folder> = self
            .folders
            .iter()
            .map(|folder| (folder.id, folder))
            .collect();
        let mut current_id = Some(candidate_id);

        while let Some(id) = current_id {
            if id == ancestor_id {
                return true;
            }
            current_id = folders_by_id.get(&id).and_then(|folder| folder.parent_id);
        }

        false
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Folder {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Host {
    pub id: Uuid,
    pub folder_id: Uuid,
    pub display_name: String,
    pub hostname: String,
    pub port: u16,
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secrets: Option<String>,
    pub jump_chain: Vec<Uuid>,
    pub ssh_options: Vec<String>,
    #[serde(default)]
    pub forwards: Vec<ForwardDefinition>,
    #[serde(default)]
    pub actions: Vec<ActionDefinition>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddFolder {
    pub parent_id: Option<Uuid>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddHost {
    pub folder_id: Option<Uuid>,
    pub display_name: String,
    pub hostname: String,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub identity_fingerprint: Option<String>,
    pub secrets: Option<String>,
    pub jump_chain: Vec<Uuid>,
    pub ssh_options: Vec<String>,
    pub forwards: Vec<ForwardDefinition>,
    pub tags: Vec<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdateHost {
    pub folder_id: Option<Uuid>,
    pub display_name: Option<String>,
    pub hostname: Option<String>,
    pub port: Option<u16>,
    pub username: Option<Option<String>>,
    pub identity_fingerprint: Option<Option<String>>,
    pub secrets: Option<Option<String>>,
    pub jump_chain: Option<Vec<Uuid>>,
    pub ssh_options: Option<Vec<String>>,
    pub forwards: Option<Vec<ForwardDefinition>>,
    pub actions: Option<Vec<ActionDefinition>>,
    pub tags: Option<Vec<String>>,
    pub notes: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ForwardDefinition {
    Local {
        bind_address: String,
        local_port: u16,
        destination_host: String,
        destination_port: u16,
    },
    Remote {
        bind_address: String,
        remote_port: u16,
        destination_host: String,
        destination_port: u16,
    },
    Dynamic {
        bind_address: String,
        local_port: u16,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionDefinition {
    pub id: Uuid,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_prepare: Option<ActionLocalCommand>,
    #[serde(default)]
    pub forwards: Vec<ActionForwardDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_launch: Option<ActionLocalCommand>,
    #[serde(default)]
    pub cleanup: Vec<ActionLocalCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionLocalCommand {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionForwardDefinition {
    Local {
        name: String,
        bind_address: String,
        local_port: ActionPort,
        destination_host: String,
        destination_port: u16,
    },
    Dynamic {
        name: String,
        bind_address: String,
        local_port: ActionPort,
    },
}

impl ActionForwardDefinition {
    pub fn name(&self) -> &str {
        match self {
            ActionForwardDefinition::Local { name, .. }
            | ActionForwardDefinition::Dynamic { name, .. } => name,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionPort {
    Auto,
    Fixed(u16),
    Env(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostSelector<'a> {
    Id(Uuid),
    Query(&'a str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedHost {
    pub id: Uuid,
    pub path: String,
    pub display_name: String,
    pub hostname: String,
    pub port: u16,
    pub username: Option<String>,
    pub identity_fingerprint: Option<String>,
    pub secrets: Option<String>,
    pub jump_chain: Vec<ResolvedJump>,
    pub ssh_options: Vec<String>,
    pub forwards: Vec<ForwardDefinition>,
    pub actions: Vec<ActionDefinition>,
    pub tags: Vec<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedJump {
    pub id: Uuid,
    pub display_name: String,
    pub hostname: String,
    pub port: u16,
    pub username: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalIdentityMapping {
    pub fingerprint: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DuplicateHostGroup {
    pub kind: DuplicateHostKind,
    pub key: String,
    pub hosts: Vec<DuplicateHostEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateHostKind {
    Path,
    Connection,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DuplicateHostEntry {
    pub id: Uuid,
    pub path: String,
    pub display_name: String,
    pub hostname: String,
    pub port: u16,
    pub username: Option<String>,
}

impl DuplicateHostEntry {
    fn from_host(host: &Host, path: String) -> Self {
        Self {
            id: host.id,
            path,
            display_name: host.display_name.clone(),
            hostname: host.hostname.clone(),
            port: host.port,
            username: host.username.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostDedupePlan {
    pub strategy: HostDedupeStrategy,
    pub groups: Vec<HostDedupeGroup>,
    pub remove_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HostDedupeStrategy {
    Path,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostDedupeGroup {
    pub path: String,
    pub keep: DuplicateHostEntry,
    pub remove: Vec<DuplicateHostEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostDedupeResult {
    pub strategy: HostDedupeStrategy,
    pub removed: Vec<DuplicateHostEntry>,
    pub removed_count: usize,
    pub rewritten_jump_references: usize,
}

fn duplicate_groups(
    grouped: HashMap<String, Vec<DuplicateHostEntry>>,
    kind: DuplicateHostKind,
) -> Vec<DuplicateHostGroup> {
    let mut groups = grouped
        .into_iter()
        .filter_map(|(key, mut hosts)| {
            if hosts.len() < 2 {
                return None;
            }
            hosts.sort_by(|left, right| left.path.cmp(&right.path).then(left.id.cmp(&right.id)));
            Some(DuplicateHostGroup {
                kind: kind.clone(),
                key,
                hosts,
            })
        })
        .collect::<Vec<_>>();
    groups.sort_by(|left, right| {
        left.key
            .cmp(&right.key)
            .then_with(|| left.hosts[0].path.cmp(&right.hosts[0].path))
    });
    groups
}

fn connection_duplicate_key(host: &Host) -> String {
    let identity = host.identity_fingerprint.as_deref().unwrap_or("");
    format!(
        "hostname={};port={};username={};identity={};jumps={};ssh_options={};forwards={}",
        host.hostname.to_ascii_lowercase(),
        host.port,
        host.username.as_deref().unwrap_or(""),
        identity,
        host.jump_chain
            .iter()
            .map(Uuid::to_string)
            .collect::<Vec<_>>()
            .join(","),
        host.ssh_options.join("\u{1f}"),
        serde_json::to_string(&host.forwards).unwrap_or_else(|_| "[]".to_string())
    )
}

fn validate_name(field: &'static str, value: &str) -> Result<(), StasshError> {
    if value.trim().is_empty() {
        return Err(StasshError::InvalidValue {
            field,
            reason: "must not be empty".to_string(),
        });
    }

    if value.contains('/') {
        return Err(StasshError::InvalidValue {
            field,
            reason: "must not contain '/'".to_string(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_serializes_identity_as_fingerprint_field() {
        let host = Host {
            id: Uuid::new_v4(),
            folder_id: Uuid::new_v4(),
            display_name: "web".to_string(),
            hostname: "web.example".to_string(),
            port: 22,
            username: None,
            identity_fingerprint: Some("SHA256:abc".to_string()),
            secrets: None,
            jump_chain: Vec::new(),
            ssh_options: Vec::new(),
            forwards: Vec::new(),
            actions: Vec::new(),
            tags: Vec::new(),
            notes: None,
        };

        let value = serde_json::to_value(&host).unwrap();

        assert_eq!(value["identity_fingerprint"], "SHA256:abc");
        assert!(value.get("identity").is_none());
    }

    #[test]
    fn host_deserializes_identity_fingerprint_field() {
        let value = serde_json::json!({
            "id": Uuid::new_v4(),
            "folder_id": Uuid::new_v4(),
            "display_name": "web",
            "hostname": "web.example",
            "port": 22,
            "username": null,
            "identity_fingerprint": "SHA256:abc",
            "jump_chain": [],
            "ssh_options": [],
            "tags": [],
            "notes": null
        });

        let host: Host = serde_json::from_value(value).unwrap();

        assert_eq!(host.identity_fingerprint.as_deref(), Some("SHA256:abc"));
    }

    #[test]
    fn vault_deserializes_without_actions_field() {
        let root_id = Uuid::new_v4();
        let value = serde_json::json!({
            "format_version": CURRENT_FORMAT_VERSION,
            "folders": [{
                "id": root_id,
                "parent_id": null,
                "name": "Root"
            }],
            "hosts": []
        });

        let vault: Vault = serde_json::from_value(value).unwrap();

        assert!(vault.actions.is_empty());
    }

    #[test]
    fn resolved_host_includes_common_and_host_actions() {
        let mut vault = Vault::new();
        let common = ActionDefinition {
            id: Uuid::new_v4(),
            name: "Desktop".to_string(),
            local_prepare: None,
            forwards: Vec::new(),
            remote_command: None,
            local_launch: None,
            cleanup: Vec::new(),
        };
        let local = ActionDefinition {
            id: Uuid::new_v4(),
            name: "Host console".to_string(),
            local_prepare: None,
            forwards: Vec::new(),
            remote_command: None,
            local_launch: None,
            cleanup: Vec::new(),
        };
        vault.actions.push(common.clone());
        let host = vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "web".to_string(),
                hostname: "web.example".to_string(),
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
        vault
            .hosts
            .iter_mut()
            .find(|stored| stored.id == host.id)
            .unwrap()
            .actions = vec![local.clone()];

        let resolved = vault.resolve_host(HostSelector::Id(host.id)).unwrap();

        assert_eq!(resolved.actions, vec![common, local]);
    }

    #[test]
    fn host_id_survives_rename_and_move() {
        let mut vault = Vault::new();
        let folder = vault
            .add_folder(AddFolder {
                parent_id: None,
                name: "Customers".to_string(),
            })
            .unwrap();
        let host = vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "router".to_string(),
                hostname: "router.example".to_string(),
                port: Some(22),
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

        let stored = vault
            .hosts
            .iter_mut()
            .find(|stored| stored.id == host.id)
            .unwrap();
        stored.display_name = "edge-router".to_string();
        stored.folder_id = folder.id;

        let resolved = vault.resolve_host(HostSelector::Id(host.id)).unwrap();
        assert_eq!(resolved.id, host.id);
        assert_eq!(resolved.path, "Customers/edge-router");
    }

    #[test]
    fn search_matches_path_hostname_user_and_tags() {
        let mut vault = Vault::new();
        vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "db-01".to_string(),
                hostname: "10.0.0.5".to_string(),
                port: None,
                username: Some("postgres".to_string()),
                identity_fingerprint: None,
                secrets: None,
                jump_chain: Vec::new(),
                ssh_options: Vec::new(),
                forwards: Vec::new(),
                tags: vec!["production".to_string()],
                notes: None,
            })
            .unwrap();

        assert_eq!(vault.search_hosts("db production").len(), 1);
        assert_eq!(vault.search_hosts("postgres 10.0").len(), 1);
    }

    #[test]
    fn mutation_methods_update_without_replacing_host_id() {
        let mut vault = Vault::new();
        let folder = vault
            .add_folder(AddFolder {
                parent_id: None,
                name: "Lab".to_string(),
            })
            .unwrap();
        let host = vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "pi".to_string(),
                hostname: "pi.local".to_string(),
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

        let updated = vault
            .update_host(
                HostSelector::Id(host.id),
                UpdateHost {
                    folder_id: Some(folder.id),
                    display_name: Some("rescue-pi".to_string()),
                    port: Some(2222),
                    username: Some(Some("root".to_string())),
                    ..UpdateHost::default()
                },
            )
            .unwrap();

        assert_eq!(updated.id, host.id);
        assert_eq!(vault.host_path(&updated), "Lab/rescue-pi");
        assert_eq!(updated.port, 2222);
        assert_eq!(updated.username.as_deref(), Some("root"));
    }

    #[test]
    fn deleting_host_removes_it_from_jump_chains() {
        let mut vault = Vault::new();
        let jump = vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "bastion".to_string(),
                hostname: "bastion.example".to_string(),
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
        let target = vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "db".to_string(),
                hostname: "10.0.0.5".to_string(),
                port: None,
                username: None,
                identity_fingerprint: None,
                secrets: None,
                jump_chain: vec![jump.id],
                ssh_options: Vec::new(),
                forwards: Vec::new(),
                tags: Vec::new(),
                notes: None,
            })
            .unwrap();

        vault.delete_host(HostSelector::Id(jump.id)).unwrap();
        let resolved = vault.resolve_host(HostSelector::Id(target.id)).unwrap();

        assert!(resolved.jump_chain.is_empty());
    }

    #[test]
    fn folder_delete_requires_empty_non_root_folder() {
        let mut vault = Vault::new();
        let folder = vault
            .add_folder(AddFolder {
                parent_id: None,
                name: "Customers".to_string(),
            })
            .unwrap();
        vault
            .add_host(AddHost {
                folder_id: Some(folder.id),
                display_name: "web".to_string(),
                hostname: "web.example".to_string(),
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

        assert!(matches!(
            vault.delete_folder(folder.id),
            Err(StasshError::FolderNotEmpty(_))
        ));
        assert!(matches!(
            vault.delete_folder(vault.root_folder_id()),
            Err(StasshError::CannotDeleteRootFolder)
        ));
    }

    #[test]
    fn folder_move_rejects_descendant_parent() {
        let mut vault = Vault::new();
        let parent = vault
            .add_folder(AddFolder {
                parent_id: None,
                name: "Parent".to_string(),
            })
            .unwrap();
        let child = vault
            .add_folder(AddFolder {
                parent_id: Some(parent.id),
                name: "Child".to_string(),
            })
            .unwrap();

        assert!(matches!(
            vault.move_folder(parent.id, child.id),
            Err(StasshError::InvalidValue { .. })
        ));
    }

    #[test]
    fn jump_resolution_reports_missing_jump_host() {
        let mut vault = Vault::new();
        let missing = Uuid::new_v4();
        let result = vault.add_host(AddHost {
            folder_id: None,
            display_name: "db".to_string(),
            hostname: "10.0.0.5".to_string(),
            port: None,
            username: None,
            identity_fingerprint: None,
            secrets: None,
            jump_chain: vec![missing],
            ssh_options: Vec::new(),
            forwards: Vec::new(),
            tags: Vec::new(),
            notes: None,
        });

        assert!(matches!(result, Err(StasshError::HostNotFound(_))));
    }

    #[test]
    fn detects_duplicate_host_paths() {
        let mut vault = Vault::new();
        vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "web".to_string(),
                hostname: "web-a.example".to_string(),
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
        vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "web".to_string(),
                hostname: "web-b.example".to_string(),
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

        let groups = vault.duplicate_hosts();

        assert!(groups.iter().any(|group| {
            group.kind == DuplicateHostKind::Path && group.key == "web" && group.hosts.len() == 2
        }));
    }

    #[test]
    fn detects_duplicate_host_connections() {
        let mut vault = Vault::new();
        vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "web-a".to_string(),
                hostname: "WEB.example".to_string(),
                port: Some(2222),
                username: Some("deploy".to_string()),
                identity_fingerprint: None,
                secrets: None,
                jump_chain: Vec::new(),
                ssh_options: vec!["ServerAliveInterval 30".to_string()],
                forwards: Vec::new(),
                tags: Vec::new(),
                notes: None,
            })
            .unwrap();
        vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "web-b".to_string(),
                hostname: "web.example".to_string(),
                port: Some(2222),
                username: Some("deploy".to_string()),
                identity_fingerprint: None,
                secrets: None,
                jump_chain: Vec::new(),
                ssh_options: vec!["ServerAliveInterval 30".to_string()],
                forwards: Vec::new(),
                tags: Vec::new(),
                notes: None,
            })
            .unwrap();
        vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "web-c".to_string(),
                hostname: "web.example".to_string(),
                port: Some(2222),
                username: Some("deploy".to_string()),
                identity_fingerprint: None,
                secrets: None,
                jump_chain: Vec::new(),
                ssh_options: vec!["ForwardAgent yes".to_string()],
                forwards: Vec::new(),
                tags: Vec::new(),
                notes: None,
            })
            .unwrap();

        let groups = vault.duplicate_hosts();
        let connection_group = groups
            .iter()
            .find(|group| group.kind == DuplicateHostKind::Connection)
            .unwrap();

        assert_eq!(connection_group.hosts.len(), 2);
        assert_eq!(
            connection_group
                .hosts
                .iter()
                .map(|host| host.path.as_str())
                .collect::<Vec<_>>(),
            vec!["web-a", "web-b"]
        );
    }

    #[test]
    fn plans_host_dedupe_for_path_duplicates_only() {
        let mut vault = Vault::new();
        let first = vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "web".to_string(),
                hostname: "web-a.example".to_string(),
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
        let second = vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "web".to_string(),
                hostname: "web-b.example".to_string(),
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
        vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "alias".to_string(),
                hostname: "web-a.example".to_string(),
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

        let plan = vault.host_dedupe_plan();

        assert_eq!(plan.strategy, HostDedupeStrategy::Path);
        assert_eq!(plan.remove_count, 1);
        assert_eq!(plan.groups.len(), 1);
        assert_eq!(plan.groups[0].path, "web");
        assert_eq!(plan.groups[0].keep.id, first.id);
        assert_eq!(plan.groups[0].remove[0].id, second.id);
    }

    #[test]
    fn applies_host_dedupe_and_rewrites_jump_references() {
        let mut vault = Vault::new();
        let first = vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "bastion".to_string(),
                hostname: "bastion-a.example".to_string(),
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
        let second = vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "bastion".to_string(),
                hostname: "bastion-b.example".to_string(),
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
        let target = vault
            .add_host(AddHost {
                folder_id: None,
                display_name: "db".to_string(),
                hostname: "db.example".to_string(),
                port: None,
                username: None,
                identity_fingerprint: None,
                secrets: None,
                jump_chain: vec![second.id],
                ssh_options: Vec::new(),
                forwards: Vec::new(),
                tags: Vec::new(),
                notes: None,
            })
            .unwrap();
        let plan = vault.host_dedupe_plan();

        let result = vault.apply_host_dedupe_plan(&plan);
        let resolved = vault.resolve_host(HostSelector::Id(target.id)).unwrap();

        assert_eq!(result.removed_count, 1);
        assert_eq!(result.removed[0].id, second.id);
        assert_eq!(result.rewritten_jump_references, 1);
        assert!(vault.host(second.id).is_none());
        assert_eq!(resolved.jump_chain[0].id, first.id);
    }
}
