//! Right-click context menu for workspace sidebar entries.
//! This file holds pure state and labels; painting lives in
//! termwindow/render/sidebar.rs and input routing in
//! termwindow/mouseevent.rs + termwindow/keyevent.rs.

use config::keyassignment::SpawnCommand;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarMenuAction {
    Rename,
    SetDefaultCwd,
    SetDefaultCommand,
    SetDefaultProfile,
    MoveUp,
    MoveDown,
    Remove,
}

/// (action, label) in display order; the index into this table is
/// what `UIItemType::SidebarMenuItem` carries.
pub const MENU_ITEMS: &[(SidebarMenuAction, &str)] = &[
    (SidebarMenuAction::Rename, "Rename workspace"),
    (SidebarMenuAction::SetDefaultCwd, "Set default directory"),
    (SidebarMenuAction::SetDefaultCommand, "Set default command"),
    (SidebarMenuAction::SetDefaultProfile, "Set default profile"),
    (SidebarMenuAction::MoveUp, "Move up"),
    (SidebarMenuAction::MoveDown, "Move down"),
    (SidebarMenuAction::Remove, "Remove from list"),
];

#[derive(Clone, Debug, PartialEq)]
pub struct SidebarMenuState {
    pub workspace: String,
    /// Anchor (mouse click position), in window pixels.
    pub x: f32,
    pub y: f32,
    /// Index into MENU_ITEMS currently under the mouse.
    pub hovered: Option<usize>,
}

/// One row in the "Set default profile" flyout.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileMenuEntry {
    pub label: String,
    /// `None` is the "Default shell" row: choosing it clears the
    /// workspace's runtime profile override.
    pub profile: Option<SpawnCommand>,
}

/// The flyout listing launch_menu profiles, opened by the
/// `SetDefaultProfile` menu action. Same visual language and input
/// routing as the main context menu; the index into `entries` is what
/// `UIItemType::SidebarProfileMenuItem` carries.
#[derive(Clone, Debug, PartialEq)]
pub struct SidebarProfileMenuState {
    pub workspace: String,
    /// Anchor (the main menu's position), in window pixels.
    pub x: f32,
    pub y: f32,
    pub hovered: Option<usize>,
    pub entries: Vec<ProfileMenuEntry>,
}
