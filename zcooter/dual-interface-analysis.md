# Analysis: Supporting Old and New Interfaces Simultaneously

## Current State

The old interface (as of `ef032a1`) had:
- **3 separate screens**: SearchFields, PerformingReplacement, Results
- **Bordered text fields** in a 90%-width centered box, 7 rows tall
- **Header** ("scooter" title) and **footer** (key hints)
- **Match-level navigation** only (j/k)
- **Results always included** (opt-out replacement)
- All keybindings hard-coded

The new interface has:
- **Single unified screen** with inline replacement progress and popup results
- **Compact borderless fields** using full terminal width, 4 rows tall
- **Inline toggles** on the search row
- **Fluid navigation** (Up/Down from anywhere, Ctrl+Up/Down for files)
- **Opt-in results** (Space to toggle, Ctrl+W for all)
- **Configurable keybindings**

## What Would Need to Change

### 1. Config Option

Add a top-level config entry:

```toml
[ui]
layout = "compact"   # "classic" or "compact" (default: "compact")
```

Corresponding CLI flag: `--classic-ui` / `-C` (sets `layout = "classic"`).

### 2. Enum / State

Add a layout mode enum accessible from `App`:

```rust
// In config
pub enum UILayout {
    Classic,  // old bordered 3-screen
    Compact,  // new borderless unified
}
```

This would be stored in `Config` and propagated to the `App` struct (or read from `app.config` wherever the view is rendered).

### 3. View Layer (`scooter/src/ui/view.rs`)

This is the largest area of change. The `render()` function would need to branch:

```rust
pub fn render(app: &mut App, frame: &mut Frame<'_>) {
    match app.config.ui.layout {
        UILayout::Classic => render_classic(app, frame),
        UILayout::Compact => render_compact(app, frame),
    }
}
```

**For the classic renderer:**
- The old `render()` dispatch logic (header/footer, Screen match, 90% width) would need to be restored or kept from `ef032a1`.
- The old `render_search_field()` function (bordered fields) still exists in the git history and could be resurrected.
- The old 3-screen flow: `Screen::SearchFields`, `Screen::PerformingReplacement`, `Screen::Results` — these enum variants already exist; the code that transitions to them would need to be re-enabled conditionally.
- The old `render_search_results()` call signature differs (it takes `wrap_text` but not `file_column_width_pct` or `file_list_height`).

**For the compact renderer:**
- Keep the current `render()` body as-is.

**Shared code that works for both:**
- `build_search_results()` — result list building logic
- `build_preview()` — preview window/diff rendering
- Highlighting, syntax coloring, diff computation
- Cache layer (`scooter/src/ui/cache.rs`)

**Layout-specific code that diverges:**
- Field rendering (bordered vs compact inline)
- Toggle placement (separate rows vs inline)
- Match count format (`(42)` vs `3/42`)
- Navigation behavior (Up/Down scope, wrapping)
- Default inclusion state (opt-in vs opt-out) — this is in `app.rs`, not `view.rs`

### 4. App Logic (`scooter-core/src/app.rs`)

Several behavioral differences are in the core, not just the view:

| Aspect | Classic | Compact | Impact |
|--------|---------|---------|--------|
| Default `included` | `true` | `false` | `build_search_results()` in app.rs |
| Replacement screen | `Screen::PerformingReplacement` | inline banner | `perform_replacement()` transition |
| Results screen | `Screen::Results` | popup | `ReplacementCompleted` handler |
| Replacement reset | partial (keep search text) | full `self.reset()` + toast | `ReplacementCompleted` handler |
| Up/Down scope | results focus only | any focus | `handle_key_event()` top-level interception |
| File navigation | no Up/Down file nav | Ctrl+Up/Down | key interception in `handle_key_event()` |
| Column resizing | no | Ctrl+Left/Right | key interception |
| Toggle-all default | `a` | `Ctrl+W` | keybinding defaults |
| Match count format | `(  42)` | `3/42` | view.rs only |

**Approach:** Make these behaviors conditional on the layout mode:

```rust
// In App methods:
if self.config.ui.layout == UILayout::Classic {
    // old behavior: transition to Screen::PerformingReplacement
} else {
    // new behavior: inline banner
}
```

This affects roughly 10–15 locations in `app.rs`.

### 5. Keybindings (`scooter-core/src/config/keys.rs`)

The keymap defaults should differ by layout:
- Classic: `a` for toggle-all, no `Ctrl+Up/Down` file nav, no `Ctrl+Left/Right` resize
- Compact: `Ctrl+W` for toggle-all, `Ctrl+Up/Down` file nav, `Ctrl+Left/Right` resize

**Approach:** `KeyMap::from_config()` already takes a `KeysConfig`. The defaults could be selected based on layout:

```rust
impl KeyMap {
    pub fn from_config(keys: &KeysConfig, layout: UILayout) -> Result<Self, Vec<KeyConflict>> {
        let defaults = match layout {
            UILayout::Classic => &CLASSIC_DEFAULTS,
            UILayout::Compact => &COMPACT_DEFAULTS,
        };
        // merge user config on top of defaults
    }
}
```

### 6. Fields (`scooter-core/src/fields.rs`)

Minimal changes:
- `disable_prepopulated_fields` behavior is already config-driven, no change needed.
- Compact labels ("search", "fixed", etc.) vs classic labels ("Search text", "Fixed strings") — could be driven by the layout enum.
- **`focus_fields` config** (commit f4358ce): The new `search.focus_fields` option defines Tab navigation order per field name. In a dual-interface setup, the two layouts may want different default tab orders (e.g., classic has 7 rows with different priorities than compact's 4 rows). The focus list mechanism would need to accept layout-specific defaults or allow the user to set separate focus lists per layout. The `focus_impl()` and `focus_field_indices()` functions already decouple navigation from fixed sequential indices, making this extension straightforward — the default focus list just needs to be selected based on the active layout.

### 7. Replace State (`scooter-core/src/replace.rs`)

- `num_files` field on `ReplaceState`/`ReplaceStats` — keep unconditionally (harmless for classic).
- No other changes needed.

### 8. Config (`scooter-core/src/config.rs`)

Add the `UILayout` enum and `[ui]` section:

```toml
[ui]
# "classic" = old bordered 3-screen interface
# "compact" = new borderless unified interface (default)
layout = "compact"
```

### 9. CLI (`scooter/src/main.rs`)

Add `--classic-ui` / `-C` flag that sets `layout = "classic"` override (same pattern as `--editable`, `--file-list-height`).

### 10. Tests

- All existing tests should pass for compact mode (current behavior).
- Classic mode would need its own tests or the existing tests would need to be parameterized.
- Integration tests that check snapshot output would need mode-specific expected output.

## Estimated Effort

| Component | Effort | Notes |
|-----------|--------|-------|
| Config + CLI flag | Small | Add enum, config section, CLI arg |
| `view.rs` dual render | Large | ~700 lines of classic renderer to restore/maintain, plus branching |
| `app.rs` conditional behavior | Medium | ~15 conditional blocks for screen transitions, defaults, navigation |
| Keybinding defaults | Medium | Split defaults by layout, merge in `from_config()` |
| Tests | Medium | Parameterize or duplicate for both modes |
| Labels / field names | Small | Conditional label selection |
| Match count format | Small | Conditional format in view.rs |

**Total: Large** — roughly 800–1200 lines of restored/adapted code plus ~500 lines of conditional branching. The main cost is maintaining two parallel view renderers and two sets of behavioral defaults going forward.

## Recommendation

If the goal is to let existing users opt into the classic layout while new users get compact:

1. **Make compact the default** (as it is now).
2. **Restore the classic renderer** as a separate function branch in `view.rs`.
3. **Gate behavioral differences** on the layout config in `app.rs` (screen transitions, inclusion defaults, navigation scope).
4. **Split keybinding defaults** by layout in `keys.rs`.
5. **Add `--classic-ui`** CLI flag and `[ui] layout = "classic"` config option.

The hardest part is the view layer — maintaining two renderers means any future UI change needs to be applied in both (or one is left to stagnate).
