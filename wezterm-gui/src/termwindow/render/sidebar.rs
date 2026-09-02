use crate::quad::TripleLayerQuadAllocator;
use crate::sidebar::{ResolvedColors, SidebarItem, SidebarState};
use crate::termwindow::render::RenderScreenLineParams;
use crate::termwindow::{UIItem, UIItemType};
use finl_unicode::grapheme_clusters::Graphemes;
use mux::renderable::RenderableDimensions;
use termwiz::cell::{unicode_column_width, CellAttributes, Intensity};
use termwiz::color::{ColorSpec, SrgbaTuple};
use termwiz::surface::SEQ_ZERO;
use wezterm_term::color::ColorAttribute;
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

fn styled_line(text: &str, fg: SrgbaTuple, bg: SrgbaTuple, width: usize, bold: bool) -> Line {
    let mut attrs = CellAttributes::default();
    attrs
        .set_foreground(ColorSpec::TrueColor(fg))
        .set_background(ColorSpec::TrueColor(bg));
    if bold {
        attrs.set_intensity(Intensity::Bold);
    }
    let text = truncate_to_width(text, width);
    let padded = format!("{text:<width$}");
    crate::tabbar::parse_status_text(&padded, attrs)
}

fn row_colors(
    colors: &ResolvedColors,
    item: &SidebarItem,
    is_active: bool,
    is_open: bool,
) -> (SrgbaTuple, SrgbaTuple) {
    match item {
        SidebarItem::NewButton => (colors.foreground, colors.background),
        _ => {
            if is_active {
                (colors.active_fg, colors.active_bg)
            } else if is_open {
                (colors.foreground, colors.background)
            } else {
                (colors.inactive_fg, colors.background)
            }
        }
    }
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
        let default_bg = self
            .palette()
            .resolve_bg(ColorAttribute::Default)
            .to_linear()
            .mul_alpha(if window_is_transparent {
                0.
            } else {
                self.config.text_background_opacity
            });

        for row in &sidebar.rows {
            if y + cell_height > bottom {
                break;
            }
            let (fg, bg) = row_colors(&colors, &row.item, row.is_active, row.is_open);
            let title = styled_line(
                &format!(" {}", row.title),
                fg,
                bg,
                width_cells,
                row.is_active,
            );
            self.paint_chrome_line(
                &title,
                UIItemType::Sidebar(row.item.clone()),
                default_bg,
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
                    colors.background,
                    width_cells,
                    false,
                );
                self.paint_chrome_line(
                    &subtitle,
                    UIItemType::Sidebar(row.item.clone()),
                    default_bg,
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
        while y + cell_height <= bottom {
            self.paint_chrome_line(
                &blank,
                UIItemType::Sidebar(SidebarItem::None),
                default_bg,
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
}
