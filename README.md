# SideTerm

[简体中文](README.zh-CN.md)

**SideTerm** is a fork of [WezTerm](https://github.com/wezterm/wezterm) — a GPU-accelerated cross-platform terminal emulator and multiplexer written in Rust — with one major addition: **a workspace sidebar that turns the terminal from "a pile of tabs" into a two-level "workspaces → tabs" structure**, inspired by [Taxis](https://github.com/mufeedali/taxis) (a Ptyxis fork).

Everything upstream WezTerm can do, SideTerm can do; your existing `wezterm.lua` keeps working unchanged. This README documents only **what SideTerm adds on top of upstream**. For everything else, see the upstream docs at <https://wezterm.org/>.

![Screenshot](docs/screenshots/two.png)

## What's different from upstream WezTerm

### 1. Workspace sidebar

A sidebar runs down the full left edge of the window (the tab bar sits to its right) and lists your workspaces:

- **Merged list**: workspaces declared in your config plus workspaces that are actually alive in the mux. Configured-but-not-open workspaces are shown greyed out; live ones show a tab-count badge and, if configured, a cwd / command subtitle.
- **Left-click** an entry: switch to that workspace (`SwitchToWorkspace`), or — if it isn't open yet — create it with a first tab that gets the configured cwd and default command applied.
- **Right-click** an entry: a floating context menu (rendered as native GUI elements, not a terminal overlay):
  - Rename workspace
  - Set default directory
  - Set default command
  - Move up / Move down (reorders the sidebar list; in-memory only)
  - Remove from list (hides the entry; the workspace itself is not killed)
- **＋ button** at the top: create a new workspace by typing a name.
- Changes made through the menu (cwd / command / order) are **runtime overrides held in memory** — they are never written back to your Lua config. Resolution order: runtime override > `workspaces` config > unset.

### 2. Per-workspace default cwd and default command

Each workspace can carry defaults that apply to **every new tab spawned into it**:

- **Default cwd** is used as a fallback when spawning, after explicit cwd and OSC7-inherited cwd: explicit > inherited > workspace default > system default.
- **Default command** is *injected into the shell as typed text* (not `exec` replacement): the tab spawns a normal shell, waits for its first output, then sends the command. Exiting the command returns you to the shell prompt; the tab stays open. Injection happens once per newly spawned tab, never on respawn or session restore, and never for the very first tab at app startup.

### 3. GUI-rendered dialogs and menu

The sidebar's context menu, rename prompt, and edit dialogs are painted with the box-model GUI element system (like the fancy tab bar), with hover states, rounded corners and a border — instead of reusing terminal-cell overlays.

### 4. Rebrand and packaging

- The user-visible name is **SideTerm** (window title, menus, about/quit items, update banner). Binary names, crate names, config file paths, `WEZTERM_*` environment variables, the Lua module name and the Windows AppUserModelID are **unchanged**, so existing configs keep working and upstream merges stay easy.
- A Windows Inno Setup installer (`SideTerm-<ver>-setup.exe`) with its own AppId, so SideTerm can be **installed side-by-side with WezTerm**. Built via a manually triggered GitHub Actions workflow.

## New configuration

All new options live in your regular `wezterm.lua`. Everything is optional; unspecified colors are derived from the terminal foreground/background so the sidebar blends in.

### `workspaces` (new section)

Declares workspaces for the sidebar, with optional per-workspace defaults:

```lua
return {
  workspaces = {
    { name = "api",     cwd = "D:/code/api", default_command = "npm run dev" },
    { name = "wezterm", cwd = "C:/Users/you/Projects/wezterm" },
  },
}
```

| Field | Type | Description |
|---|---|---|
| `name` | string (required) | Workspace name; must match the mux workspace name |
| `cwd` | path (optional) | Default working directory for new tabs in this workspace |
| `default_command` | string (optional) | Text injected into the shell of newly spawned tabs. Empty / whitespace-only values are treated as unset |

Empty names are ignored with a warning; duplicate names are first-match-wins.

### Sidebar options

| Option | Type | Default | Description |
|---|---|---|---|
| `enable_sidebar` | bool | `true` ⚠️ | Show the workspace sidebar. **Differs from the original design: SideTerm enables it by default** so the feature is discoverable; upstream WezTerm has no sidebar at all |
| `sidebar_width` | usize (cells) | `24` | Sidebar width, in terminal cells |
| `sidebar_hide_when_narrow` | bool | `true` | Automatically hide the sidebar when the window is too narrow for it to be useful |

### `colors.sidebar` (new palette section)

```lua
return {
  colors = {
    sidebar = {
      background = "#202020",           -- sidebar background (default: terminal background)
      foreground = "#ffffff",           -- entry text (default: terminal foreground)
      inactive_foreground = "#808080",  -- text for not-yet-open workspaces
      subtitle_foreground = "#a0a0a0",  -- cwd / command subtitle line
      active = { bg_color = "#303030", fg_color = "#ffffff" }, -- active workspace row
      hover  = { bg_color = "#2a2a2a", fg_color = "#ffffff" }, -- row under the mouse
      active_indicator = "#5294e2",     -- left-edge bar on the active row
      menu_border = "#404040",          -- context-menu border
    },
  },
}
```

`active` and `hover` accept the same `{ bg_color, fg_color, ... }` shape as tab-bar color entries. Anything left unspecified is derived from the terminal palette.

### New key assignment: `ToggleSidebar`

`ToggleSidebar` shows/hides the sidebar per window. It is **not bound to any key by default** — bind it yourself, per WezTerm convention:

```lua
local wezterm = require 'wezterm'
return {
  keys = {
    { key = 'b', mods = 'CTRL|SHIFT', action = wezterm.action.ToggleSidebar },
  },
}
```

## Compatibility and upstream sync

- `main` carries the SideTerm changes; the `upstream` branch tracks upstream WezTerm and is merged back regularly.
- The mux's existing workspace persistence, session restore, and all other WezTerm features are untouched.

## Installation

Grab `SideTerm-<ver>-setup.exe` (installer, coexists with WezTerm) or the portable `SideTerm-windows-<ver>.zip` from the [Releases](https://github.com/FlintyLemming/sideterm/releases) page. For other platforms, build from source following the [upstream build instructions](https://wezterm.org/install/source.html).

## Credits

All credit for the terminal itself goes to [Wez Furlong](https://github.com/wez) and the WezTerm contributors. SideTerm only adds the workspace sidebar and packaging described above. If WezTerm is useful to you, consider [sponsoring the upstream project](https://wezterm.org/sponsor.html).
