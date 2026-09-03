use crate::sidebar::{
    fitting_rows, style_for_row, RowGeometry, SidebarItem, SidebarState, EDGE_PAD_H, EDGE_PAD_V,
    ROW_GAP, ROW_PAD_H, ROW_PAD_V,
};
use crate::termwindow::box_model::*;
use crate::termwindow::render::corners::*;
use crate::termwindow::UIItemType;
use crate::utilsprites::RenderMetrics;
use config::{Dimension, DimensionContext};

impl crate::TermWindow {
    /// Build the sidebar as a box-model element tree — rounded "pill"
    /// rows shaped in the title font, the same machinery the fancy tab
    /// bar uses — and compute its layout. The result is cached in
    /// `sidebar_element`; `invalidate_sidebar` drops both caches.
    pub fn build_sidebar_element(&self) -> anyhow::Result<ComputedElement> {
        let font = self.fonts.title_font()?;
        let metrics = RenderMetrics::with_font_metrics(&font.metrics());
        let cell_h = metrics.cell_size.height as f32;
        let cell_w = metrics.cell_size.width as f32;

        let sidebar = self.sidebar.as_ref().unwrap();
        let colors = sidebar.colors;

        let border = self.get_os_border();
        let sidebar_width = self.sidebar_pixel_width();
        // Start below the tab bar when it is at the top, so the two
        // never overlap.
        let top = border.top.get() as f32
            + if self.show_tab_bar && !self.config.tab_bar_at_bottom {
                self.tab_bar_pixel_height()?
            } else {
                0.
            };
        let bottom = self.dimensions.pixel_height as f32 - border.bottom.get() as f32;
        let available = (bottom - top).max(0.);

        let geom = RowGeometry::from_cell_height(cell_h);
        let fit = fitting_rows(&sidebar.rows, available, &geom);

        let window_is_transparent =
            !self.window_background.is_empty() || self.config.window_background_opacity != 1.0;
        let bg_alpha = if window_is_transparent {
            0.
        } else {
            self.config.text_background_opacity
        };

        // Pill rows span the full width between the container's
        // horizontal padding: strip width minus edge padding, row
        // padding and the 1px border on each side.
        let row_min_width =
            (sidebar_width - 2. * (EDGE_PAD_H + ROW_PAD_H) * cell_w - 2.).max(0.);

        let corner = |poly: &'static [crate::customglyph::Poly]| SizedPoly {
            width: Dimension::Cells(0.5),
            height: Dimension::Cells(0.5),
            poly,
        };

        let mut row_eles = vec![];
        for row in sidebar.rows.iter().take(fit) {
            let style = style_for_row(row, &colors);
            let bg = style.bg.to_linear().mul_alpha(bg_alpha);
            let hover_bg = style.hover_bg.to_linear().mul_alpha(bg_alpha);

            let mut kids = vec![
                Element::new(&font, ElementContent::Text(row.title.clone()))
                    .display(DisplayType::Block),
            ];
            if let Some(subtitle) = &row.subtitle {
                kids.push(
                    Element::new(&font, ElementContent::Text(subtitle.clone()))
                        .display(DisplayType::Block)
                        .colors(ElementColors {
                            text: colors.subtitle_fg.to_linear().into(),
                            ..ElementColors::default()
                        }),
                );
            }

            row_eles.push(
                Element::new(&font, ElementContent::Children(kids))
                    .display(DisplayType::Block)
                    .item_type(UIItemType::Sidebar(row.item.clone()))
                    .min_width(Some(Dimension::Pixels(row_min_width)))
                    .margin(BoxDimension {
                        left: Dimension::Cells(0.),
                        right: Dimension::Cells(0.),
                        top: Dimension::Cells(0.),
                        bottom: Dimension::Cells(ROW_GAP),
                    })
                    .padding(BoxDimension {
                        left: Dimension::Cells(ROW_PAD_H),
                        right: Dimension::Cells(ROW_PAD_H),
                        top: Dimension::Cells(ROW_PAD_V),
                        bottom: Dimension::Cells(ROW_PAD_V),
                    })
                    // The border doubles as the corner fill color, so it
                    // must match the background (same trick as tabs).
                    .border(BoxDimension::new(Dimension::Pixels(1.)))
                    .border_corners(Some(Corners {
                        top_left: corner(TOP_LEFT_ROUNDED_CORNER),
                        top_right: corner(TOP_RIGHT_ROUNDED_CORNER),
                        bottom_left: corner(BOTTOM_LEFT_ROUNDED_CORNER),
                        bottom_right: corner(BOTTOM_RIGHT_ROUNDED_CORNER),
                    }))
                    .colors(ElementColors {
                        border: BorderColor::new(bg),
                        bg: bg.into(),
                        text: style.fg.to_linear().into(),
                    })
                    .hover_colors(Some(ElementColors {
                        border: BorderColor::new(hover_bg),
                        bg: hover_bg.into(),
                        text: style.hover_fg.to_linear().into(),
                    })),
            );
        }

        let container = Element::new(&font, ElementContent::Children(row_eles))
            .display(DisplayType::Block)
            // Clicks on the strip between/below rows must not fall
            // through to the terminal.
            .item_type(UIItemType::Sidebar(SidebarItem::None))
            .min_width(Some(Dimension::Pixels(sidebar_width)))
            .min_height(Some(Dimension::Pixels(available)))
            .padding(BoxDimension {
                left: Dimension::Cells(EDGE_PAD_H),
                right: Dimension::Cells(EDGE_PAD_H),
                top: Dimension::Cells(EDGE_PAD_V),
                bottom: Dimension::Cells(EDGE_PAD_V),
            })
            .colors(ElementColors {
                bg: colors.background.to_linear().mul_alpha(bg_alpha).into(),
                ..ElementColors::default()
            });

        let mut computed = self.compute_element(
            &LayoutContext {
                height: DimensionContext {
                    dpi: self.dimensions.dpi as f32,
                    pixel_max: self.dimensions.pixel_height as f32,
                    pixel_cell: cell_h,
                },
                width: DimensionContext {
                    dpi: self.dimensions.dpi as f32,
                    pixel_max: self.dimensions.pixel_width as f32,
                    pixel_cell: cell_w,
                },
                bounds: euclid::rect(0., 0., sidebar_width, available),
                metrics: &metrics,
                gl_state: self.render_state.as_ref().unwrap(),
                // Chrome layer, like the fancy tab bar; above the
                // panes, below modals (100). The sidebar menu paints
                // into the same layer after the sidebar, so its quads
                // land on top.
                zindex: 10,
            },
            &container,
        )?;

        computed.translate(euclid::vec2(border.left.get() as f32, top));
        Ok(computed)
    }

    pub fn paint_sidebar(&mut self) -> anyhow::Result<()> {
        if self.sidebar.is_none() {
            let palette = self.palette().clone();
            self.sidebar
                .replace(SidebarState::new(&self.config, &palette));
        }
        if self.sidebar_element.is_none() {
            let element = self.build_sidebar_element()?;
            self.sidebar_element.replace(element);
        }

        let mut items = self.sidebar_element.as_ref().unwrap().ui_items();
        self.ui_items.append(&mut items);

        let gl_state = self.render_state.as_ref().unwrap();
        let computed = self.sidebar_element.as_ref().unwrap();
        self.render_element(computed, gl_state, None)?;
        Ok(())
    }

    /// Paint the right-click context menu as a box-model element tree
    /// in the sidebar's visual language: a rounded card holding one
    /// pill row per action, with hover driven by `hover_colors`.
    /// Rebuilt each paint; menus are transient and the glyph caches
    /// make this cheap, so there's no element cache to invalidate.
    pub fn paint_sidebar_menu(&mut self) -> anyhow::Result<()> {
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

        let font = self.fonts.title_font()?;
        let metrics = RenderMetrics::with_font_metrics(&font.metrics());
        let cell_w = metrics.cell_size.width as f32;
        let win_w = self.dimensions.pixel_width as f32;
        let win_h = self.dimensions.pixel_height as f32;

        fn layout_context<'a>(
            term_window: &'a crate::TermWindow,
            metrics: &'a RenderMetrics,
            win_w: f32,
            win_h: f32,
            cell_w: f32,
            zindex: i8,
        ) -> LayoutContext<'a> {
            LayoutContext {
                height: DimensionContext {
                    dpi: term_window.dimensions.dpi as f32,
                    pixel_max: win_h,
                    pixel_cell: metrics.cell_size.height as f32,
                },
                width: DimensionContext {
                    dpi: term_window.dimensions.dpi as f32,
                    pixel_max: win_w,
                    pixel_cell: cell_w,
                },
                bounds: euclid::rect(0., 0., win_w, win_h),
                metrics,
                gl_state: term_window.render_state.as_ref().unwrap(),
                zindex,
            }
        }

        // Probe-measure each label so every row floors at exactly the
        // widest label's shaped width (cell-width estimates inflate
        // badly for the proportional title font).
        let mut row_min_width: f32 = 0.;
        for (_, label) in crate::sidebar_menu::MENU_ITEMS {
            let probe = Element::new(&font, ElementContent::Text(label.to_string()));
            let w = self
                .compute_element(&layout_context(self, &metrics, win_w, win_h, cell_w, 10), &probe)?
                .bounds
                .width();
            row_min_width = row_min_width.max(w);
        }

        let corner = |poly: &'static [crate::customglyph::Poly]| SizedPoly {
            width: Dimension::Cells(0.5),
            height: Dimension::Cells(0.5),
            poly,
        };
        let rounded = || Corners {
            top_left: corner(TOP_LEFT_ROUNDED_CORNER),
            top_right: corner(TOP_RIGHT_ROUNDED_CORNER),
            bottom_left: corner(BOTTOM_LEFT_ROUNDED_CORNER),
            bottom_right: corner(BOTTOM_RIGHT_ROUNDED_CORNER),
        };

        let bg = colors.background.to_linear();
        let mut row_eles = vec![];
        for (idx, (_, label)) in crate::sidebar_menu::MENU_ITEMS.iter().enumerate() {
            row_eles.push(
                Element::new(&font, ElementContent::Text(label.to_string()))
                    .display(DisplayType::Block)
                    .item_type(UIItemType::SidebarMenuItem(idx))
                    .min_width(Some(Dimension::Pixels(row_min_width)))
                    .margin(BoxDimension {
                        left: Dimension::Cells(0.),
                        right: Dimension::Cells(0.),
                        top: Dimension::Cells(0.),
                        bottom: Dimension::Cells(ROW_GAP),
                    })
                    .padding(BoxDimension {
                        left: Dimension::Cells(ROW_PAD_H),
                        right: Dimension::Cells(ROW_PAD_H),
                        top: Dimension::Cells(ROW_PAD_V),
                        bottom: Dimension::Cells(ROW_PAD_V),
                    })
                    // The border doubles as the corner fill color, so
                    // it must match the background (same trick as the
                    // sidebar rows).
                    .border(BoxDimension::new(Dimension::Pixels(1.)))
                    .border_corners(Some(rounded()))
                    .colors(ElementColors {
                        border: BorderColor::new(bg),
                        bg: bg.into(),
                        text: colors.foreground.to_linear().into(),
                    })
                    .hover_colors(Some(ElementColors {
                        border: BorderColor::new(colors.hover_bg.to_linear()),
                        bg: colors.hover_bg.to_linear().into(),
                        text: colors.hover_fg.to_linear().into(),
                    })),
            );
        }

        let card = Element::new(&font, ElementContent::Children(row_eles))
            .display(DisplayType::Block)
            // Clicks on the menu's padding must neither dispatch an
            // action nor count as click-outside-to-dismiss.
            .item_type(UIItemType::SidebarMenuChrome)
            .padding(BoxDimension {
                left: Dimension::Cells(EDGE_PAD_H),
                right: Dimension::Cells(EDGE_PAD_H),
                top: Dimension::Cells(EDGE_PAD_V),
                bottom: Dimension::Cells(EDGE_PAD_V),
            })
            .border(BoxDimension::new(Dimension::Pixels(1.)))
            .border_corners(Some(rounded()))
            .colors(ElementColors {
                border: BorderColor::new(colors.menu_border.to_linear()),
                // Opaque so the pane below doesn't bleed through.
                bg: bg.into(),
                text: colors.foreground.to_linear().into(),
            });

        // Lay out at the origin, then clamp the anchor so the menu
        // stays fully inside the window (spec §3). Chrome layer, same
        // as the sidebar, which paints into it first — the menu lands
        // on top.
        let mut computed = self.compute_element(
            &layout_context(self, &metrics, win_w, win_h, cell_w, 10),
            &card,
        )?;

        let menu_w = computed.bounds.width();
        let menu_h = computed.bounds.height();
        let mx = menu.x.clamp(0., (win_w - menu_w).max(0.));
        let my = menu.y.clamp(0., (win_h - menu_h).max(0.));
        computed.translate(euclid::vec2(
            mx - computed.bounds.min_x(),
            my - computed.bounds.min_y(),
        ));

        let mut items = computed.ui_items();
        self.ui_items.append(&mut items);

        let gl_state = self.render_state.as_ref().unwrap();
        self.render_element(&computed, gl_state, None)?;
        Ok(())
    }
}
