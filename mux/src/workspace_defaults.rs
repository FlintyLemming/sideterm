use config::WorkspaceEntry;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceMetadata {
    pub cwd: Option<PathBuf>,
    pub default_command: Option<String>,
}

/// Resolve the effective (cwd, default_command) for `workspace`.
///
/// Priority, per field: runtime `metadata` override > `config_entries`
/// (first match wins) > unset. Whitespace-only commands count as unset.
pub fn resolve_workspace_defaults_impl(
    metadata: Option<&WorkspaceMetadata>,
    config_entries: &[WorkspaceEntry],
    workspace: &str,
) -> (Option<PathBuf>, Option<String>) {
    let from_config = config_entries
        .iter()
        .find(|e| !e.name.trim().is_empty() && e.name == workspace);

    let cwd = metadata
        .and_then(|m| m.cwd.clone())
        .or_else(|| from_config.and_then(|e| e.cwd.clone()));

    let command = metadata
        .and_then(|m| m.default_command.clone())
        .or_else(|| from_config.and_then(|e| e.default_command.clone()))
        .and_then(|cmd| (!cmd.trim().is_empty()).then_some(cmd));

    (cwd, command)
}

/// Insert the workspace default cwd into the spawn cwd priority chain:
/// explicit/inherited `cwd` wins; the workspace default only fills a `None`.
pub fn apply_workspace_default_cwd(
    cwd: Option<String>,
    workspace_default: Option<PathBuf>,
) -> Option<String> {
    cwd.or_else(|| workspace_default.map(|p| p.to_string_lossy().to_string()))
}

#[cfg(test)]
mod test {
    use super::*;

    fn config_entries() -> Vec<WorkspaceEntry> {
        vec![
            WorkspaceEntry {
                name: "api".to_string(),
                cwd: Some(PathBuf::from("D:/code/api")),
                default_command: Some("npm run dev".to_string()),
            },
            WorkspaceEntry {
                name: "blank-cmd".to_string(),
                cwd: None,
                default_command: Some("   ".to_string()),
            },
        ]
    }

    #[test]
    fn falls_back_to_config() {
        let (cwd, cmd) = resolve_workspace_defaults_impl(None, &config_entries(), "api");
        assert_eq!(cwd, Some(PathBuf::from("D:/code/api")));
        assert_eq!(cmd, Some("npm run dev".to_string()));
    }

    #[test]
    fn runtime_override_beats_config_per_field() {
        // Only cwd overridden: command must still come from config.
        let meta = WorkspaceMetadata {
            cwd: Some(PathBuf::from("E:/override")),
            default_command: None,
        };
        let (cwd, cmd) = resolve_workspace_defaults_impl(Some(&meta), &config_entries(), "api");
        assert_eq!(cwd, Some(PathBuf::from("E:/override")));
        assert_eq!(cmd, Some("npm run dev".to_string()));

        // Both overridden.
        let meta = WorkspaceMetadata {
            cwd: Some(PathBuf::from("E:/override")),
            default_command: Some("make".to_string()),
        };
        let (cwd, cmd) = resolve_workspace_defaults_impl(Some(&meta), &config_entries(), "api");
        assert_eq!(cwd, Some(PathBuf::from("E:/override")));
        assert_eq!(cmd, Some("make".to_string()));
    }

    #[test]
    fn whitespace_only_command_is_unset() {
        let (cwd, cmd) = resolve_workspace_defaults_impl(None, &config_entries(), "blank-cmd");
        assert_eq!(cwd, None);
        assert_eq!(cmd, None);
    }

    #[test]
    fn unknown_workspace_has_no_defaults() {
        assert_eq!(
            resolve_workspace_defaults_impl(None, &config_entries(), "nope"),
            (None, None)
        );
    }

    #[test]
    fn duplicate_config_entries_resolve_first_match_wins() {
        let mut entries = config_entries();
        entries.push(WorkspaceEntry {
            name: "api".to_string(),
            cwd: Some(PathBuf::from("Z:/later")),
            default_command: None,
        });
        let (cwd, _) = resolve_workspace_defaults_impl(None, &entries, "api");
        assert_eq!(cwd, Some(PathBuf::from("D:/code/api")));
    }

    #[test]
    fn workspace_default_cwd_is_only_a_fallback() {
        // Explicit or inherited cwd wins over the workspace default.
        assert_eq!(
            apply_workspace_default_cwd(
                Some("C:/explicit".to_string()),
                Some(PathBuf::from("D:/workspace-default")),
            ),
            Some("C:/explicit".to_string())
        );
        // Workspace default kicks in when nothing earlier produced a cwd.
        assert_eq!(
            apply_workspace_default_cwd(None, Some(PathBuf::from("D:/workspace-default"))),
            Some("D:/workspace-default".to_string())
        );
        // No workspace default: stays None so the system default applies downstream.
        assert_eq!(apply_workspace_default_cwd(None, None), None);
    }
}
