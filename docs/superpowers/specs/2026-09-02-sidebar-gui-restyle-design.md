# 侧边栏 GUI 化：现代列表行 + 右键浮层菜单 设计

日期：2026-09-02
分支：workspace-sidebar
前置：`docs/superpowers/specs/2026-09-02-workspace-sidebar-design.md`（计划一～三已合入）

## 背景与目标

当前侧边栏（`wezterm-gui/src/termwindow/render/sidebar.rs`）把每个 workspace 渲染成一行终端文字单元格，观感是 TUI；右键点击条目目前是空占位（`mouseevent.rs` 的 `mouse_event_sidebar` 右键 arm 只留注释），用户点了没有反应。

本设计做两件事：

1. **外观**：侧边栏改为「现代列表行」风格（VSCode 式：行之间无间距、整行色块、active 行左侧指示条、hover 整行变色），用 GUI quad 绘制背景而非纯文字单元格。
2. **右键菜单**：右键条目弹出 **GUI 浮层菜单**（在窗口内用 quad 绘制、随鼠标交互），提供六项动作：重命名 / 设置默认目录 / 设置默认命令 / 上移 / 下移 / 从列表移除。

本设计**取代** `docs/superpowers/plans/2026-09-02-workspace-sidebar-4-overlays.md` 中的 Task 2（termwiz 菜单 overlay）与 Task 3 Step 3（右键接 overlay 菜单）；Task 1（prompt/confirm 的 Rust 回调变体）保留并纳入本设计作为输入基础设施；Task 3 Step 4（＋ 按钮行输入命名）**不在本次范围**，＋ 按钮维持现状。

## 非目标

- 菜单项不可配置（用户已澄清：「可配置」指右键能配置 workspace，不是菜单可定制）
- prompt/confirm 输入框不做 GUI 化，沿用 termwiz overlay
- 不写回 Lua 配置文件；运行时覆盖（cwd/命令/顺序/隐藏）仍只存内存（沿用前置 spec ①⑦）
- ＋ 按钮交互不变

## 现状锚点

- 渲染：`wezterm-gui/src/termwindow/render/sidebar.rs`（`paint_sidebar`，逐行 `render_screen_line`）
- 模型：`wezterm-gui/src/sidebar.rs`（`SidebarState::new` 预渲染 `Line`；`SidebarItem::{None, Entry, NewButton}`）
- 鼠标：`wezterm-gui/src/termwindow/mouseevent.rs:569`（`mouse_event_sidebar`；右键 arm 为空占位）
- 绘制顺序：`render/paint.rs:271-282`（panes → splits → sidebar → tab bar → borders → modal）；sidebar 已在 pane 之后绘制，菜单接在其后即可覆盖终端区域
- 键盘入口：`termwindow/keyevent.rs:599`（`key_event_impl`）
- hover 钩子：`mouseevent.rs:37-61`（`enter_ui_item`/`leave_ui_item`，目前 Sidebar 为空 arm）
- 颜色：`config/src/color.rs:464`（`SidebarColors`）
- mux API（均已实现）：`Mux::rename_workspace` / `set_workspace_metadata` / `get_workspace_metadata` / `resolve_workspace_defaults` / `move_workspace_in_sidebar` / `hide_workspace_in_sidebar`（`mux/src/lib.rs:665-769`）

## 设计

### 1. 侧边栏渲染重构

**模型层**（`wezterm-gui/src/sidebar.rs`）：

- `SidebarRow` 由「预渲染 `Line`」改为结构化数据：

  ```rust
  pub struct SidebarRow {
      pub item: SidebarItem,
      pub title: String,            // 含 " (n)" tab 数徽标，不含前导空格
      pub subtitle: Option<String>,
      pub is_active: bool,
      pub is_open: bool,
  }
  ```

- `SidebarState` 额外缓存解析后的 `ResolvedColors`（新增 hover、指示条色，见 §2），颜色不再在构建时烧进 `Line`，而在 paint 时按行状态（active / hover / open / inactive）选取。

**渲染层**（`render/sidebar.rs` 的 `paint_sidebar` 重写）：

- 每行行高 = 1 cell（有副标题则 2 cell，两行为一个命中单元）。
- 每行先画整行背景 quad（`filled_rectangle`，宽 = sidebar 像素宽）；active 行在左侧再画 3px 宽指示条 quad。
- 文字仍用 `render_screen_line` 画在行内，左侧留约 1 cell padding（active 行文字避开指示条）。
- 栏底部剩余空间照旧用背景色填满。

**Hover**：

- `TermWindow` 新增 `sidebar_hover: Option<SidebarItem>`。
- `enter_ui_item`/`leave_ui_item` 的 `Sidebar(item)` arm 更新该字段；变化时触发重绘（`invalidate_sidebar()` + 请求窗口重绘，参照 tab bar hover 的 `update_title_post_status` 路径，实现计划在 mouseevent 的 UI item 切换处统一处理）。
- hover 行整行用 hover 色绘制。

### 2. `colors.sidebar` 扩展

`config/src/color.rs` 的 `SidebarColors` 新增三个可选字段（含 `overlay_with` 与 `resolves_to` 合并逻辑同步扩展）：

| 字段 | 类型 | 默认 |
|---|---|---|
| `hover` | `Option<TabBarColor>` | bg 由 sidebar 背景按亮度方向提亮/压暗约 10% 派生，fg = `foreground` |
| `active_indicator` | `Option<RgbaColor>` | `active.fg_color`（未配 active 则为 palette 前景色） |
| `menu_border` | `Option<RgbaColor>` | 由背景派生的对比色 |

菜单配色不做独立字段：菜单背景/文字/hover 直接继承 sidebar 的 `background` / `foreground` / `hover`。

### 3. 右键 GUI 浮层菜单

**新文件 `wezterm-gui/src/sidebar_menu.rs`**：

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarMenuAction { Rename, SetDefaultCwd, SetDefaultCommand, MoveUp, MoveDown, Remove }

pub struct SidebarMenuState {
    pub workspace: String,
    pub x: f32,                 // 弹出锚点（像素）
    pub y: f32,
    pub hovered: Option<usize>, // 当前 hover 的菜单项索引
}
```

- `TermWindow` 新增 `sidebar_menu: Option<SidebarMenuState>`。
- 菜单项标签固定六项（英文标签，与计划 4 一致：Rename workspace / Set default directory / Set default command / Move up / Move down / Remove from list）。
- `UIItemType` 新增 `SidebarMenuItem(usize)`（索引对应动作），整行命中。

**绘制**：`paint_sidebar_menu(&mut self, layers)` 在 `paint_pass` 中紧随 `paint_sidebar` 之后调用（`render/paint.rs:272` 后）。菜单背景、边框（1px）、hover 行高亮用 quad 绘制；文本用 `render_screen_line`。菜单宽 = 最长标签 cell 宽 + 2 cell padding；高 = 项数 × cell 高。靠近窗口右/下边缘时向左/上翻转，保证完整可见。

**鼠标路由**（`mouseevent.rs`）：

- `mouse_event_impl` 顶部：菜单打开时——
  - hover 到 `SidebarMenuItem(idx)`：更新 `hovered` 并重绘；
  - 左键按下命中 `SidebarMenuItem(idx)`：关闭菜单并 `dispatch_sidebar_menu_action(workspace, action)`；
  - 任意按下命中菜单外：关闭菜单并**吞掉**这次点击（标准菜单行为）；**例外**：右键命中另一个 `Entry` 条目时，改为直接对该条目重开菜单（锚点移到新点击处）。
- 右键 `SidebarItem::Entry(name)`（现有空 arm）：设置 `sidebar_menu = Some(...)`，锚点 = 点击坐标，重绘。

**键盘**（`keyevent.rs` 的 `key_event_impl` 顶部）：菜单打开时——Esc 关闭并吞掉；其他任意键关闭菜单后照常传递给 pane。

**动作分发**（`termwindow/mod.rs` 新方法）：

```rust
fn show_sidebar_menu(&mut self, workspace: String, x: f32, y: f32)
fn close_sidebar_menu(&mut self)
fn dispatch_sidebar_menu_action(&mut self, workspace: String, action: SidebarMenuAction)
```

- `MoveUp` / `MoveDown`：`Mux::get().move_workspace_in_sidebar(&workspace, ∓1)`，然后 `invalidate_sidebar()`。
- `Remove`：开 confirm overlay（§4 回调变体），确认后 `hide_workspace_in_sidebar`；菜单已关，overlay 独立。
- `Rename`：prompt overlay 预填当前名 → 回调中 `Mux::get().rename_workspace(&old, new)`；空名/同名不改。
- `SetDefaultCwd` / `SetDefaultCommand`：prompt overlay 预填 `resolve_workspace_defaults` 当前值 → 回调中更新 `WorkspaceMetadata` 并 `set_workspace_metadata`；空行 = 清除运行时覆盖（沿用计划 4 语义）。
- 所有 mux 变更后 `invalidate_sidebar()`，下次 paint 重建模型。

**线程模型**：prompt/confirm 的回调在 overlay 线程执行，只调线程安全的 mux API；需要 TermWindow 上下文的操作（再开一层 overlay）经 `window.notify(TermWindowNotif::Apply(...))` 回主循环（沿用计划 4 的模型）。

### 4. prompt/confirm Rust 回调变体（= 计划 4 Task 1 原样）

- `overlay/prompt.rs` 新增 `show_line_prompt_overlay_with_callback(term, description, prompt, initial_value, callback: FnOnce(Option<String>))`，现有 `show_line_prompt_overlay` 重构为它的薄封装（Lua 事件路径行为不变）。
- `overlay/confirm.rs` 新增 `show_confirmation_overlay_with_callback(term, message, callback: FnOnce(bool))`。
- TermWindow 侧加 `prompt_for_workspace_value(description, prompt, initial, callback)` 辅助（start_overlay + assign_overlay + spawn，参照 `show_confirmation`）。

### 5. 错误处理与边界

- 菜单打开期间目标 workspace 被移除/改名：动作分发时 mux 方法对不存在的 workspace 为 no-op（`move_workspace_in_sidebar` / `hide_workspace_in_sidebar` 现状即如此），不崩溃。
- 菜单打开时窗口缩放 / sidebar 隐藏（`update_show_sidebar`）：关闭菜单（在 `update_show_sidebar` 与 resize 路径调用 `close_sidebar_menu`）。
- 菜单打开时切换 workspace 或 tab：菜单跟随窗口状态，发现 `sidebar_menu` 的目标已不在 `compute_sidebar_entries` 中时关闭（paint 时校验一次即可）。
- 行高超出可视区域：现有 `paint_sidebar` 的截断逻辑保留。

### 6. 测试

- `config` crate：`SidebarColors` 新字段解析与默认值测试（仿 `parses_sidebar_options`）。
- `wezterm-gui`：`sidebar.rs` 模型层单测（行数据构建、徽标、active/open 状态）——若现模块无测试基建则以编译 + 手工验证为准。
- `cargo check -p wezterm-gui` / `cargo test -p config`。
- 手工验证清单：
  1. 外观：行整行着色、active 指示条、hover 整行变色、副标题两行命中同一项
  2. 右键条目 → 浮层菜单出现在点击处；hover 高亮跟随；点项执行；点菜单外关闭且点击不穿透到 pane；Esc 关闭；按其他键关闭且按键传到 pane
  3. 菜单在窗口右/下边缘右键时翻转不超出窗口
  4. 六项动作逐一验证（语义同计划 4 Step 6 清单 2-6）
  5. 菜单打开时拖窄窗口触发 sidebar 隐藏 → 菜单关闭
  6. 重启后运行时覆盖复位（沿用前置 spec ⑦）

## 提交划分（建议）

1. `config: extend colors.sidebar with hover/active_indicator/menu_border`
2. `gui: restructure sidebar model to plain row data`
3. `gui: paint sidebar as modern list rows with hover and active indicator`
4. `gui: add Rust-callback variants of prompt and confirmation overlays`
5. `gui: sidebar right-click floating menu with workspace actions`
