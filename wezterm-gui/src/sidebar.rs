use config::{ConfigHandle, SidebarColors};
use finl_unicode::grapheme_clusters::Graphemes;
use mux::Mux;
use termwiz::cell::{unicode_column_width, CellAttributes, Intensity};
use termwiz::color::ColorSpec;
use wezterm_term::color::ColorPalette;
use wezterm_term::Line;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarItem {
    None,
    Entry(String),
    NewButton,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SidebarRow {
    pub item: SidebarItem,
    pub title: Line,
    pub subtitle: Option<Line>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SidebarState {
    pub rows: Vec<SidebarRow>,
}

#[derive(Clone, Copy)]
struct ResolvedColors {
    background: ColorSpec,
    foreground: ColorSpec,
    active_bg: ColorSpec,
    active_fg: ColorSpec,
    inactive_fg: ColorSpec,
    subtitle_fg: ColorSpec,
}

fn color_spec(rgb: config::RgbaColor) -> ColorSpec {
    ColorSpec::TrueColor(rgb.into())
}

fn resolve_colors(config: &ConfigHandle, palette: &ColorPalette) -> ResolvedColors {
    let empty = SidebarColors::default();
    let sc = config
        .resolved_palette
        .sidebar
        .as_ref()
        .unwrap_or(&empty);

    let background = sc
        .background
        .map(color_spec)
        .unwrap_or(ColorSpec::Default);
    let foreground = sc
        .foreground
        .map(color_spec)
        .unwrap_or(ColorSpec::Default);
    let (active_bg, active_fg) = match &sc.active {
        Some(active) => (color_spec(active.bg_color), color_spec(active.fg_color)),
        None => (ColorSpec::Default, ColorSpec::TrueColor(palette.foreground)),
    };
    let inactive_fg = sc
        .inactive_foreground
        .map(color_spec)
        .unwrap_or(ColorSpec::PaletteIndex(8));
    let subtitle_fg = sc
        .subtitle_foreground
        .map(color_spec)
        .unwrap_or(ColorSpec::PaletteIndex(8));

    ResolvedColors {
        background,
        foreground,
        active_bg,
        active_fg,
        inactive_fg,
        subtitle_fg,
    }
}

fn truncate_to_width(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for g in Graphemes::new(text) {
        let w = unicode_column_width(g, None).max(1);
        if used + w > width {
            break;
        }
        out.push_str(g);
        used += w;
    }
    out
}

fn styled_line(text: &str, fg: ColorSpec, bg: ColorSpec, width: usize, bold: bool) -> Line {
    let mut attrs = CellAttributes::default();
    attrs.set_foreground(fg).set_background(bg);
    if bold {
        attrs.set_intensity(Intensity::Bold);
    }
    let text = truncate_to_width(text, width);
    let padded = format!("{text:<width$}");
    crate::tabbar::parse_status_text(&padded, attrs)
}

impl SidebarState {
    pub fn new(config: &ConfigHandle, palette: &ColorPalette, width_cells: usize) -> Self {
        let mux = Mux::get();
        let active = mux.active_workspace();
        let colors = resolve_colors(config, palette);

        let mut rows = vec![SidebarRow {
            item: SidebarItem::NewButton,
            title: styled_line(
                " + New workspace",
                colors.foreground,
                colors.background,
                width_cells,
                false,
            ),
            subtitle: None,
        }];

        for entry in mux.compute_sidebar_entries() {
            let is_active = entry.name == active;
            let is_open = entry.tab_count.is_some();
            let (fg, bg) = if is_active {
                (colors.active_fg, colors.active_bg)
            } else if is_open {
                (colors.foreground, colors.background)
            } else {
                (colors.inactive_fg, colors.background)
            };
            let badge = entry
                .tab_count
                .map(|n| format!(" ({n})"))
                .unwrap_or_default();
            rows.push(SidebarRow {
                item: SidebarItem::Entry(entry.name.clone()),
                title: styled_line(
                    &format!(" {}{badge}", entry.name),
                    fg,
                    bg,
                    width_cells,
                    is_active,
                ),
                subtitle: entry.subtitle.map(|s| {
                    styled_line(
                        &format!("   {s}"),
                        colors.subtitle_fg,
                        colors.background,
                        width_cells,
                        false,
                    )
                }),
            });
        }

        Self { rows }
    }
}
