use crate::quad::TripleLayerQuadAllocator;
use crate::sidebar::{SidebarItem, SidebarState};
use crate::termwindow::render::RenderScreenLineParams;
use crate::termwindow::{UIItem, UIItemType};
use mux::renderable::RenderableDimensions;
use termwiz::cell::CellAttributes;
use termwiz::surface::SEQ_ZERO;
use wezterm_term::color::ColorAttribute;
use wezterm_term::Line;
use window::color::LinearRgba;

impl crate::TermWindow {
    pub fn paint_sidebar(&mut self, layers: &mut TripleLayerQuadAllocator) -> anyhow::Result<()> {
        if self.sidebar.is_none() {
            let palette = self.palette().clone();
            self.sidebar.replace(SidebarState::new(
                &self.config,
                &palette,
                self.config.sidebar_width,
            ));
        }
        let sidebar = self.sidebar.as_ref().unwrap().clone();

        let border = self.get_os_border();
        let palette = self.palette().clone();
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
        let x = border.left.get();
        let bottom = self.dimensions.pixel_height as f32 - border.bottom.get() as f32;

        let window_is_transparent =
            !self.window_background.is_empty() || self.config.window_background_opacity != 1.0;
        let gl_state = self.render_state.as_ref().unwrap();
        let white_space = gl_state.util_sprites.white_space.texture_coords();
        let filled_box = gl_state.util_sprites.filled_box.texture_coords();
        let default_bg = palette
            .resolve_bg(ColorAttribute::Default)
            .to_linear()
            .mul_alpha(if window_is_transparent {
                0.
            } else {
                self.config.text_background_opacity
            });

        let paint_line = |this: &mut Self,
                              line: &Line,
                              item: &SidebarItem,
                              y: f32,
                              layers: &mut TripleLayerQuadAllocator|
         -> anyhow::Result<()> {
            this.render_screen_line(
                RenderScreenLineParams {
                    top_pixel_y: y,
                    left_pixel_x: x as f32,
                    pixel_width: sidebar_width,
                    stable_line_idx: None,
                    line,
                    selection: 0..0,
                    cursor: &Default::default(),
                    palette: &palette,
                    dims: &RenderableDimensions {
                        cols: this.config.sidebar_width,
                        physical_top: 0,
                        scrollback_rows: 0,
                        scrollback_top: 0,
                        viewport_rows: 1,
                        dpi: this.terminal_size.dpi,
                        pixel_height: cell_height as usize,
                        pixel_width: sidebar_width as usize,
                        reverse_video: false,
                    },
                    config: &this.config,
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
                    use_pixel_positioning: this.config.experimental_pixel_positioning,
                    render_metrics: this.render_metrics,
                    shape_key: None,
                    password_input: false,
                },
                layers,
            )?;
            this.ui_items.push(UIItem {
                x: x as usize,
                y: y as usize,
                width: sidebar_width as usize,
                height: cell_height as usize,
                item_type: UIItemType::Sidebar(item.clone()),
            });
            Ok(())
        };

        for row in &sidebar.rows {
            if y + cell_height > bottom {
                break;
            }
            paint_line(self, &row.title, &row.item, y, layers)?;
            y += cell_height;
            if let Some(subtitle) = &row.subtitle {
                if y + cell_height > bottom {
                    break;
                }
                paint_line(self, subtitle, &row.item, y, layers)?;
                y += cell_height;
            }
        }

        // Fill the rest of the strip with the default background so the
        // terminal's padding doesn't peek through below the last row.
        let blank = Line::from_text(
            &" ".repeat(self.config.sidebar_width),
            &CellAttributes::default(),
            SEQ_ZERO,
            None,
        );
        while y + cell_height <= bottom {
            paint_line(self, &blank, &SidebarItem::None, y, layers)?;
            y += cell_height;
        }

        Ok(())
    }
}
