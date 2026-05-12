# Zcooter Key Bindings

Zcooter provides two key binding profiles:

- **Default bindings** — shipped in `default-config.toml`, preserved for backward compatibility with the original scooter project.
- **Vim bindings** — shipped in `zcooter/vim-config.toml`, offering a vim-inspired layout with prefix-key sequences like `:q`.

To switch, copy the desired config to `~/.config/scooter/config.toml`.

## Default Bindings

All bindings below are configurable. The key format uses `C-` for Ctrl, `A-` or `M-` for Alt, `S-` for Shift. Prefix sequences like `:q` are written as-is.

### General (all screens)

| Action | Key | Notes |
|--------|-----|-------|
| Quit | `C-c` | |
| Reset (clear all) | `C-r` | Cancels search, clears fields, resets state |
| Help menu | `C-h` | |

### Search screen — both focuses

| Action | Key | Notes |
|--------|-----|-------|
| Toggle preview wrapping | `C-l` | |
| Toggle hidden files (dotfiles) | `C-t` | |
| Toggle multiline search | `A-m` | Allows regex to span multiple lines |
| Toggle escape sequences in replace | `A-e` | `\n` becomes newline, `\t` tab, `\\` backslash |
| Toggle fixed strings | `A-f` | Literal search (no regex) |
| Toggle whole word | `A-w` | Match whole words only |
| Toggle case sensitive | `A-c` | Case-sensitive matching |
| Shrink file column | `C-left` | Minimum 10% |
| Grow file column | `C-right` | Maximum 80% |

### Search screen — fields focused

| Action | Key | Notes |
|--------|-----|-------|
| Focus search field | `C-s` | |
| Focus replace field | `C-r` | |
| Focus include field | `C-i` | |
| Focus exclude field | `C-e` | |
| Focus fixed strings toggle | `C-t` | |
| Switch to results | `down` / `up` | Both arrows enter results focus |
| Trigger search | `enter` | |
| Next field (Tab) | `tab` | |
| Previous field (Shift+Tab) | `S-tab` | |
| Unlock CLI-prepopulated fields | `A-u` | |
| Open file finder | `A-f` | Include/exclude fields only |

### Search screen — results focused

#### Navigation

| Action | Key | Notes |
|--------|-----|-------|
| Move down | `j` / `C-n` / `down` | Wraps to top |
| Move up | `k` / `C-p` / `up` | Wraps to bottom |
| Jump to next file | `C-down` | First match in next file |
| Jump to previous file | `C-up` | First match in previous file |
| Down half page | `C-d` | |
| Up half page | `C-u` | |
| Down full page | `C-f` / `PageDown` | |
| Up full page | `C-b` / `PageUp` | |
| Jump to first result | `g` | |
| Jump to last result | `G` | |

#### Actions

| Action | Key | Notes |
|--------|-----|-------|
| Execute replacement | `enter` | On all selected results |
| Back to fields | `C-o` | Return focus to search fields |
| Open in editor | `e` | Opens file at match line |
| Toggle result inclusion | `Space` | Toggles current result, auto-advances |
| Toggle all results | `C-w` | Select/deselect everything |
| Toggle current file | `*` | Select/deselect all matches in current file |
| Enter insert mode | `i` | Switch focus to fields |
| Backspace to search | `Backspace` | Focus search field, delete last char |
| Visual select mode | `v` | Select a range of results |
| Flip visual direction | `A-;` | Swap anchor in visual select |
| Command mode | `Esc` | Next key is treated as command, not text |

### Results screen (post-replacement)

| Action | Key | Notes |
|--------|-----|-------|
| Scroll errors down | `j` / `down` / `C-n` | |
| Scroll errors up | `k` / `up` / `C-p` | |
| Quit | `enter` / `q` | |

### Implicit behaviors

| Context | Behavior | Notes |
|---------|----------|-------|
| File finder open | `Esc` closes, `Enter` confirms, `j/k/Up/Down` navigate, `Backspace` deletes from query | Hardcoded — modal overlay |
| Popup open | `Esc` closes it | |
| `Esc` pressed (fields or results focus) | Activates command mode — next key is treated as command | Use `Esc : q` to quit from fields, `Esc Esc` to cancel |
| Unbound printable char (results focus) | Switches to fields focus and types the char | Text input fallback |
| Unbound printable char (fields focus) | Typed into the current field | Standard text entry |

---

## Differences from the Original Scooter Project

Zcooter forked from [scooter](https://github.com/nicbarker/scooter) at commit `ef032a1`. The key bindings have evolved significantly:

### Removed bindings

| Binding | Original purpose | Reason for removal |
|---------|-----------------|-------------------|
| `Esc` → back to fields | Exit results, return to search | Replaced by `C-o`. Esc now activates command mode instead. |
| `Ctrl+T` → open file finder | Open file finder popup | Conflicted with `toggle_hidden_files`. File finder moved to `A-f`. |

### Changed default keys

| Action | Original scooter | Zcooter | Reason |
|--------|-----------------|---------|--------|
| Toggle all selected | `C-a` → `C-g` → `C-w` | `C-w` | Avoided tmux conflict (`C-a`) and git conflict (`C-g`) |
| Open file finder | `C-t` | `A-f` | Avoided conflict with `toggle_hidden_files` |
| Back to fields | `C-o`, `Esc` | `C-o` only | Esc now activates command mode |

### New commands (not in original scooter)

| Action | Key (default) | Key (vim) | Description |
|--------|---------------|-----------|-------------|
| Focus search field | `C-s` | `/` | Jump directly to search field |
| Focus replace field | `C-r` | `%` | Jump directly to replace field |
| Focus include field | `C-i` | `A-i` | Jump to include files field |
| Focus exclude field | `C-e` | `A-e` | Jump to exclude files field |
| Focus fixed field | `C-t` | `zf` | Jump to fixed strings toggle |
| Switch fields → results | `down`/`up` | `down`/`up` | Was hardcoded, now configurable |
| Shrink file column | `C-left` | `C-left`/`<` | Was hardcoded, now configurable |
| Grow file column | `C-right` | `C-right`/`>` | Was hardcoded, now configurable |
| Toggle current file | `*` | `*` | Select/deselect all matches in the current file |
| Enter insert mode | `i` | `i` | Switch from results to fields focus |
| Backspace to search | `Backspace` | `Backspace` | Focus search + delete last char |

### Structural changes

All key bindings that were previously hardcoded in `handle_special_cases()` are now fully configurable through the `[keys]` section in `config.toml`. This includes the Ctrl+letter field shortcuts (`C-s`, `C-r`, `C-i`, `C-e`, `C-t`), Ctrl+arrow column resize, and bare Up/Down arrow transitions between fields and results. Users who prefer the original behavior can set these in their config — the defaults match the previous hardcoded values exactly.

---

## Vim Bindings

The vim profile (`zcooter/vim-config.toml`) maps keys to familiar vim equivalents and adds prefix-key sequences for common commands.

### What changed vs default

| Action | Default | Vim | Rationale |
|--------|---------|-----|-----------|
| Quit | `C-c` | `:q`, `C-c` | Vim's `:q` via prefix-key system |
| Reset | `C-r` | `:r` | Prefix sequence |
| Help | `C-h` | `:h`, `?` | Vim's `:help` and `?` |
| Focus search | `C-s` | `/` | Vim's search command |
| Focus replace | `C-r` | `%` | Vim's substitute |
| Focus fixed | `C-t` | `zf` | `z` leader for toggles |
| Focus include | `C-i` | `A-i` | Avoids conflict |
| Focus exclude | `C-e` | `A-e` | Avoids conflict |
| Preview wrapping | `C-l` | `zl` | `z` leader for preview |
| Hidden files | `C-t` | `zh` | `z` leader, frees C-t |
| Multiline | `A-m` | `zm` | `z` leader |
| Escape sequences | `A-e` | `ze` | `z` leader |
| Toggle all | `C-w` | `a` | `a` = all (frees C-w) |
| Flip multiselect dir | `A-;` | `o` | Vim's visual mode `o` |
| Open in editor | `e` | `:e`, `e` | Prefix sequence for `:edit` |
| Jump to first | `g` | `g`, `Home` | Arrow support |
| Jump to last | `G` | `G`, `End` | Arrow support |
| Next file | `C-down` | `C-down`, `J` | Shift+J/K for file navigation |
| Prev file | `C-up` | `C-up`, `K` | |
| Shrink column | `C-left` | `C-left`, `<` | Additional `<` binding |
| Grow column | `C-right` | `C-right`, `>` | Additional `>` binding |

### Prefix-key sequences

The prefix-key system allows two-key sequences like `:q`. Press the first key (the prefix), then the second key within the same keypress. If no matching sequence exists, the prefix is discarded and the second key is processed normally.

| Sequence | Action | Available in vim profile |
|----------|--------|------------------------|
| `:q` | Quit | Yes |
| `:r` | Reset | Yes |
| `:h` | Help menu | Yes |
| `:e` | Open in editor | Yes |

To add prefix sequences to a custom config, include them in the key array:

```toml
[keys.general]
quit = [":q", "C-c", "q"]
```

The prefix `:` is consumed on the first keypress. The second keypress triggers the command if the sequence is registered, or is processed normally if not.

### Esc-prefix (command mode)

When the search fields are focused (insert mode), bare letter keys are entered as text. To type command keys like `:q`, `/`, or `zl`, press **Esc** first to enter command mode. This also works in results focus (normal mode).

```
Esc : q      quit
Esc /        focus search field
Esc z l      toggle line wrapping
Esc Esc      cancel (no-op)
```

Control keys, Alt keys, Tab, and Enter always work as commands directly without needing Esc.
