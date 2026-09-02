//! Sidebar entry model: merges configured workspaces with live mux
//! workspaces and applies in-memory display overrides.

use crate::workspace_defaults::{resolve_workspace_defaults_impl, WorkspaceMetadata};
use config::WorkspaceEntry;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarEntry {
    pub name: String,
    /// `Some(n)` for a live workspace with `n` tabs;
    /// `None` for a configured-but-not-open workspace.
    pub tab_count: Option<usize>,
    /// Basename of the effective default cwd, or the default command
    /// when no cwd is set. `None` when the workspace has no defaults.
    pub subtitle: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SidebarOverrides {
    /// Explicit display order. Names not listed keep their natural
    /// order after the listed ones.
    pub order: Vec<String>,
    /// Entries hidden via "remove from list"; the workspace itself
    /// is never killed.
    pub hidden: HashSet<String>,
}

/// Merge configured workspaces with live mux workspaces into the
/// sidebar's display list.
///
/// Natural order: config entries (file order, first-wins on
/// duplicates, empty names skipped), then live-only workspaces in the
/// given order. `overrides.order` pulls named entries to the front in
/// its own order; `overrides.hidden` entries are dropped.
pub fn compute_sidebar_entries(
    config_entries: &[WorkspaceEntry],
    live: &[(String, usize)],
    metadata: &HashMap<String, WorkspaceMetadata>,
    overrides: &SidebarOverrides,
) -> Vec<SidebarEntry> {
    let mut names: Vec<String> = Vec::new();
    for entry in config_entries {
        if !entry.name.trim().is_empty() && !names.contains(&entry.name) {
            names.push(entry.name.clone());
        }
    }
    for (name, _) in live {
        if !names.contains(name) {
            names.push(name.clone());
        }
    }

    if !overrides.order.is_empty() {
        let mut ordered: Vec<String> = Vec::new();
        for name in &overrides.order {
            if let Some(pos) = names.iter().position(|n| n == name) {
                ordered.push(names.remove(pos));
            }
        }
        ordered.extend(names);
        names = ordered;
    }

    names
        .into_iter()
        .filter(|name| !overrides.hidden.contains(name))
        .map(|name| {
            let tab_count = live.iter().find(|(n, _)| n == &name).map(|(_, c)| *c);
            let (cwd, command) =
                resolve_workspace_defaults_impl(metadata.get(&name), config_entries, &name);
            let subtitle = cwd
                .as_ref()
                .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
                .or(command);
            SidebarEntry {
                name,
                tab_count,
                subtitle,
            }
        })
        .collect()
}

/// Move `name` by `delta` positions within `order`, first
/// materializing `order` from the `current` display order if it is
/// empty, and appending any current names missing from it.
pub fn move_in_order(current: &[String], order: &mut Vec<String>, name: &str, delta: isize) {
    if order.is_empty() {
        *order = current.to_vec();
    }
    for n in current {
        if !order.contains(n) {
            order.push(n.clone());
        }
    }
    if let Some(idx) = order.iter().position(|n| n == name) {
        let item = order.remove(idx);
        let new_idx = (idx as isize + delta).clamp(0, order.len() as isize) as usize;
        order.insert(new_idx, item);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::path::PathBuf;

    fn config_entries() -> Vec<WorkspaceEntry> {
        vec![
            WorkspaceEntry {
                name: "api".to_string(),
                cwd: Some(PathBuf::from("D:/code/api")),
                default_command: Some("npm run dev".to_string()),
            },
            WorkspaceEntry {
                name: "docs".to_string(),
                cwd: None,
                default_command: None,
            },
        ]
    }

    #[test]
    fn merges_config_and_live() {
        let live = vec![("api".to_string(), 3usize), ("scratch".to_string(), 1usize)];
        let entries =
            compute_sidebar_entries(&config_entries(), &live, &HashMap::new(), &SidebarOverrides::default());
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        // config order first, then live-only workspaces
        assert_eq!(names, vec!["api", "docs", "scratch"]);
        assert_eq!(entries[0].tab_count, Some(3));
        // configured but not open
        assert_eq!(entries[1].tab_count, None);
        // live but unconfigured: no subtitle
        assert_eq!(entries[2].subtitle, None);
    }

    #[test]
    fn subtitle_is_cwd_basename() {
        let entries = compute_sidebar_entries(
            &config_entries(),
            &[],
            &HashMap::new(),
            &SidebarOverrides::default(),
        );
        assert_eq!(entries[0].subtitle, Some("api".to_string()));
        assert_eq!(entries[1].subtitle, None);
    }

    #[test]
    fn subtitle_falls_back_to_command() {
        let config = vec![WorkspaceEntry {
            name: "srv".to_string(),
            cwd: None,
            default_command: Some("npm run dev".to_string()),
        }];
        let entries =
            compute_sidebar_entries(&config, &[], &HashMap::new(), &SidebarOverrides::default());
        assert_eq!(entries[0].subtitle, Some("npm run dev".to_string()));
    }

    #[test]
    fn subtitle_uses_runtime_override() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "api".to_string(),
            WorkspaceMetadata {
                cwd: Some(PathBuf::from("E:/elsewhere")),
                default_command: None,
            },
        );
        let entries =
            compute_sidebar_entries(&config_entries(), &[], &metadata, &SidebarOverrides::default());
        assert_eq!(entries[0].subtitle, Some("elsewhere".to_string()));
    }

    #[test]
    fn overrides_reorder_and_hide() {
        let live = vec![("api".to_string(), 1usize), ("scratch".to_string(), 2usize)];
        let overrides = SidebarOverrides {
            order: vec!["scratch".to_string(), "docs".to_string()],
            hidden: HashSet::from(["api".to_string()]),
        };
        let entries = compute_sidebar_entries(&config_entries(), &live, &HashMap::new(), &overrides);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["scratch", "docs"]);
    }

    #[test]
    fn move_in_order_swaps_adjacent() {
        let current: Vec<String> = vec!["a".into(), "b".into(), "c".into()];

        // cold start: order materializes from current display order
        let mut order = vec![];
        move_in_order(&current, &mut order, "b", -1);
        assert_eq!(order, vec!["b".to_string(), "a".to_string(), "c".to_string()]);

        // clamped at the top: no-op
        let mut order = vec!["b".to_string(), "a".to_string(), "c".to_string()];
        move_in_order(&current, &mut order, "b", -1);
        assert_eq!(order, vec!["b".to_string(), "a".to_string(), "c".to_string()]);

        // move down
        move_in_order(&current, &mut order, "b", 1);
        assert_eq!(order, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    }
}
