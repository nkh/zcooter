# Scooter — Vim-Style Key Bindings Cheat Sheet

> Quick reference for scooter's vim-inspired key bindings.
> Launch with: `scooter --config zcooter/vim-config.toml`

---

## Key Syntax

| Notation   | Meaning                          | Example         |
|------------|----------------------------------|-----------------|
| `a`        | Bare letter                      | `j`, `k`, `/`  |
| `C-x`      | Control + key                    | `C-c`, `C-d`   |
| `A-x`      | Alt + key                        | `A-i`, `A-u`   |
| `S-x`      | Shift + key                      | `S-tab`, `G`   |
| `:x`       | Colon-prefix two-key sequence    | `:q`, `:h`, `:e` |
| `xy`       | Leader-key two-key sequence      | `zl`, `zh`, `zf` |
| `enter`    | Enter key                        | —              |
| `tab`      | Tab key                          | —              |
| `backspace`| Backspace key                    | —              |
| `up/down`  | Arrow keys                       | —              |
| `pageup/down` | Page navigation keys            | —              |

### Two-Key Sequences

- **Colon-prefix** (`:x`) — Press `:` then a letter. Modelled after vim's `:command` system. Used for top-level commands.
- **Leader-key** (`xy`) — Two letters typed in quick succession. The first letter is the "leader". `z` is the leader for toggle commands.

---

## Esc-prefix (Command Mode)

| Key Sequence | Action                                           |
|--------------|--------------------------------------------------|
| `Esc Esc`    | Cancel (no-op)                                   |
| `Esc : q`    | Quit scooter                                     |
| `Esc : r`    | Reset all fields                                 |
| `Esc : h`    | Show help menu                                   |
| `Esc : e`    | Open in editor                                   |
| `Esc /`      | Focus search field                               |
| `Esc z l`    | Toggle line wrapping                             |

> In **insert mode** (fields focused), bare letters are typed as text.
> Press **Esc** first to type command keys (`:q`, `/`, `zl`, etc.).
> In **normal mode** (results focused), prefix keys (`:`, `z`) work
> directly, but Esc ensures they aren't forwarded to the search field.
> Control keys, Alt keys, Tab, and Enter always work as commands directly.

---

## General Commands (All Screens)

| Key          | Action                                   |
|--------------|------------------------------------------|
| `:q` / `C-c` | Quit scooter                            |
| `:r`         | Reset all fields, return to search screen|
| `:h` / `?`   | Show help menu with all key bindings     |

---

## Search Screen — Common (Both Modes)

### z-Leader Toggles (press `z` then the second key)

| Key  | Action                                              |
|------|-----------------------------------------------------|
| `zl` | Toggle line wrapping in preview pane                |
| `zh` | Toggle hidden files (dotfiles)                      |
| `zm` | Toggle multiline regex search                       |
| `ze` | Toggle escape-sequence interpretation in replace    |
| `zc` | Toggle fixed-strings mode (literal search)          |
| `zw` | Toggle whole-word matching                          |
| `zs` | Toggle case-sensitive matching                      |

### Column Resizing

| Key            | Action                              |
|----------------|--------------------------------------|
| `C-left` / `<` | Shrink the file-name column width   |
| `C-right` / `>`| Grow the file-name column width     |

---

## Search Screen — Fields Focused (Insert Mode)

| Key     | Action                                           |
|---------|---------------------------------------------------|
| `enter` | Trigger search                                   |
| `tab`   | Focus next field                                 |
| `S-tab` | Focus previous field                             |
| `A-u`   | Unlock pre-populated CLI fields                  |
| `A-f`   | Open file finder popup (include/exclude fields)  |
| `/`     | Focus search field                               |
| `%`     | Focus replace field                              |
| `A-i`   | Focus include field                              |
| `A-e`   | Focus exclude field                              |
| `zf`    | Focus fixed-strings toggle (z-leader)            |
| `down`  | Switch to results (normal mode)                  |
| `up`    | Switch to results (normal mode)                  |

---

## Search Screen — Results Focused (Normal Mode)

### Navigation

| Key                  | Action                              |
|----------------------|--------------------------------------|
| `j` / `C-n` / `down` | Move to next result (wraps)  |
| `k` / `C-p` / `up`   | Move to previous result (wraps)|
| `C-down` / `J`       | Jump to first match in next file     |
| `C-up` / `K`         | Jump to first match in previous file |
| `C-d`                | Scroll down half a page              |
| `C-u`                | Scroll up half a page                |
| `C-f` / `pagedown`   | Scroll down a full page              |
| `C-b` / `pageup`     | Scroll up a full page                |
| `g` / `home`         | Jump to first result                 |
| `G` / `end`          | Jump to last result                  |

### Mode Switching

| Key         | Action                                        |
|-------------|-----------------------------------------------|
| `Esc`       | Command mode — type a command key (both modes) |
| `i`         | Enter insert mode (fields) — vim-style        |
| `backspace` | Enter insert mode AND delete last char in search |
| `C-o`       | Go back to search fields (insert mode)        |

### Actions

| Key         | Action                                        |
|-------------|-----------------------------------------------|
| `enter`     | Trigger replacement                           |
| `:e` / `e`  | Open current file in `$EDITOR`                |
| `space`     | Toggle inclusion of highlighted result        |
| `a`         | Toggle inclusion of all results               |
| `*`         | Toggle all results in current file            |
| `v`         | Toggle visual / multiselect mode              |
| `o`         | Flip multiselect direction (up vs down)       |

---

## Results Screen (Post-Replacement)

| Key                   | Action                    |
|-----------------------|---------------------------|
| `j` / `down` / `C-n`  | Scroll errors down        |
| `k` / `up` / `C-p`    | Scroll errors up          |
| `enter` / `q`         | Quit scooter              |

---

## Quick Reference: z-Leader Commands

| Key  | Action                                    |
|------|-------------------------------------------|
| `zl` | Toggle preview line wrapping              |
| `zh` | Toggle hidden files                       |
| `zm` | Toggle multiline search                   |
| `ze` | Toggle escape-sequence interpretation     |
| `zc` | Toggle fixed-strings mode                |
| `zw` | Toggle whole-word matching                |
| `zs` | Toggle case-sensitive matching            |
| `zf` | Focus fixed-strings toggle (fields mode)  |

## Quick Reference: Colon Commands

| Key  | Action                    |
|------|---------------------------|
| `:q` | Quit scooter              |
| `:r` | Reset all fields          |
| `:h` | Show help menu            |
| `:e` | Open in editor            |
