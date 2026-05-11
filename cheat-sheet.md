# Scooter — Default Key Bindings Cheat Sheet

> Quick reference for scooter's default key bindings.
> Launch with: `scooter`

---

## Key Syntax

| Notation    | Meaning                          | Example             |
|-------------|----------------------------------|---------------------|
| `C-x`       | Control + key                    | `C-c`, `C-d`        |
| `A-x`       | Alt + key                        | `A-m`, `A-u`        |
| `S-x`       | Shift + key                      | `S-tab`             |
| `enter`     | Enter key                        | —                   |
| `tab`       | Tab key                          | —                   |
| `backspace` | Backspace key                    | —                   |
| `up/down`   | Arrow keys                       | —                   |
| `pageup/down` | Page navigation keys            | —                   |

---

## General Commands (All Screens)

| Key   | Action                                   |
|-------|------------------------------------------|
| `C-c` | Quit scooter                             |
| `C-r` | Reset all fields, return to search screen|
| `C-h` | Show help menu with all key bindings     |

---

## Search Screen — Common (Both Modes)

| Key       | Action                                              |
|-----------|-----------------------------------------------------|
| `C-l`     | Toggle line wrapping in preview pane                |
| `C-t`     | Toggle hidden files (dotfiles)                      |
| `A-m`     | Toggle multiline regex search                       |
| `A-e`     | Toggle escape-sequence interpretation in replace    |
| `A-f`     | Toggle fixed-strings mode (literal search)          |
| `A-w`     | Toggle whole-word matching                          |
| `A-c`     | Toggle case-sensitive matching                      |
| `C-left`  | Shrink the file-name column width                   |
| `C-right` | Grow the file-name column width                     |

---

## Search Screen — Fields Focused (Insert Mode)

| Key       | Action                                           |
|-----------|---------------------------------------------------|
| `enter`   | Trigger search                                   |
| `tab`     | Focus next field                                 |
| `S-tab`   | Focus previous field                             |
| `A-u`     | Unlock pre-populated CLI fields                  |
| `A-f`     | Open file finder popup (include/exclude fields)  |
| `C-s`     | Focus search field                               |
| `C-r`     | Focus replace field                              |
| `C-i`     | Focus include field                              |
| `C-e`     | Focus exclude field                              |
| `C-t`     | Focus fixed-strings toggle                       |
| `down`    | Switch to results (normal mode)                  |
| `up`      | Switch to results (normal mode)                  |

---

## Search Screen — Results Focused (Normal Mode)

### Navigation

| Key         | Action                              |
|-------------|--------------------------------------|
| `j` / `C-n` / `down` | Move to next result (wraps)  |
| `k` / `C-p` / `up`   | Move to previous result (wraps)|
| `C-down`    | Jump to first match in next file     |
| `C-up`      | Jump to first match in previous file |
| `C-d`       | Scroll down half a page              |
| `C-u`       | Scroll up half a page                |
| `C-f` / `pagedown` | Scroll down a full page         |
| `C-b` / `pageup`   | Scroll up a full page            |
| `g`         | Jump to first result                 |
| `G`         | Jump to last result                  |

### Actions

| Key       | Action                                        |
|-----------|-----------------------------------------------|
| `enter`   | Trigger replacement                           |
| `C-o`     | Go back to search fields (insert mode)        |
| `e`       | Open current file in `$EDITOR`                |
| `space`   | Toggle inclusion of highlighted result        |
| `C-w`     | Toggle inclusion of all results               |
| `v`       | Toggle visual / multiselect mode              |
| `A-;`     | Flip multiselect direction (up vs down)       |
| `*`       | Toggle all results in current file            |

---

## Results Screen (Post-Replacement)

| Key                   | Action                    |
|-----------------------|---------------------------|
| `j` / `down` / `C-n`  | Scroll errors down        |
| `k` / `up` / `C-p`    | Scroll errors up          |
| `enter` / `q`         | Quit scooter              |
