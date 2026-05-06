# Technical Change Summary: ef032a1 → HEAD

## 1. Unified UI — Elimination of 3-Screen Context Switching (commit d21620c)

### Before
The UI had three distinct `Screen` variants rendered in `render()`:
1. **`Screen::SearchFields`** — bordered text fields (7 rows), file list + preview below
2. **`Screen::PerformingReplacement`** — full-screen progress bar replacing everything
3. **`Screen::Results`** — full-screen tallies/errors table

When replacement completed, the user was thrown from screen 1 → screen 2 → screen 3, losing sight of the search fields and results.

### After
Everything is unified into **`Screen::SearchFields`** only (the other two variants remain in the enum but are never transitioned to):
- Replacement progress renders as a **centered banner overlay** on top of the existing layout (`render_replacement_progress_banner()`).
- Replacement results appear as a **dismissable popup** (`Popup::ReplacementResults`), closable with any key press.

### Key changes in `scooter-core/src/app.rs`
- Added `replacement_progress: Option<PerformingReplacementState>` to `SearchFieldsState`
- `perform_replacement()` no longer transitions to `Screen::PerformingReplacement`; instead it stores progress in `replacement_progress`
- Added `Popup::ReplacementResults(ReplaceState)` variant to `Popup` enum
- `ReplacementCompleted` handler now shows a toast instead of transitioning to `Screen::Results`

### Key changes in `scooter/src/ui/view.rs`
- `render()` no longer has a `match` on `Screen::*` for the main layout; it always renders the search-fields layout
- Added `render_replacement_progress_banner()` and `render_replacement_results_popup()`
- Header (`"scooter"` title) and footer (key hints) lines removed to maximize vertical space

---

## 2. Compact, Borderless UI (commit 5b2b398)

### Before
- Bordered `Block::bordered()` text fields with labels like "Search text", "Replace text"
- `"scooter"` header line, key hints footer line
- 90% max-width constraint (`default_width()` at 90%)
- 7 field rows + separator + results + preview
- File names prefixed with `*` for included results
- Numbered result indices

### After
- Compact inline `label: value` fields without borders
- No header or footer lines
- Full terminal width (no margin)
- 4 visual rows: `search:`, `replace:`, `include:`, `exclude:`
- `[x]` checkbox for inclusion status only (no `*` prefix)
- Legacy boxed renderer kept as `_render_search_fields_boxed()` (dead_code)

### Implementation
- Replaced `render_search_field()` (per-field bordered rendering) with `render_compact_search_fields()` (all fields in one pass)
- Labels renamed: "Search text" → "search", "Fixed strings" → "fixed", "Match whole word" → "match", "Match case" → "case sensitive"

---

## 3. Inline Toggle Fields (commit bbe895a)

### Before
`Fixed strings`, `Match whole word`, and `Match case` were separate bordered text fields, each occupying a full row (total 7 field rows).

### After
All three toggles moved to the **right side of the search row**, rendered as `[X]`/`[ ]` checkboxes. Field count dropped from 7 rows to 4 rows. Preview panel widened by 15 characters (stolen from file list area).

### Code changes
- Toggles rendered inline in `render_compact_search_fields()`, row 0
- File list column base width reduced by 15 to compensate
- Toggle navigation still works via Tab/Shift+Tab

---

## 4. Safety: Opt-In Results (commit 435d7e4, issue #2)

### Before
All search results defaulted to `included = true`, meaning pressing Replace would replace everything immediately.

### After
- Results default to `included = false`
- `Space` toggles individual result inclusion **and** auto-advances to the next result (issue #5)
- `Ctrl+W` toggles all results at once (configurable via `keys.search.results.toggle_all_selected`)
- Attempting replacement with no selection shows an error message

### Code changes in `scooter-core/src/app.rs`
- `build_search_results()` and `build_test_results()` set `included: false` by default
- New keybindings: `toggle_all_selected` (default: `Ctrl+W`), Space on results toggles + advances

---

## 5. File Finder Popup (commit 435d7e4, issue #7)

New interactive file/directory browser for include/exclude fields:
- Press `Ctrl+T` in `include:` or `exclude:` fields to open (configurable via `keys.search.fields.open_file_finder`)
- Type-to-filter, navigate with Up/Down/j/k
- Enter to select (appends comma-separated glob to field)
- Esc to close

### Code changes
- `FileFinderState` and `FileFinderTarget` added to `UIState` in `app.rs`
- `OpenFileFinder` command added to `CommandSearchFocusFields` enum in `commands.rs`
- `open_file_finder` keybinding added to `KeysSearchFocusFields` in `keys.rs` (default: `Ctrl+T`)
- New event handling in `handle_file_finder_key()` for file finder navigation
- File finder renders as an overlay popup in `view.rs`

---

## 6. Fluid Navigation Overhaul (commits f8d8f6a, 325772a, e551d47, a24f843)

### Before
- Up/Down only worked when focused on results
- `j`/`k` for per-match navigation, Up/Down also per-match
- No file-level navigation

### After (current state)
- **Up/Down**: navigate one result at a time (wrapping at boundaries), work from **any** focus (fields or results)
- **Ctrl+Up/Down**: jump to previous/next **file** (configurable via `keys.search.results.move_next_file`/`move_prev_file`)
- **Ctrl+Left/Right**: dynamically resize file column percentage (3% steps, range 10–80%)
- **Ctrl+S/R/I/E/T**: focus search/replace/include/exclude/fixed field
- **Ctrl+W**: toggle all selected

### Toggle-all key history
- Originally `a` (commit 435d7e4)
- Changed to `Ctrl+A` (commit f8d8f6a)
- Changed to `Ctrl+G` (commit a24f843, tmux conflict)
- Changed to `Ctrl+W` (commit e551d47)

### Code changes in `scooter-core/src/app.rs`
- `handle_key_event()` intercepts Up/Down/Ctrl+Up/Ctrl+Down/Ctrl+Left/Ctrl+Right at the top level regardless of focus
- File navigation uses `move_to_next_file()`/`move_to_prev_file()` on `SearchState`

---

## 7. Compact Labels & Right-Aligned Toggles (commit adc20cb)

- Labels further shortened: `Search text` → `search`, `Fixed strings` → `fixed`, `Match whole word` → `match`, `Match case` → `case sensitive`, `Replace text` → `replace`, `Files to include` → `include`, `Files to exclude` → `exclude`
- Toggles right-aligned to the right edge of the search row with dynamic spacer
- Removed `[done]` status and search time display; show `searching ...` during search, nothing when complete

---

## 8. Preview Default 75% / Configurable Column Width (commit 6abb560)

### Before
Default file column: 33% (preview 67%)

### After
- Default file column: 25% (preview 75%)
- New config option: `preview.file_column_percentage` (range 10–80)
- Runtime adjustment via `Ctrl+Left`/`Ctrl+Right` (3% steps)

### Code changes in `scooter-core/src/config.rs`
- Added `file_column_percentage: u16` to `PreviewConfig` (default: 25)

---

## 9. Include Filter Fix (commit 6abb560)

### Bug
The `--files-to-include` flag used ripgrep's `--type-add` override system which **whitelisted** matching files but didn't restrict to only those files. Other file types would still be searched.

### Fix
- Added a catch-all ignore pattern (`!*`) **before** the include patterns in the override chain
- This makes the include filter properly **restrict** search results to matching files only

### Code changes in `scooter-core/src/validation.rs`
- `build_ignore_overrides()` now prepends `!*` when include globs are specified

---

## 10. Single-File Mode (commit e551d47)

### Before
`scooter` only accepted directories. If you passed a file, ripgrep would search the parent directory.

### After
- Accepts a single file path as the `directory` argument
- Auto-detected via `args.directory.is_file()`
- File-specific flags (`--hidden`, `--include-git-folders`, `--files-to-include`, `--files-to-exclude`) are rejected with clear errors
- Useful from editors to preview changes on one file before committing

### Code changes in `scooter/src/main.rs`
- `validate_stdin_usage()` rejects file-specific flags when searching a single file
- `InputSource::File` variant added to handle single-file search path

---

## 11. Configurable Key Bindings (commit 6abb560)

### Before
All keybindings were hard-coded in `app.rs`.

### After
- Full keybinding configuration via `[keys]` section in `config.toml`
- Hierarchical: `[keys.general]`, `[keys.search.fields]`, `[keys.search.results]`
- Supports key sequences: `"C-s"`, `"A-u"`, `"enter"`, `"tab"`, `"S-tab"`, `"esc"`, etc.
- Conflict detection at startup with clear error messages
- Key names documented in README

### Code changes
- New `scooter-core/src/config/keys.rs` module
- `KeyMap::from_config()` builds keymaps from config, with conflict detection
- `App` stores `keymaps_compact()` etc. for lookup

---

## 12. Replacement State Reset & Cache Invalidation (commits 34af3fb, 2023591, 50f8232)

### Bug 1: Panic after replacement
Pressing keys after replacement caused panic: `"Focussed on search results but search_state is None"` because `search_state` was cleared but `focussed_section` stayed as `SearchResults`.

### Fix (34af3fb)
- `ReplacementCompleted` handler now: resets `focussed_section` to `SearchFields`, shows toast with match count and file count, adds `num_files: usize` to `ReplaceState`/`ReplaceStats`

### Bug 2: "File has changed since search" after editor
Editing a file from scooter then returning caused preview errors because caches were stale.

### Fix (2023591)
- Added `App::refresh_after_editor()` that invalidates core file cache and re-runs search

### Bug 3: UI caches not cleared
Even after core cache invalidation, UI-level caches (plain windows, highlighted windows, highlighted files, diffs) still served stale content.

### Fix (50f8232)
- Added `cache::clear_caches()` to `scooter/src/ui/cache.rs`
- Added `ui_cache_clear_requested: bool` flag on `App`
- Full `self.reset()` in `ReplacementCompleted` handler
- `refresh_after_editor()` also requests UI cache clear
- `app_runner.rs::draw()` checks flag before each render

---

## 13. Vim/Neovim Bindings in README (commits 760eded, cd49b41)

### Added then removed
- First added terminal-mode vim bindings (commit 760eded)
- Had issues with `feedkeys("scooter \<CR>")` where `\<CR>` was expanded by Vim's mapping engine
- Fixed with simpler `:terminal scooter` + `autocmd TermClose` approach
- Then removed the entire vim terminal-mode section per user request (commit cd49b41)
- Kept neovim-specific integrations (snacks.nvim, ToggleTerm)

---

## 14. Space Before Match Count (commit 4868c0b)

### Bug
The match count `(42)` appeared directly after the `case sensitive` toggle with no space, despite `total_right_width` reserving `+1` for spacing. The reservation was mathematical but no actual space character was rendered.

### Fix
Added explicit `Span::raw(" ")` between toggle spans and the count parenthesis.

---

## 15. Config: `file_list_height` (commit e945572) + CLI: `--file-list-height` (commit 1dfa85a)

### Before
When terminal width ≤ 110, file list and preview are stacked vertically with file list hardcoded to 5 lines.

### After
- Config: `preview.file_list_height = 8` in `config.toml`
- CLI: `scooter --file-list-height 8`
- Default remains 5

### Code changes
- `file_list_height: u16` added to `PreviewConfig` (default: 5)
- `--file-list-height LINES` CLI arg added to `Args` in `main.rs`
- `file_list_height_override: Option<u16>` added to `AppConfig`
- Override applied in `AppRunner::new_runner()`

---

## 16. CLI: `--editable` / `-E` (commit 7eae0af)

### Before
Fields pre-populated by CLI flags (e.g. `--search-text`) were locked (non-editable) in the TUI. Only configurable via `search.disable_prepopulated_fields = false` in config.

### After
- `scooter -E -s "foo"` or `scooter --editable --search-text "foo"`
- Overrides `search.disable_prepopulated_fields` from `true` to `false`

### Code changes
- `--editable` CLI arg (short: `-E`)
- `editable_override: bool` in `AppConfig`
- Applied in `AppRunner::new_runner()`: sets `user_config.search.disable_prepopulated_fields = false`

---

## 17. File/Match Counter (commit f12db37)

### Before
Counter showed `(  42)` — match count only, 8 chars reserved.

### After
Counter shows `3/42` — `files/matches` format, 12 chars reserved.

### Code changes in `scooter/src/ui/view.rs`
- `COUNT_WIDTH` changed from 8 to 12
- Format changed from `({num_results:>6})` to `{num_files}/{num_results}`
- `num_files` computed from unique `search_result.path` values via `HashSet`

---

## 18. GitHub Actions Workflows Removed (commit b325d2e)

All CI workflows removed:
- `.github/workflows/test.yml`
- `.github/workflows/release.yml`
- `.github/workflows/publish-core.yml`
- `.github/workflows/block-unwanted-files.yml`
- `.github/dependabot.yml`

---

## 19. `--no-file-filters` Flag & Configurable `focus_fields` (commit f4358ce)

### Before
- All 7 field names were always present in the TUI (search, replace, fixed, word, case, include, exclude).
- Tab navigation used sequential indices (`max_focusable_field()` returning a fixed upper bound) and simply cycled 0..=N.
- Ctrl+S/R/I/E/T shortcuts jumped to fixed field indices unconditionally.

### After

#### `--no-file-filters` flag
- New CLI flag that hides include/exclude filter fields from the TUI.
- Field area shrinks from 4 rows to 2 rows (only `search:` and `replace:` shown).
- `show_file_filters: bool` added to `AppRunConfig` (propagated from CLI flag).
- `view.rs` conditionally skips rendering include/exclude rows when `show_file_filters` is false.
- Tab navigation cleanly cycles through the remaining visible fields with no hidden fields to skip.

#### `focus_fields` config option
- New `search.focus_fields` config option: an ordered `Vec<String>` of field names defining Tab navigation order and which fields receive focus.
- Accepted names: `"search"`, `"replace"`, `"fixed"`, `"word"`, `"case"`, `"include"`, `"exclude"`.
- When omitted (`None`), all fields are focusable in default order.
- `--no-file-filters` automatically removes `"include"` and `"exclude"` from the focus list.
- Custom deserializer `deserialize_focus_fields` validates field names and rejects unknown names at config load time.

#### `focus_field_indices()` replacing `max_focusable_field()`
- Old approach: `max_focusable_field()` returned a fixed integer; Tab cycled 0..=N with skip logic for non-editable fields.
- New approach: `focus_field_indices()` in `app.rs` computes an ordered list of focusable field indices from the `focus_fields` config (or default list).
- Tab/Shift+Tab navigation uses this ordered list directly — no skip logic needed.

#### `focus_impl()` in `fields.rs`
- New `focus_impl()` method navigates using the focus list instead of sequential indices.
- `focus_field()` now checks whether the target field index is in the focus list; if not, the call is a no-op.
- Ctrl+S/R/I/E/T shortcuts each check focus list membership before moving focus.

### Code changes

| File | Changes |
|------|----------|
| `scooter/src/main.rs` | `--no-file-filters` CLI arg, `show_file_filters` in `AppRunConfig` |
| `scooter-core/src/config.rs` | `focus_fields` field on `SearchConfig`, `deserialize_focus_fields` validator |
| `scooter/src/ui/view.rs` | Conditional rendering of include/exclude rows based on `show_file_filters` |
| `scooter-core/src/app.rs` | `focus_field_indices()` replacing `max_focusable_field()`, Tab/Shift+Tab uses focus list, Ctrl+ shortcuts check membership |
| `scooter-core/src/fields.rs` | New `focus_impl()` for focus-list-based navigation, `focus_field()` membership check |

---

## Summary of Changed Files

| File | Lines changed | Nature |
|------|--------------|--------|
| `scooter-core/src/app.rs` | +400/−200 | Core state management, navigation, replacement flow |
| `scooter/src/ui/view.rs` | +500/−300 | Complete UI rewrite (bordered → compact, unified layout) |
| `scooter-core/src/config.rs` | +18 | New config options (file_column_percentage, file_list_height) |
| `scooter-core/src/config/keys.rs` | +16 | Keybinding config support |
| `scooter-core/src/fields.rs` | +14 | disable_prepopulated_fields support, `focus_impl()`, `focus_field()` membership check |
| `scooter-core/src/replace.rs` | +15 | num_files tracking in ReplaceState/ReplaceStats |
| `scooter-core/src/validation.rs` | +22 | Include filter fix, single-file validation |
| `scooter-core/src/search.rs` | +6 | Minor adjustments |
| `scooter-core/src/commands.rs` | +4 | Keybinding command support |
| `scooter-core/tests/app.rs` | +5 | Test updates for new ReplaceState fields |
| `scooter/src/app_runner.rs` | +25 | Cache clearing, CLI overrides, editor refresh |
| `scooter/src/main.rs` | +42 | New CLI args, single-file mode, validation, `--no-file-filters` |
| `scooter/src/ui/cache.rs` | +16 | `clear_caches()` function |
| `README.md` | +14/−? | Documentation updates |
