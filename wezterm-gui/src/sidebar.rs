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
    /// One line per configured default (directory / command /
    /// profile), each with a leading marker. Painted in the smaller
    /// subtitle font under the title.
    pub subtitle_lines: Vec<String>,
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
pub(crate) fn shift_towards_contrast(color: SrgbaTuple, amount: f32) -> SrgbaTuple {
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

pub(crate) fn resolve_colors(config: &ConfigHandle, palette: &ColorPalette) -> ResolvedColors {
    let empty = SidebarColors::default();
    let sc = config.resolved_palette.sidebar.as_ref().unwrap_or(&empty);

    let background: SrgbaTuple = sc.background.map(Into::into).unwrap_or(palette.background);
    let foreground: SrgbaTuple = sc.foreground.map(Into::into).unwrap_or(palette.foreground);
    let (active_bg, active_fg) = match &sc.active {
        Some(active) => (active.bg_color.into(), active.fg_color.into()),
        // Default to a gently contrasted fill so the active pill is
        // visible without configuration.
        None => (shift_towards_contrast(background, 0.15), palette.foreground),
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

/// Vertical rhythm of a pill row, in title-font cells. Shared by the
/// element painter and the row-fitting math so the two can't drift.
pub const ROW_PAD_V: f32 = 0.2;
pub const ROW_GAP: f32 = 0.25;
pub const EDGE_PAD_V: f32 = 0.5;
/// Horizontal padding of a row / of the strip edges, in cells.
pub const ROW_PAD_H: f32 = 0.5;
pub const EDGE_PAD_H: f32 = 0.5;

/// Colors for one row in each interaction state; the element painter
/// maps these onto ElementColors / hover_colors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowStyle {
    pub bg: SrgbaTuple,
    pub fg: SrgbaTuple,
    pub hover_bg: SrgbaTuple,
    pub hover_fg: SrgbaTuple,
}

pub fn style_for_row(row: &SidebarRow, colors: &ResolvedColors) -> RowStyle {
    let bg = if row.is_active {
        colors.active_bg
    } else {
        colors.background
    };
    let fg = match &row.item {
        SidebarItem::NewButton => colors.foreground,
        _ if row.is_active => colors.active_fg,
        _ if row.is_open => colors.foreground,
        _ => colors.inactive_fg,
    };
    RowStyle {
        bg,
        fg,
        hover_bg: colors.hover_bg,
        hover_fg: colors.hover_fg,
    }
}

/// Vertical geometry of the sidebar in pixels, derived from the
/// title- and subtitle-font cell heights the element painter lays
/// out with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowGeometry {
    /// Height of one title line.
    pub line: f32,
    /// Height of one subtitle line (smaller font).
    pub sub_line: f32,
    /// Padding above+below the text inside a row.
    pub pad_v: f32,
    /// Gap between rows.
    pub gap: f32,
    /// Padding at the top/bottom edge of the strip (per side).
    pub edge_v: f32,
}

impl RowGeometry {
    pub fn from_cell_heights(cell_height: f32, subtitle_cell_height: f32) -> Self {
        Self {
            line: cell_height,
            sub_line: subtitle_cell_height,
            pad_v: 2. * ROW_PAD_V * cell_height,
            gap: ROW_GAP * cell_height,
            edge_v: EDGE_PAD_V * cell_height,
        }
    }
}

pub fn row_height(subtitle_lines: usize, geom: &RowGeometry) -> f32 {
    geom.pad_v + geom.line + geom.sub_line * subtitle_lines as f32
}

/// How many rows (counting from the front) fit in `available` pixels.
pub fn fitting_rows(rows: &[SidebarRow], available: f32, geom: &RowGeometry) -> usize {
    let mut y = geom.edge_v;
    let mut count = 0;
    for row in rows {
        let h = row_height(row.subtitle_lines.len(), geom);
        if y + h > available - geom.edge_v {
            break;
        }
        y += h + geom.gap;
        count += 1;
    }
    count
}

/// Build the display rows from mux entries; pure so it can be unit
/// tested without a running mux.
pub fn build_rows(active_workspace: &str, entries: &[SidebarEntry]) -> Vec<SidebarRow> {
    let mut rows = vec![SidebarRow {
        item: SidebarItem::NewButton,
        title: "+ New workspace".to_string(),
        subtitle_lines: vec![],
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
            subtitle_lines: entry.subtitle_lines.clone(),
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
                subtitle_lines: vec!["\u{25b8} api".to_string(), "$ npm run dev".to_string()],
            },
            SidebarEntry {
                name: "docs".to_string(),
                tab_count: None,
                subtitle_lines: vec![],
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
        assert_eq!(
            rows[1].subtitle_lines,
            vec!["\u{25b8} api".to_string(), "$ npm run dev".to_string()]
        );

        assert_eq!(rows[2].title, "docs");
        assert!(!rows[2].is_active);
        assert!(!rows[2].is_open);
        assert_eq!(rows[2].subtitle_lines, Vec::<String>::new());
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

    fn test_colors() -> ResolvedColors {
        ResolvedColors {
            background: SrgbaTuple(0.0, 0.0, 0.0, 1.0),
            foreground: SrgbaTuple(0.9, 0.9, 0.9, 1.0),
            active_bg: SrgbaTuple(0.2, 0.4, 0.8, 1.0),
            active_fg: SrgbaTuple(1.0, 1.0, 1.0, 1.0),
            inactive_fg: SrgbaTuple(0.5, 0.5, 0.5, 1.0),
            subtitle_fg: SrgbaTuple(0.4, 0.4, 0.4, 1.0),
            hover_bg: SrgbaTuple(0.1, 0.1, 0.2, 1.0),
            hover_fg: SrgbaTuple(0.95, 0.95, 0.95, 1.0),
            active_indicator: SrgbaTuple(1.0, 1.0, 1.0, 1.0),
            menu_border: SrgbaTuple(0.3, 0.3, 0.3, 1.0),
        }
    }

    fn row(item: SidebarItem, is_active: bool, is_open: bool) -> SidebarRow {
        SidebarRow {
            item,
            title: "ws".to_string(),
            subtitle_lines: vec![],
            is_active,
            is_open,
        }
    }

    #[test]
    fn style_for_active_row_uses_active_colors() {
        let style = style_for_row(&row(SidebarItem::Entry("a".into()), true, true), &test_colors());
        assert_eq!(style.bg, test_colors().active_bg);
        assert_eq!(style.fg, test_colors().active_fg);
    }

    #[test]
    fn style_for_open_row_uses_background_and_foreground() {
        let style = style_for_row(&row(SidebarItem::Entry("a".into()), false, true), &test_colors());
        assert_eq!(style.bg, test_colors().background);
        assert_eq!(style.fg, test_colors().foreground);
    }

    #[test]
    fn style_for_closed_row_uses_inactive_fg() {
        let style = style_for_row(&row(SidebarItem::Entry("a".into()), false, false), &test_colors());
        assert_eq!(style.bg, test_colors().background);
        assert_eq!(style.fg, test_colors().inactive_fg);
    }

    #[test]
    fn style_for_new_button_uses_foreground() {
        let style = style_for_row(&row(SidebarItem::NewButton, false, false), &test_colors());
        assert_eq!(style.bg, test_colors().background);
        assert_eq!(style.fg, test_colors().foreground);
    }

    #[test]
    fn style_always_carries_hover_colors() {
        let colors = test_colors();
        for item in [SidebarItem::NewButton, SidebarItem::Entry("a".into())] {
            for (is_active, is_open) in [(false, false), (true, true)] {
                let style = style_for_row(&row(item.clone(), is_active, is_open), &colors);
                assert_eq!(style.hover_bg, colors.hover_bg);
                assert_eq!(style.hover_fg, colors.hover_fg);
            }
        }
    }

    #[test]
    fn row_height_adds_subtitle_lines() {
        // title cell 10, subtitle cell 8
        let geom = RowGeometry::from_cell_heights(10., 8.);
        assert_eq!(row_height(0, &geom), 10. + 2. * ROW_PAD_V * 10.);
        assert_eq!(row_height(1, &geom), 18. + 2. * ROW_PAD_V * 10.);
        assert_eq!(row_height(3, &geom), 34. + 2. * ROW_PAD_V * 10.);
    }

    #[test]
    fn fitting_rows_stops_at_first_row_that_overflows() {
        // title cell 10, subtitle cell 8: rows are 14, 30 (2 subtitle
        // lines), 14 px tall; gap 2.5, edge 5.
        let geom = RowGeometry::from_cell_heights(10., 8.);
        let rows = build_rows("api", &entries());
        // available 60 fits the first two rows (5+14+2.5+30 = 51.5) but
        // the third would end at 68 > 55.
        assert_eq!(fitting_rows(&rows, 60., &geom), 2);
        assert_eq!(fitting_rows(&rows, 75., &geom), 3);
        assert_eq!(fitting_rows(&rows, 10., &geom), 0);
    }
}
