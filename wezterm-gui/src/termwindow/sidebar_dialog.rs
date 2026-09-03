//! Anchored GUI dialogs for the workspace sidebar, in the same
//! box-model visual language as the sidebar itself (rounded card,
//! title font, sidebar colors). Two kinds: a single-line `Prompt`
//! (rename / set default cwd / set default command) and a `Confirm`
//! (remove from sidebar).
//!
//! Implemented as a `Modal`: the existing modal plumbing gives us
//! keyboard capture (keyevent.rs), zindex-100 rendering (paint.rs
//! paint_modal) and resize/config reconfigure for free; mouse capture
//! is opted into via `Modal::captures_mouse` and dispatched from
//! mouseevent.rs.

use crate::sidebar::ResolvedColors;
use crate::termwindow::box_model::*;
use crate::termwindow::modal::Modal;
use crate::termwindow::render::corners::{
    BOTTOM_LEFT_ROUNDED_CORNER, BOTTOM_RIGHT_ROUNDED_CORNER, TOP_LEFT_ROUNDED_CORNER,
    TOP_RIGHT_ROUNDED_CORNER,
};
use crate::termwindow::{DimensionContext, TermWindow, TermWindowNotif, UIItemType};
use crate::utilsprites::RenderMetrics;
use config::Dimension;
use finl_unicode::grapheme_clusters::Graphemes;
use std::cell::{Ref, RefCell};
use wezterm_term::{KeyCode, KeyModifiers};
use window::color::LinearRgba;
use window::{Clipboard, MouseEventKind as WMEK, MousePress, WindowOps};

/// Wrap descriptions/messages at roughly this many columns so they
/// stay within the card's width floor.
const WRAP_COLS: usize = 32;
/// Horizontal padding of the card, in title-font cells.
const CARD_PAD_H_CELLS: f32 = 1.;
/// Floor for the card's *content* width, in title-font cells.
/// (Terminal cells are much narrower than the title font is wide.)
const CARD_CONTENT_TITLE_CELLS: f32 = 20.;

/// Which dialog button a `UIItemType::SidebarDialogButton` refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogButton {
    Cancel,
    Confirm,
}

/// Single-line editing buffer with a grapheme-aware cursor. Pure so
/// the editing rules can be unit tested without a GUI.
#[derive(Default, Debug)]
pub struct LineInput {
    text: String,
    /// Byte index of the cursor; always on a grapheme boundary.
    cursor: usize,
}

fn prev_boundary(text: &str, cursor: usize) -> usize {
    Graphemes::new(&text[..cursor])
        .last()
        .map(|g| cursor - g.len())
        .unwrap_or(0)
}

fn next_boundary(text: &str, cursor: usize) -> usize {
    Graphemes::new(&text[cursor..])
        .next()
        .map(|g| cursor + g.len())
        .unwrap_or(cursor)
}

impl LineInput {
    pub fn new(text: String) -> Self {
        let cursor = text.len();
        Self { text, cursor }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Text before/after the cursor, for the two text runs flanking
    /// the caret element.
    pub fn before(&self) -> &str {
        &self.text[..self.cursor]
    }
    pub fn after(&self) -> &str {
        &self.text[self.cursor..]
    }

    pub fn insert(&mut self, s: &str) {
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    pub fn backspace(&mut self) {
        let start = prev_boundary(&self.text, self.cursor);
        self.text.drain(start..self.cursor);
        self.cursor = start;
    }

    pub fn delete(&mut self) {
        let end = next_boundary(&self.text, self.cursor);
        self.text.drain(self.cursor..end);
    }

    pub fn move_left(&mut self) {
        self.cursor = prev_boundary(&self.text, self.cursor);
    }

    pub fn move_right(&mut self) {
        self.cursor = next_boundary(&self.text, self.cursor);
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.text.len();
    }
}

enum DialogKind {
    Prompt(RefCell<LineInput>),
    Confirm,
}

enum Completion {
    Prompt(Box<dyn FnOnce(&mut TermWindow, Option<String>)>),
    Confirm(Box<dyn FnOnce(&mut TermWindow, bool)>),
}

pub struct SidebarDialog {
    kind: DialogKind,
    title: String,
    /// Pre-wrapped description lines.
    description: Vec<String>,
    confirm_label: String,
    /// Where the card's top-left wants to be (the context menu's
    /// anchor), in window pixels; clamped into the window at compute.
    anchor: (f32, f32),
    colors: RefCell<ResolvedColors>,
    element: RefCell<Option<Vec<ComputedElement>>>,
    completion: RefCell<Option<Completion>>,
    /// Last ui_item under the mouse, so hover repaints only happen on
    /// changes (hover_colors reads current_mouse_event at render).
    last_mouse_hit: RefCell<Option<UIItemType>>,
}

fn wrap_description(text: &str) -> Vec<String> {
    text.split('\n')
        .flat_map(|line| {
            textwrap::fill(line, WRAP_COLS)
                .split('\n')
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        })
        .collect()
}

impl SidebarDialog {
    /// A single-line input dialog. `on_submit` receives `Some(text)`
    /// on Enter/confirm (possibly empty) and `None` on cancel.
    pub fn prompt<F>(
        title: String,
        description: String,
        initial_value: Option<String>,
        confirm_label: String,
        anchor: (f32, f32),
        colors: ResolvedColors,
        on_submit: F,
    ) -> Self
    where
        F: FnOnce(&mut TermWindow, Option<String>) + 'static,
    {
        Self {
            kind: DialogKind::Prompt(RefCell::new(LineInput::new(
                initial_value.unwrap_or_default(),
            ))),
            title,
            description: wrap_description(&description),
            confirm_label,
            anchor,
            colors: RefCell::new(colors),
            element: RefCell::new(None),
            completion: RefCell::new(Some(Completion::Prompt(Box::new(on_submit)))),
            last_mouse_hit: RefCell::new(None),
        }
    }

    /// A yes/no confirmation. `on_result` receives true on confirm,
    /// false on cancel.
    pub fn confirm<F>(
        title: String,
        message: String,
        confirm_label: String,
        anchor: (f32, f32),
        colors: ResolvedColors,
        on_result: F,
    ) -> Self
    where
        F: FnOnce(&mut TermWindow, bool) + 'static,
    {
        Self {
            kind: DialogKind::Confirm,
            title,
            description: wrap_description(&message),
            confirm_label,
            anchor,
            colors: RefCell::new(colors),
            element: RefCell::new(None),
            completion: RefCell::new(Some(Completion::Confirm(Box::new(on_result)))),
            last_mouse_hit: RefCell::new(None),
        }
    }

    /// Run the completion callback exactly once and close the dialog.
    fn finish(&self, term_window: &mut TermWindow, accepted: bool) {
        if let Some(completion) = self.completion.borrow_mut().take() {
            match completion {
                Completion::Prompt(callback) => {
                    let value = if accepted {
                        match &self.kind {
                            DialogKind::Prompt(input) => {
                                Some(input.borrow().text().to_string())
                            }
                            DialogKind::Confirm => None,
                        }
                    } else {
                        None
                    };
                    callback(term_window, value);
                }
                Completion::Confirm(callback) => callback(term_window, accepted),
            }
        }
        term_window.cancel_modal();
    }

    /// Insert clipboard text (newlines flattened to spaces) at the
    /// cursor. Called from the clipboard future's Apply notification.
    fn insert_pasted(&self, clip: &str) {
        if let DialogKind::Prompt(input) = &self.kind {
            let single_line: String = clip
                .chars()
                .map(|c| if c == '\r' || c == '\n' { ' ' } else { c })
                .collect();
            input.borrow_mut().insert(&single_line);
        }
    }

    fn paste_clipboard(&self, term_window: &mut TermWindow) {
        let window = match term_window.window.as_ref() {
            Some(window) => window.clone(),
            None => return,
        };
        let future = window.get_clipboard(Clipboard::Clipboard);
        promise::spawn::spawn(async move {
            if let Ok(clip) = future.await {
                window.notify(TermWindowNotif::Apply(Box::new(move |term_window| {
                    if let Some(modal) = term_window.get_modal() {
                        if let Some(dialog) = modal.downcast_ref::<SidebarDialog>() {
                            dialog.insert_pasted(&clip);
                            term_window.invalidate_modal();
                        }
                    }
                })));
            }
        })
        .detach();
    }

    fn build(&self, term_window: &mut TermWindow) -> anyhow::Result<ComputedElement> {
        let font = term_window.fonts.title_font()?;
        let metrics = RenderMetrics::with_font_metrics(&font.metrics());
        let colors = *self.colors.borrow();
        let content_w = CARD_CONTENT_TITLE_CELLS * metrics.cell_size.width as f32;

        let corner = |poly: &'static [crate::customglyph::Poly]| SizedPoly {
            width: Dimension::Cells(0.35),
            height: Dimension::Cells(0.35),
            poly,
        };
        let rounded = || Corners {
            top_left: corner(TOP_LEFT_ROUNDED_CORNER),
            top_right: corner(TOP_RIGHT_ROUNDED_CORNER),
            bottom_left: corner(BOTTOM_LEFT_ROUNDED_CORNER),
            bottom_right: corner(BOTTOM_RIGHT_ROUNDED_CORNER),
        };

        let text_only = |fg: LinearRgba| ElementColors {
            border: BorderColor::default(),
            bg: LinearRgba::TRANSPARENT.into(),
            text: fg.into(),
        };

        let mut kids = vec![];
        kids.push(
            Element::new(&font, ElementContent::Text(self.title.clone()))
                .display(DisplayType::Block)
                .margin(BoxDimension {
                    bottom: Dimension::Cells(0.25),
                    ..BoxDimension::default()
                })
                .colors(text_only(colors.foreground.to_linear())),
        );
        for line in &self.description {
            kids.push(
                Element::new(&font, ElementContent::Text(line.clone()))
                    .display(DisplayType::Block)
                    .colors(text_only(colors.subtitle_fg.to_linear())),
            );
        }

        if let DialogKind::Prompt(input) = &self.kind {
            let input = input.borrow();
            // The caret is an empty fixed-size box between the two
            // text runs: inline layout positions it exactly where the
            // shaped text ends, so no manual text measurement is
            // needed and proportional fonts just work.
            let caret = Element::new(&font, ElementContent::Children(vec![]))
                .vertical_align(VerticalAlign::Middle)
                .min_width(Some(Dimension::Pixels(2.)))
                .min_height(Some(Dimension::Cells(0.8)))
                .colors(ElementColors {
                    bg: colors.foreground.to_linear().into(),
                    ..ElementColors::default()
                });
            let field = Element::new(
                &font,
                ElementContent::Children(vec![
                    Element::new(&font, ElementContent::Text(input.before().to_string())),
                    caret,
                    Element::new(&font, ElementContent::Text(input.after().to_string())),
                ]),
            )
            .display(DisplayType::Block)
            // Span the card's content width (minus the frame's 1px
            // outline on each side) so the outline doesn't hug the text.
            .min_width(Some(Dimension::Pixels(content_w - 2.)))
            .padding(BoxDimension {
                left: Dimension::Cells(0.5),
                right: Dimension::Cells(0.5),
                top: Dimension::Cells(0.3),
                bottom: Dimension::Cells(0.3),
            })
            .border_corners(Some(rounded()))
            .colors(ElementColors {
                // The corner discs take the border color, so it must
                // match the background (tab bar trick).
                border: BorderColor::new(colors.background.to_linear()),
                bg: colors.background.to_linear().into(),
                text: colors.foreground.to_linear().into(),
            });
            // A 1px rounded outline in a contrasting color needs a
            // nested frame: the box model fills each corner with a
            // solid disc in the border color, so a same-element border
            // would paint blocks at the corners.
            kids.push(
                Element::new(&font, ElementContent::Children(vec![field]))
                    .display(DisplayType::Block)
                    .margin(BoxDimension {
                        top: Dimension::Cells(0.5),
                        ..BoxDimension::default()
                    })
                    .padding(BoxDimension::new(Dimension::Pixels(1.)))
                    .border_corners(Some(rounded()))
                    .colors(ElementColors {
                        border: BorderColor::new(colors.menu_border.to_linear()),
                        bg: colors.menu_border.to_linear().into(),
                        text: colors.foreground.to_linear().into(),
                    }),
            );
        }

        // Buttons, right-aligned. Float::Right can't do this: in this
        // box model floats always inflate their container by their own
        // width. Instead, measure the buttons with a probe layout and
        // put a spacer of (content width - buttons width) before them.
        let button = |label: &str, which: DialogButton, primary: bool| {
            let (bg, fg) = if primary {
                (colors.active_bg, colors.active_fg)
            } else {
                (colors.background, colors.foreground)
            };
            let (hover_bg, hover_fg) = if primary {
                (
                    crate::sidebar::shift_towards_contrast(colors.active_bg, 0.15),
                    colors.active_fg,
                )
            } else {
                (colors.hover_bg, colors.hover_fg)
            };
            Element::new(&font, ElementContent::Text(label.to_string()))
                .item_type(UIItemType::SidebarDialogButton(which))
                .margin(BoxDimension {
                    left: Dimension::Cells(0.5),
                    ..BoxDimension::default()
                })
                .padding(BoxDimension {
                    left: Dimension::Cells(1.),
                    right: Dimension::Cells(1.),
                    top: Dimension::Cells(0.3),
                    bottom: Dimension::Cells(0.3),
                })
                .border(BoxDimension::new(Dimension::Pixels(1.)))
                .border_corners(Some(rounded()))
                .colors(ElementColors {
                    // The border doubles as the rounded-corner fill,
                    // so it must match the background (tab bar trick).
                    border: BorderColor::new(bg.to_linear()),
                    bg: bg.to_linear().into(),
                    text: fg.to_linear().into(),
                })
                .hover_colors(Some(ElementColors {
                    border: BorderColor::new(hover_bg.to_linear()),
                    bg: hover_bg.to_linear().into(),
                    text: hover_fg.to_linear().into(),
                }))
        };
        let cancel_btn = button("Cancel", DialogButton::Cancel, false);
        let confirm_btn = button(&self.confirm_label, DialogButton::Confirm, true);
        let probe = Element::new(
            &font,
            ElementContent::Children(vec![cancel_btn.clone(), confirm_btn.clone()]),
        );
        let win_w = term_window.dimensions.pixel_width as f32;
        let win_h = term_window.dimensions.pixel_height as f32;
        fn layout_context<'a>(
            term_window: &'a TermWindow,
            metrics: &'a RenderMetrics,
            win_w: f32,
            win_h: f32,
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
                    pixel_cell: metrics.cell_size.width as f32,
                },
                bounds: euclid::rect(0., 0., win_w, win_h),
                metrics,
                gl_state: term_window.render_state.as_ref().unwrap(),
                zindex: 100,
            }
        }
        let buttons_width = term_window
            .compute_element(&layout_context(term_window, &metrics, win_w, win_h), &probe)?
            .bounds
            .width();
        let spacer = Element::new(&font, ElementContent::Children(vec![]))
            .min_width(Some(Dimension::Pixels((content_w - buttons_width).max(0.))));
        kids.push(
            Element::new(
                &font,
                ElementContent::Children(vec![spacer, cancel_btn, confirm_btn]),
            )
            .display(DisplayType::Block)
            .margin(BoxDimension {
                top: Dimension::Cells(0.75),
                ..BoxDimension::default()
            }),
        );

        let card = Element::new(&font, ElementContent::Children(kids))
            .display(DisplayType::Block)
            // Lets mouse_event tell clicks on the card body (swallow)
            // apart from clicks outside it (cancel).
            .item_type(UIItemType::SidebarDialog)
            // Floor = content width + horizontal padding, so the card
            // hugs the content width above.
            .min_width(Some(Dimension::Pixels(
                content_w + 2. * CARD_PAD_H_CELLS * metrics.cell_size.width as f32,
            )))
            .padding(BoxDimension {
                left: Dimension::Cells(CARD_PAD_H_CELLS),
                right: Dimension::Cells(CARD_PAD_H_CELLS),
                top: Dimension::Cells(0.75),
                bottom: Dimension::Cells(0.75),
            })
            .border_corners(Some(rounded()))
            .colors(ElementColors {
                // The corner discs take the border color, so it must
                // match the background (tab bar trick).
                border: BorderColor::new(colors.background.to_linear()),
                // Opaque so the pane below doesn't bleed through.
                bg: colors.background.to_linear().into(),
                text: colors.foreground.to_linear().into(),
            });

        // The 1px rounded outline comes from a nested frame: the box
        // model fills each rounded corner with a solid disc in the
        // border color, so a contrasting border on the card itself
        // would paint blocks at the corners.
        let frame = Element::new(&font, ElementContent::Children(vec![card]))
            .display(DisplayType::Block)
            .item_type(UIItemType::SidebarDialog)
            .padding(BoxDimension::new(Dimension::Pixels(1.)))
            .border_corners(Some(rounded()))
            .colors(ElementColors {
                border: BorderColor::new(colors.menu_border.to_linear()),
                bg: colors.menu_border.to_linear().into(),
                text: colors.foreground.to_linear().into(),
            });

        // Lay out at the origin with the whole window available, then
        // clamp the anchor so the card stays fully inside the window.
        let mut computed = term_window.compute_element(
            &layout_context(term_window, &metrics, win_w, win_h),
            &frame,
        )?;

        let card_w = computed.bounds.width();
        let card_h = computed.bounds.height();
        let x = self.anchor.0.clamp(0., (win_w - card_w).max(0.));
        let y = self.anchor.1.clamp(0., (win_h - card_h).max(0.));
        computed.translate(euclid::vec2(
            x - computed.bounds.min_x(),
            y - computed.bounds.min_y(),
        ));
        Ok(computed)
    }
}

impl Modal for SidebarDialog {
    fn captures_mouse(&self) -> bool {
        true
    }

    fn mouse_event(
        &self,
        event: &::window::MouseEvent,
        term_window: &mut TermWindow,
    ) -> anyhow::Result<()> {
        let hit = term_window
            .ui_items
            .iter()
            .rev()
            .find(|item| item.hit_test(event.coords.x, event.coords.y))
            .map(|item| item.item_type.clone());
        match event.kind {
            WMEK::Press(MousePress::Left) => match hit {
                Some(UIItemType::SidebarDialogButton(DialogButton::Confirm)) => {
                    self.finish(term_window, true)
                }
                Some(UIItemType::SidebarDialogButton(DialogButton::Cancel)) => {
                    self.finish(term_window, false)
                }
                // Clicks on the card body are swallowed; clicks
                // anywhere else dismiss the dialog (standard popover
                // behavior).
                Some(UIItemType::SidebarDialog) => {}
                _ => self.finish(term_window, false),
            },
            WMEK::Move => {
                // Repaint only when the hovered element changes;
                // hover_colors reads current_mouse_event at render
                // time, so no geometry depends on this.
                let mut last = self.last_mouse_hit.borrow_mut();
                if *last != hit {
                    *last = hit;
                    if let Some(window) = term_window.window.as_ref() {
                        window.invalidate();
                    }
                }
            }
            // Swallow everything else (releases, wheel, right-click)
            // while the dialog is up.
            _ => {}
        }
        Ok(())
    }

    fn key_down(
        &self,
        key: KeyCode,
        mods: KeyModifiers,
        term_window: &mut TermWindow,
    ) -> anyhow::Result<bool> {
        match &self.kind {
            DialogKind::Prompt(input) => match (key, mods) {
                (KeyCode::Escape, KeyModifiers::NONE) => self.finish(term_window, false),
                (KeyCode::Enter, KeyModifiers::NONE) => self.finish(term_window, true),
                (KeyCode::Backspace, KeyModifiers::NONE) => {
                    input.borrow_mut().backspace();
                    term_window.invalidate_modal();
                }
                (KeyCode::Delete, KeyModifiers::NONE) => {
                    input.borrow_mut().delete();
                    term_window.invalidate_modal();
                }
                (KeyCode::LeftArrow, KeyModifiers::NONE) => {
                    input.borrow_mut().move_left();
                    term_window.invalidate_modal();
                }
                (KeyCode::RightArrow, KeyModifiers::NONE) => {
                    input.borrow_mut().move_right();
                    term_window.invalidate_modal();
                }
                (KeyCode::Home, KeyModifiers::NONE) => {
                    input.borrow_mut().move_home();
                    term_window.invalidate_modal();
                }
                (KeyCode::End, KeyModifiers::NONE) => {
                    input.borrow_mut().move_end();
                    term_window.invalidate_modal();
                }
                (KeyCode::Char('v'), KeyModifiers::CTRL) => self.paste_clipboard(term_window),
                (KeyCode::Char(c), KeyModifiers::NONE)
                | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
                    input.borrow_mut().insert(&c.to_string());
                    term_window.invalidate_modal();
                }
                _ => {}
            },
            DialogKind::Confirm => match (key, mods) {
                (KeyCode::Escape, KeyModifiers::NONE)
                | (KeyCode::Char('n' | 'N'), KeyModifiers::NONE) => {
                    self.finish(term_window, false)
                }
                (KeyCode::Enter, KeyModifiers::NONE)
                | (KeyCode::Char('y' | 'Y'), KeyModifiers::NONE) => {
                    self.finish(term_window, true)
                }
                _ => {}
            },
        }
        // The dialog is modal: swallow every key, even ones we don't
        // handle, so nothing leaks to the pane or key tables.
        Ok(true)
    }

    fn computed_element(
        &self,
        term_window: &mut TermWindow,
    ) -> anyhow::Result<Ref<'_, [ComputedElement]>> {
        if self.element.borrow().is_none() {
            let computed = self.build(term_window)?;
            self.element.borrow_mut().replace(vec![computed]);
        }
        Ok(Ref::map(self.element.borrow(), |v| {
            v.as_ref().unwrap().as_slice()
        }))
    }

    fn reconfigure(&self, term_window: &mut TermWindow) {
        self.element.borrow_mut().take();
        // Pick up sidebar color changes from a config reload.
        if let Some(sidebar) = term_window.sidebar.as_ref() {
            *self.colors.borrow_mut() = sidebar.colors;
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn insert_advances_cursor() {
        let mut input = LineInput::default();
        input.insert("he");
        input.insert("llo");
        assert_eq!(input.text(), "hello");
        assert_eq!(input.before(), "hello");
        assert_eq!(input.after(), "");
    }

    #[test]
    fn backspace_removes_grapheme_before_cursor() {
        let mut input = LineInput::new("héllo".to_string());
        // Land the cursor between é and l, then remove é.
        input.move_left();
        input.move_left();
        input.move_left();
        input.backspace();
        assert_eq!(input.text(), "hllo");
        assert_eq!(input.before(), "h");
    }

    #[test]
    fn backspace_at_start_is_noop() {
        let mut input = LineInput::new("ab".to_string());
        input.move_home();
        input.backspace();
        assert_eq!(input.text(), "ab");
        assert_eq!(input.before(), "");
    }

    #[test]
    fn delete_removes_grapheme_after_cursor() {
        let mut input = LineInput::new("héllo".to_string());
        input.move_home();
        input.move_right();
        input.delete();
        assert_eq!(input.text(), "hllo");
        assert_eq!(input.after(), "llo");
    }

    #[test]
    fn cursor_moves_are_grapheme_aware() {
        // Multi-byte + combining sequence: each move crosses a whole
        // grapheme, never a byte inside one.
        let mut input = LineInput::new("aé👍b".to_string());
        input.move_left();
        assert_eq!(input.before(), "aé👍");
        input.move_left();
        assert_eq!(input.before(), "aé");
        input.move_left();
        assert_eq!(input.before(), "a");
        input.move_left();
        assert_eq!(input.before(), "");
        // Clamp at the start
        input.move_left();
        assert_eq!(input.before(), "");
        input.move_end();
        assert_eq!(input.after(), "");
    }

    #[test]
    fn insert_in_the_middle() {
        let mut input = LineInput::new("ac".to_string());
        input.move_left();
        input.insert("b");
        assert_eq!(input.text(), "abc");
        assert_eq!(input.before(), "ab");
    }
}
