# Scooter — Changelog (since ef032a1)

## New Features

- **Unified single-screen UI** — search fields, results, and preview are always visible simultaneously. No more screen switching.
- **Compact layout** — borderless inline fields use the full terminal width. No wasted space on headers or footers.
- **Inline toggles** — Fixed, Word, and Case sensitive toggles sit on the search row, saving vertical space.
- **Opt-in replacement** — results start unselected. Press `Space` to toggle individual results (auto-advances), `Ctrl+W` to toggle all.
- **File finder popup** — press `Ctrl+T` in the include/exclude fields to browse and pick files interactively. Configurable via `keys.search.fields.open_file_finder`. Can be overridden with an external command via `search.file_finder_command` (e.g. `fzf --multi`).
- **Fluid navigation** — Up/Down work from anywhere (fields or results). `Ctrl+Up/Down` jumps between files. `Ctrl+Left/Right` resizes the file column.
- **Configurable keybindings** — remap any key via `[keys]` in `config.toml` (see README for full reference).
- **Single-file mode** — pass a file path instead of a directory to search within one file.
- **Preview default 75%** — file column is now 25% by default (was 33%), giving more room to the preview. Adjustable via `preview.file_column_percentage` config or `Ctrl+Left/Right`.
- **Editor re-search** — after editing a file from scooter, results are automatically refreshed.
- **File/match counter** — the counter now shows `files/matches` (e.g. `3/42`) instead of just the match count.
- **Configurable file list height** — `preview.file_list_height` in config or `--file-list-height N` on the CLI to control the compact (narrow terminal) layout.
- **`--editable` / `-E` flag** — unlock CLI-pre-populated fields for editing in the TUI without changing the config.
- **`--no-file-filters` flag** — hides the include/exclude filter fields from the TUI, shrinking the field area from 4 rows to 2 rows (search + replace only). Tab navigation cycles cleanly through the remaining visible fields.
- **Configurable `focus_fields`** — new `search.focus_fields` config option that defines an ordered list of field names for Tab navigation order. Accepted names: `search`, `replace`, `fixed`, `word`, `case`, `include`, `exclude`. When omitted, all fields are focusable in default order. Automatically integrates with `--no-file-filters` to remove hidden fields from the focus list.

## Bug Fixes

- Fixed panic when pressing keys after replacement completes.
- Fixed "File has changed since search" errors after editing a file or performing a replacement (two-layer cache invalidation).
- Fixed include filter not restricting results to matching files only.
- Fixed missing space between the case sensitive toggle and the match count.
- Fixed replacement not clearing search and replacement text after completion.

## Breaking Changes

- **UI is completely redesigned** — the old 3-screen layout with bordered fields no longer exists. The compact, borderless, single-screen layout is the only UI.
- **Results default to unselected** — you must explicitly select results before replacing (was: all selected by default).
- **File column default changed** — from 33% to 25% (preview gets 75% instead of 67%).
- **GitHub Actions removed** — CI/CD workflows have been deleted.

## New CLI Flags

| Flag | Short | Description |
|------|-------|-------------|
| `--file-list-height N` | — | Set file list height in compact/narrow layout |
| `--editable` | `-E` | Allow editing of CLI-pre-populated fields |
| `--no-file-filters` | — | Hide include/exclude filter fields from the TUI |

## New Config Options

| Option | Section | Default | Description |
|--------|---------|---------|-------------|
| `file_column_percentage` | `[preview]` | `25` | File column width as % (10–80) |
| `file_list_height` | `[preview]` | `5` | File list lines in narrow terminal layout |
| `keys.*` | `[keys]` | — | Full keybinding customization (see README) |
| `focus_fields` | `[search]` | — | Ordered list of focusable field names for Tab navigation (e.g. `["search", "replace", "fixed"]`) |
| `file_finder_command` | `[search]` | — | External command for file selection (executed via `sh -c` in the search directory; each stdout line becomes a path). Set to `null` to use the built-in file finder. Example: `"fzf --multi"` |
