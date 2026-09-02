use config::{ConfigHandle, SidebarColors};
use mux::sidebar::SidebarEntry;
use mux::Mux;
use wezterm_term::color::{ColorAttribute, ColorPalette, SrgbaTuple};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarItem {
    None,
    Entry(String),
    NewButton,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarRow {
    pub item: SidebarItem,
    /// Title text including the " (n)" tab-count badge; no leading
    /// padding (paint adds it).
    pub title: String,
    pub subtitle: Option<String>,
    pub is_active: bool,
    pub is_open: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SidebarState {
    pub rows: Vec<SidebarRow>,
    pub colors: ResolvedColors,
}

/// All colors resolved to concrete sRGB values at model build time;
/// paint picks per-row colors by state (active/hover/open/inactive).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedColors {
    pub background: SrgbaTuple,
    pub foreground: SrgbaTuple,
    pub active_bg: SrgbaTuple,
    pub active_fg: SrgbaTuple,
    pub inactive_fg: SrgbaTuple,
    pub subtitle_fg: SrgbaTuple,
    pub hover_bg: SrgbaTuple,
    pub hover_fg: SrgbaTuple,
    pub active_indicator: SrgbaTuple,
    pub menu_border: SrgbaTuple,
}

/// Lighten dark colors / darken light colors by `amount` (0..1).
fn shift_towards_contrast(color: SrgbaTuple, amount: f32) -> SrgbaTuple {
    let SrgbaTuple(r, g, b, a) = color;
    let luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    if luminance < 0.5 {
        SrgbaTuple(
            r + (1. - r) * amount,
            g + (1. - g) * amount,
            b + (1. - b) * amount,
            a,
        )
    } else {
        SrgbaTuple(r * (1. - amount), g * (1. - amount), b * (1. - amount), a)
    }
}

fn resolve_colors(config: &ConfigHandle, palette: &ColorPalette) -> ResolvedColors {
    let empty = SidebarColors::default();
    let sc = config.resolved_palette.sidebar.as_ref().unwrap_or(&empty);

    let background: SrgbaTuple = sc.background.map(Into::into).unwrap_or(palette.background);
    let foreground: SrgbaTuple = sc.foreground.map(Into::into).unwrap_or(palette.foreground);
    let (active_bg, active_fg) = match &sc.active {
        Some(active) => (active.bg_color.into(), active.fg_color.into()),
        None => (background, palette.foreground),
    };
    let inactive_fg: SrgbaTuple = sc
        .inactive_foreground
        .map(Into::into)
        .unwrap_or_else(|| palette.resolve_fg(ColorAttribute::PaletteIndex(8)));
    let subtitle_fg: SrgbaTuple = sc
        .subtitle_foreground
        .map(Into::into)
        .unwrap_or_else(|| palette.resolve_fg(ColorAttribute::PaletteIndex(8)));
    let (hover_bg, hover_fg) = match &sc.hover {
        Some(hover) => (hover.bg_color.into(), hover.fg_color.into()),
        None => (shift_towards_contrast(background, 0.1), foreground),
    };
    let active_indicator = sc.active_indicator.map(Into::into).unwrap_or(active_fg);
    let menu_border = sc
        .menu_border
        .map(Into::into)
        .unwrap_or_else(|| shift_towards_contrast(background, 0.3));

    ResolvedColors {
        background,
        foreground,
        active_bg,
        active_fg,
        inactive_fg,
        subtitle_fg,
        hover_bg,
        hover_fg,
        active_indicator,
        menu_border,
    }
}

/// Build the display rows from mux entries; pure so it can be unit
/// tested without a running mux.
pub fn build_rows(active_workspace: &str, entries: &[SidebarEntry]) -> Vec<SidebarRow> {
    let mut rows = vec![SidebarRow {
        item: SidebarItem::NewButton,
        title: "+ New workspace".to_string(),
        subtitle: None,
        is_active: false,
        is_open: false,
    }];
    for entry in entries {
        let badge = entry
            .tab_count
            .map(|n| format!(" ({n})"))
            .unwrap_or_default();
        rows.push(SidebarRow {
            item: SidebarItem::Entry(entry.name.clone()),
            title: format!("{}{badge}", entry.name),
            subtitle: entry.subtitle.clone(),
            is_active: entry.name == active_workspace,
            is_open: entry.tab_count.is_some(),
        });
    }
    rows
}

impl SidebarState {
    pub fn new(config: &ConfigHandle, palette: &ColorPalette) -> Self {
        let mux = Mux::get();
        Self {
            rows: build_rows(&mux.active_workspace(), &mux.compute_sidebar_entries()),
            colors: resolve_colors(config, palette),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn entries() -> Vec<SidebarEntry> {
        vec![
            SidebarEntry {
                name: "api".to_string(),
                tab_count: Some(3),
                subtitle: Some("api".to_string()),
            },
            SidebarEntry {
                name: "docs".to_string(),
                tab_count: None,
                subtitle: None,
            },
        ]
    }

    #[test]
    fn builds_new_button_first() {
        let rows = build_rows("api", &entries());
        assert_eq!(rows[0].item, SidebarItem::NewButton);
        assert_eq!(rows[0].title, "+ New workspace");
        assert!(!rows[0].is_active);
        assert!(!rows[0].is_open);
    }

    #[test]
    fn entry_rows_carry_badge_and_state() {
        let rows = build_rows("api", &entries());
        assert_eq!(rows[1].item, SidebarItem::Entry("api".to_string()));
        assert_eq!(rows[1].title, "api (3)");
        assert!(rows[1].is_active);
        assert!(rows[1].is_open);
        assert_eq!(rows[1].subtitle.as_deref(), Some("api"));

        assert_eq!(rows[2].title, "docs");
        assert!(!rows[2].is_active);
        assert!(!rows[2].is_open);
        assert_eq!(rows[2].subtitle, None);
    }

    #[test]
    fn contrast_shift_follows_luminance() {
        // Dark colors lighten
        let dark = shift_towards_contrast(SrgbaTuple(0.1, 0.1, 0.1, 1.), 0.1);
        assert!(dark.0 > 0.1 && dark.1 > 0.1 && dark.2 > 0.1);
        // Light colors darken
        let light = shift_towards_contrast(SrgbaTuple(0.9, 0.9, 0.9, 1.), 0.1);
        assert!(light.0 < 0.9 && light.1 < 0.9 && light.2 < 0.9);
        // Alpha is preserved
        assert_eq!(dark.3, 1.);
    }
}
