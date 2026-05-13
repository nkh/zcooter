# Configurable Key Bindings

## Overview

All key bindings in zcooter are now fully configurable via the `[keys]` section of your config file (`~/.config/scooter/config.toml`). Previously hardcoded shortcuts (Ctrl+S, Ctrl+R, Ctrl+I, Ctrl+E, Ctrl+T, Ctrl+W, Ctrl+Left/Right, Up/Down arrows) have been moved into the config system.

## New Commands

| Command | Section | Description |
|---------|---------|-------------|
| `focus_search_field` | `search.fields` | Jump to the search field from anywhere |
| `focus_replace_field` | `search.fields` | Jump to the replace field from anywhere |
| `focus_include_field` | `search.fields` | Jump to the include files field |
| `focus_exclude_field` | `search.fields` | Jump to the exclude files field |
| `focus_fixed_field` | `search.fields` | Jump to the fixed strings toggle |
| `fields_to_results` | `search.fields` | Switch focus from fields to results |
| `resize_column_shrink` | `search` | Decrease file name column width by 3% |
| `resize_column_grow` | `search` | Increase file name column width by 3% |
| `toggle_current_file_selected` | `search.results` | Select/deselect all results in the current file |
| `enter_insert_mode` | `search.results` | Switch from results to fields focus |
| `backspace_to_search` | `search.results` | Focus search field and delete last character |

## Prefix-Key System

Zcooter supports two types of two-key sequences:

- **Colon-prefix** (`:x`) — Press `:` then a letter. Modelled after vim's `:command` system.
- **Leader-key** (`xy`) — Two letters typed in quick succession. The first letter is the "leader". Used in the vim config with `z` as the leader for toggle commands.

### Esc-prefix (command mode)

When the search fields are focused (insert mode), bare letter keys are entered as text. To type command keys like `:q`, `/`, or `zl`, press **Esc** first to enter command mode:

```
Esc : q      quit
Esc /        focus search field
Esc z l      toggle line wrapping
Esc Esc      cancel (no-op)
```

In results focus (normal mode), prefix keys (`:`, `z`) and field commands (`/`, `%`) are handled directly without needing Esc — pressing Esc in results focus is a no-op.

Control keys, Alt keys, Tab, and Enter always work as commands directly without needing Esc.

If no matching second key is pressed after the prefix, the prefix is silently discarded.

### Built-in prefix bindings (vim config)

| Sequence | Command |
|----------|---------|
| `:q` | Quit |
| `:r` | Reset |
| `:h` | Show help |
| `:e` | Open in editor |

To configure prefix-key sequences in your config, use the `:x` format:

```toml
[keys.general]
quit = ":q"
```

## Default Key Bindings

### General (all screens)

| Key | Command |
|-----|---------|
| `C-c` | Quit |
| `C-r` | Reset |
| `C-h` | Show help menu |

### Search screen (common)

| Key | Command |
|-----|---------|
| `C-l` | Toggle preview wrapping |
| `C-t` | Toggle hidden files |
| `A-m` | Toggle multiline |
| `A-e` | Toggle escape sequences |
| `A-x` | Toggle fixed strings |
| `A-w` | Toggle whole word |
| `A-c` | Toggle case sensitive |
| `C-left` | Shrink file column |
| `C-right` | Grow file column |

### Search fields focus

| Key | Command |
|-----|---------|
| `enter` | Trigger search |
| `tab` | Focus next field |
| `S-tab` | Focus previous field |
| `A-u` | Unlock prepopulated fields |
| `A-f` | Open file finder |
| `C-s` | Focus search field |
| `C-r` | Focus replace field |
| `C-i` | Focus include field |
| `C-e` | Focus exclude field |
| `C-t` | Focus fixed field |
| `down` / `up` | Switch to results focus |

### Search results focus

| Key | Command |
|-----|---------|
| `j` / `C-n` / `down` | Move down |
| `k` / `C-p` / `up` | Move up |
| `C-down` | Next file |
| `C-up` | Previous file |
| `C-d` | Down half page |
| `C-u` | Up half page |
| `C-f` / `pagedown` | Down full page |
| `C-b` / `pageup` | Up full page |
| `g` | Move to top |
| `G` | Move to bottom |
| `space` | Toggle selected inclusion |
| `C-w` | Toggle all selected |
| `v` | Toggle multiselect mode |
| `A-;` | Flip multiselect direction |
| `*` | Toggle current file selected |
| `i` | Enter insert mode |
| `backspace` | Backspace to search |
| `enter` | Trigger replacement |
| `C-o` | Back to fields |
| `e` | Open in editor |
| `Esc` | Command mode (type a command key) |

## Vim-Style Config

A vim-style configuration is provided at `vim-config.toml` in the repository root. To use it:

```bash
cp vim-config.toml ~/.config/scooter/config.toml
```

Key differences from defaults:
- `:q` / `:r` / `:h` prefix sequences for quit/reset/help
- `j`/`k` for navigation (retained from defaults)
- `J`/`K` for next/previous file
- `/` to focus search, `%` to focus replace
- `<`/`>` as alternatives for column resize

## Migration Guide

### For existing users

No action required. The default config preserves all existing bindings exactly as they were hardcoded. If you have a custom config, you may need to add the new fields.

### Removed hardcoded behavior

The following shortcuts are no longer hardcoded and must be configured:

- **A-x** (toggle fixed strings) — now bound via `toggle_fixed_strings = "A-x"` in `[keys.search]` (default was previously `A-f`, which conflicted with `open_file_finder`)
- **C-w** (toggle all files) — now bound via `toggle_all_selected = "C-w"` in `[keys.search.results]`
- **C-s** (focus search) — now `focus_search_field = "C-s"` in `[keys.search.fields]`
- **C-r** (focus replace) — now `focus_replace_field = "C-r"` in `[keys.search.fields]`
- **C-i** (focus include) — now `focus_include_field = "C-i"` in `[keys.search.fields]`
- **C-e** (focus exclude) — now `focus_exclude_field = "C-e"` in `[keys.search.fields]`
- **C-t** (focus fixed) — now `focus_fixed_field = "C-t"` in `[keys.search.fields]`
- **C-left** / **C-right** (resize columns) — now in `[keys.search]`
- **Up/Down** in fields focus (switch to results) — now `fields_to_results = ["down", "up"]` in `[keys.search.fields]`
- **Esc** — now activates command mode in fields focus; press Esc then a command key. In results focus, prefix keys and field commands work directly. Previously was used to exit multiselect (removed).

If you had these in a custom config without the new fields, they will get the defaults automatically via `serde(default)`.
