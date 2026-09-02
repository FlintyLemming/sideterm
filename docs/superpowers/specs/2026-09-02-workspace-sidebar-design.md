# WezTerm 项目侧边栏（workspace sidebar）设计

日期：2026-09-02
状态：已与作者分节确认（数据模型、侧边栏 UI、cwd/命令注入），overlay 与测试部分由设计方补全

## 背景与目标

参考 Taxis（Ptyxis 分支）的改动：终端从「一堆标签页」变为「项目 → 标签页」两级结构，左侧栏列出项目，每个项目可配置默认工作目录和默认命令。

在 wezterm 中复用时做如下映射：

- 「项目」**复用 mux 已有的 workspace 机制**（整套标签页切换、后台保持运行），不新建概念。
- 配置以 **Lua 为主**，辅以**内嵌 overlay 简易编辑界面**（不自绘完整对话框）。
- 「默认命令」采用 **注入 shell** 语义（同 taxis），不是 exec 替换。
- 默认 cwd / 命令对**该 workspace 内每个新标签生效**（同 taxis）。

## ① 配置与数据模型

### 新配置段

`config` crate 的 `Config` 结构体（`config/src/config.rs`）新增：

```rust
pub workspaces: Vec<WorkspaceEntry>,
```

```rust
#[derive(...Dynamic...)]
pub struct WorkspaceEntry {
    pub name: String,                    // 必填，workspace 名
    pub cwd: Option<PathBuf>,            // 可选，默认工作目录
    pub default_command: Option<String>, // 可选，注入 shell 的文本
}
```

用 wezterm-dynamic 派生，与现有配置项同套路。`wezterm.lua` 示例：

```lua
return {
  workspaces = {
    { name = "api", cwd = "D:/code/api", default_command = "npm run dev" },
    { name = "wezterm", cwd = "C:/Users/FlintyLemming/Projects/wezterm" },
  },
}
```

### 侧边栏条目 = 两个来源的合并

| 来源 | 状态显示 | 点击行为 |
|---|---|---|
| 配置里有、mux 里还没有 | 「未打开」态（灰色） | 创建 workspace + 第一个标签（应用 cwd + 注入命令） |
| mux 里活着（无论配置里有没有） | 正常态 + 标签数徽章；配置里有的额外显示 cwd/命令副标题 | `SwitchToWorkspace` |

### 运行时覆盖

通过 overlay 给 workspace 设置的 cwd/命令**只存内存**（重启丢失），不写回 Lua 文件（写回超出 wezterm 惯例）。

存储位置：`Mux`（`mux/src/lib.rs`）新增 `workspace_metadata: HashMap<String, WorkspaceMetadata>`，其中：

```rust
pub struct WorkspaceMetadata {
    pub cwd: Option<PathBuf>,
    pub default_command: Option<String>,
}
```

**有效值解析顺序**：运行时覆盖（mux metadata）> 配置段（config.workspaces）> 无。

解析逻辑收敛为一个辅助函数，放在 `mux/src/lib.rs`（mux 已依赖 config，且 spawn 路径的调用点就在 mux）：

```rust
fn resolve_workspace_defaults(workspace: &str) -> (Option<PathBuf>, Option<String>)
```

mux 层其余部分**不动**：workspace 创建、切换、存活全用现有机制（`iter_workspaces`、`is_workspace_empty`、`set_active_workspace_for_client` 等）。

## ② 侧边栏 UI

### 渲染

wezterm 无原生控件，侧边栏仿照 `wezterm-gui/src/tabbar.rs`（fancy tab bar）用 GPU quad 自绘：

- 新增 `wezterm-gui/src/sidebar.rs`；在 `TermWindow` 渲染循环中占据窗口左侧一块区域，终端区整体右移（与 fancy tab bar 占用顶部空间同理，走现有 box model / dimensions 体系）。
- 配色：新配置 `colors.sidebar`（可选）；缺省时由终端前景/背景色推导（palette-following，同 taxis 思路），保证与终端区视觉一体。
- 每行：项目名 + 标签数徽章；有 cwd/命令的加一行副标题（显示末级目录名，悬停 tooltip 显示完整值）；当前 workspace 行高亮。

### 交互

| 动作 | 行为 |
|---|---|
| 左键点击条目 | `SwitchToWorkspace`；未打开的则创建 + 生成首个标签 |
| 右键点击条目 | 弹出上下文菜单：重命名 / 设置默认目录 / 设置默认命令 / 上移 / 下移 / 从列表移除 |
| 顶部 ＋ 按钮 | 新建 workspace（行输入 prompt 输入名字） |
| 快捷键 | 新 KeyAssignment `ToggleSidebar`，默认不绑定按键，用户在 Lua 里自行绑定（wezterm 惯例，不写死 F9） |

「上移/下移/从列表移除」作用于侧边栏的显示顺序：配置条目重排的是运行时显示序（存内存），mux 条目同理；「从列表移除」仅隐藏该条目，不杀 workspace。

### 显示控制

- `enable_sidebar = false`（**默认关**，不影响现有用户）
- `sidebar_width`（cell 数，默认约 24）
- `sidebar_hide_when_narrow`（窗口过窄自动隐藏；简化为直接隐藏，不做浮层动画——自绘体系里浮层成本高）

### 布局

宽度以 cell 为单位换算像素，终端区相应减宽；窗口 resize、workspace 切换、标签增删时复用 tab bar 的 invalidate 路径重绘。

## ③ 新标签的 cwd / 命令注入

### cwd 优先级（高 → 低）

1. 显式指定（`SpawnCommand.cwd`、命令行参数）
2. 继承当前 pane 的 cwd（现有 OSC7 机制，受同 domain 限制）
3. **workspace 默认 cwd**（新增）
4. 系统默认（`default_cwd` → home）

注入点：`mux/src/lib.rs` `spawn_tab_or_window` 中 `resolve_cwd` 返回 `None` 之后，用 `resolve_workspace_defaults` 取当前 workspace 的默认 cwd 兜底。mux 侧仅此一处小改动。

### 命令注入（打进 shell，非 exec）

- spawn 完成后（`wezterm-gui/src/spawn.rs` `spawn_command_internal` 拿到 `pane` 之后），若当前 workspace 有有效 `default_command`，调度延迟的 `pane.send_text("cmd\r")`。
- **时序**：不能 spawn 完立即发（shell 启动时 `tcflush()` 会吞输入）。轮询 pane：等到 pane 产生首次输出（shell 已打印 prompt）后再注入；超时约 5 秒则放弃并 log。
- **只对新 spawn 的标签生效一次**：`respawn`、会话恢复的标签不重放。
- **启动时第一个标签不注入**（同 taxis）：进程启动路径（`main.rs` 首个窗口）不走注入，只有 GUI 内新建标签（快捷键、侧边栏点击、＋按钮）走注入。
- 命令是注入文本，退出后回到 shell 提示符，标签不关闭。

### 覆盖范围

`SpawnTab`、`SpawnCommandInNewTab`（未显式带 cwd/args 时）、侧边栏点击新建，统一走上述路径，行为一致。

## ④ overlay 编辑界面

复用现有 overlay 机制，全部走 **Rust 侧回调**（不经 Lua 事件）：

- **右键上下文菜单**：新的轻量 termwiz overlay，渲染参考 `overlay/selector.rs`、生命周期参考 `overlay/prompt.rs`，回调直接调 mux API。
- **设置默认目录 / 默认命令**：复用 `overlay/prompt.rs` 的 `show_line_prompt_overlay`（行输入 + Rust 回调），确认后写入 mux 的 `workspace_metadata`。
- **重命名 workspace**：同上 prompt，回调调 `Mux::rename_workspace`。
- **从列表移除**：复用 `overlay/confirm.rs` 的 `show_confirmation_overlay`。

## ⑤ 错误处理

| 场景 | 行为 |
|---|---|
| 配置的 cwd 不存在/不可达 | spawn 时按现有路径报错（toast/log），不静默失败 |
| workspace 名为空或重复 | 配置加载期 validation 警告；重名条目后者忽略 |
| `default_command` 为空或纯空格 | 视为未设置（同 taxis） |
| 注入超时（shell 5 秒无输出） | 放弃注入并 log，不影响标签正常使用 |
| 注入目标 pane 已死 | 放弃注入 |
| `resolve_workspace_defaults` 查询的 workspace 无任何配置 | 返回 `(None, None)`，走系统默认 |

## ⑥ 测试

- **config**：`WorkspaceEntry` 反序列化（Lua 配置段解析、缺省字段、空命令视为未设置）——config crate 现有动态配置测试套路。
- **mux**：`resolve_workspace_defaults` 优先级（运行时覆盖 > 配置）；`spawn_tab_or_window` 的 cwd 兜底顺序（显式 > 继承 > workspace 默认 > 系统默认）用 mux 现有单测设施。
- **注入时序**：抽出「等首次输出再 send_text，带超时」为独立可测函数，用假 pane 验证。
- **GUI 侧边栏**：自绘部分不做像素级测试；条目合并逻辑（配置 ∪ mux 运行时、排序、未打开态）抽成纯函数单测。

## ⑦ 明确不做（YAGNI）

- 不自绘完整对话框 / 控件库
- 不写回 Lua 配置文件
- 不做侧边栏浮层动画（窄窗口直接隐藏）
- 不做项目级「撤销关闭标签」分桶（wezterm 现有撤销机制不动）
- 不改会话保存格式（沿用 mux 现有 workspace 持久化；配置条目天然持久）
- 不支持 exec 语义的默认命令（只用注入；用户可自行用 `default_prog`/`SpawnCommand.args` 达到 exec 效果）
