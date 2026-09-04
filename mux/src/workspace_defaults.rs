use config::keyassignment::{SpawnCommand, SpawnTabDomain};
use config::WorkspaceEntry;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkspaceMetadata {
    pub cwd: Option<PathBuf>,
    pub default_command: Option<String>,
    /// Default launch profile for new tabs in this workspace: a
    /// snapshot of a `launch_menu` entry or a mux domain (e.g. an
    /// auto-detected WSL distribution) chosen at runtime. When its
    /// `domain` names a specific domain, new tabs spawn there;
    /// otherwise they stay in the spawn request's domain.
    pub profile: Option<SpawnCommand>,
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

/// The workspace's default launch profile, from runtime metadata only.
pub fn resolve_workspace_profile_impl(
    metadata: Option<&WorkspaceMetadata>,
) -> Option<SpawnCommand> {
    metadata.and_then(|m| m.profile.clone())
}

/// What a plain spawn should run once the workspace's default profile
/// is taken into account.
pub struct ProfileOverlay {
    /// Program argv: the explicit spawn args if given, else the
    /// profile's args. `None` = default shell.
    pub args: Option<Vec<String>>,
    /// Explicit-level cwd: the spawn's cwd wins over the profile's.
    pub cwd: Option<PathBuf>,
    /// Environment variables contributed by the profile.
    pub env: HashMap<String, String>,
    /// The profile's domain, when it names a specific domain.
    /// `None` = keep whatever domain the spawn request asked for.
    pub domain: Option<SpawnTabDomain>,
    /// Whether the workspace's default command should be injected
    /// into the spawned program. True unless the spawn request
    /// carried explicit args; the assumption is that the profile's
    /// program (or the default shell) is an interactive shell the
    /// command can be typed into. If a profile runs a non-shell
    /// program, don't set a default command for that workspace.
    pub inject_default_command: bool,
}

/// Overlay the workspace's default profile onto a spawn request.
/// A spawn with explicit `args` is a specific program the user asked
/// for, so the profile is ignored entirely.
pub fn apply_workspace_profile(
    spawn_args: Option<&[String]>,
    spawn_cwd: Option<PathBuf>,
    profile: Option<&SpawnCommand>,
) -> ProfileOverlay {
    if let Some(args) = spawn_args {
        return ProfileOverlay {
            args: Some(args.to_vec()),
            cwd: spawn_cwd,
            env: HashMap::new(),
            domain: None,
            inject_default_command: false,
        };
    }
    match profile {
        Some(profile) => ProfileOverlay {
            args: profile.args.clone(),
            cwd: spawn_cwd.or_else(|| profile.cwd.clone()),
            env: profile.set_environment_variables.clone(),
            // Only a concretely named domain overrides the spawn's;
            // DefaultDomain/CurrentPaneDomain are just the profile
            // inheriting SpawnCommand's defaults, not a real choice.
            domain: match &profile.domain {
                named @ (SpawnTabDomain::DomainName(_) | SpawnTabDomain::DomainId(_)) => {
                    Some(named.clone())
                }
                _ => None,
            },
            inject_default_command: true,
        },
        None => ProfileOverlay {
            args: None,
            cwd: spawn_cwd,
            env: HashMap::new(),
            domain: None,
            inject_default_command: true,
        },
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use config::keyassignment::SpawnCommand;

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
            ..WorkspaceMetadata::default()
        };
        let (cwd, cmd) = resolve_workspace_defaults_impl(Some(&meta), &config_entries(), "api");
        assert_eq!(cwd, Some(PathBuf::from("E:/override")));
        assert_eq!(cmd, Some("npm run dev".to_string()));

        // Both overridden.
        let meta = WorkspaceMetadata {
            cwd: Some(PathBuf::from("E:/override")),
            default_command: Some("make".to_string()),
            ..WorkspaceMetadata::default()
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

    fn profile(args: Option<Vec<String>>, cwd: Option<PathBuf>) -> SpawnCommand {
        SpawnCommand {
            args,
            cwd,
            ..SpawnCommand::default()
        }
    }

    #[test]
    fn profile_comes_from_runtime_metadata_only() {
        let meta = WorkspaceMetadata {
            profile: Some(profile(Some(vec!["pwsh".to_string()]), None)),
            ..WorkspaceMetadata::default()
        };
        assert_eq!(
            resolve_workspace_profile_impl(Some(&meta)),
            Some(profile(Some(vec!["pwsh".to_string()]), None))
        );
        assert_eq!(resolve_workspace_profile_impl(None), None);
        assert_eq!(
            resolve_workspace_profile_impl(Some(&WorkspaceMetadata::default())),
            None
        );
    }

    #[test]
    fn explicit_spawn_args_ignore_the_profile() {
        let explicit = vec!["vim".to_string()];
        let overlay = apply_workspace_profile(
            Some(&explicit),
            None,
            Some(&profile(Some(vec!["pwsh".to_string()]), None)),
        );
        assert_eq!(overlay.args, Some(explicit));
        assert!(!overlay.inject_default_command);
    }

    #[test]
    fn profile_supplies_args_cwd_and_env_for_plain_spawns() {
        let mut prof = profile(
            Some(vec!["pwsh".to_string(), "-NoLogo".to_string()]),
            Some(PathBuf::from("D:/profile-cwd")),
        );
        prof.set_environment_variables
            .insert("FOO".to_string(), "bar".to_string());

        let overlay = apply_workspace_profile(None, None, Some(&prof));
        assert_eq!(
            overlay.args,
            Some(vec!["pwsh".to_string(), "-NoLogo".to_string()])
        );
        assert_eq!(overlay.cwd, Some(PathBuf::from("D:/profile-cwd")));
        assert_eq!(overlay.env.get("FOO"), Some(&"bar".to_string()));
        // The profile's program is assumed to be a shell: the
        // workspace's default command is still injected into it.
        assert!(overlay.inject_default_command);
    }

    #[test]
    fn explicit_spawn_cwd_beats_profile_cwd() {
        let overlay = apply_workspace_profile(
            None,
            Some(PathBuf::from("C:/explicit")),
            Some(&profile(
                Some(vec!["pwsh".to_string()]),
                Some(PathBuf::from("D:/profile-cwd")),
            )),
        );
        assert_eq!(overlay.cwd, Some(PathBuf::from("C:/explicit")));
    }

    #[test]
    fn profile_with_args_still_injects_default_command() {
        // A profile with args (e.g. PowerShell 7) picks the shell;
        // the workspace's default command is typed into that shell.
        let overlay = apply_workspace_profile(
            None,
            None,
            Some(&profile(
                Some(vec!["pwsh".to_string()]),
                Some(PathBuf::from("D:/profile-cwd")),
            )),
        );
        assert_eq!(overlay.args, Some(vec!["pwsh".to_string()]));
        assert_eq!(overlay.cwd, Some(PathBuf::from("D:/profile-cwd")));
        assert!(overlay.inject_default_command);
    }

    #[test]
    fn no_profile_is_a_passthrough() {
        let overlay = apply_workspace_profile(None, None, None);
        assert_eq!(overlay.args, None);
        assert_eq!(overlay.cwd, None);
        assert!(overlay.env.is_empty());
        assert_eq!(overlay.domain, None);
        assert!(overlay.inject_default_command);
    }

    #[test]
    fn profile_with_named_domain_overrides_the_spawn_domain() {
        let mut prof = profile(None, None);
        prof.domain = SpawnTabDomain::DomainName("WSL:Ubuntu".to_string());
        let overlay = apply_workspace_profile(None, None, Some(&prof));
        assert_eq!(
            overlay.domain,
            Some(SpawnTabDomain::DomainName("WSL:Ubuntu".to_string()))
        );
        // No args: the domain's default shell runs, so the workspace's
        // default command is still injected.
        assert!(overlay.inject_default_command);
    }

    #[test]
    fn profile_with_unspecified_domain_keeps_the_spawn_domain() {
        // SpawnCommand::default().domain is CurrentPaneDomain: that is
        // "no opinion", not a choice to pin the pane's domain.
        let overlay = apply_workspace_profile(
            None,
            None,
            Some(&profile(Some(vec!["pwsh".to_string()]), None)),
        );
        assert_eq!(overlay.domain, None);
    }

    #[test]
    fn explicit_spawn_args_ignore_the_profile_domain() {
        let explicit = vec!["vim".to_string()];
        let mut prof = profile(None, None);
        prof.domain = SpawnTabDomain::DomainName("WSL:Ubuntu".to_string());
        let overlay = apply_workspace_profile(Some(&explicit), None, Some(&prof));
        assert_eq!(overlay.args, Some(explicit));
        assert_eq!(overlay.domain, None);
    }
}
