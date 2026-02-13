# rim

**WIP demo**: `rim` is a terminal-first editor core focused on a state-driven architecture and unified event flow, with a planned WASM plugin system for future extensibility.

## Build and Run

```bash
# build
cargo build

# run
cargo run

# run checks
cargo check
cargo clippy
```

## Modes

- `NORMAL`
- `INSERT`
- `COMMAND`
- `VISUAL`
- `VISUAL LINE`

## Normal Mode Keys

- `h` `j` `k` `l`: move cursor
- `0` / `$`: move to line start / end
- `gg` / `G`: move to file start / end
- `i`: enter insert mode at cursor
- `a`: move right then enter insert mode
- `o`: open a new line below and enter insert mode
- `O`: open a new line above and enter insert mode
- `v`: enter visual-char mode
- `V`: enter visual-line mode
- `dd`: delete current line to slot
- `x`: cut current character into the single slot
- `p`: paste slot content after cursor
- `J`: join current line with line below
- `u`: undo
- `Ctrl+r`: redo
- `H` / `L`: switch buffer prev / next
- `{` / `}`: switch buffer prev / next
- `Ctrl+h` `Ctrl+j` `Ctrl+k` `Ctrl+l`: focus window
- `Ctrl+e` / `Ctrl+y`: scroll view down / up one line
- `Ctrl+d` / `Ctrl+u`: scroll view down / up half page
- `:`: enter command mode

Leader key: default is `Space` (`<leader>`).

Leader sequences:

- `<leader> w v`: split window vertically
- `<leader> w h`: split window horizontally
- `<leader> <Tab> n`: new tab
- `<leader> <Tab> d`: close current tab
- `<leader> <Tab> [`: switch to previous tab
- `<leader> <Tab> ]`: switch to next tab
- `<leader> b n`: create and bind a new empty `untitled` buffer
- `<leader> b d`: close current buffer

## Insert Mode Keys

- `Esc`: back to normal mode
- `Enter`: newline
- `Backspace`: delete backward
- `Tab`: insert tab character
- arrow keys: move cursor
- text input: insert characters

## Command Mode

- `Esc`: leave command mode
- `Enter`: execute command
- `Backspace`: delete command text

Implemented commands:

- `:q`, `:quit`
- `:q!`, `:quit!`
- `:qa`
- `:w`, `:w!`
- `:wa`
- `:wq`, `:wq!`
- `:e`, `:e!`
- `:e <path>`
- `:w <path>`, `:w! <path>`
- `:wq <path>`, `:wq! <path>`

File-open behavior:

- `:e <path>` opens by normalized absolute path.
- Opening the same file again reuses the existing buffer (even across tabs/windows) instead of creating duplicates.

`:q` behavior:

- if any buffer is dirty: blocked (`use :q!`)
- else if current tab has more than one window: close active window
- else if there is more than one tab: close current tab
- else: quit the app

`:qa` behavior:

- quit the app immediately

## Visual Mode Keys

- `v` (from normal): enter visual-char mode
- `v` / `V` (inside visual): switch to visual-line mode
- `Esc`: leave visual mode
- `h` `j` `k` `l`: move cursor / selection
- `0` / `$`: move to line start / end
- `gg` / `G`: move to file start / end
- `Ctrl+e` / `Ctrl+y`: scroll view down / up one line
- `Ctrl+d` / `Ctrl+u`: scroll view down / up half page
- `d`: delete selection to slot
- `y`: yank selection to slot
- `p`: replace selection with slot content
