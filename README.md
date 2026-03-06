# rim

`rim` is a terminal-first editor prototype built around a state-driven architecture:

- The primary text buffer uses `ropey::Rope`
- The kernel is separated from infrastructure, and each infrastructure concern lives in its own workspace crate
- File watching, swap recovery, and persistent undo/redo all flow through the same event pipeline

## Current Feature Set

- Core editing: movement, insert, delete, paste, undo, redo
- Three visual modes: `VISUAL`, `VISUAL LINE`, `VISUAL BLOCK`
- Multiple buffers, windows, and tabs
- Command line: save, save as, reload, open file, quit
- File watching: automatic reload after external changes
- Swap recovery: restore unsaved text after a crash
- Persistent undo/redo: reopen a file and restore history
- Windows MSVC cross compilation: `cargo win-release`

## Workspace Layout

- `rim-paths`: shared platform path rules for `logs` / `swp` / `undo`
- `rim-kernel`: pure core state machine and business logic
- `rim-app`: the single `App` container, runtime orchestrator, and TUI entrypoint
- `rim-infra-file-io`: asynchronous file I/O infrastructure
- `rim-infra-file-watcher`: file watching infrastructure
- `rim-infra-persistence`: swap and persistent undo/redo
- `rim-infra-input`: keyboard input infrastructure
- `rim-infra-ui`: ratatui rendering
- `rim-app`: application container and TUI entrypoint

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

By default, `rim` maintains three runtime directories under the user state root:

- `logs/`: runtime logs
- `swp/`: crash recovery swap files
- `undo/`: persistent undo/redo history

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

## UI Conventions

- The top bar shows the current buffer name
- A dirty buffer displays `*` after the title
- The bottom status bar shows the current mode, messages, and any pending key sequence

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

- `H` / `L`: switch to the previous / next buffer
- `{` / `}`: switch to the previous / next buffer
- `Ctrl+h` `Ctrl+j` `Ctrl+k` `Ctrl+l`: move focus to the left / down / up / right window

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

### Command Semantics

- `:q`
  - If any buffer is dirty, quitting is blocked and `:q!` is suggested
  - If the current tab has multiple windows, close the current window
  - Otherwise, if there are multiple tabs, close the current tab
  - Otherwise, quit the application
- `:q!`
  - Ignore dirty checks and use the same window / tab / app closing order as `:q`
- `:qa`
  - Quit the application immediately
- `:w`
  - Save the current buffer
- `:w!`
  - Force-save the current buffer
- `:wa`
  - Save all file-backed buffers
- `:wq`
  - Save the current buffer, then close the current closing scope
- `:wq!`
  - Force-save, then close the current closing scope
- `:e`
  - Reload the file bound to the current buffer
- `:e!`
  - Force-reload the current buffer, including a dirty buffer
- `:e <path>`
  - Open the given path after normalizing it to an absolute path
- `:w <path>` / `:w! <path>`
  - Save the current buffer to the given path
- `:wq <path>` / `:wq! <path>`
  - Save to the given path, then close the current closing scope

## File And Buffer Behavior

- Opened files are deduplicated by normalized absolute path
- Reopening the same file reuses the existing buffer instead of creating a duplicate
- New empty buffers are named `untitled`
- File-backed buffers support watch / reload / persistent history
- Closing a buffer stops file watching and closes the corresponding persistence session

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

- After opening a file, if the current text matches the persisted history state, undo / redo stacks are restored automatically
- After an external reload or swap recovery, `rim` tries to restore history again
- Normal `undo` / `redo` mostly update only `meta`
- A branch edit truncates the tail of `undo.log`, then appends the new branch

## Current Boundaries

- This is still a WIP editor prototype
- The main flows are in place, but semantics and infrastructure boundaries are still being tightened
