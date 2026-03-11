# rim

`rim` is a terminal-first editor prototype built around a state-driven architecture:

- The primary text buffer uses `ropey::Rope`
- The kernel is separated from infrastructure, and the runtime still follows a single `App` container model
- File watching, swap recovery, and persistent undo/redo all flow through the same event pipeline

## Release v0.0.2

This release focuses on runtime consistency and configuration ergonomics:

- Workspace session persistence is aligned with runtime state (tabs, windows, buffers, per-window view state, and scratch-buffer history)
- `:qa` is blocked when any buffer is dirty; `:qa!`, `:wqa`, and `:wqa!` are implemented
- `:wq` now persists workspace session consistently
- `:yazi` picker integration is stabilized for cursor/window restore behavior
- Command config supports inline table style:
  - `[normal] keymap = [ { on = "...", run = "...", desc = "..." } ]`
  - `[command] commands = [ { name = "...", run = "...", desc = "..." } ]`
- Config file changes are hot-reloaded through `FileWatcher` events (no polling loop in app runtime)
- Override semantics are explicit:
  - `normal.keymap` uses full-table replacement when provided
  - `visual.keymap` uses full-table replacement when provided
  - `command.commands` uses full-table replacement when provided

## Current Feature Set

- Core editing: movement, insert, delete, paste, undo, redo
- Three visual modes: `VISUAL`, `VISUAL LINE`, `VISUAL BLOCK`
- Multiple buffers, windows, and tabs
- Command line: save, save as, reload, open file, quit
- File watching: automatic reload after external changes
- Swap recovery: restore unsaved text after a crash
- Persistent undo/redo: reopen a file and restore history
- Workspace session restore: reopen `rim` without file arguments and restore tabs, windows, buffers, per-window view state, and scratch-buffer history
- Windows MSVC cross compilation: `cargo win-release`

## Workspace Layout

- `rim-paths`: shared platform path rules for `logs` / `swp` / `undo`
- `rim-kernel`: pure core state machine and business logic
- `rim-app`: the single `App` container, runtime orchestrator, and TUI entrypoint
- `rim-infra-storage`: unified storage infrastructure for file I/O, swap, and persistent undo/redo
- `rim-infra-file-watcher`: file watching infrastructure
- `rim-infra-input`: keyboard input infrastructure
- `rim-infra-ui`: ratatui rendering

## Architecture Snapshot

- `rim-app` keeps the only top-level container: `App`
- `rim-kernel` keeps `RimState` as the only core state aggregate
- `RimState::apply_action` remains the single business dispatch entrypoint
- `rim-kernel/src/action_handler/` is split by flow:
  - `file_flow`
  - `mode_flow`
  - `command_flow`
  - `editor_flow`
  - `post_edit_flow`
- `rim-kernel/src/state/` is split by domain:
  - `buffer`
  - `mode`
  - `window`
  - `tab`
  - `edit/` with `movement`, `core_edit`, `visual`
- `rim-infra-storage/src/worker/` handles the single storage worker loop
- `rim-infra-storage/src/swap_session/` is split into:
  - `protocol`
  - `compaction`
  - `lease`
- `rim-infra-storage/src/undo_history/` is split into:
  - `protocol`
  - `session_flow`
- `rim-kernel/src/state/session.rs` exports and restores workspace snapshots
- `rim-infra-storage/src/session.rs` persists the last workspace session snapshot

This means the current architecture boundary is no longer just crate-level. The main maintenance units are now:

- kernel action flow
- kernel state domains
- storage worker
- swap persistence
- undo persistence

## Build And Run

```bash
# Build locally
cargo build

# Run locally
cargo run -p rim-app --

# Start with one or more files opened
cargo run -p rim-app -- path/to/a.rs path/to/b.rs

# Checks
cargo check
cargo test
cargo clippy

# Windows MSVC target clippy
cargo win-clippy

# Build a Windows MSVC release binary from Linux/macOS
cargo win-release
```

Windows release artifact:

```text
target/x86_64-pc-windows-msvc/release/rim.exe
```

## Runtime Files

By default, `rim` maintains four runtime directories under the user state root:

- `logs/`: runtime logs
- `swp/`: crash recovery swap files
- `undo/`: persistent undo/redo history
- `session/`: the last restored workspace session snapshot

The root directory is resolved centrally by `rim-paths`.

Typical locations:

Linux:

```text
$XDG_STATE_HOME/rim
# or
~/.local/state/rim
```

Windows:

```text
%LOCALAPPDATA%\rim
```

macOS:

```text
~/Library/Logs/rim
```

Directory layout:

```text
rim/
├── logs/
│   └── rim.log
├── session/
│   └── last-session.json
├── swp/
│   ├── _home_zooeywm_a.txt.swp
│   └── _home_zooeywm_a.txt.<pid>.lease
└── undo/
    ├── _home_zooeywm_a.txt.undo.log
    └── _home_zooeywm_a.txt.undo.meta
```

File naming rules:

- The absolute source path is flattened into a single file name; no nested directories are created
- Path syntax characters `/ \\ : ? * " < > |` are collapsed into `_`
- A literal underscore `_` is encoded as `__`
- Examples:
  - Linux: `/home/zooeywm/a.txt` -> `_home_zooeywm_a.txt`
  - Windows: `C:\Users\zooey\a.txt` -> `C_Users_zooey_a.txt`

Workspace session rules:

- When `rim` starts without any file arguments, it first tries to load `session/last-session.json`
- When `rim` starts with file arguments, session restore is skipped and the explicit files win
- A normal quit saves the full current workspace snapshot
- The session snapshot restores:
  - buffer order
  - tab and window layout
  - active tab and active window
  - buffer text and clean baseline
  - per `window + buffer` cursor and scroll state
- Scratch / untitled buffers also keep undo / redo in the session snapshot
- File-backed buffers are rebound to file watching and swap tracking after restore, then reload undo / redo from `undo/`

## Command Registry And Keymap

`rim` now treats both built-in actions and future plugin actions as commands.

- Built-in commands are registered under stable command IDs such as `core.quit`, `core.save`, and `core.picker.yazi`
- Future plugins can register additional command IDs under their own namespace
- Normal-mode key bindings, visual-mode key bindings, and command-line aliases are all resolved through the same command registry

User overrides are loaded from:

```text
<config-root>/config.toml
<config-root>/keymaps.toml
<config-root>/commands.toml
```

If the files do not exist yet, `rim` creates full default config templates automatically on startup.

Typical config roots:

- Linux: `~/.config/rim`
- macOS: `~/Library/Application Support/rim`
- Windows: `%APPDATA%\\rim`

Example:

```toml
[editor]
leader_key = " "
cursor_scroll_threshold = 0
key_hints_width = 42
key_hints_max_height = 36

[normal]
keymap = [
  { on = "H", run = "core.buffer.next", desc = "Switch to next buffer" },
  { on = "<leader>wv", run = "core.window.split_vertical", desc = "Split vertically" },
  { on = "<F1>", run = "core.help.keymap", desc = "Show current mode key hints" },
  { on = "<Up>", run = "core.help.keymap_scroll_up", desc = "Scroll key hint window up" },
  { on = "<Down>", run = "core.help.keymap_scroll_down", desc = "Scroll key hint window down" },
  { on = "<C-p>", run = "core.help.keymap_scroll_up", desc = "Scroll key hint window up" },
  { on = "<C-n>", run = "core.help.keymap_scroll_down", desc = "Scroll key hint window down" },
  { on = ["<leader>wh", "<leader>w-"], run = "core.window.split_horizontal", desc = "Split horizontally" },
]

[visual]
keymap = [
  { on = "<Esc>", run = "core.visual.exit", desc = "Exit visual mode" },
  { on = "<F1>", run = "core.help.keymap", desc = "Show current mode key hints" },
  { on = "<Up>", run = "core.help.keymap_scroll_up", desc = "Scroll key hint window up" },
  { on = "<Down>", run = "core.help.keymap_scroll_down", desc = "Scroll key hint window down" },
  { on = "<C-p>", run = "core.help.keymap_scroll_up", desc = "Scroll key hint window up" },
  { on = "<C-n>", run = "core.help.keymap_scroll_down", desc = "Scroll key hint window down" },
]

[command]
commands = [
  { name = "qq", run = "core.quit_all", desc = "Quit application" },
  { name = "files", run = "core.picker.yazi", desc = "Open yazi picker" },
]
```

Rules:

- `run` must reference a registered command ID
- `normal.keymap` and `visual.keymap` accept `on = "..."` and `on = ["...", "..."]`
- A single string means one complete shortcut sequence such as `"<leader>wv"`
- A string array means multiple complete shortcuts bound to the same command
- `run` can be a command invocation such as `quit` or `quit!`, or a command ID such as `core.quit_all`
- `desc` is part of the runtime model and hot-reloads together with `run`
- `command.commands` defines command-line aliases entered after `:`
- `normal.keymap` and `visual.keymap` are command-oriented overrides: configured commands replace their built-in bindings, while untouched commands keep built-in defaults
- If `command.commands` is provided, it replaces the default command alias table
- Missing sections keep the built-in code defaults
- Invalid config entries are ignored and reported in the log
- `config.toml` covers editor-wide settings such as `leader_key`, `cursor_scroll_threshold`, `key_hints_width`, and `key_hints_max_height`
- Config file edits are detected at runtime and fully reloaded automatically; removing an override falls back to built-in defaults

## UI Conventions

- The top bar shows the current buffer name
- A dirty buffer displays `*` after the title
- The bottom status bar shows the current mode, messages, and any pending key sequence
- A floating window overlay is available for reusable popup UI
- The first consumer of that overlay is the current-mode key hint popup
- Long key hint entries wrap inside the floating window body without consuming the footer or breaking page navigation

## Modes

The editor currently implements these modes:

- `NORMAL`
- `INSERT`
- `COMMAND`
- `VISUAL`
- `VISUAL LINE`
- `VISUAL BLOCK`
- `INSERT BLOCK` (entered from visual block `I` / `A`)

## Normal Mode

### Cursor And Scrolling

- `h` `j` `k` `l`: move left / down / up / right
- `0`: jump to the beginning of the line
- `$`: jump to the end of the line
- `gg`: jump to the beginning of the file
- `G`: jump to the end of the file
- `Ctrl+e`: scroll the view down by one line
- `Ctrl+y`: scroll the view up by one line
- `Ctrl+d`: scroll the view down by half a page
- `Ctrl+u`: scroll the view up by half a page

### Enter Other Modes

- `i`: enter insert mode at the cursor
- `a`: move right, then enter insert mode
- `o`: open a new line below and enter insert mode
- `O`: open a new line above and enter insert mode
- `:`: enter command mode
- `v`: enter `VISUAL`
- `V`: enter `VISUAL LINE`
- `Ctrl+v`: enter `VISUAL BLOCK`

### Editing

- `x`: delete the current character into the single slot
- `dd`: delete the current line into the single slot
- `p`: paste the slot content after the cursor
- `J`: join the current line with the next line
- `u`: undo
- `Ctrl+r`: redo

### Buffer / Window / Tab

- `H` / `L`: switch to the previous / next buffer in the current tab
- `{` / `}`: switch to the previous / next buffer in the current tab
- `Ctrl+h` `Ctrl+j` `Ctrl+k` `Ctrl+l`: move focus to the left / down / up / right window
- `F1`: show current-mode single-key bindings and multi-key entry points in a floating hint window
- While the hint window is open, long entries wrap within the current page instead of pushing the footer out of view

The default leader key is `Space`.

Leader sequences:

- `<leader> w v`: vertical split
- `<leader> w h`: horizontal split
- `<leader> <Tab> n`: create a new tab
- `<leader> <Tab> d`: close the current tab
- `<leader> <Tab> [`: switch to the previous tab
- `<leader> <Tab> ]`: switch to the next tab
- `<leader> b n`: create and bind a new empty `untitled` buffer
- `<leader> b d`: close the current buffer

Pending multi-key sequences:

- Prefix-driven hints are not leader-specific; any pending multi-key sequence can open the same floating hint window
- Examples: `<leader>`, `<leader> b`, `g`, `d`
- When the floating hint window overflows, `Up` / `Down` scroll one line and `Ctrl+u` / `Ctrl+d` scroll half a page
- `Ctrl+n` / `Ctrl+p` also scroll the floating hint window by one line
- The floating hint footer shows the current page number
- `Backspace`: step back one prefix level while the hint window is open
- `Esc`: close the hint window and cancel the pending sequence

## Insert Mode

- `Esc`: return to normal mode
- `Enter`: insert a newline
- `Backspace`: delete backward
- `Tab`: insert a tab character
- `Left` `Right` `Up` `Down`: move the cursor
- Printable characters: insert text

Notes:

- A continuous insert session is grouped into a single undo step
- Consecutive adjacent pure inserts are merged into a single history edit, so persistent undo does not store `aaaa` as four separate inserts

## Visual Mode

### Common Behavior

- `Esc`: leave visual mode
- `h` `j` `k` `l`: move the selection endpoint
- `0` / `$`: jump to the beginning / end of the line
- `gg` / `G`: jump to the beginning / end of the file
- `Ctrl+e` / `Ctrl+y`: scroll the view down / up by one line
- `Ctrl+d` / `Ctrl+u`: scroll the view down / up by half a page
- `v`: switch to `VISUAL`
- `V`: switch to `VISUAL LINE`
- `Ctrl+v`: switch to `VISUAL BLOCK`
- `F1`: show current-mode visual key hints in a floating window

### Selection Operations

- `y`: yank the selection into the slot
- `d`: delete the selection into the slot
- `x`: delete the selection into the slot
- `p`: replace the current selection with the slot content
- `c`: delete the current selection and enter insert mode

## Visual Block Mode

In addition to the common visual behavior, visual block also supports:

- `I`: enter block insert at the left edge of the rectangle
- `A`: enter block insert at the right edge of the rectangle

Block insert currently supports:

- Printable characters: insert into every selected row at the same column
- `Tab`: insert a tab into every selected row
- `Backspace`: delete backward in every selected row
- `Esc`: leave block insert

Block insert currently does not support:

- `Enter`
- Arrow keys

The status bar will show: `block insert supports text, tab, backspace, esc only`

## Command Mode

### Basic Keys

- `Esc`: leave command mode
- `Enter`: execute the command
- `Backspace`: delete one command-line character
- Printable characters: edit the command line

### Implemented Commands

- `:q`
- `:quit`
- `:q!`
- `:quit!`
- `:qa`
- `:qa!`
- `:w`
- `:w!`
- `:wa`
- `:wqa`
- `:wqa!`
- `:wq`
- `:wq!`
- `:e`
- `:e!`
- `:yazi`
- `:e <path>`
- `:w <path>`
- `:w! <path>`
- `:wq <path>`
- `:wq! <path>`

### Command Semantics

- `:q`
  - If any buffer is dirty, quitting is blocked and `:q!` is suggested
  - If the current tab has multiple windows, close the current window
  - Otherwise, if there are multiple tabs, close the current tab
  - Otherwise, quit the application
- `:q!`
  - Ignore dirty checks and use the same window / tab / app closing order as `:q`
- `:qa`
  - If any buffer is dirty, quitting is blocked
  - Otherwise, quit the application
- `:qa!`
  - Ignore dirty checks and quit the application immediately
- `:w`
  - Save the current buffer
- `:w!`
  - Force-save the current buffer
- `:wa`
  - Save all file-backed buffers
- `:wqa`
  - Save all file-backed buffers, then quit the application
  - If any buffer has no file path, the command is blocked
  - If any file-backed buffer was changed externally, the command is blocked and `:wqa!` is suggested
- `:wqa!`
  - Force-save all file-backed buffers, then quit the application
  - If any buffer has no file path, the command is blocked
- `:wq`
  - Save the current buffer, then quit the application
- `:wq!`
  - Force-save the current buffer, then quit the application
- `:e`
  - Reload the file bound to the current buffer
- `:e!`
  - Force-reload the current buffer, including a dirty buffer
- `:yazi`
  - Suspend the current TUI, launch `yazi`, and open the selected file if one is chosen
- `:e <path>`
  - Open the given path after normalizing it to an absolute path
- `:w <path>` / `:w! <path>`
  - Save the current buffer to the given path
- `:wq <path>` / `:wq! <path>`
  - Save to the given path, then quit the application

## File And Buffer Behavior

- Opened files are deduplicated by normalized absolute path
- Reopening the same file reuses the existing buffer instead of creating a duplicate
- New empty buffers are named `untitled`
- If the current tab contains exactly one clean `untitled` buffer, opening a file replaces that buffer instead of adding another one
- File-backed buffers support watch / reload / persistent history
- Closing a buffer is tab-local first: it removes the buffer from the current tab, and only tears down watch / persistence when no tab references that buffer anymore

## Dirty Semantics

Dirty does not mean “this buffer was edited before”. It means “the current text differs from the clean baseline”.

The clean baseline is updated when:

- A file is opened successfully
- An external reload succeeds
- A save succeeds
- A new buffer is initialized

As a result:

- After an edit, `dirty = true`
- If you manually change the text back to the opened or saved state, `dirty` automatically returns to `false`
- If undo / redo returns the buffer to the clean text, dirty is cleared automatically

## External File Changes

- Opened files are watched by the file watcher
- When an external change is detected, a reload flow is triggered
- After an internal save, a short ignore window suppresses watcher echo and avoids self-triggered reload

## Swap Recovery

Each file keeps one `.swp` file under the state directory.

Behavior:

- When opening a file, if an existing swap is detected, `rim` prompts:
  - `[r]ecover`
  - `[d]elete`
  - `[e]dit anyway`
  - `[a]bort`
- `r`: recover the unsaved text recorded in the swap
- `d`: delete the old swap and rebuild the session from the current on-disk content
- `e`: ignore the old swap and keep editing the current on-disk content
- `a` or `Esc`: abort the buffer open operation

Notes:

- Swap uses a `BASE + edit log` model
- Log flushing is debounced
- Tail logs produced by undo are removed with `truncate` whenever possible instead of always rewriting the whole file

## Persistent Undo / Redo

Each file keeps:

- `*.undo.log`
- `*.undo.meta`

Behavior:

- Workspace session does not replace per-file undo storage for file-backed buffers
- After opening a file, if the current text matches the persisted history state, undo / redo stacks are restored automatically
- After an external reload or swap recovery, `rim` tries to restore history again
- Normal `undo` / `redo` mostly update only `meta`
- A branch edit truncates the tail of `undo.log`, then appends the new branch

## Current Boundaries

- This is still a WIP editor prototype
- The main flows are in place, but semantics and infrastructure boundaries are still being tightened
