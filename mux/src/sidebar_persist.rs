//! Persist sidebar workspace overrides (order, hidden entries, and
//! per-workspace metadata) to the data directory so they survive
//! restart. Never written back to the Lua config.

use crate::sidebar::SidebarOverrides;
use crate::workspace_defaults::WorkspaceMetadata;
use config::keyassignment::{SpawnCommand, SpawnTabDomain};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const STATE_VERSION: u32 = 1;
const STATE_FILE_NAME: &str = "sidebar-workspaces.json";

/// Path of the sidebar persistence file in the SideTerm data directory.
pub fn sidebar_state_path() -> PathBuf {
    config::DATA_DIR.join(STATE_FILE_NAME)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
struct PersistedSpawnCommand {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    args: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    set_environment_variables: HashMap<String, String>,
    #[serde(default)]
    domain: SpawnTabDomain,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
struct PersistedWorkspaceMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default_command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    profile: Option<PersistedSpawnCommand>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    profile_label: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
struct PersistedSidebarState {
    #[serde(default)]
    version: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    order: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    hidden: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    remembered: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    metadata: HashMap<String, PersistedWorkspaceMetadata>,
}

impl From<&SpawnCommand> for PersistedSpawnCommand {
    fn from(cmd: &SpawnCommand) -> Self {
        Self {
            label: cmd.label.clone(),
            args: cmd.args.clone(),
            cwd: cmd.cwd.clone(),
            set_environment_variables: cmd.set_environment_variables.clone(),
            domain: cmd.domain.clone(),
        }
    }
}

impl From<PersistedSpawnCommand> for SpawnCommand {
    fn from(cmd: PersistedSpawnCommand) -> Self {
        SpawnCommand {
            label: cmd.label,
            args: cmd.args,
            cwd: cmd.cwd,
            set_environment_variables: cmd.set_environment_variables,
            domain: cmd.domain,
            position: None,
        }
    }
}

impl From<&WorkspaceMetadata> for PersistedWorkspaceMetadata {
    fn from(meta: &WorkspaceMetadata) -> Self {
        Self {
            cwd: meta.cwd.clone(),
            default_command: meta.default_command.clone(),
            profile: meta.profile.as_ref().map(PersistedSpawnCommand::from),
            profile_label: meta.profile_label.clone(),
        }
    }
}

impl From<PersistedWorkspaceMetadata> for WorkspaceMetadata {
    fn from(meta: PersistedWorkspaceMetadata) -> Self {
        WorkspaceMetadata {
            cwd: meta.cwd,
            default_command: meta.default_command,
            profile: meta.profile.map(SpawnCommand::from),
            profile_label: meta.profile_label,
        }
    }
}

fn encode(
    overrides: &SidebarOverrides,
    metadata: &HashMap<String, WorkspaceMetadata>,
) -> PersistedSidebarState {
    let mut hidden: Vec<String> = overrides.hidden.iter().cloned().collect();
    hidden.sort();
    let metadata = metadata
        .iter()
        .filter(|(_, meta)| !meta.is_unset())
        .map(|(name, meta)| (name.clone(), PersistedWorkspaceMetadata::from(meta)))
        .collect();
    PersistedSidebarState {
        version: STATE_VERSION,
        order: overrides.order.clone(),
        hidden,
        remembered: overrides.remembered.clone(),
        metadata,
    }
}

fn decode(state: PersistedSidebarState) -> (SidebarOverrides, HashMap<String, WorkspaceMetadata>) {
    let overrides = SidebarOverrides {
        order: state.order,
        hidden: state.hidden.into_iter().collect(),
        remembered: state.remembered,
    };
    let metadata = state
        .metadata
        .into_iter()
        .map(|(name, meta)| (name, WorkspaceMetadata::from(meta)))
        .collect();
    (overrides, metadata)
}

fn atomic_write(path: &Path, contents: &str) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, contents)?;
    if let Err(err) = std::fs::rename(&tmp, path) {
        // Windows cannot rename over an existing file.
        let _ = std::fs::remove_file(path);
        std::fs::rename(&tmp, path).map_err(|rename_err| {
            let _ = std::fs::remove_file(&tmp);
            anyhow::anyhow!("rename {tmp:?} -> {path:?} failed after {err}: {rename_err}")
        })?;
    }
    Ok(())
}

/// Load persisted sidebar state from `path`. Missing file is an empty
/// state; corrupt / unreadable files are logged by the caller.
pub fn load_from_path(
    path: &Path,
) -> anyhow::Result<(SidebarOverrides, HashMap<String, WorkspaceMetadata>)> {
    let data = std::fs::read_to_string(path)?;
    let state: PersistedSidebarState = serde_json::from_str(&data)?;
    Ok(decode(state))
}

/// Write sidebar state to `path`, replacing any previous file.
pub fn save_to_path(
    path: &Path,
    overrides: &SidebarOverrides,
    metadata: &HashMap<String, WorkspaceMetadata>,
) -> anyhow::Result<()> {
    let state = encode(overrides, metadata);
    let json = serde_json::to_string_pretty(&state)?;
    atomic_write(path, &json)
}

/// Load from the default data-directory path. Missing file → empty
/// state. Other errors are logged and treated as empty so a corrupt
/// file cannot prevent startup.
pub fn load_default() -> (SidebarOverrides, HashMap<String, WorkspaceMetadata>) {
    let path = sidebar_state_path();
    match load_from_path(&path) {
        Ok(state) => state,
        Err(err) => {
            if path.exists() {
                log::error!(
                    "failed to load sidebar workspace state from {}: {err:#}",
                    path.display()
                );
            }
            (SidebarOverrides::default(), HashMap::new())
        }
    }
}

/// Save to the default data-directory path. Errors are logged.
pub fn save_default(overrides: &SidebarOverrides, metadata: &HashMap<String, WorkspaceMetadata>) {
    let path = sidebar_state_path();
    if let Err(err) = save_to_path(&path, overrides, metadata) {
        log::error!(
            "failed to persist sidebar workspace state to {}: {err:#}",
            path.display()
        );
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use config::keyassignment::SpawnTabDomain;
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_DIR_SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_path() -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "sideterm-sidebar-persist-{}-{}",
            std::process::id(),
            TEST_DIR_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(STATE_FILE_NAME);
        (dir, path)
    }

    fn cleanup(dir: &Path) {
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn roundtrip_order_hidden_and_metadata() {
        let (dir, path) = temp_path();
        let mut metadata = HashMap::new();
        metadata.insert(
            "api".to_string(),
            WorkspaceMetadata {
                cwd: Some(PathBuf::from("/code/api")),
                default_command: Some("npm run dev".to_string()),
                ..WorkspaceMetadata::default()
            },
        );
        let mut profile = SpawnCommand::default();
        profile.args = Some(vec!["pwsh".to_string(), "-NoLogo".to_string()]);
        profile.domain = SpawnTabDomain::DomainName("WSL:Ubuntu".to_string());
        metadata.insert(
            "wsl".to_string(),
            WorkspaceMetadata {
                profile: Some(profile),
                profile_label: Some("domain `WSL:Ubuntu`".to_string()),
                ..WorkspaceMetadata::default()
            },
        );
        // Unset metadata must not be written.
        metadata.insert("blank".to_string(), WorkspaceMetadata::default());

        let overrides = SidebarOverrides {
            order: vec!["wsl".to_string(), "api".to_string(), "scratch".to_string()],
            hidden: HashSet::from(["old".to_string()]),
            remembered: vec!["scratch".to_string()],
        };

        save_to_path(&path, &overrides, &metadata).unwrap();
        let (loaded_overrides, loaded_metadata) = load_from_path(&path).unwrap();

        assert_eq!(loaded_overrides.order, overrides.order);
        assert_eq!(loaded_overrides.hidden, overrides.hidden);
        assert_eq!(loaded_overrides.remembered, overrides.remembered);
        assert_eq!(loaded_metadata.len(), 2);
        assert_eq!(loaded_metadata["api"].cwd, Some(PathBuf::from("/code/api")));
        assert_eq!(
            loaded_metadata["api"].default_command.as_deref(),
            Some("npm run dev")
        );
        assert_eq!(
            loaded_metadata["wsl"].profile.as_ref().map(|p| &p.domain),
            Some(&SpawnTabDomain::DomainName("WSL:Ubuntu".to_string()))
        );
        assert_eq!(
            loaded_metadata["wsl"]
                .profile
                .as_ref()
                .and_then(|p| p.args.clone()),
            Some(vec!["pwsh".to_string(), "-NoLogo".to_string()])
        );
        assert!(!loaded_metadata.contains_key("blank"));

        cleanup(&dir);
    }

    #[test]
    fn missing_file_is_an_error_for_load_from_path() {
        let (dir, path) = temp_path();
        assert!(load_from_path(&path).is_err());
        cleanup(&dir);
    }

    #[test]
    fn empty_state_roundtrip() {
        let (dir, path) = temp_path();
        save_to_path(&path, &SidebarOverrides::default(), &HashMap::new()).unwrap();
        let (overrides, metadata) = load_from_path(&path).unwrap();
        assert_eq!(overrides, SidebarOverrides::default());
        assert!(metadata.is_empty());
        cleanup(&dir);
    }
}
