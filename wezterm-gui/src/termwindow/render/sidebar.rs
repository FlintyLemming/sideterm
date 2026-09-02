use crate::quad::TripleLayerQuadAllocator;
use crate::sidebar::{SidebarItem, SidebarState};
use crate::termwindow::render::RenderScreenLineParams;
use crate::termwindow::{UIItem, UIItemType};
use finl_unicode::grapheme_clusters::Graphemes;
use mux::renderable::RenderableDimensions;
use termwiz::cell::{unicode_column_width, CellAttributes, Intensity};
use termwiz::color::{ColorSpec, SrgbaTuple};
use termwiz::surface::SEQ_ZERO;
use wezterm_term::Line;
use window::color::LinearRgba;

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

fn styled_line(text: &str, fg: SrgbaTuple, width: usize, bold: bool) -> Line {
    let mut attrs = CellAttributes::default();
    attrs.set_foreground(ColorSpec::TrueColor(fg));
    if bold {
        attrs.set_intensity(Intensity::Bold);
    }
    let text = truncate_to_width(text, width);
    let padded = format!("{text:<width$}");
    crate::tabbar::parse_status_text(&padded, attrs)
}

impl crate::TermWindow {
    /// Render one line of sidebar/menu chrome: a single-row
    /// render_screen_line plus its UIItem hit rect.
    #[allow(clippy::too_many_arguments)]
    fn paint_chrome_line(
        &mut self,
        line: &Line,
        item_type: UIItemType,
        default_bg: LinearRgba,
        left: f32,
        top: f32,
        pixel_width: f32,
        cols: usize,
        layers: &mut TripleLayerQuadAllocator,
    ) -> anyhow::Result<()> {
        let palette = self.palette().clone();
        let cell_height = self.render_metrics.cell_size.height as f32;
        let window_is_transparent =
            !self.window_background.is_empty() || self.config.window_background_opacity != 1.0;
        let gl_state = self.render_state.as_ref().unwrap();
        let white_space = gl_state.util_sprites.white_space.texture_coords();
        let filled_box = gl_state.util_sprites.filled_box.texture_coords();
        self.render_screen_line(
            RenderScreenLineParams {
                top_pixel_y: top,
                left_pixel_x: left,
                pixel_width,
                stable_line_idx: None,
                line,
                selection: 0..0,
                cursor: &Default::default(),
                palette: &palette,
                dims: &RenderableDimensions {
                    cols,
                    physical_top: 0,
                    scrollback_rows: 0,
                    scrollback_top: 0,
                    viewport_rows: 1,
                    dpi: self.terminal_size.dpi,
                    pixel_height: cell_height as usize,
                    pixel_width: pixel_width as usize,
                    reverse_video: false,
                },
                config: &self.config,
                cursor_border_color: LinearRgba::default(),
                foreground: palette.foreground.to_linear(),
                pane: None,
                is_active: true,
                selection_fg: LinearRgba::default(),
                selection_bg: LinearRgba::default(),
                cursor_fg: LinearRgba::default(),
                cursor_bg: LinearRgba::default(),
                cursor_is_default_color: true,
                white_space,
                filled_box,
                window_is_transparent,
                default_bg,
                style: None,
                font: None,
                use_pixel_positioning: self.config.experimental_pixel_positioning,
                render_metrics: self.render_metrics,
                shape_key: None,
                password_input: false,
            },
            layers,
        )?;
        self.ui_items.push(UIItem {
            x: left as usize,
            y: top as usize,
            width: pixel_width as usize,
            height: cell_height as usize,
            item_type,
        });
        Ok(())
    }

    pub fn paint_sidebar(&mut self, layers: &mut TripleLayerQuadAllocator) -> anyhow::Result<()> {
        if self.sidebar.is_none() {
            let palette = self.palette().clone();
            self.sidebar
                .replace(SidebarState::new(&self.config, &palette));
        }
        let sidebar = self.sidebar.as_ref().unwrap().clone();
        let colors = sidebar.colors;

        let border = self.get_os_border();
        let sidebar_width = self.sidebar_pixel_width();
        let cell_height = self.render_metrics.cell_size.height as f32;
        // Start below the tab bar when it is at the top, so the two
        // never overlap.
        let mut y = border.top.get() as f32
            + if self.show_tab_bar && !self.config.tab_bar_at_bottom {
                self.tab_bar_pixel_height()?
            } else {
                0.
            };
        let x = border.left.get() as f32;
        let bottom = self.dimensions.pixel_height as f32 - border.bottom.get() as f32;
        let width_cells = self.config.sidebar_width;

        let window_is_transparent =
            !self.window_background.is_empty() || self.config.window_background_opacity != 1.0;
        let bg_alpha = if window_is_transparent {
            0.
        } else {
            self.config.text_background_opacity
        };

        for row in &sidebar.rows {
            if y + cell_height > bottom {
                break;
            }
            let hovered =
                row.item != SidebarItem::None && self.sidebar_hover.as_ref() == Some(&row.item);
            let bg_srgb = if hovered {
                colors.hover_bg
            } else if row.is_active {
                colors.active_bg
            } else {
                colors.background
            };
            let fg_srgb = if hovered {
                colors.hover_fg
            } else {
                match &row.item {
                    SidebarItem::NewButton => colors.foreground,
                    _ => {
                        if row.is_active {
                            colors.active_fg
                        } else if row.is_open {
                            colors.foreground
                        } else {
                            colors.inactive_fg
                        }
                    }
                }
            };
            let bg = bg_srgb.to_linear().mul_alpha(bg_alpha);

            let row_y = y;
            let row_height = cell_height * if row.subtitle.is_some() { 2. } else { 1. };
            let visible_height = row_height.min(bottom - row_y).max(0.);

            // Full-row background block (spans the subtitle line too)
            self.filled_rectangle(
                layers,
                0,
                euclid::rect(x, row_y, sidebar_width, visible_height),
                bg,
            )?;
            // Active indicator: 3px bar along the left edge. The 1-cell
            // text padding below keeps glyphs clear of it.
            if row.is_active {
                self.filled_rectangle(
                    layers,
                    0,
                    euclid::rect(x, row_y, 3., visible_height),
                    colors.active_indicator.to_linear(),
                )?;
            }

            let title = styled_line(
                &format!(" {}", row.title),
                fg_srgb,
                width_cells,
                row.is_active,
            );
            self.paint_chrome_line(
                &title,
                UIItemType::Sidebar(row.item.clone()),
                bg,
                x,
                y,
                sidebar_width,
                width_cells,
                layers,
            )?;
            y += cell_height;
            if let Some(subtitle) = &row.subtitle {
                if y + cell_height > bottom {
                    break;
                }
                let subtitle = styled_line(
                    &format!("   {subtitle}"),
                    colors.subtitle_fg,
                    width_cells,
                    false,
                );
                self.paint_chrome_line(
                    &subtitle,
                    UIItemType::Sidebar(row.item.clone()),
                    bg,
                    x,
                    y,
                    sidebar_width,
                    width_cells,
                    layers,
                )?;
                y += cell_height;
            }
        }

        // Fill the rest of the strip with the default background so the
        // terminal's padding doesn't peek through below the last row.
        let blank = Line::from_text(
            &" ".repeat(width_cells),
            &CellAttributes::default(),
            SEQ_ZERO,
            None,
        );
        let blank_bg = colors.background.to_linear().mul_alpha(bg_alpha);
        while y + cell_height <= bottom {
            self.paint_chrome_line(
                &blank,
                UIItemType::Sidebar(SidebarItem::None),
                blank_bg,
                x,
                y,
                sidebar_width,
                width_cells,
                layers,
            )?;
            y += cell_height;
        }

        Ok(())
    }

    pub fn paint_sidebar_menu(
        &mut self,
        layers: &mut TripleLayerQuadAllocator,
    ) -> anyhow::Result<()> {
        let menu = match self.sidebar_menu.clone() {
            Some(menu) => menu,
            None => return Ok(()),
        };

        // The target workspace may have been renamed or removed while
        // the menu is open; acting on a ghost entry would be confusing,
        // so close instead (spec §5).
        let still_listed = mux::Mux::get()
            .compute_sidebar_entries()
            .iter()
            .any(|entry| entry.name == menu.workspace);
        if !still_listed {
            self.close_sidebar_menu();
            return Ok(());
        }

        let colors = match self.sidebar.as_ref() {
            Some(sidebar) => sidebar.colors,
            None => return Ok(()),
        };

        let cell_width = self.render_metrics.cell_size.width as f32;
        let cell_height = self.render_metrics.cell_size.height as f32;

        // Menu colors inherit the sidebar's background/foreground/hover
        // (spec §2); the border gets its own color.
        let label_cells = crate::sidebar_menu::MENU_ITEMS
            .iter()
            .map(|(_, label)| unicode_column_width(label, None))
            .max()
            .unwrap_or(0);
        let menu_cols = label_cells + 2; // one cell of padding on each side
        let menu_width = menu_cols as f32 * cell_width;
        let menu_height = crate::sidebar_menu::MENU_ITEMS.len() as f32 * cell_height;

        // Keep the menu fully inside the window: clamp the anchor when
        // it would overflow the right/bottom edge (spec §3).
        let max_x = (self.dimensions.pixel_width as f32 - menu_width).max(0.);
        let max_y = (self.dimensions.pixel_height as f32 - menu_height).max(0.);
        let mx = menu.x.clamp(0., max_x);
        let my = menu.y.clamp(0., max_y);

        // 1px border plus an opaque body so the pane below doesn't
        // bleed through.
        self.filled_rectangle(
            layers,
            0,
            euclid::rect(mx - 1., my - 1., menu_width + 2., menu_height + 2.),
            colors.menu_border.to_linear(),
        )?;
        self.filled_rectangle(
            layers,
            0,
            euclid::rect(mx, my, menu_width, menu_height),
            colors.background.to_linear(),
        )?;

        if let Some(hovered) = menu.hovered {
            self.filled_rectangle(
                layers,
                0,
                euclid::rect(
                    mx,
                    my + hovered as f32 * cell_height,
                    menu_width,
                    cell_height,
                ),
                colors.hover_bg.to_linear(),
            )?;
        }

        for (idx, (_, label)) in crate::sidebar_menu::MENU_ITEMS.iter().enumerate() {
            let top = my + idx as f32 * cell_height;
            let is_hovered = menu.hovered == Some(idx);
            let fg = if is_hovered {
                colors.hover_fg
            } else {
                colors.foreground
            };
            let default_bg = if is_hovered {
                colors.hover_bg.to_linear()
            } else {
                colors.background.to_linear()
            };
            let line = styled_line(&format!(" {label} "), fg, menu_cols, false);
            self.paint_chrome_line(
                &line,
                UIItemType::SidebarMenuItem(idx),
                default_bg,
                mx,
                top,
                menu_width,
                menu_cols,
                layers,
            )?;
        }
        Ok(())
    }
}
