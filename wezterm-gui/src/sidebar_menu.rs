//! Right-click context menu for workspace sidebar entries.
//! This file holds pure state and labels; painting lives in
//! termwindow/render/sidebar.rs and input routing in
//! termwindow/mouseevent.rs + termwindow/keyevent.rs.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarMenuAction {
    Rename,
    SetDefaultCwd,
    SetDefaultCommand,
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
