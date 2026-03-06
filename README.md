# rim

`rim` 是一个终端优先的编辑器原型，核心采用状态驱动架构：

- 主 buffer 使用 `ropey::Rope`
- 内核与设施分离，设施通过 workspace 中的独立 crate 接入
- 文件监视、swap 恢复、持久化 undo/redo 都走统一事件流

## 当前功能总览

- 普通编辑：移动、插入、删除、粘贴、撤销、重做
- 三种 visual 模式：`VISUAL`、`VISUAL LINE`、`VISUAL BLOCK`
- 多 buffer、多 window、多 tab
- 命令行：保存、另存为、重载、打开文件、退出
- 文件监视：外部改动自动重载
- swap 恢复：崩溃后恢复未保存文本
- 持久化 undo/redo：重新打开文件后恢复历史
- Windows MSVC 交叉编译：`cargo win-release`

## 工作空间结构

- `crates/rim-paths`：共享平台目录规则（`logs` / `swp` / `undo` 根路径）
- `crates/rim-kernel`：纯业务核心与状态机
- `crates/rim-app`：唯一容器 `App`
- `crates/rim-infra-file-io`：异步文件读写设施
- `crates/rim-infra-file-watcher`：文件监视设施
- `crates/rim-infra-persistence`：swap 与持久化 undo/redo
- `crates/rim-infra-input`：键盘输入设施
- `crates/rim-infra-ui`：ratatui 渲染
- `crates/rim-tui`：TUI 入口

## 构建与运行

```bash
# 本机构建
cargo build

# 本地运行
cargo run -p rim-tui --

# 打开一个或多个文件启动
cargo run -p rim-tui -- path/to/a.rs path/to/b.rs

# 检查
cargo check
cargo test
cargo clippy

# Windows MSVC 目标 clippy
cargo win-clippy

# 从 Linux/macOS 主机构建 Windows MSVC release 二进制
cargo win-release
```

Windows release 产物路径：

```text
target/x86_64-pc-windows-msvc/release/rim.exe
```

## 运行时文件

默认会在用户状态目录下维护三类运行时文件：

- `logs/`：运行日志
- `swp/`：崩溃恢复用 swap 文件
- `undo/`：持久化 undo/redo 历史文件

目录根路径由 `crates/rim-paths` 统一决定。

在 Linux 上通常位于：

```text
$XDG_STATE_HOME/rim
# 或
~/.local/state/rim
```

在 Windows 上通常位于：

```text
%LOCALAPPDATA%\rim
```

在 macOS 上通常位于：

```text
~/Library/Logs/rim
```

目录结构如下：

```text
rim/
├── logs/
│   └── rim.log
├── swp/
│   ├── _home_zooeywm_a.txt.swp
│   └── _home_zooeywm_a.txt.<pid>.lease
└── undo/
    ├── _home_zooeywm_a.txt.undo.log
    └── _home_zooeywm_a.txt.undo.meta
```

文件命名规则：

- 源文件绝对路径会被平铺成单个文件名，不创建嵌套目录
- 路径语法字符 `/ \ : ? * " < > |` 会压成 `_`
- 字面下划线 `_` 会编码为 `__`
- 例如：
  - Linux：`/home/zooeywm/a.txt` -> `_home_zooeywm_a.txt`
  - Windows：`C:\Users\zooey\a.txt` -> `C_Users_zooey_a.txt`

## UI 约定

- 顶部栏显示当前 buffer 名称
- dirty buffer 会在标题后显示 `*`
- 底部状态栏显示当前模式、消息和待完成按键序列

## 模式

当前实现的编辑模式：

- `NORMAL`
- `INSERT`
- `COMMAND`
- `VISUAL`
- `VISUAL LINE`
- `VISUAL BLOCK`
- `INSERT BLOCK`（由 visual block 的 `I` / `A` 进入）

## 普通模式

### 光标与滚动

- `h` `j` `k` `l`：左 / 下 / 上 / 右移动
- `0`：跳到行首
- `$`：跳到行尾
- `gg`：跳到文件开头
- `G`：跳到文件结尾
- `Ctrl+e`：视图下滚一行
- `Ctrl+y`：视图上滚一行
- `Ctrl+d`：视图下滚半页
- `Ctrl+u`：视图上滚半页

### 进入其他模式

- `i`：在光标处进入插入模式
- `a`：光标右移后进入插入模式
- `o`：在下方新开一行并进入插入模式
- `O`：在上方新开一行并进入插入模式
- `:`：进入命令模式
- `v`：进入 `VISUAL`
- `V`：进入 `VISUAL LINE`
- `Ctrl+v`：进入 `VISUAL BLOCK`

### 编辑

- `x`：删除当前字符到单一 slot
- `dd`：删除当前行到单一 slot
- `p`：在光标后粘贴 slot 内容
- `J`：将当前行与下一行拼接
- `u`：undo
- `Ctrl+r`：redo

### buffer / window / tab

- `H` / `L`：切换到上一个 / 下一个 buffer
- `{` / `}`：切换到上一个 / 下一个 buffer
- `Ctrl+h` `Ctrl+j` `Ctrl+k` `Ctrl+l`：切换焦点到左 / 下 / 上 / 右侧 window

Leader key 默认是空格 `Space`。

Leader 序列：

- `<leader> w v`：垂直分屏
- `<leader> w h`：水平分屏
- `<leader> <Tab> n`：新建 tab
- `<leader> <Tab> d`：关闭当前 tab
- `<leader> <Tab> [`：切换到上一个 tab
- `<leader> <Tab> ]`：切换到下一个 tab
- `<leader> b n`：新建并绑定一个空 `untitled` buffer
- `<leader> b d`：关闭当前 buffer

## 插入模式

- `Esc`：回到普通模式
- `Enter`：插入换行
- `Backspace`：向后删除
- `Tab`：插入制表符
- `Left` `Right` `Up` `Down`：移动光标
- 普通字符输入：插入文本

说明：

- 一次连续 insert 输入会归并成一个 undo 步骤
- 连续相邻纯插入会归并成一条 history edit，因此持久化 undo 文件不会把 `aaaa` 记成四条独立插入

## Visual 模式

### 通用行为

- `Esc`：退出 visual
- `h` `j` `k` `l`：移动选择端点
- `0` / `$`：跳到行首 / 行尾
- `gg` / `G`：跳到文件开头 / 结尾
- `Ctrl+e` / `Ctrl+y`：视图下滚 / 上滚一行
- `Ctrl+d` / `Ctrl+u`：视图下滚 / 上滚半页
- `v`：切换到 `VISUAL`
- `V`：切换到 `VISUAL LINE`
- `Ctrl+v`：切换到 `VISUAL BLOCK`

### 选择操作

- `y`：复制选区到 slot
- `d`：删除选区到 slot
- `x`：删除选区到 slot
- `p`：用 slot 内容替换当前选区
- `c`：删除当前选区并进入插入模式

## Visual Block 模式

除通用 visual 行为外，还支持：

- `I`：在矩形左边界进入块插入
- `A`：在矩形右边界进入块插入

块插入模式下当前支持：

- 普通字符输入：对块内所有行同步插入
- `Tab`：对块内所有行同步插入 tab
- `Backspace`：对块内所有行同步回删
- `Esc`：退出块插入

当前不支持的块插入键：

- `Enter`
- 方向键

对应状态栏会提示：`block insert supports text, tab, backspace, esc only`

## Command 模式

### 基本按键

- `Esc`：退出命令模式
- `Enter`：执行命令
- `Backspace`：删除命令行字符
- 普通字符输入：编辑命令行

### 已实现命令

- `:q`
- `:quit`
- `:q!`
- `:quit!`
- `:qa`
- `:w`
- `:w!`
- `:wa`
- `:wq`
- `:wq!`
- `:e`
- `:e!`
- `:e <path>`
- `:w <path>`
- `:w! <path>`
- `:wq <path>`
- `:wq! <path>`

### 命令语义

- `:q`
  - 若任意 buffer dirty，则阻止退出并提示使用 `:q!`
  - 若当前 tab 有多个 window，则关闭当前 window
  - 否则若有多个 tab，则关闭当前 tab
  - 否则退出程序
- `:q!`
  - 忽略 dirty 检查，沿用与 `:q` 相同的 window / tab / app 关闭顺序
- `:qa`
  - 立即退出程序
- `:w`
  - 保存当前 buffer
- `:w!`
  - 强制保存当前 buffer
- `:wa`
  - 保存所有文件型 buffer
- `:wq`
  - 保存当前 buffer 后退出当前关闭层级
- `:wq!`
  - 强制保存后退出当前关闭层级
- `:e`
  - 重新加载当前 buffer 对应文件
- `:e!`
  - 强制重新加载当前 buffer，对 dirty buffer 也生效
- `:e <path>`
  - 打开指定路径，路径会先规范化为绝对路径
- `:w <path>` / `:w! <path>`
  - 另存当前 buffer 到指定路径
- `:wq <path>` / `:wq! <path>`
  - 另存后退出当前关闭层级

## 文件与 buffer 行为

- 打开文件时，内部使用规范化绝对路径去重
- 同一个文件再次打开时，会复用已有 buffer，而不是创建重复 buffer
- 新建空 buffer 时使用 `untitled` 命名
- 文件型 buffer 支持 watch / reload / 持久化历史
- 关闭 buffer 时会停止 watch，并关闭对应持久化会话

## dirty 语义

dirty 不是“发生过编辑”而是“当前文本是否偏离 clean 基线”。

clean 基线会在这些时机更新：

- 成功打开文件
- 成功外部重载
- 成功保存
- 新建 buffer 初始化时

因此：

- 改动后 `dirty = true`
- 手动改回到打开或保存时的文本后，`dirty` 会自动恢复为 `false`
- undo / redo 回到 clean 文本时，也会自动清除 dirty

## 外部文件变更

- 已打开文件会被文件监视器观察
- 外部修改被检测到后，会触发重载流程
- 内部保存后会有一个短时间忽略窗口，避免保存自己触发 reload 回声

## swap 恢复

每个文件会在状态目录下维护一份 `.swp`。

行为：

- 打开文件时，如果检测到已有 swap，会提示：
  - `[r]ecover`
  - `[d]elete`
  - `[e]dit anyway`
  - `[a]bort`
- `r`：恢复 swap 中未保存文本
- `d`：删除旧 swap，并用当前磁盘内容重新建立会话
- `e`：忽略旧 swap，直接继续编辑当前磁盘内容
- `a` 或 `Esc`：中止这次 buffer 打开

说明：

- swap 是 `BASE + edit log` 结构
- 日志会做 debounce flush
- undo 触发的尾部可逆日志会尽量通过 `truncate` 消掉，而不是总是整文件重写

## 持久化 undo / redo

每个文件会在状态目录下维护：

- `*.undo.log`
- `*.undo.meta`

行为：

- 打开文件后，如果当前文本与持久化历史匹配，会自动恢复 undo / redo 栈
- 外部 reload 或 swap recover 后，也会重新尝试恢复历史
- 普通 `undo` / `redo` 主要只更新 `meta`
- 发生分叉编辑时，会对 `undo.log` 做尾部 `truncate` 后再 append 新分支

## 当前已知边界

- 这是一个 WIP 编辑器原型，功能已具备主流程，但仍在持续收敛语义与设施边界
