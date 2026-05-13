# Scooter — Changelog (since ef032a1)

## New Features

- **Unified single-screen UI** — search fields, results, and preview are always visible simultaneously. No more screen switching.
- **Compact layout** — borderless inline fields use the full terminal width. No wasted space on headers or footers.
- **Inline toggles** — Fixed, Word, and Case sensitive toggles sit on the search row, saving vertical space.
- **Opt-in replacement** — results start unselected. Press `Space` to toggle individual results (auto-advances), `Ctrl+W` to toggle all.
- **File finder popup** — press `Alt+F` in the include/exclude fields to browse and pick files interactively. Configurable via `keys.search.fields.open_file_finder`. Can be overridden with an external command via `search.file_finder_command` (e.g. `fzf --multi`).
- **Fluid navigation** — Up/Down work from anywhere (fields or results). `Ctrl+Up/Down` jumps between files. `Ctrl+Left/Right` resizes the file column.
- **Configurable keybindings** — remap any key via `[keys]` in `config.toml` (see README for full reference).
- **Single-file mode** — pass a file path instead of a directory to search within one file.
- **Preview default 75%** — file column is now 25% by default (was 33%), giving more room to the preview. Adjustable via `preview.file_column_percentage` config or `Ctrl+Left/Right`.
- **Editor re-search** — after editing a file from scooter, results are automatically refreshed.
- **File/match counter** — the counter now shows `files/matches` (e.g. `3/42`) instead of just the match count.
- **Configurable file list height** — `preview.file_list_height` in config or `--file-list-height N` on the CLI to control the compact (narrow terminal) layout.
- **`--editable` / `-E` flag** — unlock CLI-pre-populated fields for editing in the TUI without changing the config.
- **`--no-file-filters` flag** — hides the include/exclude filter fields from the TUI, shrinking the field area from 4 rows to 2 rows (search + replace only). Tab navigation cycles cleanly through the remaining visible fields. The reclaimed space goes to the file list and preview.
- **Substring file filter matching** — include/exclude filter values now match anywhere in the file path (e.g. `test` matches `src/test/main.rs`). Patterns containing `/` or `*` are treated as globs (e.g. `*.rs`, `src/`).
- **Configurable `focus_fields`** — new `search.focus_fields` config option that defines an ordered list of field names for Tab navigation order. Accepted names: `search`, `replace`, `fixed`, `word`, `case`, `include`, `exclude`. When omitted, all fields are focusable in default order. Automatically integrates with `--no-file-filters` to remove hidden fields from the focus list.
- **Default config template** — `default-config.toml` in the repo root contains all configuration sections and their default values, fully commented. Can be copied to `~/.config/scooter/config.toml` as a starting point.
- **Vim bindings document** — `zcooter/vim-bindings.md` documents VimRun-based Vim/Neovim integration with `Scooter`, `VScooter` commands and leader-key mappings for opening scooter on the current file, git repo, or with a visual selection as search text.
- **Esc-prefix command mode** — when search fields are focused, pressing `Esc` first enters command mode so that subsequent keypresses trigger commands (quit, reset, help, toggles) instead of being typed into search/replace text. In results focus, prefix keys work directly without the Esc prefix. This prevents accidental command triggering while editing fields.
- **Configurable toggle bindings** — `toggle_fixed_strings` (default `A-x`), `toggle_match_whole_word` (default `A-w`), `toggle_match_case` (default `A-c`) can now be remapped in `[keys.search]`.
- **Help screen reflects configured bindings** — the help screen displays your actual keybindings from `config.toml`, not hard-coded defaults. Changing a binding in config immediately updates what the help screen shows.
- **Key bindings reference document** — `zcooter/key-bindings.md` provides comprehensive documentation of all keybindings, their defaults, Esc-prefix behavior, and configuration options.
- **Vim-style keybindings config** — `zcooter/vim-config.toml` provides a complete vim-inspired configuration with `z`-leader toggles (`zl` line wrap, `zh` hidden files, `zm` multiline, `zc` fixed strings, `zw` whole word, `zs` case), colon commands (`:q`, `:r`, `:h`), and hjkl navigation.
- **Configurable column resize keys** — `resize_column_shrink` (default `C-left`) and `resize_column_grow` (default `C-right`) in `[keys.search]`.
- **Configurable field-focus keys** — `focus_search_field`, `focus_replace_field`, `focus_include_field`, `focus_exclude_field`, `focus_fixed_field`, and `fields_to_results` in `[keys.search.fields]` allow full control over field navigation.
- **Configurable results keys** — `toggle_current_file_selected` (`*`), `enter_insert_mode` (`i`), `backspace_to_search` (`backspace`) in `[keys.search.results]`.
- **Auto-refresh search after editor** — search results are automatically re-run when returning from the editor, catching any external file changes.

## Bug Fixes

- Fixed panic when pressing keys after replacement completes.
- Fixed "File has changed since search" errors after editing a file or performing a replacement (two-layer cache invalidation).
- Fixed include filter not restricting results to matching files only.
- Fixed missing space between the case sensitive toggle and the match count.
- Fixed replacement not clearing search and replacement text after completion.
- Fixed blank lines appearing when `--no-file-filters` is set (space now reclaimed by file list/preview).
- Fixed escape key delay when navigating back from results to fields — removed `Esc` from `back_to_fields` binding (use `Ctrl+O` instead). The delay was caused by terminal escape sequence disambiguation (~100ms). Removed the stale escape deprecation popup.
- Fixed file finder key (`Ctrl+T`) conflicting with `toggle_hidden_files` (same default key). Changed file finder default to `Alt+F`.
- Fixed field-only keys (Tab, Enter, Shift+Tab) leaking into search text when focus is on results — these keys now correctly trigger their commands instead of being inserted as text.
- Fixed `A-f` key conflict — `toggle_fixed_strings` default changed from `A-f` to `A-x` to avoid shadowing `open_file_finder = "A-f"` in the fields keymap (the `lookup()` method checks `search_common` before focus-specific commands, so the common binding would always win).
- Fixed Alt-fallback for terminals with short escape timeout — in environments like vim's `:terminal`, `Esc+key` sequences get fused into `Alt+key` by the terminal. zcooter now detects this and strips the Alt modifier when an Esc prefix was expected, allowing prefix keys to work in vim terminal.
- Fixed two-char leader-key sequences (e.g., `zc`, `zh`) not being parsed correctly in the config file.

## Breaking Changes

- **UI is completely redesigned** — the old 3-screen layout with bordered fields no longer exists. The compact, borderless, single-screen layout is the only UI.
- **Results default to unselected** — you must explicitly select results before replacing (was: all selected by default).
- **File column default changed** — from 33% to 25% (preview gets 75% instead of 67%).
- **GitHub Actions removed** — CI/CD workflows have been deleted.

## New CLI Flags

| Flag | Short | Description |
|------|-------|-------------|
| `--file-list-height N` | — | Set file list height in compact/narrow layout |
| `--editable` | — | Allow editing of CLI-pre-populated fields (note: `-E` short flag is unavailable due to collision with `--files-to-exclude`) |
| `--no-file-filters` | — | Hide include/exclude filter fields from the TUI |
| `--file-finder-command CMD` | — | External command for file selection in include/exclude fields (overrides config) |
| `--open-file-finder-key KEY` | — | Key that opens the file finder in include/exclude fields (e.g. `"A-f"`) |
| `--config FILE` | — | Use the specified config file instead of the default config.toml |

## New Config Options

| Option | Section | Default | Description |
|--------|---------|---------|-------------|
| `file_column_percentage` | `[preview]` | `25` | File column width as % (10–80) |
| `file_list_height` | `[preview]` | `5` | File list lines in narrow terminal layout |
| `keys.*` | `[keys]` | — | Full keybinding customization (see README) |
| `focus_fields` | `[search]` | — | Ordered list of focusable field names for Tab navigation (e.g. `["search", "replace", "fixed"]`) |
| `file_finder_command` | `[search]` | — | External command for file selection (executed via `sh -c` in the search directory; each stdout line becomes a path). Set to `null` to use the built-in file finder. Example: `"fzf --multi"` |
