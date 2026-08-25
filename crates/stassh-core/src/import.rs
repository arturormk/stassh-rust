use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::identity::{DerivedIdentity, IdentityFileResolver};
use crate::local::LocalConfig;
use crate::model::{AddHost, ForwardDefinition, HostSelector, StasshError, UpdateHost, Vault};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportOpenSshSummary {
    pub imported: Vec<String>,
    pub skipped: Vec<String>,
    pub warnings: Vec<String>,
}

impl ImportOpenSshSummary {
    fn new() -> Self {
        Self {
            imported: Vec::new(),
            skipped: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct OpenSshHostBlock {
    aliases: Vec<String>,
    hostname: Option<String>,
    user: Option<String>,
    port: Option<u16>,
    proxy_jump: Option<String>,
    identity_files: Vec<String>,
    forwards: Vec<ForwardDefinition>,
    unsupported_options: Vec<String>,
}

pub fn import_openssh_config(
    vault: &mut Vault,
    contents: &str,
) -> Result<ImportOpenSshSummary, StasshError> {
    import_openssh_config_inner(vault, contents, None)
}

pub struct IdentityImportContext<'a> {
    pub local_config: &'a mut LocalConfig,
    pub config_path: &'a Path,
    pub home_dir: Option<&'a Path>,
    pub resolver: &'a dyn IdentityFileResolver,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSshConfigRead {
    pub contents: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum OpenSshConfigReadError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
}

pub fn read_openssh_config_with_includes(
    path: &Path,
    home_dir: Option<&Path>,
) -> Result<OpenSshConfigRead, OpenSshConfigReadError> {
    let mut warnings = Vec::new();
    let mut stack = Vec::new();
    let contents =
        read_openssh_config_with_includes_inner(path, home_dir, &mut warnings, &mut stack)?;
    Ok(OpenSshConfigRead { contents, warnings })
}

pub fn import_openssh_config_with_identities(
    vault: &mut Vault,
    contents: &str,
    identity_context: IdentityImportContext<'_>,
) -> Result<ImportOpenSshSummary, StasshError> {
    import_openssh_config_inner(vault, contents, Some(identity_context))
}

fn import_openssh_config_inner(
    vault: &mut Vault,
    contents: &str,
    mut identity_context: Option<IdentityImportContext<'_>>,
) -> Result<ImportOpenSshSummary, StasshError> {
    let blocks = parse_blocks(contents);
    let mut summary = ImportOpenSshSummary::new();
    let mut imported_alias_ids = HashMap::new();
    let mut identity_cache = HashMap::new();
    let mut pending_jumps = Vec::new();

    for block in &blocks {
        for alias in block
            .aliases
            .iter()
            .filter(|alias| is_concrete_alias(alias))
        {
            if vault
                .search_hosts(alias)
                .iter()
                .any(|host| host.display_name == *alias || vault.host_path(host) == alias.as_str())
            {
                summary
                    .skipped
                    .push(format!("{alias}: host already exists"));
                continue;
            }

            let effective_block = effective_host_block(alias, &blocks);
            let mut ssh_options = effective_block.unsupported_options.clone();
            let identity = import_identity_files(
                alias,
                &effective_block.identity_files,
                &mut ssh_options,
                &mut summary,
                identity_context.as_mut(),
                &mut identity_cache,
            );

            let host = vault.add_host(AddHost {
                folder_id: None,
                display_name: alias.clone(),
                hostname: effective_block.hostname.unwrap_or_else(|| alias.clone()),
                port: effective_block.port,
                username: effective_block.user,
                identity_fingerprint: identity,
                jump_chain: Vec::new(),
                ssh_options,
                forwards: effective_block.forwards,
                tags: vec!["imported:openssh".to_string()],
                notes: None,
            })?;
            summary.imported.push(alias.clone());
            imported_alias_ids.insert(alias.clone(), host.id);

            if let Some(proxy_jump) = effective_block.proxy_jump {
                pending_jumps.push((alias.clone(), proxy_jump));
            }
        }

        for alias in block
            .aliases
            .iter()
            .filter(|alias| !is_concrete_alias(alias) && alias.as_str() != "*")
        {
            summary
                .skipped
                .push(format!("{alias}: wildcard or negated Host pattern"));
        }
    }

    for (alias, proxy_jump) in pending_jumps {
        let mut jump_chain = Vec::new();
        let mut unresolved = Vec::new();

        for jump_alias in parse_proxy_jump_aliases(&proxy_jump) {
            if let Some(id) = imported_alias_ids.get(&jump_alias).copied() {
                jump_chain.push(id);
                continue;
            }

            match vault.resolve_host(HostSelector::Query(&jump_alias)) {
                Ok(host) => jump_chain.push(host.id),
                Err(_) => unresolved.push(jump_alias),
            }
        }

        if !jump_chain.is_empty() {
            vault.update_host(
                HostSelector::Query(&alias),
                UpdateHost {
                    jump_chain: Some(jump_chain),
                    ..UpdateHost::default()
                },
            )?;
        }

        for jump_alias in unresolved {
            summary.warnings.push(format!(
                "{alias}: could not resolve ProxyJump target {jump_alias}"
            ));
        }
    }

    Ok(summary)
}

fn read_openssh_config_with_includes_inner(
    path: &Path,
    home_dir: Option<&Path>,
    warnings: &mut Vec<String>,
    stack: &mut Vec<PathBuf>,
) -> Result<String, OpenSshConfigReadError> {
    let canonical_path = canonical_config_path(path);
    if stack.contains(&canonical_path) {
        warnings.push(format!(
            "{}: skipped recursive Include cycle",
            path.display()
        ));
        return Ok(String::new());
    }

    let contents = fs::read_to_string(path).map_err(|source| OpenSshConfigReadError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    stack.push(canonical_path);

    let mut expanded = String::new();
    for raw_line in contents.lines() {
        let line = strip_comment(raw_line).trim();
        let include_value = split_keyword_value(line)
            .filter(|(keyword, _)| keyword.eq_ignore_ascii_case("Include"))
            .map(|(_, value)| value);

        let Some(include_value) = include_value else {
            expanded.push_str(raw_line);
            expanded.push('\n');
            continue;
        };

        let patterns = split_ssh_words(include_value);
        if patterns.is_empty() {
            warnings.push(format!("{}: ignored empty Include", path.display()));
            continue;
        }

        for pattern in patterns {
            let Some(resolved_pattern) = resolve_include_pattern(&pattern, path, home_dir) else {
                warnings.push(format!(
                    "{}: skipped Include {pattern}; OpenSSH tokens are not resolved yet",
                    path.display()
                ));
                continue;
            };

            match expand_include_pattern(&resolved_pattern) {
                Ok(matches) if matches.is_empty() => {
                    warnings.push(format!(
                        "{}: Include {pattern} matched no files",
                        path.display()
                    ));
                }
                Ok(matches) => {
                    for include_path in matches {
                        match read_openssh_config_with_includes_inner(
                            &include_path,
                            home_dir,
                            warnings,
                            stack,
                        ) {
                            Ok(included) => {
                                expanded.push_str(&included);
                                if !included.ends_with('\n') {
                                    expanded.push('\n');
                                }
                            }
                            Err(error) => warnings.push(error.to_string()),
                        }
                    }
                }
                Err(error) => warnings.push(format!(
                    "{}: failed to expand Include {pattern}: {error}",
                    path.display()
                )),
            }
        }
    }

    stack.pop();
    Ok(expanded)
}

fn import_identity_files(
    alias: &str,
    identity_files: &[String],
    ssh_options: &mut Vec<String>,
    summary: &mut ImportOpenSshSummary,
    mut identity_context: Option<&mut IdentityImportContext<'_>>,
    identity_cache: &mut HashMap<PathBuf, Result<DerivedIdentity, String>>,
) -> Option<String> {
    let mut identity = None;

    for identity_file in identity_files {
        if identity.is_some() {
            ssh_options.push(format!("IdentityFile {identity_file}"));
            summary.warnings.push(format!(
                "{alias}: preserved additional IdentityFile {identity_file}; only one portable identity is supported per host right now"
            ));
            continue;
        }

        let Some(context) = identity_context.as_deref_mut() else {
            ssh_options.push(format!("IdentityFile {identity_file}"));
            summary.warnings.push(format!(
                "{alias}: imported IdentityFile as raw SSH option; identity mapping context was not provided"
            ));
            continue;
        };

        let Some(resolved_path) =
            resolve_identity_file_path(identity_file, context.config_path, context.home_dir)
        else {
            ssh_options.push(format!("IdentityFile {identity_file}"));
            summary.warnings.push(format!(
                "{alias}: preserved IdentityFile {identity_file}; OpenSSH tokens or unsupported path form are not resolved yet"
            ));
            continue;
        };

        if !resolved_path.exists() {
            ssh_options.push(format!("IdentityFile {identity_file}"));
            summary.warnings.push(format!(
                "{alias}: preserved IdentityFile {identity_file}; resolved path does not exist: {}",
                resolved_path.display()
            ));
            continue;
        }

        let cache_key = canonical_identity_path(&resolved_path);
        let derived_result = identity_cache
            .entry(cache_key.clone())
            .or_insert_with(|| {
                context
                    .resolver
                    .derive_identity(&cache_key, preferred_name_for_path(&cache_key))
                    .map_err(|error| error.to_string())
            })
            .clone();

        match derived_result {
            Ok(derived) => {
                if let Err(error) = context.local_config.map_identity(
                    derived.fingerprint.clone(),
                    cache_key,
                    derived.preferred_name.clone(),
                ) {
                    ssh_options.push(format!("IdentityFile {identity_file}"));
                    summary.warnings.push(format!(
                        "{alias}: could not store identity mapping for {identity_file}: {error}; preserved raw IdentityFile"
                    ));
                    continue;
                }
                summary.warnings.push(format!(
                    "{alias}: mapped IdentityFile {identity_file} to {}",
                    derived.fingerprint
                ));
                identity = Some(derived.fingerprint);
            }
            Err(error) => {
                ssh_options.push(format!("IdentityFile {identity_file}"));
                summary.warnings.push(format!(
                    "{alias}: could not derive fingerprint from IdentityFile {identity_file}: {error}; preserved raw IdentityFile"
                ));
            }
        }
    }

    identity
}

fn effective_host_block(alias: &str, blocks: &[OpenSshHostBlock]) -> OpenSshHostBlock {
    let mut effective = OpenSshHostBlock {
        aliases: vec![alias.to_string()],
        ..OpenSshHostBlock::default()
    };

    for block in blocks
        .iter()
        .filter(|block| host_block_matches(block, alias))
    {
        merge_first_value_wins(&mut effective, block);
    }

    effective
}

fn merge_first_value_wins(effective: &mut OpenSshHostBlock, block: &OpenSshHostBlock) {
    if effective.hostname.is_none() {
        effective.hostname = block.hostname.clone();
    }
    if effective.user.is_none() {
        effective.user = block.user.clone();
    }
    if effective.port.is_none() {
        effective.port = block.port;
    }
    if effective.proxy_jump.is_none() {
        effective.proxy_jump = block.proxy_jump.clone();
    }

    effective
        .identity_files
        .extend(block.identity_files.iter().cloned());
    effective.forwards.extend(block.forwards.iter().cloned());

    for option in &block.unsupported_options {
        let keyword = raw_option_keyword(option);
        if !effective
            .unsupported_options
            .iter()
            .any(|existing| raw_option_keyword(existing).eq_ignore_ascii_case(keyword))
        {
            effective.unsupported_options.push(option.clone());
        }
    }
}

fn host_block_matches(block: &OpenSshHostBlock, alias: &str) -> bool {
    let mut matched = false;

    for pattern in &block.aliases {
        if let Some(negated) = pattern.strip_prefix('!') {
            if host_pattern_matches(negated, alias) {
                return false;
            }
            continue;
        }

        if host_pattern_matches(pattern, alias) {
            matched = true;
        }
    }

    matched
}

fn host_pattern_matches(pattern: &str, alias: &str) -> bool {
    if pattern.contains('[') || pattern.contains(']') {
        return false;
    }
    if pattern.contains('*') || pattern.contains('?') {
        return glob_component_matches(pattern, alias);
    }
    pattern == alias
}

fn raw_option_keyword(option: &str) -> &str {
    split_keyword_value(option)
        .map(|(keyword, _)| keyword)
        .unwrap_or(option)
}

fn resolve_identity_file_path(
    identity_file: &str,
    config_path: &Path,
    home_dir: Option<&Path>,
) -> Option<PathBuf> {
    if identity_file.contains('%') {
        return None;
    }

    let path = unquote(identity_file);
    if path == "~" {
        return home_dir.map(Path::to_path_buf);
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return home_dir.map(|home| home.join(rest));
    }

    let path = PathBuf::from(path);
    if path.is_absolute() {
        Some(path)
    } else {
        let parent = config_path.parent().unwrap_or_else(|| Path::new("."));
        Some(parent.join(path))
    }
}

fn resolve_include_pattern(
    include_pattern: &str,
    config_path: &Path,
    home_dir: Option<&Path>,
) -> Option<PathBuf> {
    if include_pattern.contains('%') {
        return None;
    }

    let path = unquote(include_pattern);
    if path == "~" {
        return home_dir.map(Path::to_path_buf);
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return home_dir.map(|home| home.join(rest));
    }

    let path = PathBuf::from(path);
    if path.is_absolute() {
        Some(path)
    } else {
        let parent = config_path.parent().unwrap_or_else(|| Path::new("."));
        Some(parent.join(path))
    }
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value)
}

fn preferred_name_for_path(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(ToString::to_string)
}

fn canonical_identity_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn canonical_config_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn expand_include_pattern(pattern: &Path) -> std::io::Result<Vec<PathBuf>> {
    if !path_has_glob(pattern) {
        return Ok(vec![pattern.to_path_buf()]);
    }

    let mut matches = Vec::new();
    expand_glob_components(PathBuf::new(), pattern, &mut matches)?;
    matches.sort();
    Ok(matches)
}

fn path_has_glob(path: &Path) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|value| value.contains('*') || value.contains('?'))
    })
}

fn expand_glob_components(
    prefix: PathBuf,
    pattern: &Path,
    matches: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    let components = pattern.components().collect::<Vec<_>>();
    expand_glob_component_slice(prefix, &components, matches)
}

fn expand_glob_component_slice(
    prefix: PathBuf,
    components: &[std::path::Component<'_>],
    matches: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    let Some((component, rest)) = components.split_first() else {
        if prefix.is_file() {
            matches.push(prefix);
        }
        return Ok(());
    };

    let value = component.as_os_str().to_string_lossy();
    if value.contains('*') || value.contains('?') {
        let search_dir = if prefix.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            prefix.clone()
        };

        let mut entries = match fs::read_dir(&search_dir) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .collect::<Vec<_>>(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        entries.sort();

        for entry in entries {
            let Some(file_name) = entry.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if glob_component_matches(&value, file_name) {
                expand_glob_component_slice(entry, rest, matches)?;
            }
        }
        return Ok(());
    }

    let next = prefix.join(component.as_os_str());
    expand_glob_component_slice(next, rest, matches)
}

fn glob_component_matches(pattern: &str, value: &str) -> bool {
    glob_component_matches_inner(
        pattern.chars().collect::<Vec<_>>().as_slice(),
        value.chars().collect::<Vec<_>>().as_slice(),
    )
}

fn glob_component_matches_inner(pattern: &[char], value: &[char]) -> bool {
    match pattern.split_first() {
        None => value.is_empty(),
        Some(('*', rest)) => {
            glob_component_matches_inner(rest, value)
                || (!value.is_empty() && glob_component_matches_inner(pattern, &value[1..]))
        }
        Some(('?', rest)) => !value.is_empty() && glob_component_matches_inner(rest, &value[1..]),
        Some((expected, rest)) => value.split_first().is_some_and(|(actual, value_rest)| {
            expected == actual && glob_component_matches_inner(rest, value_rest)
        }),
    }
}

fn split_ssh_words(value: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote = None;

    for character in value.chars() {
        match (quote, character) {
            (Some(active), ch) if ch == active => quote = None,
            (Some(_), ch) => current.push(ch),
            (None, '"' | '\'') => quote = Some(character),
            (None, ch) if ch.is_whitespace() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            (None, ch) => current.push(ch),
        }
    }

    if !current.is_empty() {
        words.push(current);
    }

    words
}

fn parse_blocks(contents: &str) -> Vec<OpenSshHostBlock> {
    let mut blocks = Vec::new();
    let mut current: Option<OpenSshHostBlock> = None;
    let mut in_match_block = false;

    for raw_line in contents.lines() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }

        let Some((keyword, value)) = split_keyword_value(line) else {
            if let Some(block) = &mut current {
                block.unsupported_options.push(line.to_string());
            }
            continue;
        };

        if keyword.eq_ignore_ascii_case("Match") {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            in_match_block = true;
            continue;
        }

        if keyword.eq_ignore_ascii_case("Host") {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            in_match_block = false;
            current = Some(OpenSshHostBlock {
                aliases: value.split_whitespace().map(ToString::to_string).collect(),
                ..OpenSshHostBlock::default()
            });
            continue;
        }

        if in_match_block {
            continue;
        }

        let Some(block) = &mut current else {
            continue;
        };

        match keyword.to_ascii_lowercase().as_str() {
            "hostname" => block.hostname = Some(value.to_string()),
            "user" => block.user = Some(value.to_string()),
            "port" => match value.parse::<u16>() {
                Ok(port) => block.port = Some(port),
                Err(_) => block.unsupported_options.push(format!("{keyword} {value}")),
            },
            "proxyjump" => block.proxy_jump = Some(value.to_string()),
            "identityfile" => block.identity_files.push(value.to_string()),
            "localforward" => match parse_local_forward(value) {
                Some(forward) => block.forwards.push(forward),
                None => block.unsupported_options.push(format!("{keyword} {value}")),
            },
            "remoteforward" => match parse_remote_forward(value) {
                Some(forward) => block.forwards.push(forward),
                None => block.unsupported_options.push(format!("{keyword} {value}")),
            },
            "dynamicforward" => match parse_dynamic_forward(value) {
                Some(forward) => block.forwards.push(forward),
                None => block.unsupported_options.push(format!("{keyword} {value}")),
            },
            _ => block.unsupported_options.push(format!("{keyword} {value}")),
        }
    }

    if let Some(block) = current {
        blocks.push(block);
    }

    blocks
}

fn strip_comment(line: &str) -> &str {
    line.split_once('#')
        .map(|(before, _)| before)
        .unwrap_or(line)
}

fn split_keyword_value(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim();
    if let Some((keyword, value)) = trimmed.split_once(char::is_whitespace) {
        return Some((keyword, value.trim()));
    }
    trimmed.split_once('=').map(|(keyword, value)| {
        let keyword = keyword.trim();
        let value = value.trim();
        (keyword, value)
    })
}

fn is_concrete_alias(alias: &str) -> bool {
    !alias.starts_with('!')
        && !alias.contains('*')
        && !alias.contains('?')
        && !alias.contains('[')
        && !alias.contains(']')
}

fn parse_proxy_jump_aliases(value: &str) -> Vec<String> {
    if value.eq_ignore_ascii_case("none") {
        return Vec::new();
    }

    value
        .split(',')
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() {
                return None;
            }
            Some(proxy_jump_host_part(part).to_string())
        })
        .collect()
}

fn proxy_jump_host_part(value: &str) -> &str {
    let without_user = value
        .rsplit_once('@')
        .map(|(_, host)| host)
        .unwrap_or(value);
    without_user
        .split_once(':')
        .map(|(host, _)| host)
        .unwrap_or(without_user)
}

fn parse_local_forward(value: &str) -> Option<ForwardDefinition> {
    parse_tcp_forward(value).map(
        |(bind_address, listen_port, destination_host, destination_port)| {
            ForwardDefinition::Local {
                bind_address,
                local_port: listen_port,
                destination_host,
                destination_port,
            }
        },
    )
}

fn parse_remote_forward(value: &str) -> Option<ForwardDefinition> {
    parse_tcp_forward(value).map(
        |(bind_address, listen_port, destination_host, destination_port)| {
            ForwardDefinition::Remote {
                bind_address,
                remote_port: listen_port,
                destination_host,
                destination_port,
            }
        },
    )
}

fn parse_dynamic_forward(value: &str) -> Option<ForwardDefinition> {
    let value = value.trim();
    if let Ok(local_port) = value.parse::<u16>() {
        return Some(ForwardDefinition::Dynamic {
            bind_address: "127.0.0.1".to_string(),
            local_port,
        });
    }

    let parts = value.split(':').collect::<Vec<_>>();
    match parts.as_slice() {
        [bind_address, local_port] => Some(ForwardDefinition::Dynamic {
            bind_address: (*bind_address).to_string(),
            local_port: local_port.parse().ok()?,
        }),
        _ => None,
    }
}

fn parse_tcp_forward(value: &str) -> Option<(String, u16, String, u16)> {
    let fields = value.split_whitespace().collect::<Vec<_>>();
    match fields.as_slice() {
        [listen, destination] => {
            let (bind_address, listen_port) = parse_listen_address(listen)?;
            let (destination_host, destination_port) = parse_host_port(destination)?;
            Some((
                bind_address,
                listen_port,
                destination_host,
                destination_port,
            ))
        }
        [single] => {
            let parts = single.split(':').collect::<Vec<_>>();
            match parts.as_slice() {
                [
                    bind_address,
                    listen_port,
                    destination_host,
                    destination_port,
                ] => Some((
                    (*bind_address).to_string(),
                    listen_port.parse().ok()?,
                    (*destination_host).to_string(),
                    destination_port.parse().ok()?,
                )),
                [listen_port, destination_host, destination_port] => Some((
                    "127.0.0.1".to_string(),
                    listen_port.parse().ok()?,
                    (*destination_host).to_string(),
                    destination_port.parse().ok()?,
                )),
                _ => None,
            }
        }
        _ => None,
    }
}

fn parse_listen_address(value: &str) -> Option<(String, u16)> {
    if let Ok(port) = value.parse::<u16>() {
        return Some(("127.0.0.1".to_string(), port));
    }

    let (bind_address, port) = value.rsplit_once(':')?;
    Some((bind_address.to_string(), port.parse().ok()?))
}

fn parse_host_port(value: &str) -> Option<(String, u16)> {
    let (host, port) = value.rsplit_once(':')?;
    Some((host.to_string(), port.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::Path;

    use crate::identity::{DerivedIdentity, IdentityDeriveError};

    use super::*;

    struct FakeResolver;

    impl IdentityFileResolver for FakeResolver {
        fn derive_identity(
            &self,
            path: &Path,
            preferred_name: Option<String>,
        ) -> Result<DerivedIdentity, IdentityDeriveError> {
            Ok(DerivedIdentity {
                fingerprint: format!("SHA256:{}", path.file_stem().unwrap().to_string_lossy()),
                preferred_name,
            })
        }
    }

    struct CountingResolver {
        calls: Cell<usize>,
        fail: bool,
    }

    impl CountingResolver {
        fn succeeds() -> Self {
            Self {
                calls: Cell::new(0),
                fail: false,
            }
        }

        fn fails() -> Self {
            Self {
                calls: Cell::new(0),
                fail: true,
            }
        }
    }

    impl IdentityFileResolver for CountingResolver {
        fn derive_identity(
            &self,
            path: &Path,
            preferred_name: Option<String>,
        ) -> Result<DerivedIdentity, IdentityDeriveError> {
            self.calls.set(self.calls.get() + 1);
            if self.fail {
                return Err(IdentityDeriveError::MissingFingerprint);
            }

            Ok(DerivedIdentity {
                fingerprint: format!("SHA256:{}", path.file_stem().unwrap().to_string_lossy()),
                preferred_name,
            })
        }
    }

    #[test]
    fn imports_basic_concrete_hosts() {
        let mut vault = Vault::new();

        let summary = import_openssh_config(
            &mut vault,
            r#"
Host *
    ServerAliveInterval 30

Host bastion
    HostName bastion.example.com
    User admin
    Port 2222

Host db
    HostName 10.0.0.5
    User root
    ProxyJump bastion
    IdentityFile ~/.ssh/acme
"#,
        )
        .unwrap();

        assert_eq!(summary.imported, vec!["bastion", "db"]);
        assert!(summary.skipped.is_empty());
        assert_eq!(vault.hosts.len(), 2);

        let bastion = vault.resolve_host(HostSelector::Query("bastion")).unwrap();
        assert_eq!(bastion.hostname, "bastion.example.com");
        assert_eq!(bastion.port, 2222);
        assert_eq!(bastion.username.as_deref(), Some("admin"));
        assert_eq!(bastion.ssh_options, vec!["ServerAliveInterval 30"]);

        let db = vault.resolve_host(HostSelector::Query("db")).unwrap();
        assert_eq!(db.hostname, "10.0.0.5");
        assert_eq!(db.jump_chain.len(), 1);
        assert_eq!(
            db.ssh_options,
            vec!["ServerAliveInterval 30", "IdentityFile ~/.ssh/acme"]
        );
        assert!(
            summary
                .warnings
                .iter()
                .any(|warning| { warning.contains("imported IdentityFile as raw SSH option") })
        );
    }

    #[test]
    fn imports_identity_file_into_host_identity_and_local_mapping() {
        let dir =
            std::env::temp_dir().join(format!("stassh-import-identity-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join(".ssh")).unwrap();
        let key_path = dir.join(".ssh/acme");
        std::fs::write(&key_path, "not a real key; fake resolver does not read it").unwrap();
        let config_path = dir.join("config");
        let mut vault = Vault::new();
        let mut local_config = LocalConfig::new();

        let summary = import_openssh_config_with_identities(
            &mut vault,
            "Host db\n    HostName 10.0.0.5\n    IdentityFile ~/.ssh/acme\n",
            IdentityImportContext {
                local_config: &mut local_config,
                config_path: &config_path,
                home_dir: Some(&dir),
                resolver: &FakeResolver,
            },
        )
        .unwrap();

        let db = vault.resolve_host(HostSelector::Query("db")).unwrap();
        assert_eq!(db.identity_fingerprint.as_deref(), Some("SHA256:acme"));
        assert_eq!(db.ssh_options, Vec::<String>::new());
        assert_eq!(
            local_config.identity_path("SHA256:acme"),
            Some(key_path.as_path())
        );
        assert!(
            summary
                .warnings
                .iter()
                .any(|warning| warning.contains("mapped IdentityFile"))
        );

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn caches_identity_derivation_across_imported_hosts() {
        let dir =
            std::env::temp_dir().join(format!("stassh-import-cache-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join(".ssh")).unwrap();
        let key_path = dir.join(".ssh/acme");
        std::fs::write(&key_path, "not a real key; fake resolver does not read it").unwrap();
        let config_path = dir.join("config");
        let mut vault = Vault::new();
        let mut local_config = LocalConfig::new();
        let resolver = CountingResolver::succeeds();

        import_openssh_config_with_identities(
            &mut vault,
            "Host db1\n    IdentityFile ~/.ssh/acme\nHost db2\n    IdentityFile ~/.ssh/acme\n",
            IdentityImportContext {
                local_config: &mut local_config,
                config_path: &config_path,
                home_dir: Some(&dir),
                resolver: &resolver,
            },
        )
        .unwrap();

        let db1 = vault.resolve_host(HostSelector::Query("db1")).unwrap();
        let db2 = vault.resolve_host(HostSelector::Query("db2")).unwrap();
        assert_eq!(resolver.calls.get(), 1);
        assert_eq!(db1.identity_fingerprint.as_deref(), Some("SHA256:acme"));
        assert_eq!(db2.identity_fingerprint.as_deref(), Some("SHA256:acme"));

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn caches_identity_derivation_failures_across_imported_hosts() {
        let dir =
            std::env::temp_dir().join(format!("stassh-import-cache-fail-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join(".ssh")).unwrap();
        let key_path = dir.join(".ssh/acme");
        std::fs::write(&key_path, "not a real key; fake resolver does not read it").unwrap();
        let config_path = dir.join("config");
        let mut vault = Vault::new();
        let mut local_config = LocalConfig::new();
        let resolver = CountingResolver::fails();

        import_openssh_config_with_identities(
            &mut vault,
            "Host db1\n    IdentityFile ~/.ssh/acme\nHost db2\n    IdentityFile ~/.ssh/acme\n",
            IdentityImportContext {
                local_config: &mut local_config,
                config_path: &config_path,
                home_dir: Some(&dir),
                resolver: &resolver,
            },
        )
        .unwrap();

        let db1 = vault.resolve_host(HostSelector::Query("db1")).unwrap();
        let db2 = vault.resolve_host(HostSelector::Query("db2")).unwrap();
        assert_eq!(resolver.calls.get(), 1);
        assert!(db1.identity_fingerprint.is_none());
        assert!(db2.identity_fingerprint.is_none());
        assert_eq!(db1.ssh_options, vec!["IdentityFile ~/.ssh/acme"]);
        assert_eq!(db2.ssh_options, vec!["IdentityFile ~/.ssh/acme"]);

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn preserves_tokenized_identity_file_as_raw_option() {
        let dir =
            std::env::temp_dir().join(format!("stassh-import-token-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("config");
        let mut vault = Vault::new();
        let mut local_config = LocalConfig::new();

        let summary = import_openssh_config_with_identities(
            &mut vault,
            "Host db\n    IdentityFile ~/.ssh/%h\n",
            IdentityImportContext {
                local_config: &mut local_config,
                config_path: &config_path,
                home_dir: Some(&dir),
                resolver: &FakeResolver,
            },
        )
        .unwrap();

        let db = vault.resolve_host(HostSelector::Query("db")).unwrap();
        assert!(db.identity_fingerprint.is_none());
        assert_eq!(db.ssh_options, vec!["IdentityFile ~/.ssh/%h"]);
        assert!(
            summary
                .warnings
                .iter()
                .any(|warning| warning.contains("preserved IdentityFile"))
        );

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn multiple_identity_files_map_first_and_preserve_rest() {
        let dir =
            std::env::temp_dir().join(format!("stassh-import-multi-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = dir.join("first");
        let second = dir.join("second");
        std::fs::write(&first, "first").unwrap();
        std::fs::write(&second, "second").unwrap();
        let config_path = dir.join("config");
        let mut vault = Vault::new();
        let mut local_config = LocalConfig::new();

        import_openssh_config_with_identities(
            &mut vault,
            &format!(
                "Host db\n    IdentityFile {}\n    IdentityFile {}\n",
                first.display(),
                second.display()
            ),
            IdentityImportContext {
                local_config: &mut local_config,
                config_path: &config_path,
                home_dir: Some(&dir),
                resolver: &FakeResolver,
            },
        )
        .unwrap();

        let db = vault.resolve_host(HostSelector::Query("db")).unwrap();
        assert_eq!(db.identity_fingerprint.as_deref(), Some("SHA256:first"));
        assert_eq!(
            db.ssh_options,
            vec![format!("IdentityFile {}", second.display())]
        );

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_direct_include_before_import() {
        let dir =
            std::env::temp_dir().join(format!("stassh-import-include-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let root = dir.join("config");
        let included = dir.join("included.conf");
        std::fs::write(
            &root,
            "Include included.conf\nHost root\n    HostName root.example\n",
        )
        .unwrap();
        std::fs::write(&included, "Host included\n    HostName included.example\n").unwrap();

        let read = read_openssh_config_with_includes(&root, None).unwrap();
        let mut vault = Vault::new();
        let summary = import_openssh_config(&mut vault, &read.contents).unwrap();

        assert!(read.warnings.is_empty());
        assert_eq!(summary.imported, vec!["included", "root"]);
        assert_eq!(
            vault
                .resolve_host(HostSelector::Query("included"))
                .unwrap()
                .hostname,
            "included.example"
        );

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_glob_includes_in_sorted_order() {
        let dir = std::env::temp_dir().join(format!(
            "stassh-import-include-glob-{}",
            uuid::Uuid::new_v4()
        ));
        let include_dir = dir.join("conf.d");
        std::fs::create_dir_all(&include_dir).unwrap();
        let root = dir.join("config");
        std::fs::write(&root, "Include conf.d/*.conf\n").unwrap();
        std::fs::write(
            include_dir.join("b.conf"),
            "Host beta\n    HostName beta.example\n",
        )
        .unwrap();
        std::fs::write(
            include_dir.join("a.conf"),
            "Host alpha\n    HostName alpha.example\n",
        )
        .unwrap();

        let read = read_openssh_config_with_includes(&root, None).unwrap();
        let mut vault = Vault::new();
        let summary = import_openssh_config(&mut vault, &read.contents).unwrap();

        assert!(read.warnings.is_empty());
        assert_eq!(summary.imported, vec!["alpha", "beta"]);

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_nested_includes() {
        let dir = std::env::temp_dir().join(format!(
            "stassh-import-include-nested-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let root = dir.join("config");
        let first = dir.join("first.conf");
        let second = dir.join("second.conf");
        std::fs::write(&root, "Include first.conf\n").unwrap();
        std::fs::write(&first, "Include second.conf\nHost first\n").unwrap();
        std::fs::write(&second, "Host second\n    HostName second.example\n").unwrap();

        let read = read_openssh_config_with_includes(&root, None).unwrap();
        let mut vault = Vault::new();
        let summary = import_openssh_config(&mut vault, &read.contents).unwrap();

        assert!(read.warnings.is_empty());
        assert_eq!(summary.imported, vec!["second", "first"]);

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn warns_and_skips_recursive_include_cycles() {
        let dir = std::env::temp_dir().join(format!(
            "stassh-import-include-cycle-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let root = dir.join("config");
        let looped = dir.join("loop.conf");
        std::fs::write(&root, "Include loop.conf\nHost root\n").unwrap();
        std::fs::write(&looped, "Include config\nHost looped\n").unwrap();

        let read = read_openssh_config_with_includes(&root, None).unwrap();
        let mut vault = Vault::new();
        let summary = import_openssh_config(&mut vault, &read.contents).unwrap();

        assert!(
            read.warnings
                .iter()
                .any(|warning| warning.contains("recursive Include cycle"))
        );
        assert_eq!(summary.imported, vec!["looped", "root"]);

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn applies_host_star_defaults_after_concrete_blocks() {
        let mut vault = Vault::new();
        import_openssh_config(
            &mut vault,
            r#"
Host web
    HostName web.example
    User deploy

Host *
    Port 2222
    ServerAliveInterval 30
"#,
        )
        .unwrap();

        let web = vault.resolve_host(HostSelector::Query("web")).unwrap();
        assert_eq!(web.hostname, "web.example");
        assert_eq!(web.username.as_deref(), Some("deploy"));
        assert_eq!(web.port, 2222);
        assert_eq!(web.ssh_options, vec!["ServerAliveInterval 30"]);
    }

    #[test]
    fn keeps_first_matching_scalar_value_for_host_star_defaults() {
        let mut vault = Vault::new();
        import_openssh_config(
            &mut vault,
            r#"
Host *
    User default-user
    Port 22

Host web
    HostName web.example
    User deploy
    Port 2222
"#,
        )
        .unwrap();

        let web = vault.resolve_host(HostSelector::Query("web")).unwrap();
        assert_eq!(web.hostname, "web.example");
        assert_eq!(web.username.as_deref(), Some("default-user"));
        assert_eq!(web.port, 22);
    }

    #[test]
    fn imports_identity_file_inherited_from_host_star() {
        let dir = std::env::temp_dir().join(format!(
            "stassh-import-host-star-identity-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(dir.join(".ssh")).unwrap();
        let key_path = dir.join(".ssh/acme");
        std::fs::write(&key_path, "not a real key; fake resolver does not read it").unwrap();
        let config_path = dir.join("config");
        let mut vault = Vault::new();
        let mut local_config = LocalConfig::new();

        import_openssh_config_with_identities(
            &mut vault,
            "Host db\n    HostName db.example\nHost *\n    IdentityFile ~/.ssh/acme\n",
            IdentityImportContext {
                local_config: &mut local_config,
                config_path: &config_path,
                home_dir: Some(&dir),
                resolver: &FakeResolver,
            },
        )
        .unwrap();

        let db = vault.resolve_host(HostSelector::Query("db")).unwrap();
        assert_eq!(db.identity_fingerprint.as_deref(), Some("SHA256:acme"));
        assert_eq!(db.ssh_options, Vec::<String>::new());
        assert_eq!(
            local_config.identity_path("SHA256:acme"),
            Some(key_path.as_path())
        );

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn imports_supported_forward_forms() {
        let mut vault = Vault::new();
        import_openssh_config(
            &mut vault,
            r#"
Host web
    HostName web.example
    LocalForward 127.0.0.1:8080 127.0.0.1:80
    RemoteForward 127.0.0.1:9000:127.0.0.1:9000
    DynamicForward 127.0.0.1:1080
"#,
        )
        .unwrap();

        let web = vault.resolve_host(HostSelector::Query("web")).unwrap();
        assert_eq!(
            web.forwards,
            vec![
                ForwardDefinition::Local {
                    bind_address: "127.0.0.1".to_string(),
                    local_port: 8080,
                    destination_host: "127.0.0.1".to_string(),
                    destination_port: 80,
                },
                ForwardDefinition::Remote {
                    bind_address: "127.0.0.1".to_string(),
                    remote_port: 9000,
                    destination_host: "127.0.0.1".to_string(),
                    destination_port: 9000,
                },
                ForwardDefinition::Dynamic {
                    bind_address: "127.0.0.1".to_string(),
                    local_port: 1080,
                },
            ]
        );
    }

    #[test]
    fn skips_existing_hosts() {
        let mut vault = Vault::new();
        import_openssh_config(&mut vault, "Host web\nHostName web.example\n").unwrap();
        let summary =
            import_openssh_config(&mut vault, "Host web\nHostName web2.example\n").unwrap();

        assert!(summary.imported.is_empty());
        assert_eq!(summary.skipped, vec!["web: host already exists"]);
    }

    #[test]
    fn parses_proxy_jump_aliases() {
        assert_eq!(
            parse_proxy_jump_aliases("admin@bastion:2222,gateway"),
            vec!["bastion", "gateway"]
        );
        assert!(parse_proxy_jump_aliases("none").is_empty());
    }
}
