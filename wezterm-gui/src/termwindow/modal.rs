use crate::termwindow::box_model::ComputedElement;
use crate::TermWindow;
use config::keyassignment::KeyAssignment;
use downcast_rs::{impl_downcast, Downcast};
use std::cell::Ref;
use wezterm_term::{KeyCode, KeyModifiers};

pub trait Modal: Downcast {
    fn perform_assignment(
        &self,
        _assignment: &KeyAssignment,
        _term_window: &mut TermWindow,
    ) -> bool {
        false
    }
    /// Whether the modal owns the mouse while it is active. When true,
    /// mouseevent.rs routes every window mouse event to `mouse_event`
    /// instead of the pane/tab bar/sidebar handling. Defaults to
    /// false, preserving the click-through behavior of the older
    /// modals (palette, selectors).
    fn captures_mouse(&self) -> bool {
        false
    }
    /// Mouse event with pixel coordinates, dispatched only when
    /// `captures_mouse` is true.
    fn mouse_event(
        &self,
        _event: &::window::MouseEvent,
        _term_window: &mut TermWindow,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    fn key_down(
        &self,
        key: KeyCode,
        mods: KeyModifiers,
        term_window: &mut TermWindow,
    ) -> anyhow::Result<bool>;
    fn computed_element(
        &self,
        term_window: &mut TermWindow,
    ) -> anyhow::Result<Ref<'_, [ComputedElement]>>;
    fn reconfigure(&self, term_window: &mut TermWindow);
}
impl_downcast!(Modal);
