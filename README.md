# rim

**WIP demo**: `rim` is a terminal-first editor core focused on a state-driven architecture and unified event flow, with a planned WASM plugin system for future extensibility.

## Modes

- `NORMAL`
- `INSERT`
- `COMMAND`

## Normal Mode Keys

- `h` `j` `k` `l`: move cursor
- `0` / `$`: move to line start / end
- `i`: enter insert mode at cursor
- `a`: move right then enter insert mode
- `x`: cut current character into the single slot
- `p`: paste slot content after cursor
- `H` / `V`: split window horizontally / vertically
- `Ctrl+h` `Ctrl+j` `Ctrl+k` `Ctrl+l`: focus window
- `Ctrl+w`: close active window
- `t`: new tab
- `X`: close current tab
- `[` / `]`: switch tab prev / next
- `{` / `}`: switch buffer prev / next
- `:`: enter command mode

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
- `:w`
- `:wq`
- `:wa`
- `:w <path>`
- `:wq <path>`
