# Sidebar dialogs & menu GUI restyle — design

Date: 2026-09-03
Branch: workspace-sidebar
Status: approved in chat (user: "可以，你开发吧")

## 1. Goal

The sidebar's right-click workspace menu and the edit interactions it
triggers (rename, set default cwd, set default command, remove
confirmation) currently render through two TUI-style paths:

- the menu is painted with `paint_chrome_line` (cell-grid
  `render_screen_line`), square borders, monospace cell rows;
- rename/cwd/command use `overlay::prompt::show_line_prompt_overlay_with_callback`
  (a termwiz LineEditor overlay inside the pane);
- remove uses `overlay::confirm::show_confirmation_overlay_with_callback`
  (termwiz `[Y]es / [N]o` reversed-text buttons).

All of these should use the same visual language as the sidebar itself:
box-model `Element` trees, title font, rounded pills, hover states
(spec: `2026-09-02-sidebar-gui-restyle-design.md`).

Scope: sidebar-triggered interactions only. Other overlays (command
palette, charselect, generic PromptInputLine/Confirmation used by Lua
config) are untouched.

## 2. Approach

Reuse the existing `termwindow::modal::Modal` trait, which already
provides everything a dialog needs:

- keyboard capture: `keyevent.rs` routes every key to
  `modal.key_down` first; `Ok(true)` swallows it;
- rendering: `paint.rs` `paint_modal` paints
  `modal.computed_element` at zindex 100, above chrome (10) and panes (0);
- resize/config reload: `resize.rs` calls `modal.reconfigure`;
- cancellation: `cancel_modal`.

Chosen over: (B) ad-hoc dialog state fields on `TermWindow` with manual
input interception (reinvents the Modal plumbing), and (C) native OS
dialogs (blocking, visually foreign).

## 3. Menu restyle

`paint_sidebar_menu` (render/sidebar.rs) is replaced by
`build_sidebar_menu_element`, mirroring `build_sidebar_element`:

- rounded container, 1px border in `colors.menu_border`, background
  `colors.background`, corner polys from `render::corners`;
- one pill row per `sidebar_menu::MENU_ITEMS` entry, padding/gap taken
  from the same `ROW_PAD_*`/`ROW_GAP` constants as the sidebar so the
  rhythm matches; `hover_colors` drives the hover fill (`colors.hover_bg`
  / `colors.hover_fg`) — the manual hover rectangle goes away;
- each row keeps `UIItemType::SidebarMenuItem(idx)` for hit testing;
- title font, clamped anchor position (`menu.x/y` clamped to window
  bounds) unchanged;
- the element tree is cached on `TermWindow` (`sidebar_menu_element`)
  and rebuilt when the menu opens/closes/hover changes; hover changes
  must invalidate the cache so `hover_colors` applies;
- still painted into the chrome layer (zindex 10) after the sidebar so
  it lands on top; the "workspace vanished while menu open" guard
  stays.

## 4. `SidebarDialog` modal

New module `wezterm-gui/src/sidebar_dialog.rs` implementing `Modal`.

### 4.1 Kinds

```rust
enum DialogKind {
    Prompt(PromptState),   // single-line input
    Confirm,               // message + buttons
}

struct PromptState {
    text: String,          // grapheme-safe editing buffer
    cursor: usize,         // byte index, always on a char boundary
}
```

Construction:

```rust
SidebarDialog::prompt(title, description, initial, submit_label, on_submit)
SidebarDialog::confirm(title, message, confirm_label, on_result)
```

Callbacks are `FnOnce` closures running on the main thread (the modal
lives on the GUI thread — unlike the termwiz overlay callbacks, no
thread hop is needed), so they can touch both `Mux` and `TermWindow`
(via the `&mut TermWindow` passed to `key_down`/`mouse_event`).

### 4.2 Layout (computed_element)

Anchored popover: the dialog's top-left is the position where the
context menu was (passed in at construction), clamped so the card stays
fully inside the window. zindex 100.

Card (rounded, `menu_border` 1px border, `background` fill, generous
padding in cells):

- title: bold, `foreground`;
- description/message: `subtitle_fg`, wrapped;
- (Prompt only) input field: rounded rectangle with border; inside, an
  inline row of three elements — text-before-cursor, cursor bar
  (1-2px wide empty Element with `foreground` bg), text-after-cursor —
  so the caret lands correctly with the proportional title font without
  manual text measurement. Placeholder/empty text keeps a zero-width
  before/after so the caret still shows.
- button row, right-aligned: `Cancel`, and `Save`/`Remove` (confirm
  button uses active/accent styling). Buttons are pill Elements with
  `hover_colors` and `UIItemType::SidebarDialogButton(..)`.

All colors come from the sidebar's `ResolvedColors`; the card picks up
config reloads via `reconfigure` (drop cached element).

### 4.3 Keyboard (basic editing)

Handled in `key_down`, all returning `Ok(true)`:

- `Char(c)` (NONE/SHIFT): insert at cursor;
- Backspace/Delete: remove grapheme before/after cursor;
- Left/Right/Home/End: move cursor (grapheme-aware);
- Ctrl+V: paste clipboard text (single line; newlines stripped);
- Enter: submit (`on_submit(Some(text))`, then `cancel_modal`);
- Escape: cancel (`on_submit(None)` / `on_result(false)`, then
  `cancel_modal`).

The edit operations live in a small pure state machine
(`insert`, `backspace`, `delete`, `move_cursor`) in `sidebar_dialog.rs`
with unit tests; the Modal impl is a thin shell over it.

### 4.4 Mouse

`Modal::mouse_event` currently has no call site. Add dispatch in
`mouseevent.rs`: when `get_modal()` is Some, deliver the mouse event to
the modal instead of the pane/sidebar/menu handling.

The dialog hit-tests via the ui_items produced by its computed element
(`UIItemType::SidebarDialogButton(ButtonId)`); hover state feeds back
into element hover_colors. Click outside the card cancels (same as
Escape). While a dialog is open the sidebar menu is closed first.

### 4.5 Wiring

`dispatch_sidebar_menu_action` (termwindow/mod.rs):

- Rename / SetDefaultCwd / SetDefaultCommand: build
  `SidebarDialog::prompt(...)` with the existing mux-mutating closures
  (moved verbatim from the current overlay callbacks) and
  `set_modal(...)`. `prompt_for_workspace_value` is deleted.
- Remove: `SidebarDialog::confirm(...)`, then on confirm
  `Mux::hide_workspace_in_sidebar`.

`overlay::prompt::show_line_prompt_overlay_with_callback` and
`overlay::confirm::show_confirmation_overlay_with_callback` lose their
only callers and are removed (the Lua-facing variants stay).

Opening a dialog closes the menu; `update_show_sidebar` /
resize cancel the dialog via the existing modal paths.

## 5. Error handling

- Empty/unchanged rename stays a no-op (same rule as today) — the
  dialog allows submitting anything; the callback decides.
- Workspace removed/renamed while a dialog is open: callbacks capture
  the workspace name and mux calls no-op on unknown workspaces, as
  today.
- Element build failure: log and `cancel_modal` rather than leaving a
  stale modal (matches CommandPalette's `expect`-free error path via
  `anyhow::Result`).

## 6. Testing

- Unit tests for the text-edit state machine (grapheme boundaries,
  cursor clamping, insert/delete at ends).
- Existing pure-function tests (sidebar rows/geometry) unchanged; menu
  geometry stays in testable helpers where practical.
- Manual visual verification: build, run, screenshot (PrintWindow) the
  menu, prompt dialog, confirm dialog in both hover and editing states.
