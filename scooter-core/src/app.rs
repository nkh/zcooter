use std::{
    cmp::{max, min},
    collections::HashMap,
    io::Cursor,
    iter::{self, Iterator},
    mem,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use fancy_regex::Regex as FancyRegex;
use ignore::WalkState;
use log::{debug, warn};
use tokio::{
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    task::{self, JoinHandle},
};

use crate::{
    commands::{
        Command, CommandGeneral, CommandSearchFields, CommandSearchFocusFields,
        CommandSearchFocusResults, KeyMap, display_conflict_errors,
    },
    config::Config,
    errors::AppError,
    fields::{FieldName, SearchFieldValues, SearchFields},
    file_content::{FileContentProvider, default_file_content_provider},
    keyboard::{KeyCode, KeyEvent, KeyModifiers},
    line_reader::{BufReadExt, LineEnding},
    replace::{self, PerformingReplacementState, ReplaceState},
    replace::{replace_all_if_match, replacement_for_match, replacement_for_match_in_haystack},
    search::Searcher,
    search::{
        FileSearcher, MatchContent, ParsedSearchConfig, SearchResult, SearchResultWithReplacement,
        SearchType, contains_search, search_multiline,
    },
    utils::{Either, Either::Left, Either::Right, ceil_div},
    validation::{
        DirConfig, SearchConfig, ValidationErrorHandler, ValidationResult,
        validate_search_configuration,
    },
};

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(300);
const PREVIEW_DEBOUNCE: Duration = Duration::from_millis(300);

/// Spawn a task that sleeps for `delay` and then runs `on_fire`. Used to
/// debounce both search and preview-replacement refreshes.
fn spawn_debounced<F>(delay: Duration, on_fire: F) -> JoinHandle<()>
where
    F: FnOnce() + Send + 'static,
{
    tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        on_fire();
    })
}

#[derive(Debug, Clone)]
pub enum InputSource {
    Directory(PathBuf),
    Stdin(Arc<String>),
}

#[derive(Debug)]
pub enum ExitState {
    Stats(ReplaceState),
    StdinState(ExitAndReplaceState),
}

#[derive(Debug)]
pub enum EventHandlingResult {
    Rerender,
    Exit(Option<Box<ExitState>>),
    None,
    /// Request the TUI runner to suspend the terminal, run an external file finder
    /// command, and insert the selected paths into the target field.
    ExternalFileFinder {
        command: String,
        target: FileFinderTarget,
        base_dir: PathBuf,
    },
}

impl EventHandlingResult {
    pub(crate) fn new_exit_stats(stats: ReplaceState) -> EventHandlingResult {
        Self::new_exit(ExitState::Stats(stats))
    }

    fn new_exit(exit_state: ExitState) -> EventHandlingResult {
        EventHandlingResult::Exit(Some(Box::new(exit_state)))
    }
}

#[derive(Debug)]
pub enum BackgroundProcessingEvent {
    AddSearchResult(SearchResult),
    AddSearchResults(Vec<SearchResult>),
    SearchCompleted,
    ReplacementCompleted(ReplaceState),
    UpdateReplacements {
        start: usize,
        end: usize,
        cancelled: Arc<AtomicBool>,
    },
    UpdateAllReplacements {
        cancelled: Arc<AtomicBool>,
    },
}

#[derive(Debug)]
pub enum AppEvent {
    PerformSearch { generation: u64 },
    DismissToast { generation: u64 },
    /// Re-run the current search because the on-disk file contents may have
    /// changed since the last search was performed (e.g. the user saved the
    /// file in their editor while zcooter was open).
    RetrySearch,
}

#[derive(Debug)]
pub enum InternalEvent {
    App(AppEvent),
    Background(BackgroundProcessingEvent),
}

#[derive(Debug)]
pub struct ExitAndReplaceState {
    pub stdin: Arc<String>,
    pub search_config: ParsedSearchConfig,
    pub replace_results: Vec<SearchResultWithReplacement>,
}

#[derive(Debug)]
pub enum Event {
    LaunchEditor((PathBuf, usize)),
    ExitAndReplace(ExitAndReplaceState),
    Rerender,
    Internal(InternalEvent),
}

#[derive(Debug, PartialEq, Eq)]
struct MultiSelected {
    anchor: usize,
    primary: usize,
}
impl MultiSelected {
    fn ordered(&self) -> (usize, usize) {
        if self.anchor < self.primary {
            (self.anchor, self.primary)
        } else {
            (self.primary, self.anchor)
        }
    }

    fn flip_direction(&mut self) {
        (self.anchor, self.primary) = (self.primary, self.anchor);
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Selected {
    Single(usize),
    Multi(MultiSelected),
}

/// Lifecycle of a single search, modelled explicitly so the view and the
/// event handlers can't disagree about what's currently happening.
///
/// Typical transitions: `Pending` → `Running` → `Complete`. `Pending` models
/// the window between a user edit and the debounced search actually starting;
/// `Invalid` means the current inputs no longer describe a runnable search but
/// stale results may still be visible; `Running` is an in-flight search;
/// `Complete` is terminal.
#[derive(Clone, Copy, Debug)]
pub enum SearchPhase {
    Pending,
    Invalid,
    Running {
        started: Instant,
    },
    Complete {
        started: Instant,
        completed: Instant,
    },
}

impl SearchPhase {
    pub fn is_complete(self) -> bool {
        matches!(self, SearchPhase::Complete { .. })
    }

    /// Wall-clock time for the search this phase describes. `None` for
    /// `Pending` — the debounce hasn't fired yet, so there's no meaningful
    /// elapsed time to report.
    pub fn elapsed(self) -> Option<Duration> {
        match self {
            SearchPhase::Pending | SearchPhase::Invalid => None,
            SearchPhase::Running { started } => Some(started.elapsed()),
            SearchPhase::Complete { started, completed } => Some(completed.duration_since(started)),
        }
    }
}

#[derive(Debug)]
pub struct SearchState {
    pub results: Vec<SearchResultWithReplacement>,

    selected: Selected,
    // TODO: make the view logic with scrolling etc. into a generic component
    pub view_offset: usize,           // Updated by UI, not app
    pub num_displayed: Option<usize>, // Updated by UI, not app

    processing_receiver: UnboundedReceiver<BackgroundProcessingEvent>,
    processing_sender: UnboundedSender<BackgroundProcessingEvent>,

    pub last_render: Instant,
    pub phase: SearchPhase,
    pub cancelled: Arc<AtomicBool>,
}

impl SearchState {
    pub fn new(
        processing_sender: UnboundedSender<BackgroundProcessingEvent>,
        processing_receiver: UnboundedReceiver<BackgroundProcessingEvent>,
        cancelled: Arc<AtomicBool>,
    ) -> Self {
        Self {
            results: vec![],
            selected: Selected::Single(0),
            view_offset: 0,
            num_displayed: None,
            processing_sender,
            processing_receiver,
            last_render: Instant::now(),
            phase: SearchPhase::Running {
                started: Instant::now(),
            },
            cancelled,
        }
    }

    fn move_selected_up_by(&mut self, n: usize) {
        if self.results.is_empty() {
            return;
        }
        let pos = self.primary_selected_pos();
        let new_pos = if pos == 0 {
            // Wrap to last result
            self.results.len().saturating_sub(1)
        } else {
            pos.saturating_sub(n)
        };
        self.move_primary_sel(new_pos);
    }

    fn move_selected_down_by(&mut self, n: usize) {
        if self.results.is_empty() {
            return;
        }
        let pos = self.primary_selected_pos();
        let end = self.results.len().saturating_sub(1);
        let new_pos = if pos >= end {
            // Wrap to first result
            0
        } else {
            min(pos + n, end)
        };
        self.move_primary_sel(new_pos);
    }

    fn move_selected_up(&mut self) {
        self.move_selected_up_by(1);
    }

    fn move_selected_down(&mut self) {
        self.move_selected_down_by(1);
    }

    /// Move to the first match in the next file (issue #4)
    pub fn move_to_next_file(&mut self) {
        if self.results.is_empty() {
            return;
        }
        let current_path = &self.results[self.primary_selected_pos()].search_result.path;
        // Search forward for a result with a different path
        for i in (self.primary_selected_pos() + 1)..self.results.len() {
            if &self.results[i].search_result.path != current_path {
                self.move_primary_sel(i);
                return;
            }
        }
        // Wrap to first file if at the last file
        if &self.results[0].search_result.path != current_path {
            self.move_primary_sel(0);
        }
    }

    /// Move to the first match in the previous file (issue #4)
    pub fn move_to_prev_file(&mut self) {
        if self.results.is_empty() {
            return;
        }
        let current_path = &self.results[self.primary_selected_pos()].search_result.path;
        // Search backward for a result with a different path
        for i in (0..self.primary_selected_pos()).rev() {
            if &self.results[i].search_result.path != current_path {
                self.move_primary_sel(i);
                return;
            }
        }
        // Wrap to last file if at the first file
        let last = self.results.len() - 1;
        if &self.results[last].search_result.path != current_path {
            self.move_primary_sel(last);
        }
    }

    fn move_selected_up_full_page(&mut self) {
        self.move_selected_up_by(max(self.num_displayed.unwrap(), 1));
    }

    fn move_selected_down_full_page(&mut self) {
        self.move_selected_down_by(max(self.num_displayed.unwrap(), 1));
    }

    fn move_selected_up_half_page(&mut self) {
        self.move_selected_up_by(max(ceil_div(self.num_displayed.unwrap(), 2), 1));
    }

    fn move_selected_down_half_page(&mut self) {
        self.move_selected_down_by(max(ceil_div(self.num_displayed.unwrap(), 2), 1));
    }

    fn move_selected_top(&mut self) {
        self.move_primary_sel(0);
    }

    fn move_selected_bottom(&mut self) {
        self.move_primary_sel(self.results.len().saturating_sub(1));
    }

    fn move_primary_sel(&mut self, idx: usize) {
        self.selected = match &self.selected {
            Selected::Single(_) => Selected::Single(idx),
            Selected::Multi(MultiSelected { anchor, .. }) => Selected::Multi(MultiSelected {
                anchor: *anchor,
                primary: idx,
            }),
        };
    }

    fn toggle_selected_inclusion(&mut self) {
        let all_included = self
            .selected_fields()
            .iter()
            .all(|res| res.search_result.included);
        self.selected_fields_mut().iter_mut().for_each(|selected| {
            selected.search_result.included = !all_included;
        });
        // Auto-advance to next result after toggling (issue #5)
        if !self.results.is_empty() {
            self.move_selected_down();
        }
    }

    fn toggle_all_selected(&mut self) {
        let all_included = self.results.iter().all(|res| res.search_result.included);
        self.results
            .iter_mut()
            .for_each(|res| res.search_result.included = !all_included);
    }

    fn toggle_current_file_selected(&mut self) {
        if self.results.is_empty() {
            return;
        }
        let current_path = self.results[self.primary_selected_pos()]
            .search_result
            .path
            .clone();
        let all_included = self
            .results
            .iter()
            .filter(|r| r.search_result.path == current_path)
            .all(|r| r.search_result.included);
        self.results
            .iter_mut()
            .filter(|r| r.search_result.path == current_path)
            .for_each(|r| r.search_result.included = !all_included);
    }

    // TODO: add tests
    fn selected_range(&self) -> (usize, usize) {
        match &self.selected {
            Selected::Single(sel) => (*sel, *sel),
            Selected::Multi(ms) => ms.ordered(),
        }
    }

    fn selected_fields(&self) -> &[SearchResultWithReplacement] {
        if self.results.is_empty() {
            return &[];
        }
        let (low, high) = self.selected_range();
        &self.results[low..=high]
    }

    fn selected_fields_mut(&mut self) -> &mut [SearchResultWithReplacement] {
        if self.results.is_empty() {
            return &mut [];
        }
        let (low, high) = self.selected_range();
        &mut self.results[low..=high]
    }

    pub fn primary_selected_field_mut(&mut self) -> Option<&mut SearchResultWithReplacement> {
        let sel = self.primary_selected_pos();
        if !self.results.is_empty() {
            Some(&mut self.results[sel])
        } else {
            None
        }
    }

    pub fn primary_selected_pos(&self) -> usize {
        match self.selected {
            Selected::Single(sel) => sel,
            Selected::Multi(MultiSelected { primary, .. }) => primary,
        }
    }

    fn toggle_multiselect_mode(&mut self) {
        self.selected = match &self.selected {
            Selected::Single(sel) => Selected::Multi(MultiSelected {
                anchor: *sel,
                primary: *sel,
            }),
            Selected::Multi(MultiSelected { primary, .. }) => Selected::Single(*primary),
        };
    }

    pub fn is_selected(&self, idx: usize) -> bool {
        match &self.selected {
            Selected::Single(sel) => idx == *sel,
            Selected::Multi(ms) => {
                let (low, high) = ms.ordered();
                idx >= low && idx <= high
            }
        }
    }

    fn multiselect_enabled(&self) -> bool {
        match &self.selected {
            Selected::Single(_) => false,
            Selected::Multi(_) => true,
        }
    }

    pub fn is_primary_selected(&self, idx: usize) -> bool {
        idx == self.primary_selected_pos()
    }

    fn flip_multiselect_direction(&mut self) {
        match &mut self.selected {
            Selected::Single(_) => {}
            Selected::Multi(ms) => {
                ms.flip_direction();
            }
        }
    }

    /// Transition `Running → Complete`. No-op from any other phase — the
    /// search this completion is for has been superseded.
    pub fn set_complete_now(&mut self) {
        if let SearchPhase::Running { started } = self.phase {
            self.phase = SearchPhase::Complete {
                started,
                completed: Instant::now(),
            };
        }
    }

    pub fn set_pending(&mut self) {
        self.phase = SearchPhase::Pending;
    }

    pub fn set_invalid(&mut self) {
        self.phase = SearchPhase::Invalid;
    }

    /// Signal the background search task for this state to stop. The task
    /// polls this flag and terminates at the next opportunity; any batches
    /// it had already queued are filtered out in `add_search_results`.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FocussedSection {
    SearchFields,
    SearchResults,
}

#[derive(Debug)]
pub struct PreviewUpdateStatus {
    replace_debounce_timer: JoinHandle<()>,
    update_replacement_cancelled: Arc<AtomicBool>,
    replacements_updated: usize,
    total_replacements_to_update: usize,
}

impl PreviewUpdateStatus {
    fn new(
        replace_debounce_timer: JoinHandle<()>,
        update_replacement_cancelled: Arc<AtomicBool>,
    ) -> Self {
        Self {
            replace_debounce_timer,
            update_replacement_cancelled,
            replacements_updated: 0,
            total_replacements_to_update: 0,
        }
    }
}

/// Snapshot of every input that affects *search* results (not replacement).
/// Used to skip scheduling debounced searches when the inputs haven't changed
/// since we last scheduled — cursor movements and idempotent toggles don't
/// need to re-run a search.
///
/// Deliberately excludes `replacement_text` (only affects preview) and
/// `interpret_escape_sequences` (only affects replacement interpretation).
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct SearchKey {
    search_text: String,
    fixed_strings: bool,
    advanced_regex: bool,
    match_whole_word: bool,
    match_case: bool,
    multiline: bool,
    dir: Option<DirSearchKey>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirSearchKey {
    include_globs: String,
    exclude_globs: String,
    include_hidden: bool,
    include_git_folders: bool,
    directory: PathBuf,
}

#[derive(Debug)]
pub struct SearchFieldsState {
    pub focussed_section: FocussedSection,
    pub search_state: Option<SearchState>, // Becomes Some when search begins
    pub search_debounce_timer: Option<JoinHandle<()>>,
    pub preview_update_state: Option<PreviewUpdateStatus>,
    /// Key of the most recently scheduled/run search. Cleared whenever
    /// `search_state` is cleared (e.g. on empty text) so that re-typing the
    /// same query runs the search again. Boxed to keep the `Screen` enum
    /// compact.
    pub last_scheduled_key: Option<Box<SearchKey>>,
    /// Monotonic counter for debounced search requests so queued internal
    /// events can be identified as stale after later edits.
    next_search_generation: u64,
    /// Generation of the currently pending debounced search, if any.
    pending_search_generation: Option<u64>,
    /// Inline replacement progress — shown as a banner overlay instead of
    /// switching to a separate screen.
    pub replacement_progress: Option<PerformingReplacementState>,
    /// Width of the file name column as a percentage of the results area (10–80).
    /// Adjustable at runtime with Ctrl+Left / Ctrl+Right. Default comes from
    /// `preview.file_column_percentage` in config (25 by default).
    pub file_column_width_pct: u16,
}

impl Default for SearchFieldsState {
    fn default() -> Self {
        Self {
            focussed_section: FocussedSection::SearchFields,
            search_state: None,
            search_debounce_timer: None,
            preview_update_state: None,
            last_scheduled_key: None,
            next_search_generation: 0,
            pending_search_generation: None,
            replacement_progress: None,
            file_column_width_pct: 25,
        }
    }
}

impl SearchFieldsState {
    pub fn replacements_in_progress(&self) -> Option<(usize, usize)> {
        self.preview_update_state.as_ref().and_then(|p| {
            if p.replacements_updated != p.total_replacements_to_update {
                Some((p.replacements_updated, p.total_replacements_to_update))
            } else {
                None
            }
        })
    }

    pub fn cancel_preview_updates(&mut self) {
        if let Some(ref mut state) = self.preview_update_state {
            state.replace_debounce_timer.abort();
            state
                .update_replacement_cancelled
                .store(true, Ordering::Relaxed);
        }
        self.preview_update_state = None;
    }

    pub fn abort_search_debounce(&mut self) {
        if let Some(timer) = self.search_debounce_timer.take() {
            timer.abort();
        }
        self.pending_search_generation = None;
    }

    /// Cancel both async refresh paths owned by this state: any pending
    /// preview-replacement update and any pending debounced search. Does
    /// *not* signal an in-flight search task to stop — that's
    /// `App::cancel_search` / `SearchState::cancel`.
    pub fn cancel_pending_async_work(&mut self) {
        self.cancel_preview_updates();
        self.abort_search_debounce();
    }

    pub fn next_search_generation(&mut self) -> u64 {
        let generation = self.next_search_generation;
        self.next_search_generation = self.next_search_generation.wrapping_add(1);
        generation
    }
}

#[derive(Debug)]
pub enum Screen {
    SearchFields(SearchFieldsState),
    PerformingReplacement(PerformingReplacementState),
    Results(ReplaceState),
}

impl Screen {
    fn name(&self) -> &str {
        // TODO: is there a better way of doing this?
        match &self {
            Screen::SearchFields(_) => "SearchFields",
            Screen::PerformingReplacement(_) => "PerformingReplacement",
            Screen::Results(_) => "Results",
        }
    }

    fn unwrap_search_fields_state_mut(&mut self) -> &mut SearchFieldsState {
        let name = self.name().to_owned();
        let Screen::SearchFields(search_fields_state) = self else {
            panic!("Expected current_screen to be SearchFields, found {name}");
        };
        search_fields_state
    }
}

#[derive(Debug)]
pub enum Popup {
    Error,
    Help,
    Text { title: String, body: String },
    ReplacementResults(ReplaceState),
}

#[derive(Debug, Clone)]
struct Toast {
    message: String,
    generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct AppRunConfig {
    pub include_hidden: bool,
    pub include_git_folders: bool,
    pub advanced_regex: bool,
    pub multiline: bool,
    pub immediate_search: bool,
    pub immediate_replace: bool,
    pub print_results: bool,
    pub print_on_exit: bool,
    pub interpret_escape_sequences: bool,
    /// When false, the include and exclude filter fields are hidden in the TUI.
    pub show_file_filters: bool,
}

#[allow(clippy::derivable_impls)]
impl Default for AppRunConfig {
    fn default() -> Self {
        Self {
            include_hidden: false,
            include_git_folders: false,
            advanced_regex: false,
            multiline: false,
            immediate_search: false,
            immediate_replace: false,
            print_results: false,
            print_on_exit: false,
            interpret_escape_sequences: false,
            show_file_filters: true,
        }
    }
}

#[derive(Debug)]
pub struct EventChannels {
    pub sender: UnboundedSender<Event>,
    receiver: UnboundedReceiver<Event>,
}

impl EventChannels {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        Self { sender, receiver }
    }

    pub async fn recv(&mut self) -> Option<Event> {
        self.receiver.recv().await
    }
}

impl Default for EventChannels {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default)]
struct HintState {
    has_shown_multiline_hint: bool,
}

/// State for the interactive file finder popup (issue #7)
#[derive(Debug)]
pub struct FileFinderState {
    pub query: String,
    pub entries: Vec<String>,
    pub selected: usize,
    pub target_field: FileFinderTarget,
    pub base_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFinderTarget {
    IncludeFiles,
    ExcludeFiles,
}

#[derive(Debug)]
pub struct UIState {
    pub current_screen: Screen,
    pub popup: Option<Popup>,
    pub file_finder: Option<FileFinderState>,
    toast: Option<Toast>,
    errors: Vec<AppError>,
    hints: HintState,
    pub pending_prefix: Option<KeyEvent>,
    /// When true, the next key press in fields focus is treated as a command
    /// lookup rather than text input.  Set by pressing Esc, cleared when a
    /// command is executed or when Esc is pressed again (cancel).
    pending_escape: bool,
}

impl UIState {
    pub fn new(current_screen: Screen) -> Self {
        Self {
            current_screen,
            popup: None,
            file_finder: None,
            toast: None,
            errors: Vec::new(),
            hints: HintState::default(),
            pending_prefix: None,
            pending_escape: false,
        }
    }

    pub fn add_error(&mut self, error: AppError) {
        self.errors.push(error);
    }

    pub fn errors(&self) -> &[AppError] {
        &self.errors
    }

    pub fn clear_errors(&mut self) {
        self.errors.clear();
    }
}

pub struct App {
    pub config: Config,
    key_map: KeyMap,
    pub search_fields: SearchFields,
    pub searcher: Option<Searcher>,
    pub input_source: InputSource,
    pub run_config: AppRunConfig,
    pub event_channels: EventChannels,
    pub ui_state: UIState,
    file_content_provider: Arc<dyn FileContentProvider>,
    /// Set to true when UI-level caches (file windows, diffs) should be cleared.
    /// The frontend checks and resets this flag.
    ui_caches_invalidated: bool,
}

impl App {
    /// Returns the ordered list of field indices that should receive focus during Tab navigation.
    /// Combines the config `focus_fields` setting with `show_file_filters`.
    /// When `show_file_filters` is false, include/exclude are removed from the list.
    /// When `focus_fields` is None, all fields (respecting show_file_filters) are in default order.
    fn focus_field_indices(&self) -> Vec<usize> {
        // Map from config field name to array index
        let name_to_idx: &[(&str, usize)] = &[
            ("search", 0),
            ("replace", 1),
            ("fixed", 2),
            ("word", 3),
            ("case", 4),
            ("include", 5),
            ("exclude", 6),
        ];

        let indices: Vec<usize> = if let Some(ref names) = self.config.search.focus_fields {
            names
                .iter()
                .filter_map(|name| {
                    name_to_idx
                        .iter()
                        .find(|(n, _)| *n == name.as_str())
                        .map(|(_, idx)| *idx)
                })
                .collect()
        } else {
            // Default order: all fields
            (0..7).collect()
        };

        // If file filters are hidden, filter out include (5) and exclude (6)
        if self.run_config.show_file_filters {
            indices
        } else {
            indices.into_iter().filter(|&i| i < 5).collect()
        }
    }
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("config", &self.config)
            .field("key_map", &self.key_map)
            .field("search_fields", &self.search_fields)
            .field("searcher", &self.searcher)
            .field("input_source", &self.input_source)
            .field("run_config", &self.run_config)
            .field("event_channels", &self.event_channels)
            .field("ui_state", &self.ui_state)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
enum SearchStrategy {
    Files(FileSearcher),
    Text {
        haystack: Arc<String>,
        config: ParsedSearchConfig,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
enum ReplacementCacheKey {
    File(PathBuf),
    Stdin,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PreviewOutcome {
    Replacement(String),
    NoMatch,
    Error(String),
}

fn result_with_outcome(
    search_result: SearchResult,
    outcome: PreviewOutcome,
) -> Option<SearchResultWithReplacement> {
    match outcome {
        PreviewOutcome::Replacement(replacement) => Some(SearchResultWithReplacement {
            search_result,
            replacement,
            replace_result: None,
            preview_error: None,
        }),
        PreviewOutcome::Error(error) => Some(SearchResultWithReplacement {
            search_result,
            replacement: String::new(),
            replace_result: None,
            preview_error: Some(error),
        }),
        PreviewOutcome::NoMatch => None,
    }
}

fn apply_outcome(result: &mut SearchResultWithReplacement, outcome: PreviewOutcome) -> bool {
    match outcome {
        PreviewOutcome::Replacement(replacement) => {
            result.replacement = replacement;
            result.preview_error = None;
            true
        }
        PreviewOutcome::Error(error) => {
            result.replacement.clear();
            result.preview_error = Some(error);
            true
        }
        PreviewOutcome::NoMatch => false,
    }
}

struct ReplacementContext<'a> {
    input_source: &'a InputSource,
    searcher: &'a Searcher,
    needs_context: bool,
    file_content_provider: Arc<dyn FileContentProvider>,
    file_cache: HashMap<PathBuf, Arc<String>>,
    replacement_cache: HashMap<ReplacementCacheKey, HashMap<(usize, usize), String>>,
}

impl<'a> ReplacementContext<'a> {
    fn new(
        input_source: &'a InputSource,
        searcher: &'a Searcher,
        needs_context: bool,
        file_content_provider: Arc<dyn FileContentProvider>,
    ) -> Self {
        Self {
            input_source,
            searcher,
            needs_context,
            file_content_provider,
            file_cache: HashMap::new(),
            replacement_cache: HashMap::new(),
        }
    }

    fn replacement_for_search_result(&mut self, res: &SearchResult) -> PreviewOutcome {
        match &res.content {
            MatchContent::Line { content, .. } => {
                replace_all_if_match(content, self.searcher.search(), self.searcher.replace())
                    .map_or(PreviewOutcome::NoMatch, PreviewOutcome::Replacement)
            }
            MatchContent::ByteRange {
                content,
                byte_start,
                byte_end,
                ..
            } => {
                if self.needs_context {
                    return self.replacement_for_byte_range_with_context(
                        res,
                        content,
                        *byte_start,
                        *byte_end,
                    );
                }

                if contains_search(content, self.searcher.search()) {
                    return PreviewOutcome::Replacement(replacement_for_match(
                        content,
                        self.searcher.search(),
                        self.searcher.replace(),
                    ));
                }

                PreviewOutcome::NoMatch
            }
        }
    }

    fn replacement_for_byte_range_with_context(
        &mut self,
        res: &SearchResult,
        content: &str,
        byte_start: usize,
        byte_end: usize,
    ) -> PreviewOutcome {
        let haystack = match self.haystack_for_result(res) {
            Ok(haystack) => haystack,
            Err(error) => return PreviewOutcome::Error(error),
        };

        if haystack.get(byte_start..byte_end) != Some(content) {
            let message = if res.path.is_some() {
                "File changed since search".to_string()
            } else {
                "Input changed since search".to_string()
            };
            return PreviewOutcome::Error(message);
        }

        if let Some(map) = self.replacement_map_for_result(res, haystack.as_str())
            && let Some(replacement) = map.get(&(byte_start, byte_end))
        {
            return PreviewOutcome::Replacement(replacement.clone());
        }

        // NOTE: advanced regex lookarounds require the full haystack. If we run the
        // regex against the matched substring only, lookbehind/lookahead checks fail
        // and we silently "replace" with the original text. Using the full haystack
        // keeps the TUI preview/replacement consistent with headless mode.
        if let Some(replacement) = replacement_for_match_in_haystack(
            self.searcher.search(),
            self.searcher.replace(),
            haystack.as_str(),
            byte_start,
            byte_end,
        ) {
            return PreviewOutcome::Replacement(replacement);
        }

        PreviewOutcome::NoMatch
    }

    fn replacement_map_for_result(
        &mut self,
        res: &SearchResult,
        haystack: &str,
    ) -> Option<&HashMap<(usize, usize), String>> {
        let SearchType::PatternAdvanced(pattern) = self.searcher.search() else {
            return None;
        };
        let key = self.replacement_cache_key(res)?;
        let replace = self.searcher.replace();
        Some(
            self.replacement_cache
                .entry(key)
                .or_insert_with(|| build_replacement_map(pattern, replace, haystack)),
        )
    }

    fn replacement_cache_key(&self, res: &SearchResult) -> Option<ReplacementCacheKey> {
        if let Some(path) = res.path.as_ref() {
            Some(ReplacementCacheKey::File(path.clone()))
        } else if matches!(self.input_source, InputSource::Stdin(_)) {
            Some(ReplacementCacheKey::Stdin)
        } else {
            None
        }
    }

    fn haystack_for_result(&mut self, res: &SearchResult) -> Result<Arc<String>, String> {
        if let Some(path) = res.path.as_ref() {
            if let Some(cached) = self.file_cache.get(path) {
                return Ok(Arc::clone(cached));
            }

            match self.read_file_content(path) {
                Ok(contents) => {
                    self.file_cache.insert(path.clone(), Arc::clone(&contents));
                    Ok(contents)
                }
                Err(err) => {
                    let message = format!("Failed to read file for replacement preview: {err}");
                    warn!(
                        "Failed to read file for multiline replacement preview {path}: {err}",
                        path = path.display()
                    );
                    Err(message)
                }
            }
        } else if let InputSource::Stdin(stdin) = self.input_source {
            Ok(Arc::clone(stdin))
        } else {
            Err("Missing input source for replacement preview".to_string())
        }
    }

    fn read_file_content(&self, path: &Path) -> anyhow::Result<Arc<String>> {
        self.file_content_provider.read_to_string(path)
    }
}

fn build_replacement_map(
    pattern: &FancyRegex,
    replace: &str,
    haystack: &str,
) -> HashMap<(usize, usize), String> {
    let mut map = HashMap::new();
    for caps in pattern.captures_iter(haystack).flatten() {
        if let Some(mat) = caps.get(0) {
            let mut out = String::new();
            caps.expand(replace, &mut out);
            map.insert((mat.start(), mat.end()), out);
        }
    }
    map
}

// Macro to get the background processing receiver from current_screen, needed because
// methods can't express split borrows but macros can
macro_rules! get_bg_receiver {
    ($self:expr) => {
        match &mut $self.ui_state.current_screen {
            Screen::SearchFields(SearchFieldsState {
                search_state,
                replacement_progress,
                ..
            }) => {
                if let &mut Some(ref mut rp) = replacement_progress {
                    Some(&mut rp.processing_receiver)
                } else {
                    search_state.as_mut().map(|s| &mut s.processing_receiver)
                }
            }
            Screen::PerformingReplacement(PerformingReplacementState {
                processing_receiver,
                ..
            }) => Some(processing_receiver),
            Screen::Results(_) => None,
        }
    };
}

macro_rules! recv_optional {
    ($opt_receiver:expr) => {
        async {
            match $opt_receiver {
                Some(r) => r.recv().await,
                None => None,
            }
        }
    };
}

impl<'a> App {
    pub fn new(
        input_source: InputSource,
        search_field_values: &SearchFieldValues<'a>,
        app_run_config: AppRunConfig,
        config: Config,
    ) -> anyhow::Result<Self> {
        let search_fields = SearchFields::with_values(
            search_field_values,
            config.search.disable_prepopulated_fields,
        );

        let mut search_fields_state = SearchFieldsState::default();
        search_fields_state.file_column_width_pct = config
            .preview
            .file_column_percentage
            .clamp(10, 80);
        if app_run_config.immediate_search {
            search_fields_state.focussed_section = FocussedSection::SearchResults;
        }

        let key_map = KeyMap::from_config(&config.keys).map_err(display_conflict_errors)?;

        let search_immediately =
            app_run_config.immediate_search || !search_field_values.search.value.is_empty();

        let mut app = Self {
            config,
            key_map,
            search_fields,
            searcher: None,
            input_source,
            run_config: app_run_config,
            event_channels: EventChannels::new(),
            ui_state: UIState::new(Screen::SearchFields(search_fields_state)),
            file_content_provider: default_file_content_provider(),
            ui_caches_invalidated: false,
        };

        if search_immediately {
            app.perform_search_background();
        }

        Ok(app)
    }

    pub fn set_file_content_provider(&mut self, provider: Arc<dyn FileContentProvider>) {
        self.file_content_provider = provider;
    }

    /// Returns true if UI-level caches should be cleared, resetting the flag.
    /// The frontend should call this before each draw and clear caches if true.
    pub fn take_ui_cache_clear_request(&mut self) -> bool {
        std::mem::take(&mut self.ui_caches_invalidated)
    }

    fn request_ui_cache_clear(&mut self) {
        self.ui_caches_invalidated = true;
    }

    fn replacement_context<'b>(
        input_source: &'b InputSource,
        searcher: &'b Searcher,
        file_content_provider: Arc<dyn FileContentProvider>,
    ) -> ReplacementContext<'b> {
        let needs_context = searcher.search().needs_haystack_context();
        ReplacementContext::new(input_source, searcher, needs_context, file_content_provider)
    }

    pub fn handle_internal_event(&mut self, event: InternalEvent) -> EventHandlingResult {
        match event {
            InternalEvent::App(app_event) => self.handle_app_event(app_event),
            InternalEvent::Background(bg_event) => {
                self.handle_background_processing_event(bg_event)
            }
        }
    }

    #[allow(clippy::needless_pass_by_value)]
    fn handle_app_event(&mut self, app_event: AppEvent) -> EventHandlingResult {
        match app_event {
            AppEvent::PerformSearch { generation } => {
                let Screen::SearchFields(search_fields_state) = &mut self.ui_state.current_screen
                else {
                    return EventHandlingResult::None;
                };
                if search_fields_state.pending_search_generation != Some(generation) {
                    return EventHandlingResult::None;
                }
                search_fields_state.pending_search_generation = None;
                let Some(search_config) = self.validate_fields().unwrap() else {
                    self.invalidate_search_state_and_key();
                    return EventHandlingResult::Rerender;
                };
                self.searcher = Some(search_config);
                self.perform_search_already_validated();
                EventHandlingResult::Rerender
            }
            AppEvent::DismissToast { generation } => {
                self.dismiss_toast_if_generation_matches(generation);
                EventHandlingResult::Rerender
            }
            AppEvent::RetrySearch => {
                if self.searcher.is_some() {
                    self.file_content_provider.clear();
                    self.request_ui_cache_clear();
                    self.perform_search_already_validated();
                }
                EventHandlingResult::Rerender
            }
        }
    }

    fn cancel_search(&mut self) {
        if let Screen::SearchFields(SearchFieldsState {
            search_state: Some(state),
            ..
        }) = &self.ui_state.current_screen
        {
            state.cancel();
        }
    }

    fn cancel_replacement(&mut self) {
        if let Screen::PerformingReplacement(PerformingReplacementState { cancelled, .. }) =
            &mut self.ui_state.current_screen
        {
            cancelled.store(true, Ordering::Relaxed);
        }
        // Also check inline replacement progress
        if let Screen::SearchFields(SearchFieldsState {
            replacement_progress: Some(PerformingReplacementState { cancelled, .. }),
            ..
        }) = &mut self.ui_state.current_screen
        {
            cancelled.store(true, Ordering::Relaxed);
        }
    }

    pub fn cancel_in_progress_tasks(&mut self) {
        self.cancel_search();
        self.cancel_replacement();

        if let Screen::SearchFields(ref mut search_fields_state) = self.ui_state.current_screen {
            search_fields_state.cancel_pending_async_work();
        }
    }

    pub fn reset(&mut self) {
        self.cancel_in_progress_tasks();
        let mut run_config = self.run_config.clone();
        run_config.immediate_search = false;
        self.file_content_provider.clear();
        let provider = Arc::clone(&self.file_content_provider);

        *self = Self::new(
            self.input_source.clone(), // TODO: avoid cloning
            &SearchFieldValues::default(),
            run_config,
            std::mem::take(&mut self.config),
        )
        .expect("App initialisation errors should have been detected on initial construction");
        self.file_content_provider = provider;
    }

    /// Re-run the current search from scratch, invalidating file caches.
    /// Called after returning from an external editor so that any file
    /// changes are picked up and stale results are replaced.
    pub fn refresh_after_editor(&mut self, edited_path: &Path) {
        self.file_content_provider.invalidate(edited_path);
        self.request_ui_cache_clear();
        self.perform_search_background();
    }

    pub async fn event_recv(&mut self) -> Event {
        tokio::select! {
            Some(event) = self.event_channels.recv() => event,
            Some(bg_event) = recv_optional!(get_bg_receiver!(self)) => {
                Event::Internal(InternalEvent::Background(bg_event))
            }
        }
    }

    pub fn background_processing_reciever(
        &mut self,
    ) -> Option<&mut UnboundedReceiver<BackgroundProcessingEvent>> {
        get_bg_receiver!(self)
    }

    /// Called when searching explicitly: shows error popup if there have been validation failures
    //
    /// NOTE: validation should have been performed (with `validate_fields`) before calling
    // TODO: how can we enforce validation by type system?
    fn perform_search_foreground(&mut self) {
        if !matches!(self.ui_state.current_screen, Screen::SearchFields(_)) {
            log::warn!(
                "Called perform_search_with_error_popup on screen {}",
                self.ui_state.current_screen.name()
            );
            return;
        }

        if !self.errors().is_empty() {
            self.set_popup(Popup::Error);
        } else if self.search_fields.search().text().is_empty() {
            self.add_error(AppError {
                name: "Search field must not be empty".to_string(),
                long: "Please enter some search text".to_string(),
            });
        } else {
            if !self.run_config.multiline
                && !self.search_fields.fixed_strings().checked
                && self.search_fields.search().text().contains(r"\n")
                && !self.ui_state.hints.has_shown_multiline_hint
            {
                let key_hint = self
                    .config
                    .keys
                    .search
                    .toggle_multiline
                    .first()
                    .map(|k| format!(" Press {k} to enable."))
                    .unwrap_or_default();
                self.show_toast(
                    format!(r"Search contains \n but multiline is off.{key_hint}"),
                    Duration::from_secs(5),
                );
                self.ui_state.hints.has_shown_multiline_hint = true;
            }

            let Screen::SearchFields(ref mut search_fields_state) = self.ui_state.current_screen
            else {
                panic!(
                    "Expected SearchFields, found {:?}",
                    self.ui_state.current_screen.name()
                );
            };
            search_fields_state.focussed_section = FocussedSection::SearchResults;
            // Check if search has been performed
            if search_fields_state.search_state.is_some() {
                if self.run_config.immediate_replace && self.search_has_completed() {
                    self.perform_replacement();
                }
            } else {
                self.perform_search_background();
            }
        }
    }

    /// Called when searching in the background e.g. when entering chars into the search field: does not show
    /// error popup if there are validation errors
    pub fn perform_search_background(&mut self) {
        if !matches!(self.ui_state.current_screen, Screen::SearchFields(_)) {
            log::warn!(
                "Called perform_search_if_valid on screen {}",
                self.ui_state.current_screen.name()
            );
            return;
        }

        if self.search_fields.search().text().is_empty() {
            self.clear_search_state_and_key();
            return;
        }

        self.ui_state
            .current_screen
            .unwrap_search_fields_state_mut()
            .cancel_pending_async_work();

        let Some(search_config) = self.validate_fields().unwrap() else {
            self.invalidate_search_state_and_key();
            return;
        };
        self.searcher = Some(search_config);
        self.perform_search_already_validated();
    }

    /// NOTE: validation should have been performed (with `validate_fields`) before calling
    // TODO: how can we enforce validation by type system - e.g. pass in searcher?
    fn perform_search_already_validated(&mut self) {
        self.cancel_search();
        self.file_content_provider.clear();
        let key = self.current_search_key();
        let Screen::SearchFields(ref mut search_fields_state) = self.ui_state.current_screen else {
            log::warn!(
                "Called perform_search_unwrap on screen {}",
                self.ui_state.current_screen.name()
            );
            return;
        };
        search_fields_state.cancel_pending_async_work();

        // Empty searches are short-circuited upstream (`enter_chars_into_field`
        // clears state and returns early); any remaining path that reaches
        // here with empty text should produce no search and no state.
        if self.search_fields.search().text().is_empty() {
            search_fields_state.search_state = None;
            search_fields_state.last_scheduled_key = None;
            return;
        }

        let (background_processing_sender, background_processing_receiver) =
            mpsc::unbounded_channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let search_state = SearchState::new(
            background_processing_sender.clone(),
            background_processing_receiver,
            Arc::clone(&cancelled),
        );

        let strategy = match &self.searcher {
            Some(Searcher::FileSearcher(file_searcher)) => {
                SearchStrategy::Files(file_searcher.clone())
            }
            Some(Searcher::TextSearcher { search_config }) => {
                let InputSource::Stdin(ref stdin) = self.input_source else {
                    panic!("Expected InputSource::Stdin, found {:?}", self.input_source);
                };
                SearchStrategy::Text {
                    haystack: Arc::clone(stdin),
                    config: search_config.clone(),
                }
            }
            None => {
                panic!("Fields should have been parsed")
            }
        };

        Self::spawn_search_task(
            strategy,
            background_processing_sender,
            self.event_channels.sender.clone(),
            cancelled,
        );

        search_fields_state.search_state = Some(search_state);
        search_fields_state.last_scheduled_key = Some(Box::new(key));
    }

    #[allow(clippy::needless_pass_by_value)]
    fn update_all_replacements(&mut self, cancelled: Arc<AtomicBool>) -> EventHandlingResult {
        if cancelled.load(Ordering::Relaxed) {
            return EventHandlingResult::None;
        }
        let Screen::SearchFields(SearchFieldsState {
            search_state: Some(search_state),
            preview_update_state: Some(preview_update_state),
            ..
        }) = &mut self.ui_state.current_screen
        else {
            return EventHandlingResult::None;
        };

        preview_update_state.total_replacements_to_update = search_state.results.len();

        #[allow(clippy::items_after_statements)]
        static STEP: usize = 7919; // Slightly random so that increments seem more natural in UI

        let num_results = search_state.results.len();
        for start in (0..num_results).step_by(STEP) {
            let end = (start + STEP - 1).min(num_results.saturating_sub(1));
            let _ = search_state.processing_sender.send(
                BackgroundProcessingEvent::UpdateReplacements {
                    start,
                    end,
                    cancelled: cancelled.clone(),
                },
            );
        }

        EventHandlingResult::Rerender
    }

    #[allow(clippy::needless_pass_by_value)]
    fn update_replacements(
        &mut self,
        start: usize,
        end: usize,
        cancelled: Arc<AtomicBool>,
    ) -> EventHandlingResult {
        if cancelled.load(Ordering::Relaxed) {
            return EventHandlingResult::None;
        }
        let searcher = self
            .searcher
            .as_ref()
            .expect("Fields should have been parsed");
        let mut context = Self::replacement_context(
            &self.input_source,
            searcher,
            Arc::clone(&self.file_content_provider),
        );
        let Screen::SearchFields(SearchFieldsState {
            search_state: Some(search_state),
            preview_update_state: Some(preview_update_state),
            ..
        }) = &mut self.ui_state.current_screen
        else {
            return EventHandlingResult::None;
        };
        for res in &mut search_state.results[start..=end] {
            if !apply_outcome(
                res,
                context.replacement_for_search_result(&res.search_result),
            ) {
                // Handle race condition where search results are being updated
                // The new search results will already have the correct replacement so no need to update
                return EventHandlingResult::Rerender;
            }
        }
        preview_update_state.replacements_updated += end - start + 1;

        EventHandlingResult::Rerender
    }

    pub fn perform_replacement(&mut self) {
        if !self.ready_to_replace() {
            return;
        }

        // Guard: no results selected for replacement (opt-in model)
        if let Screen::SearchFields(ref state) = self.ui_state.current_screen {
            if let Some(ref search_state) = state.search_state {
                if search_state.results.iter().all(|r| !r.search_result.included) {
                    self.add_error(AppError {
                        name: "No results selected".to_string(),
                        long: "Press Space to include results, or 'a' to select all, before replacing.".to_string(),
                    });
                    return;
                }
            }
        }

        let temp_placeholder = Screen::SearchFields(SearchFieldsState::default());
        match mem::replace(
            &mut self.ui_state.current_screen,
            temp_placeholder, // Will get reset if we are not on `SearchComplete` screen
        ) {
            Screen::SearchFields(mut search_fields_state) => {
                let Some(state) = search_fields_state.search_state.take() else {
                    // No search state — put it back and return
                    self.ui_state.current_screen = Screen::SearchFields(search_fields_state);
                    return;
                };

                let results = state.results;
                let total_replacements = results
                    .iter()
                    .filter(|r| r.search_result.included)
                    .count();
                let replacements_completed = Arc::new(AtomicUsize::new(0));

                let Some(searcher) = self.validate_fields().unwrap() else {
                    panic!("Attempted to replace with invalid fields");
                };
                match searcher {
                    Searcher::FileSearcher(file_searcher) => {
                        let (background_processing_sender, background_processing_receiver) =
                            mpsc::unbounded_channel();
                        let cancelled = Arc::new(AtomicBool::new(false));

                        replace::perform_replacement(
                            results,
                            background_processing_sender.clone(),
                            cancelled.clone(),
                            replacements_completed.clone(),
                            self.event_channels.sender.clone(),
                            Some(file_searcher),
                            self.file_content_provider.clone(),
                        );

                        // Stay on SearchFields screen — show progress inline
                        search_fields_state.replacement_progress =
                            Some(PerformingReplacementState::new(
                                background_processing_receiver,
                                cancelled,
                                replacements_completed,
                                total_replacements,
                            ));
                        // search_state stays None — results have been consumed
                    }
                    Searcher::TextSearcher { search_config } => {
                        let InputSource::Stdin(ref stdin) = self.input_source else {
                            panic!("Expected stdin input source, found {:?}", self.input_source)
                        };
                        self.event_channels
                            .sender
                            .send(Event::ExitAndReplace(ExitAndReplaceState {
                                stdin: Arc::clone(stdin),
                                replace_results: results,
                                search_config,
                            }))
                            .expect("Failed to send ExitAndReplace event");
                        // stdin mode exits immediately — no need to restore screen
                        return;
                    }
                }

                self.ui_state.current_screen = Screen::SearchFields(search_fields_state);
            }
            screen => self.ui_state.current_screen = screen,
        }
    }

    fn ready_to_replace(&mut self) -> bool {
        if !self.search_has_completed() {
            self.add_error(AppError {
                name: "Search still in progress".to_string(),
                long: "Try again when search is complete".to_string(),
            });
            return false;
        } else if !self.is_preview_updated() {
            self.add_error(AppError {
                name: "Updating replacement preview".to_string(),
                long: "Try again when complete".to_string(),
            });
            return false;
        } else if !self
            .background_processing_reciever()
            .is_some_and(|r| r.is_empty())
        {
            self.add_error(AppError {
                name: "Background processing in progress".to_string(),
                long: "Try again in a moment".to_string(),
            });
            return false;
        }
        true
    }

    pub fn handle_background_processing_event(
        &mut self,
        event: BackgroundProcessingEvent,
    ) -> EventHandlingResult {
        match event {
            BackgroundProcessingEvent::AddSearchResult(result) => {
                self.add_search_results(iter::once(result))
            }
            BackgroundProcessingEvent::AddSearchResults(results) => {
                self.add_search_results(results)
            }
            BackgroundProcessingEvent::SearchCompleted => {
                if let Screen::SearchFields(SearchFieldsState {
                    search_state: Some(state),
                    focussed_section,
                    ..
                }) = &mut self.ui_state.current_screen
                {
                    state.set_complete_now();
                    if state.phase.is_complete()
                        && self.run_config.immediate_replace
                        && *focussed_section == FocussedSection::SearchResults
                    {
                        self.perform_replacement();
                    }
                }
                EventHandlingResult::Rerender
            }
            BackgroundProcessingEvent::ReplacementCompleted(replace_state) => {
                if self.run_config.print_results {
                    EventHandlingResult::new_exit_stats(replace_state)
                } else {
                    // Build the toast message before resetting (reset clears state)
                    let file_word = if replace_state.num_files == 1 {
                        "file"
                    } else {
                        "files"
                    };
                    let message = if replace_state.errors.is_empty() {
                        format!(
                            "Replaced {} match{} in {} {}",
                            replace_state.num_successes,
                            if replace_state.num_successes == 1 { "" } else { "es" },
                            replace_state.num_files,
                            file_word,
                        )
                    } else {
                        format!(
                            "Replaced {} match{} in {} {} ({} error{})",
                            replace_state.num_successes,
                            if replace_state.num_successes == 1 { "" } else { "es" },
                            replace_state.num_files,
                            file_word,
                            replace_state.errors.len(),
                            if replace_state.errors.len() == 1 { "" } else { "s" },
                        )
                    };

                    // Full reset: clears search text, replacement text, results, caches
                    self.reset();
                    // Request UI cache clear so stale file windows are evicted
                    self.request_ui_cache_clear();
                    // Show toast on the fresh app
                    self.show_toast(message, Duration::from_secs(4));
                    EventHandlingResult::Rerender
                }
            }
            BackgroundProcessingEvent::UpdateAllReplacements { cancelled } => {
                self.update_all_replacements(cancelled)
            }
            BackgroundProcessingEvent::UpdateReplacements {
                start,
                end,
                cancelled,
            } => self.update_replacements(start, end, cancelled),
        }
    }

    fn add_search_results<I>(&mut self, results: I) -> EventHandlingResult
    where
        I: IntoIterator<Item = SearchResult>,
    {
        // Skip stale batches. When the user edits the search, we flip the
        // current state's cancelled flag — any batches the superseded task
        // had already queued must not be appended, and must not be run
        // through the new searcher (which would match against unrelated
        // positions and emit bogus previews).
        if let Screen::SearchFields(SearchFieldsState {
            search_state: Some(state),
            ..
        }) = &self.ui_state.current_screen
            && state.cancelled.load(Ordering::Relaxed)
        {
            return EventHandlingResult::None;
        }

        let mut rerender = false;
        let searcher = self
            .searcher
            .as_ref()
            .expect("searcher should not be None when adding search results");
        let mut context = Self::replacement_context(
            &self.input_source,
            searcher,
            Arc::clone(&self.file_content_provider),
        );
        if let Screen::SearchFields(SearchFieldsState {
            search_state: Some(search_in_progress_state),
            ..
        }) = &mut self.ui_state.current_screen
        {
            let mut results_with_replacements = Vec::new();
            for res in results {
                let outcome = context.replacement_for_search_result(&res);
                if let Some(updated) = result_with_outcome(res, outcome) {
                    results_with_replacements.push(updated);
                }
            }
            search_in_progress_state
                .results
                .append(&mut results_with_replacements);

            // Slightly random duration so that time taken isn't a round number
            if search_in_progress_state.last_render.elapsed() >= Duration::from_millis(92) {
                rerender = true;
                search_in_progress_state.last_render = Instant::now();
            }
        }
        if rerender {
            EventHandlingResult::Rerender
        } else {
            EventHandlingResult::None
        }
    }

    /// Should only be called on `Screen::SearchFields`, and when focussed section is `FocussedSection::SearchFields`
    #[allow(clippy::too_many_lines, clippy::needless_pass_by_value)]
    fn handle_command_search_fields(
        &mut self,
        event: CommandSearchFocusFields,
    ) -> EventHandlingResult {
        match event {
            CommandSearchFocusFields::UnlockPrepopulatedFields => {
                self.unlock_prepopulated_fields();
                EventHandlingResult::Rerender
            }
            CommandSearchFocusFields::TriggerSearch => {
                self.perform_search_foreground();
                EventHandlingResult::Rerender
            }
            CommandSearchFocusFields::FocusPreviousField => {
                let indices = self.focus_field_indices();
                self.search_fields.focus_prev(
                    self.config.search.disable_prepopulated_fields,
                    &indices,
                );
                EventHandlingResult::Rerender
            }
            CommandSearchFocusFields::FocusNextField => {
                let indices = self.focus_field_indices();
                self.search_fields.focus_next(
                    self.config.search.disable_prepopulated_fields,
                    &indices,
                );
                EventHandlingResult::Rerender
            }
            CommandSearchFocusFields::OpenFileFinder => {
                let target = match self.search_fields.highlighted_field().name {
                    FieldName::IncludeFiles => Some(FileFinderTarget::IncludeFiles),
                    FieldName::ExcludeFiles => Some(FileFinderTarget::ExcludeFiles),
                    _ => None,
                };
                if let Some(target) = target {
                    if let Some(ref command) = self.config.search.file_finder_command {
                        let base_dir = match &self.input_source {
                            InputSource::Directory(dir) => dir.clone(),
                            InputSource::Stdin(_) => PathBuf::from("."),
                        };
                        EventHandlingResult::ExternalFileFinder {
                            command: command.clone(),
                            target,
                            base_dir,
                        }
                    } else {
                        self.open_file_finder(target);
                        EventHandlingResult::Rerender
                    }
                } else {
                    EventHandlingResult::None
                }
            }
            CommandSearchFocusFields::FocusSearchField => self.focus_field(0),
            CommandSearchFocusFields::FocusReplaceField => self.focus_field(1),
            CommandSearchFocusFields::FocusIncludeField => self.focus_field(5),
            CommandSearchFocusFields::FocusExcludeField => self.focus_field(6),
            CommandSearchFocusFields::FocusFixedField => self.focus_field(2),
            CommandSearchFocusFields::FieldsToResults => {
                // Switch from fields focus to results focus
                if let Screen::SearchFields(ref state) = self.ui_state.current_screen {
                    let has_results = state
                        .search_state
                        .as_ref()
                        .map_or(false, |s| !s.results.is_empty());
                    if has_results {
                        let sfs = self
                            .ui_state
                            .current_screen
                            .unwrap_search_fields_state_mut();
                        sfs.focussed_section = FocussedSection::SearchResults;
                        // Up key → jump to last entry, Down key → stay at first entry
                        // We don't know which key triggered this, so just keep current pos
                        EventHandlingResult::Rerender
                    } else {
                        EventHandlingResult::None
                    }
                } else {
                    EventHandlingResult::None
                }
            }
            CommandSearchFocusFields::EnterChars(key_code, key_modifiers) => {
                self.enter_chars_into_field(key_code, key_modifiers)
            }
        }
    }

    fn enter_chars_into_field(
        &mut self,
        key_code: KeyCode,
        key_modifiers: KeyModifiers,
    ) -> EventHandlingResult {
        let Screen::SearchFields(_) = self.ui_state.current_screen else {
            return EventHandlingResult::None;
        };
        if let FieldName::FixedStrings = self.search_fields.highlighted_field().name {
            // TODO: ideally this should only happen when the field is checked, but for now this will do
            self.search_fields.search_mut().clear_error();
        }

        self.search_fields.highlighted_field_mut().handle_keys(
            key_code,
            key_modifiers,
            self.config.search.disable_prepopulated_fields,
        );
        if let FieldName::Replace = self.search_fields.highlighted_field().name {
            return self.handle_replacement_config_change();
        }

        // Empty search: cancel any in-flight work, drop results, and skip the
        // debounce entirely. Rendering the "Search is empty" banner from live
        // text (see view.rs) means this produces no transient "Still
        // searching…" flash.
        if self.search_fields.search().text().is_empty() {
            self.ui_state
                .current_screen
                .unwrap_search_fields_state_mut()
                .cancel_pending_async_work();
            self.clear_search_state_and_key();
            return EventHandlingResult::Rerender;
        }

        if !self.revalidate_and_store_searcher() {
            self.ui_state
                .current_screen
                .unwrap_search_fields_state_mut()
                .cancel_pending_async_work();
            self.invalidate_search_state_and_key();
            return EventHandlingResult::Rerender;
        }

        // If every search-relevant input is identical to what we last
        // scheduled (e.g. cursor keys, or a keystroke that didn't change
        // any text/checkbox/glob), the previous search is still current —
        // skip scheduling another.
        let key = self.current_search_key();
        let event_sender = self.event_channels.sender.clone();
        let sfs = self
            .ui_state
            .current_screen
            .unwrap_search_fields_state_mut();
        if sfs.last_scheduled_key.as_deref() == Some(&key) {
            return EventHandlingResult::Rerender;
        }
        sfs.cancel_pending_async_work();

        // Existing results are now stale w.r.t. the user's current query;
        // keep them visible (intentional — no flicker) but flip the phase so
        // the view shows "Still searching…" rather than "Search complete".
        // Cancelling also stops any in-flight batches from being appended
        // — see `add_search_results`.
        if let Some(state) = sfs.search_state.as_mut() {
            state.cancel();
            state.set_pending();
        }
        let generation = sfs.next_search_generation();
        sfs.last_scheduled_key = Some(Box::new(key));
        sfs.pending_search_generation = Some(generation);
        sfs.search_debounce_timer = Some(spawn_debounced(SEARCH_DEBOUNCE, move || {
            let _ = event_sender.send(Event::Internal(InternalEvent::App(
                AppEvent::PerformSearch { generation },
            )));
        }));
        EventHandlingResult::Rerender
    }

    /// Drop the current search state, cancel any in-flight search, and clear
    /// the last-scheduled key so that re-typing the same query runs again.
    fn clear_search_state_and_key(&mut self) {
        self.cancel_search();
        let Screen::SearchFields(ref mut search_fields_state) = self.ui_state.current_screen else {
            return;
        };
        search_fields_state.search_state = None;
        search_fields_state.last_scheduled_key = None;
        search_fields_state.pending_search_generation = None;
    }

    /// Mark the current results as stale because the search inputs are invalid.
    /// We keep the results visible to avoid flicker, but cancel any in-flight
    /// work and surface an explicit non-complete phase.
    fn invalidate_search_state_and_key(&mut self) {
        self.cancel_search();
        let Screen::SearchFields(ref mut search_fields_state) = self.ui_state.current_screen else {
            return;
        };
        if let Some(state) = search_fields_state.search_state.as_mut() {
            state.set_invalid();
        }
        search_fields_state.last_scheduled_key = None;
        search_fields_state.pending_search_generation = None;
    }

    fn revalidate_and_store_searcher(&mut self) -> bool {
        if let Some(search_config) = self.validate_fields().unwrap() {
            self.searcher = Some(search_config);
            true
        } else {
            false
        }
    }

    fn refresh_selected_and_schedule_preview_updates(&mut self) {
        let Some(searcher) = self.searcher.as_ref() else {
            panic!("Fields should have been parsed")
        };
        // Immediately update replacement on the selected result; remaining results update async.
        let mut context = Self::replacement_context(
            &self.input_source,
            searcher,
            Arc::clone(&self.file_content_provider),
        );

        let Screen::SearchFields(ref mut search_fields_state) = self.ui_state.current_screen else {
            return;
        };
        // Defensive cancel: this helper may be reused independently from
        // `handle_replacement_config_change`, and `cancel_preview_updates` is idempotent.
        search_fields_state.cancel_preview_updates();
        let Some(state) = search_fields_state.search_state.as_mut() else {
            return;
        };
        if let Some(highlighted) = state.primary_selected_field_mut() {
            let _ = apply_outcome(
                highlighted,
                context.replacement_for_search_result(&highlighted.search_result),
            );
        }

        let sender = state.processing_sender.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_clone = Arc::clone(&cancelled);
        let handle = spawn_debounced(PREVIEW_DEBOUNCE, move || {
            let _ = sender.send(BackgroundProcessingEvent::UpdateAllReplacements {
                cancelled: cancelled_clone,
            });
        });
        search_fields_state.preview_update_state =
            Some(PreviewUpdateStatus::new(handle, cancelled));
    }

    fn handle_replacement_config_change(&mut self) -> EventHandlingResult {
        self.ui_state
            .current_screen
            .unwrap_search_fields_state_mut()
            .cancel_preview_updates();
        if !self.revalidate_and_store_searcher() {
            return EventHandlingResult::Rerender;
        }
        self.refresh_selected_and_schedule_preview_updates();
        EventHandlingResult::Rerender
    }

    fn get_search_state_unwrap(&mut self) -> &mut SearchState {
        self.ui_state
            .current_screen
            .unwrap_search_fields_state_mut()
            .search_state
            .as_mut()
            .expect("Focussed on search results but search_state is None")
    }

    #[allow(dead_code)]
    fn get_search_state_if_results(&mut self) -> Option<&mut SearchState> {
        if let Screen::SearchFields(ref mut state) = self.ui_state.current_screen {
            state.search_state.as_mut()
        } else {
            None
        }
    }

    fn focus_field(&mut self, field_index: usize) -> EventHandlingResult {
        // Only focus the field if it's in the current focus list
        let indices = self.focus_field_indices();
        if !indices.contains(&field_index) {
            return EventHandlingResult::None;
        }
        let sfs = self
            .ui_state
            .current_screen
            .unwrap_search_fields_state_mut();
        sfs.focussed_section = FocussedSection::SearchFields;
        self.search_fields.highlighted = field_index;
        EventHandlingResult::Rerender
    }

    /// Should only be called on `Screen::SearchFields`, and when focussed section is `FocussedSection::SearchResults`
    #[allow(clippy::needless_pass_by_value)]
    fn handle_command_search_results(
        &mut self,
        event: CommandSearchFocusResults,
    ) -> EventHandlingResult {
        assert!(
            matches!(self.ui_state.current_screen, Screen::SearchFields(_)),
            "Expected current_screen to be SearchFields, found {}",
            self.ui_state.current_screen.name()
        );

        match event {
            CommandSearchFocusResults::TriggerReplacement => {
                self.perform_replacement();
                EventHandlingResult::Rerender
            }
            CommandSearchFocusResults::BackToFields => {
                let search_fields_state = self
                    .ui_state
                    .current_screen
                    .unwrap_search_fields_state_mut();
                search_fields_state.focussed_section = FocussedSection::SearchFields;
                EventHandlingResult::Rerender
            }
            CommandSearchFocusResults::OpenInEditor => {
                let search_fields_state = self
                    .ui_state
                    .current_screen
                    .unwrap_search_fields_state_mut();
                if let Some(ref mut search_in_progress_state) = search_fields_state.search_state {
                    let selected = search_in_progress_state
                        .primary_selected_field_mut()
                        .expect("Expected to find selected field");
                    if let Some(ref path) = selected.search_result.path {
                        self.event_channels
                            .sender
                            .send(Event::LaunchEditor((
                                path.clone(),
                                selected.search_result.start_line_number(),
                            )))
                            .expect("Failed to send event");
                    }
                }
                EventHandlingResult::Rerender
            }
            CommandSearchFocusResults::MoveDown => {
                self.get_search_state_unwrap().move_selected_down();
                EventHandlingResult::Rerender
            }
            CommandSearchFocusResults::MoveUp => {
                self.get_search_state_unwrap().move_selected_up();
                EventHandlingResult::Rerender
            }
            CommandSearchFocusResults::MoveNextFile => {
                self.get_search_state_unwrap().move_to_next_file();
                EventHandlingResult::Rerender
            }
            CommandSearchFocusResults::MovePrevFile => {
                self.get_search_state_unwrap().move_to_prev_file();
                EventHandlingResult::Rerender
            }
            CommandSearchFocusResults::MoveDownHalfPage => {
                self.get_search_state_unwrap()
                    .move_selected_down_half_page();
                EventHandlingResult::Rerender
            }
            CommandSearchFocusResults::MoveDownFullPage => {
                self.get_search_state_unwrap()
                    .move_selected_down_full_page();
                EventHandlingResult::Rerender
            }
            CommandSearchFocusResults::MoveUpHalfPage => {
                self.get_search_state_unwrap().move_selected_up_half_page();
                EventHandlingResult::Rerender
            }
            CommandSearchFocusResults::MoveUpFullPage => {
                self.get_search_state_unwrap().move_selected_up_full_page();
                EventHandlingResult::Rerender
            }
            CommandSearchFocusResults::MoveTop => {
                self.get_search_state_unwrap().move_selected_top();
                EventHandlingResult::Rerender
            }
            CommandSearchFocusResults::MoveBottom => {
                self.get_search_state_unwrap().move_selected_bottom();
                EventHandlingResult::Rerender
            }
            CommandSearchFocusResults::ToggleSelectedInclusion => {
                self.get_search_state_unwrap().toggle_selected_inclusion();
                EventHandlingResult::Rerender
            }
            CommandSearchFocusResults::ToggleAllSelected => {
                self.get_search_state_unwrap().toggle_all_selected();
                EventHandlingResult::Rerender
            }
            CommandSearchFocusResults::ToggleMultiselectMode => {
                self.get_search_state_unwrap().toggle_multiselect_mode();
                EventHandlingResult::Rerender
            }
            CommandSearchFocusResults::FlipMultiselectDirection => {
                self.get_search_state_unwrap().flip_multiselect_direction();
                EventHandlingResult::Rerender
            }
            CommandSearchFocusResults::ToggleCurrentFileSelected => {
                self.get_search_state_unwrap().toggle_current_file_selected();
                EventHandlingResult::Rerender
            }
            CommandSearchFocusResults::EnterInsertMode => {
                // Switch from results focus to fields focus
                let sfs = self
                    .ui_state
                    .current_screen
                    .unwrap_search_fields_state_mut();
                sfs.focussed_section = FocussedSection::SearchFields;
                EventHandlingResult::Rerender
            }
            CommandSearchFocusResults::BackspaceToSearch => {
                // Focus the search field and delete the last character
                self.focus_field(0);
                self.enter_chars_into_field(KeyCode::Backspace, KeyModifiers::NONE)
            }
        }
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> EventHandlingResult {
        let command = match self.handle_special_cases(key_event) {
            Left(command) => command,
            Right(event_handling_result) => return event_handling_result,
        };

        // Note that general commands are looked up after screen-specific commands in `.lookup`, so this if will only be hit
        // if there are no screen-specific commands
        if let Command::General(command) = command {
            match command {
                CommandGeneral::Quit => {
                    self.reset();
                    return EventHandlingResult::Exit(None);
                }
                CommandGeneral::Reset => {
                    self.reset();
                    return EventHandlingResult::Rerender;
                }
                CommandGeneral::ShowHelpMenu => {
                    self.set_popup(Popup::Help);
                    return EventHandlingResult::Rerender;
                }
            }
        }

        match &mut self.ui_state.current_screen {
            Screen::SearchFields(search_fields_state) => {
                // If replacement is in progress, ignore all keys (quit handled above)
                if search_fields_state.replacement_progress.is_some() {
                    return EventHandlingResult::None;
                }

                let Command::SearchFields(command) = command else {
                    panic!("Expected SearchFields command, found {command:?}");
                };

                match command {
                    CommandSearchFields::TogglePreviewWrapping => {
                        self.config.preview.wrap_text = !self.config.preview.wrap_text;
                        self.show_toggle_toast("Text wrapping", self.config.preview.wrap_text);
                        EventHandlingResult::Rerender
                    }
                    CommandSearchFields::ToggleHiddenFiles => {
                        if matches!(self.input_source, InputSource::Stdin(_)) {
                            return EventHandlingResult::None;
                        }
                        self.run_config.include_hidden = !self.run_config.include_hidden;
                        self.show_toggle_toast("Hidden files", self.run_config.include_hidden);
                        self.perform_search_background();
                        EventHandlingResult::Rerender
                    }
                    CommandSearchFields::ToggleMultiline => {
                        self.run_config.multiline = !self.run_config.multiline;
                        if self.run_config.multiline {
                            self.ui_state.hints.has_shown_multiline_hint = false;
                        }
                        self.show_toggle_toast("Multiline", self.run_config.multiline);
                        self.perform_search_background();
                        EventHandlingResult::Rerender
                    }
                    CommandSearchFields::ToggleInterpretEscapeSequences => {
                        self.run_config.interpret_escape_sequences =
                            !self.run_config.interpret_escape_sequences;
                        self.show_toggle_toast(
                            "Escape sequences",
                            self.run_config.interpret_escape_sequences,
                        );
                        self.handle_replacement_config_change()
                    }
                    CommandSearchFields::ToggleFixedStrings => {
                        let checked = {
                            let cb = self.search_fields.fixed_strings_mut();
                            cb.checked = !cb.checked;
                            cb.checked
                        };
                        self.show_toggle_toast("Fixed strings", checked);
                        self.handle_replacement_config_change()
                    }
                    CommandSearchFields::ToggleMatchWholeWord => {
                        let checked = {
                            let cb = self.search_fields.whole_word_mut();
                            cb.checked = !cb.checked;
                            cb.checked
                        };
                        self.show_toggle_toast("Whole word", checked);
                        self.handle_replacement_config_change()
                    }
                    CommandSearchFields::ToggleMatchCase => {
                        let checked = {
                            let cb = self.search_fields.match_case_mut();
                            cb.checked = !cb.checked;
                            cb.checked
                        };
                        self.show_toggle_toast("Case sensitive", checked);
                        self.handle_replacement_config_change()
                    }
                    CommandSearchFields::ResizeColumnShrink => {
                        let sfs = self
                            .ui_state
                            .current_screen
                            .unwrap_search_fields_state_mut();
                        sfs.file_column_width_pct = sfs.file_column_width_pct.saturating_sub(3).clamp(10, 80);
                        EventHandlingResult::Rerender
                    }
                    CommandSearchFields::ResizeColumnGrow => {
                        let sfs = self
                            .ui_state
                            .current_screen
                            .unwrap_search_fields_state_mut();
                        sfs.file_column_width_pct = sfs.file_column_width_pct.saturating_add(3).clamp(10, 80);
                        EventHandlingResult::Rerender
                    }
                    CommandSearchFields::SearchFocusFields(command) => {
                        if !matches!(
                            search_fields_state.focussed_section,
                            FocussedSection::SearchFields
                        ) {
                            panic!(
                                "Expected FocussedSection::SearchFields, found {:?}",
                                search_fields_state.focussed_section
                            );
                        }
                        self.handle_command_search_fields(command)
                    }
                    CommandSearchFields::SearchFocusResults(command) => {
                        if !matches!(
                            search_fields_state.focussed_section,
                            FocussedSection::SearchResults
                        ) {
                            panic!(
                                "Expected FocussedSection::SearchResults, found {:?}",
                                search_fields_state.focussed_section
                            );
                        }
                        self.handle_command_search_results(command)
                    }
                }
            }
            // These screens are no longer used — replacement progress is shown inline
            // on SearchFields, and results are shown as a popup. Kept for compatibility.
            Screen::PerformingReplacement(_) => EventHandlingResult::None,
            Screen::Results(replace_state) => {
                let Command::Results(command) = command else {
                    panic!("Expected Results command, found {command:?}");
                };
                replace_state.handle_command_results(command)
            }
        }
    }

    fn handle_special_cases(
        &mut self,
        key_event: KeyEvent,
    ) -> Either<Command, EventHandlingResult> {
        // File finder takes priority over everything except Quit
        if self.ui_state.file_finder.is_some() {
            if matches!(key_event.code, KeyCode::Esc) {
                self.close_file_finder();
                return Right(EventHandlingResult::Rerender);
            }
            return Right(self.handle_file_finder_key(key_event));
        }

        // Handle pending prefix key (for two-key sequences like :q and zl)
        if let Some(prefix_key) = self.ui_state.pending_prefix.take() {
            if let Some(command) = self.key_map.lookup_prefix(prefix_key, key_event) {
                return Left(command);
            }
            // No match for prefix combo — fall through to normal processing
        }

        // Handle pending escape in fields focus.
        // When active, the next key press is treated as a command lookup
        // rather than text input.  This lets users type literal ':', '/', '%',
        // etc. in search fields — only Esc+:q triggers quit, not bare :q.
        if self.ui_state.pending_escape {
            if let Screen::SearchFields(state) = &self.ui_state.current_screen {
                if state.focussed_section == FocussedSection::SearchFields {
                    self.ui_state.pending_escape = false;

                    // Esc pressed again → cancel
                    if matches!(key_event.code, KeyCode::Esc) {
                        return Right(EventHandlingResult::None);
                    }

                    // Try command lookup (handles tab, enter, control combos, etc.)
                    let maybe_cmd = self
                        .key_map
                        .lookup(&self.ui_state.current_screen, key_event);
                    if let Some(cmd) = maybe_cmd {
                        return Left(cmd);
                    }

                    // No direct match — check if this is a prefix key
                    // (e.g. ':' for ':q', 'z' for 'zl')
                    if key_event.prefix.is_none()
                        && !key_event.modifiers.contains(KeyModifiers::CONTROL)
                        && !key_event.modifiers.contains(KeyModifiers::ALT)
                        && self.key_map.has_prefix_for(key_event)
                    {
                        self.ui_state.pending_prefix = Some(key_event);
                        return Right(EventHandlingResult::None);
                    }

                    // No match at all — discard the key (don't enter as text)
                    return Right(EventHandlingResult::None);
                }
            }
            // Not in fields focus — clear and fall through
            self.ui_state.pending_escape = false;
        }

        // In fields focus, Esc activates command mode (pending_escape).
        // Bare character keys always enter as text — no command interception.
        if matches!(key_event.code, KeyCode::Esc) && key_event.prefix.is_none() {
            if let Screen::SearchFields(state) = &self.ui_state.current_screen {
                if state.focussed_section == FocussedSection::SearchFields {
                    self.ui_state.pending_escape = true;
                    return Right(EventHandlingResult::None);
                }
            }
        }

        let maybe_event = self
            .key_map
            .lookup(&self.ui_state.current_screen, key_event);

        // Quit should take precedent over closing popup etc.
        if !matches!(maybe_event, Some(Command::General(CommandGeneral::Quit))) {
            if self.ui_state.popup.is_some() {
                self.clear_popup();
                return Right(EventHandlingResult::Rerender);
            }
        }

        let event = if let Some(event) = maybe_event {
            event
        } else {
            if let Screen::SearchFields(state) = &self.ui_state.current_screen {
                if state.focussed_section == FocussedSection::SearchFields {
                    // Fields focus: enter unmatched keys as text.
                    // No prefix check here — bare chars always type text.
                    // Prefix-key sequences require Esc first (see above).
                    return Left(Command::SearchFields(CommandSearchFields::SearchFocusFields(
                        CommandSearchFocusFields::EnterChars(key_event.code, key_event.modifiers),
                    )));
                } else {
                    // In results focus: check if this is a prefix key first
                    if key_event.prefix.is_none()
                        && !key_event.modifiers.contains(KeyModifiers::CONTROL)
                        && !key_event.modifiers.contains(KeyModifiers::ALT)
                        && self.key_map.has_prefix_for(key_event)
                    {
                        self.ui_state.pending_prefix = Some(key_event);
                        return Right(EventHandlingResult::None);
                    }

                    // Check if this key is a field-specific command (e.g. "/" for
                    // focus_search_field).  Execute it without inserting the char.
                    if key_event.prefix.is_none()
                        && !key_event.modifiers.contains(KeyModifiers::CONTROL)
                        && !key_event.modifiers.contains(KeyModifiers::ALT)
                    {
                        if let Some(field_cmd) = self.key_map.lookup_search_fields(key_event) {
                            let sfs = self
                                .ui_state
                                .current_screen
                                .unwrap_search_fields_state_mut();
                            sfs.focussed_section = FocussedSection::SearchFields;
                            return Left(Command::SearchFields(
                                CommandSearchFields::SearchFocusFields(field_cmd),
                            ));
                        }
                    }

                    // Forward unbound char keys to the search field
                    if let KeyCode::Char(_) = key_event.code {
                        if !key_event.modifiers.contains(KeyModifiers::CONTROL)
                            && !key_event.modifiers.contains(KeyModifiers::ALT)
                        {
                            let sfs = self
                                .ui_state
                                .current_screen
                                .unwrap_search_fields_state_mut();
                            sfs.focussed_section = FocussedSection::SearchFields;
                            return Right(self.enter_chars_into_field(
                                key_event.code,
                                key_event.modifiers,
                            ));
                        }
                    }
                    return Right(EventHandlingResult::None);
                }
            } else {
                return Right(EventHandlingResult::None);
            }
        };

        Left(event)
    }

    pub fn current_search_key(&self) -> SearchKey {
        let dir = match &self.input_source {
            InputSource::Directory(directory) => Some(DirSearchKey {
                include_globs: self.search_fields.include_files().text().to_owned(),
                exclude_globs: self.search_fields.exclude_files().text().to_owned(),
                include_hidden: self.run_config.include_hidden,
                include_git_folders: self.run_config.include_git_folders,
                directory: directory.clone(),
            }),
            InputSource::Stdin(_) => None,
        };
        SearchKey {
            search_text: self.search_fields.search().text().to_owned(),
            fixed_strings: self.search_fields.fixed_strings().checked,
            advanced_regex: self.run_config.advanced_regex,
            match_whole_word: self.search_fields.whole_word().checked,
            match_case: self.search_fields.match_case().checked,
            multiline: self.run_config.multiline,
            dir,
        }
    }

    pub fn validate_fields(&mut self) -> anyhow::Result<Option<Searcher>> {
        let search_config = SearchConfig {
            search_text: self.search_fields.search().text(),
            replacement_text: self.search_fields.replace().text(),
            fixed_strings: self.search_fields.fixed_strings().checked,
            advanced_regex: self.run_config.advanced_regex,
            match_whole_word: self.search_fields.whole_word().checked,
            match_case: self.search_fields.match_case().checked,
            multiline: self.run_config.multiline,
            interpret_escape_sequences: self.run_config.interpret_escape_sequences,
        };
        let dir_config = match &self.input_source {
            InputSource::Directory(directory) => Some(DirConfig {
                include_globs: Some(self.search_fields.include_files().text()),
                exclude_globs: Some(self.search_fields.exclude_files().text()),
                include_hidden: self.run_config.include_hidden,
                include_git_folders: self.run_config.include_git_folders,
                directory: directory.clone(),
            }),
            InputSource::Stdin(_) => None,
        };

        let mut error_handler = AppErrorHandler::new();
        let result = validate_search_configuration(search_config, dir_config, &mut error_handler)?;
        error_handler.apply_to_app(self);

        let maybe_searcher = match result {
            ValidationResult::Success((search_config, dir_config)) => match &self.input_source {
                InputSource::Directory(_) => {
                    let file_searcher = FileSearcher::new(
                        search_config,
                        dir_config.expect("Found None dir_config when searching through files"),
                    );
                    Some(Searcher::FileSearcher(file_searcher))
                }
                InputSource::Stdin(_) => Some(Searcher::TextSearcher { search_config }),
            },
            ValidationResult::ValidationErrors => None,
        };
        Ok(maybe_searcher)
    }

    fn spawn_search_task(
        strategy: SearchStrategy,
        background_processing_sender: UnboundedSender<BackgroundProcessingEvent>,
        event_sender: UnboundedSender<Event>,
        cancelled: Arc<AtomicBool>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let sender_for_search = background_processing_sender.clone();
            let mut search_handle = task::spawn_blocking(move || {
                match strategy {
                    SearchStrategy::Files(file_searcher) => {
                        file_searcher.walk_files(Some(&cancelled), || {
                            let sender = sender_for_search.clone();
                            Box::new(move |results| {
                                // Ignore error - likely state reset, thread about to be killed
                                let _ = sender
                                    .send(BackgroundProcessingEvent::AddSearchResults(results));
                                WalkState::Continue
                            })
                        });
                    }
                    SearchStrategy::Text { haystack, config } => {
                        // When multiline is enabled, search the entire haystack at once
                        if config.multiline {
                            for result in search_multiline(&haystack, &config.search, None) {
                                if cancelled.load(Ordering::Relaxed) {
                                    break;
                                }
                                // Ignore error - likely state reset, thread about to be killed
                                let _ = sender_for_search
                                    .send(BackgroundProcessingEvent::AddSearchResult(result));
                            }
                        } else {
                            // Default line-by-line search
                            let cursor = Cursor::new(haystack.as_bytes());
                            for (idx, line_result) in cursor.lines_with_endings().enumerate() {
                                if cancelled.load(Ordering::Relaxed) {
                                    break;
                                }

                                let (line_ending, line) = match read_line(line_result) {
                                    Ok(res) => res,
                                    Err(e) => {
                                        debug!("Error when reading line {idx}: {e}");
                                        continue;
                                    }
                                };
                                if contains_search(&line, &config.search) {
                                    let line_number = idx + 1;
                                    let result = SearchResult::new_line(
                                        None,
                                        line_number,
                                        line,
                                        line_ending,
                                        false,
                                    );
                                    // Ignore error - likely state reset, thread about to be killed
                                    let _ = sender_for_search
                                        .send(BackgroundProcessingEvent::AddSearchResult(result));
                                }
                            }
                        }
                    }
                }
            });

            let mut rerender_interval = tokio::time::interval(Duration::from_millis(92)); // Slightly random duration so that time taken isn't a round number
            rerender_interval.tick().await;

            loop {
                tokio::select! {
                    res = &mut search_handle => {
                        if let Err(e) = res {
                            warn!("Search thread panicked: {e}");
                        }
                        break;
                    },
                    _ = rerender_interval.tick() => {
                        let _ = event_sender.send(Event::Rerender);
                    }
                }
            }

            if let Err(err) =
                background_processing_sender.send(BackgroundProcessingEvent::SearchCompleted)
            {
                // Log and ignore error: likely have gone back to previous screen
                warn!("Found error when attempting to send SearchCompleted event: {err}");
            }
        })
    }

    pub fn show_popup(&self) -> bool {
        self.ui_state.popup.is_some()
    }

    pub fn popup(&self) -> Option<&Popup> {
        self.ui_state.popup.as_ref()
    }

    pub fn errors(&self) -> Vec<AppError> {
        let app_errors = self.ui_state.errors().iter().cloned();
        let field_errors = self.search_fields.errors().into_iter();
        app_errors.chain(field_errors).collect()
    }

    pub fn add_error(&mut self, error: AppError) {
        self.ui_state.popup = Some(Popup::Error);
        self.ui_state.add_error(error);
    }

    fn clear_popup(&mut self) {
        self.ui_state.popup = None;
        self.ui_state.clear_errors();
    }

    fn set_popup(&mut self, popup: Popup) {
        self.ui_state.popup = Some(popup);
    }

    /// Open file finder popup for include/exclude fields (issue #7)
    fn open_file_finder(&mut self, target: FileFinderTarget) {
        let base_dir = match &self.input_source {
            InputSource::Directory(dir) => dir.clone(),
            InputSource::Stdin(_) => PathBuf::from("."),
        };
        let entries = Self::list_directory_entries(&base_dir, "", 50);
        self.ui_state.file_finder = Some(FileFinderState {
            query: String::new(),
            entries,
            selected: 0,
            target_field: target,
            base_dir,
        });
    }

    fn close_file_finder(&mut self) {
        self.ui_state.file_finder = None;
    }

    fn handle_file_finder_key(&mut self, key_event: KeyEvent) -> EventHandlingResult {
        let Some(finder) = &mut self.ui_state.file_finder else {
            return EventHandlingResult::None;
        };

        match key_event.code {
            KeyCode::Enter => {
                // Insert selected entry into the target field
                let selected_entry = finder.entries.get(finder.selected).cloned();
                let target = finder.target_field;
                self.close_file_finder();
                if let Some(entry) = selected_entry {
                    self.insert_file_path(target, &entry);
                }
                EventHandlingResult::Rerender
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !finder.entries.is_empty() {
                    finder.selected = (finder.selected + 1) % finder.entries.len();
                }
                EventHandlingResult::Rerender
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if !finder.entries.is_empty() {
                    finder.selected = finder
                        .selected
                        .checked_sub(1)
                        .unwrap_or(finder.entries.len() - 1);
                }
                EventHandlingResult::Rerender
            }
            KeyCode::Backspace => {
                finder.query.pop();
                finder.entries = Self::list_directory_entries(&finder.base_dir, &finder.query, 50);
                finder.selected = 0;
                EventHandlingResult::Rerender
            }
            KeyCode::Char(c) => {
                finder.query.push(c);
                finder.entries = Self::list_directory_entries(&finder.base_dir, &finder.query, 50);
                finder.selected = 0;
                EventHandlingResult::Rerender
            }
            _ => EventHandlingResult::None,
        }
    }

    pub fn insert_file_path(&mut self, target: FileFinderTarget, path: &str) {
        let field = match target {
            FileFinderTarget::IncludeFiles => self.search_fields.include_files_mut(),
            FileFinderTarget::ExcludeFiles => self.search_fields.exclude_files_mut(),
        };
        let current = field.text();
        let separator = if current.is_empty() || current.ends_with(',') { "" } else { ", " };
        let new_text = format!("{current}{separator}{path}");
        // Use enter_chars to insert the text
        let chars: Vec<char> = new_text.chars().collect();
        let existing: Vec<char> = current.chars().collect();
        for c in chars.iter().skip(existing.len()) {
            field.enter_char(*c);
        }
    }

    fn list_directory_entries(base_dir: &Path, query: &str, limit: usize) -> Vec<String> {
        let mut entries = Vec::new();
        let query_lower = query.to_lowercase();

        // List top-level directories and files
        if let Ok(read_dir) = std::fs::read_dir(base_dir) {
            for entry in read_dir.flatten() {
                if entries.len() >= limit {
                    break;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if !query_lower.is_empty()
                    && !name.to_lowercase().contains(&query_lower)
                {
                    continue;
                }
                let suffix = if entry.file_type().map_or(false, |ft| ft.is_dir()) {
                    "/"
                } else {
                    ""
                };
                entries.push(format!("{name}{suffix}"));
            }
        }

        entries.sort_by(|a, b| {
            // Directories first
            let a_dir = a.ends_with('/');
            let b_dir = b.ends_with('/');
            b_dir.cmp(&a_dir).then_with(|| a.to_lowercase().cmp(&b.to_lowercase()))
        });
        entries.truncate(limit);
        entries
    }

    pub fn toast_message(&self) -> Option<&str> {
        self.ui_state.toast.as_ref().map(|t| t.message.as_str())
    }

    pub fn file_finder(&self) -> Option<&FileFinderState> {
        self.ui_state.file_finder.as_ref()
    }

    fn show_toast(&mut self, message: String, duration: Duration) {
        let generation = self.ui_state.toast.as_ref().map_or(1, |t| t.generation + 1);
        self.ui_state.toast = Some(Toast {
            message,
            generation,
        });

        let event_sender = self.event_channels.sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(duration).await;
            let _ = event_sender.send(Event::Internal(InternalEvent::App(
                AppEvent::DismissToast { generation },
            )));
        });
    }

    fn show_toggle_toast(&mut self, name: &str, enabled: bool) {
        let status = if enabled { "ON" } else { "OFF" };
        self.show_toast(format!("{name}: {status}"), Duration::from_millis(1500));
    }

    fn dismiss_toast_if_generation_matches(&mut self, generation: u64) {
        if let Some(toast) = &self.ui_state.toast
            && toast.generation == generation
        {
            self.ui_state.toast = None;
        }
    }

    pub fn keymaps_all(&self) -> Vec<(String, String)> {
        self.keymaps_impl(false)
    }

    pub fn keymaps_compact(&self) -> Vec<(String, String)> {
        self.keymaps_impl(true)
    }

    #[allow(clippy::too_many_lines)]
    fn keymaps_impl(&self, compact: bool) -> Vec<(String, String)> {
        enum Show {
            Both,
            FullOnly,
            #[allow(dead_code)]
            CompactOnly,
        }

        macro_rules! keymap {
            ($($path:tt).+, $name:expr, $show:expr $(,)?) => {
                (
                    format!("<{}>", self.config.keys.$($path).+.iter()
                        .map(|k| k.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")),
                    $name,
                    $show,
                )
            };
        }

        let current_screen_keys = match &self.ui_state.current_screen {
            Screen::SearchFields(search_fields_state) => {
                // During inline replacement progress, show minimal keymaps
                if search_fields_state.replacement_progress.is_some() {
                    vec![keymap!(general.quit, "quit", Show::Both)]
                } else {
                    let mut keys = vec![];
                match search_fields_state.focussed_section {
                    FocussedSection::SearchFields => {
                        keys.extend([
                            keymap!(search.fields.trigger_search, "jump to results", Show::Both),
                            keymap!(search.fields.focus_next_field, "focus next", Show::Both),
                            keymap!(
                                search.fields.focus_previous_field,
                                "focus previous",
                                Show::FullOnly,
                            ),
                            ("<space>".to_string(), "toggle checkbox", Show::FullOnly), // TODO(key-remap): add to config?
                        ]);
                        if self.config.search.disable_prepopulated_fields {
                            keys.push(keymap!(
                                search.fields.unlock_prepopulated_fields,
                                "unlock pre-populated fields",
                                if self.search_fields.fields.iter().any(|f| f.set_by_cli) {
                                    Show::Both
                                } else {
                                    Show::FullOnly
                                },
                            ));
                        }
                    }
                    FocussedSection::SearchResults => {
                        keys.extend([
                            keymap!(
                                search.results.toggle_selected_inclusion,
                                "toggle",
                                Show::Both,
                            ),
                            keymap!(
                                search.results.toggle_all_selected,
                                "toggle all",
                                Show::FullOnly,
                            ),
                            keymap!(
                                search.results.toggle_multiselect_mode,
                                "toggle multi-select mode",
                                Show::FullOnly,
                            ),
                            keymap!(
                                search.results.flip_multiselect_direction,
                                "flip multi-select direction",
                                Show::FullOnly,
                            ),
                            keymap!(
                                search.results.open_in_editor,
                                "open in editor",
                                Show::FullOnly,
                            ),
                            keymap!(
                                search.results.back_to_fields,
                                "back to search fields",
                                Show::Both,
                            ),
                            keymap!(search.results.move_down, "down (wraps)", Show::FullOnly),
                            keymap!(search.results.move_up, "up (wraps)", Show::FullOnly),
                            keymap!(
                                search.results.move_next_file,
                                "next file",
                                Show::FullOnly
                            ),
                            keymap!(
                                search.results.move_prev_file,
                                "prev file",
                                Show::FullOnly
                            ),
                            keymap!(
                                search.results.move_up_half_page,
                                "up half a page",
                                Show::FullOnly
                            ),
                            keymap!(
                                search.results.move_down_half_page,
                                "down half a page",
                                Show::FullOnly
                            ),
                            keymap!(
                                search.results.move_up_full_page,
                                "up a full page",
                                Show::FullOnly
                            ),
                            keymap!(
                                search.results.move_down_full_page,
                                "down a full page",
                                Show::FullOnly
                            ),
                            keymap!(search.results.move_top, "jump to top", Show::FullOnly),
                            keymap!(search.results.move_bottom, "jump to bottom", Show::FullOnly),
                        ]);
                        if self.search_has_completed() {
                            keys.push(keymap!(
                                search.results.trigger_replacement,
                                "replace selected",
                                Show::Both,
                            ));
                        }
                    }
                }
                keys.push(keymap!(
                    search.toggle_preview_wrapping,
                    "toggle text wrapping in preview",
                    Show::FullOnly,
                ));
                if matches!(self.input_source, InputSource::Directory(_)) {
                    keys.push(keymap!(
                        search.toggle_hidden_files,
                        "toggle hidden files",
                        Show::FullOnly,
                    ));
                }
                keys.push(keymap!(
                    search.toggle_multiline,
                    "toggle multiline",
                    Show::FullOnly,
                ));
                keys.push(keymap!(
                    search.toggle_interpret_escape_sequences,
                    "toggle escape sequences",
                    Show::FullOnly,
                ));
                keys.push(keymap!(
                    search.toggle_fixed_strings,
                    "toggle fixed strings",
                    Show::FullOnly,
                ));
                keys.push(keymap!(
                    search.toggle_match_whole_word,
                    "toggle whole word",
                    Show::FullOnly,
                ));
                keys.push(keymap!(
                    search.toggle_match_case,
                    "toggle case sensitive",
                    Show::FullOnly,
                ));
                keys
                } // end of else (non-replacement-progress)
            }
            Screen::PerformingReplacement(_) => vec![],
            Screen::Results(replace_state) => {
                if !replace_state.errors.is_empty() {
                    vec![
                        keymap!(results.scroll_errors_down, "down", Show::Both),
                        keymap!(results.scroll_errors_up, "up", Show::Both),
                    ]
                } else {
                    vec![]
                }
            }
        };

        let on_search_results = if let Screen::SearchFields(ref s) = self.ui_state.current_screen {
            s.focussed_section == FocussedSection::SearchResults
        } else {
            false
        };

        let esc_help = if on_search_results {
            "close popup / exit multi-select".to_string()
        } else {
            "command mode (type a command key)".to_string()
        };

        let additional_keys = vec![
            keymap!(
                general.reset,
                "reset",
                if on_search_results {
                    Show::FullOnly
                } else {
                    Show::Both
                },
            ),
            keymap!(general.show_help_menu, "help", Show::Both),
            ("<esc>".to_string(), esc_help.as_str(), Show::FullOnly),
            keymap!(general.quit, "quit", Show::Both),
        ];

        // Hard-coded shortcuts handled by interception (not in keymap)
        let intercepted_keys: Vec<(String, &str, Show)> = {
            let mut k = vec![];
            // Field focus shortcuts
            k.push(("<C-s>".to_string(), "focus search", Show::FullOnly));
            k.push(("<C-r>".to_string(), "focus replace", Show::FullOnly));
            k.push(("<C-i>".to_string(), "focus include", Show::FullOnly));
            k.push(("<C-e>".to_string(), "focus exclude", Show::FullOnly));
            k.push(("<C-t>".to_string(), "focus fixed toggle", Show::FullOnly));
            // Toggle all (hard-coded)
            k.push(("<C-w>".to_string(), "toggle all", Show::FullOnly));
            // Column resize (hard-coded)
            if on_search_results {
                k.push(("<C-left>".to_string(), "narrow file column", Show::FullOnly));
                k.push(("<C-right>".to_string(), "widen file column", Show::FullOnly));
            }
            k
        };

        let all_keys = current_screen_keys
            .into_iter()
            .chain(additional_keys)
            .chain(intercepted_keys);

        all_keys
            .filter_map(move |(from, to, show)| {
                let include = match show {
                    Show::Both => true,
                    Show::CompactOnly => compact,
                    Show::FullOnly => !compact,
                };
                if include {
                    Some((from, to.to_owned()))
                } else {
                    None
                }
            })
            .collect()
    }

    #[allow(dead_code)]
    fn multiselect_enabled(&self) -> bool {
        match &self.ui_state.current_screen {
            Screen::SearchFields(SearchFieldsState {
                search_state: Some(state),
                ..
            }) => state.multiselect_enabled(),
            _ => false,
        }
    }

    #[allow(dead_code)]
    fn toggle_multiselect_mode(&mut self) {
        match &mut self.ui_state.current_screen {
            Screen::SearchFields(SearchFieldsState {
                search_state: Some(state),
                ..
            }) => state.toggle_multiselect_mode(),
            _ => panic!(
                "Tried to disable multi-select on {:?}",
                self.ui_state.current_screen.name()
            ),
        }
    }

    fn unlock_prepopulated_fields(&mut self) {
        for field in &mut self.search_fields.fields {
            field.set_by_cli = false;
        }
    }

    pub fn search_has_completed(&self) -> bool {
        if let Screen::SearchFields(SearchFieldsState {
            search_state: Some(state),
            ..
        }) = &self.ui_state.current_screen
        {
            // `Complete` already implies the debounce has fired (state only
            // transitions out of `Pending` when `perform_search_already_validated`
            // runs), so no separate debounce-timer check is needed.
            state.phase.is_complete()
        } else {
            false
        }
    }

    pub fn is_preview_updated(&self) -> bool {
        if let Screen::SearchFields(SearchFieldsState {
            search_state:
                Some(SearchState {
                    processing_receiver,
                    ..
                }),
            preview_update_state,
            ..
        }) = &self.ui_state.current_screen
        {
            processing_receiver.is_empty()
                && preview_update_state
                    .as_ref()
                    .is_none_or(|p| p.replace_debounce_timer.is_finished())
        } else {
            false
        }
    }
}

fn read_line(
    line_result: Result<(Vec<u8>, LineEnding), std::io::Error>,
) -> anyhow::Result<(LineEnding, String)> {
    let (line_bytes, line_ending) = line_result?;
    let line = String::from_utf8(line_bytes)?;
    Ok((line_ending, line))
}

#[allow(clippy::struct_field_names)]
#[derive(Clone, Debug, PartialEq, Eq)]
struct AppErrorHandler {
    search_errors: Option<(String, String)>,
    include_errors: Option<(String, String)>,
    exclude_errors: Option<(String, String)>,
}

impl AppErrorHandler {
    fn new() -> Self {
        Self {
            search_errors: None,
            include_errors: None,
            exclude_errors: None,
        }
    }

    fn apply_to_app(&self, app: &mut App) {
        if let Some((error, detail)) = &self.search_errors {
            app.search_fields
                .search_mut()
                .set_error(error.clone(), detail.clone());
        }

        if let Some((error, detail)) = &self.include_errors {
            app.search_fields
                .include_files_mut()
                .set_error(error.clone(), detail.clone());
        }

        if let Some((error, detail)) = &self.exclude_errors {
            app.search_fields
                .exclude_files_mut()
                .set_error(error.clone(), detail.clone());
        }
    }
}

impl ValidationErrorHandler for AppErrorHandler {
    fn handle_search_text_error(&mut self, error: &str, detail: &str) {
        self.search_errors = Some((error.to_owned(), detail.to_string()));
    }

    fn handle_include_files_error(&mut self, error: &str, detail: &str) {
        self.include_errors = Some((error.to_owned(), detail.to_string()));
    }

    fn handle_exclude_files_error(&mut self, error: &str, detail: &str) {
        self.exclude_errors = Some((error.to_owned(), detail.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        line_reader::LineEnding,
        replace::{ReplaceResult, ReplaceStats},
        search::{SearchResult, SearchResultWithReplacement},
    };
    use rand::RngExt;

    use super::*;

    #[test]
    fn replacement_context_skips_stale_results() {
        let input_source = InputSource::Stdin(Arc::new(String::new()));
        let searcher = Searcher::TextSearcher {
            search_config: ParsedSearchConfig {
                search: SearchType::Fixed("foo".to_string()),
                replace: "bar".to_string(),
                multiline: false,
            },
        };
        let mut context = ReplacementContext::new(
            &input_source,
            &searcher,
            searcher.search().needs_haystack_context(),
            default_file_content_provider(),
        );
        let result = SearchResult::new_line(None, 1, "baz".to_string(), LineEnding::Lf, true);

        assert!(matches!(
            context.replacement_for_search_result(&result),
            PreviewOutcome::NoMatch
        ));
    }

    fn random_num() -> usize {
        let mut rng = rand::rng();
        rng.random_range(1..10000)
    }

    fn search_result_with_replacement(included: bool) -> SearchResultWithReplacement {
        let line_num = random_num();
        SearchResultWithReplacement {
            search_result: SearchResult::new_line(
                Some(PathBuf::from("random/file")),
                line_num,
                "foo".to_owned(),
                LineEnding::Lf,
                included,
            ),
            replacement: "bar".to_owned(),
            replace_result: None,
            preview_error: None,
        }
    }

    fn build_test_results(num_results: usize) -> Vec<SearchResultWithReplacement> {
        (0..num_results)
            .map(|i| SearchResultWithReplacement {
                search_result: SearchResult::new_line(
                    Some(PathBuf::from(format!("test{i}.txt"))),
                    1,
                    format!("test line {i}").to_string(),
                    LineEnding::Lf,
                    false,
                ),
                replacement: format!("replacement {i}").to_string(),
                replace_result: None,
                preview_error: None,
            })
            .collect()
    }

    fn build_test_search_state(num_results: usize) -> SearchState {
        let results = build_test_results(num_results);
        build_test_search_state_with_results(results)
    }

    fn build_test_search_state_with_results(
        results: Vec<SearchResultWithReplacement>,
    ) -> SearchState {
        let (processing_sender, processing_receiver) = mpsc::unbounded_channel();
        SearchState {
            results,
            selected: Selected::Single(0),
            view_offset: 0,
            num_displayed: Some(5),
            processing_receiver,
            processing_sender,
            cancelled: Arc::new(AtomicBool::new(false)),
            last_render: Instant::now(),
            phase: SearchPhase::Running {
                started: Instant::now(),
            },
        }
    }

    #[test]
    fn test_toggle_all_selected_when_all_selected() {
        let mut search_state = build_test_search_state_with_results(vec![
            search_result_with_replacement(true),
            search_result_with_replacement(true),
            search_result_with_replacement(true),
        ]);
        search_state.toggle_all_selected();
        assert_eq!(
            search_state
                .results
                .iter()
                .map(|res| res.search_result.included)
                .collect::<Vec<_>>(),
            vec![false, false, false]
        );
    }

    #[test]
    fn test_toggle_all_selected_when_none_selected() {
        let mut search_state = build_test_search_state_with_results(vec![
            search_result_with_replacement(false),
            search_result_with_replacement(false),
            search_result_with_replacement(false),
        ]);
        search_state.toggle_all_selected();
        assert_eq!(
            search_state
                .results
                .iter()
                .map(|res| res.search_result.included)
                .collect::<Vec<_>>(),
            vec![true, true, true]
        );
    }

    #[test]
    fn test_toggle_all_selected_when_some_selected() {
        let mut search_state = build_test_search_state_with_results(vec![
            search_result_with_replacement(true),
            search_result_with_replacement(false),
            search_result_with_replacement(true),
        ]);
        search_state.toggle_all_selected();
        assert_eq!(
            search_state
                .results
                .iter()
                .map(|res| res.search_result.included)
                .collect::<Vec<_>>(),
            vec![true, true, true]
        );
    }

    #[test]
    fn test_toggle_all_selected_when_no_results() {
        let mut search_state = build_test_search_state_with_results(vec![]);
        search_state.toggle_all_selected();
        assert_eq!(
            search_state
                .results
                .iter()
                .map(|res| res.search_result.included)
                .collect::<Vec<_>>(),
            vec![] as Vec<bool>
        );
    }

    fn success_result() -> SearchResultWithReplacement {
        let line_num = random_num();
        SearchResultWithReplacement {
            search_result: SearchResult::new_line(
                Some(PathBuf::from("random/file")),
                line_num,
                "foo".to_owned(),
                LineEnding::Lf,
                true,
            ),
            replacement: "bar".to_owned(),
            replace_result: Some(ReplaceResult::Success),
            preview_error: None,
        }
    }

    fn ignored_result() -> SearchResultWithReplacement {
        let line_num = random_num();
        SearchResultWithReplacement {
            search_result: SearchResult::new_line(
                Some(PathBuf::from("random/file")),
                line_num,
                "foo".to_owned(),
                LineEnding::Lf,
                false,
            ),
            replacement: "bar".to_owned(),
            replace_result: None,
            preview_error: None,
        }
    }

    fn error_result() -> SearchResultWithReplacement {
        let line_num = random_num();
        SearchResultWithReplacement {
            search_result: SearchResult::new_line(
                Some(PathBuf::from("random/file")),
                line_num,
                "foo".to_owned(),
                LineEnding::Lf,
                true,
            ),
            replacement: "bar".to_owned(),
            replace_result: Some(ReplaceResult::Error("error".to_owned())),
            preview_error: None,
        }
    }

    #[tokio::test]
    async fn test_calculate_statistics_all_success() {
        let search_results_with_replacements =
            vec![success_result(), success_result(), success_result()];

        let (results, _preview_errored, _num_ignored) =
            crate::replace::split_results(search_results_with_replacements);
        let stats = crate::replace::calculate_statistics(results);

        assert_eq!(
            stats,
            ReplaceStats {
                num_successes: 3,
                errors: vec![],
            }
        );
    }

    #[tokio::test]
    async fn test_calculate_statistics_with_ignores_and_errors() {
        let error_result = error_result();
        let search_results_with_replacements = vec![
            success_result(),
            ignored_result(),
            success_result(),
            error_result.clone(),
            ignored_result(),
        ];

        let (results, _preview_errored, _num_ignored) =
            crate::replace::split_results(search_results_with_replacements);
        let stats = crate::replace::calculate_statistics(results);

        assert_eq!(
            stats,
            ReplaceStats {
                num_successes: 2,
                errors: vec![error_result],
            }
        );
    }

    #[tokio::test]
    async fn test_search_state_toggling() {
        fn included(state: &SearchState) -> Vec<bool> {
            state
                .results
                .iter()
                .map(|r| r.search_result.included)
                .collect::<Vec<_>>()
        }

        let mut state = build_test_search_state(3);

        // Results start unselected (#2: opt-in replacement)
        assert_eq!(included(&state), [false, false, false]);
        // Toggle selected (index 0): unselected → selected, then auto-move down (#5)
        state.toggle_selected_inclusion();
        assert_eq!(included(&state), [true, false, false]);
        assert_eq!(state.primary_selected_pos(), 1);
        // Toggle again: unselected → selected at index 1, auto-move down
        state.toggle_selected_inclusion();
        assert_eq!(included(&state), [true, true, false]);
        assert_eq!(state.primary_selected_pos(), 2);
        // Toggle at last item: unselected → selected, stays at 2 (wraps to 0)
        state.toggle_selected_inclusion();
        assert_eq!(included(&state), [true, true, true]);
    }

    #[tokio::test]
    async fn test_search_state_movement_single() {
        let mut state = build_test_search_state(3);

        assert_eq!(state.selected, Selected::Single(0));
        state.move_selected_down();
        assert_eq!(state.selected, Selected::Single(1));
        state.move_selected_down();
        assert_eq!(state.selected, Selected::Single(2));
        state.move_selected_down();
        assert_eq!(state.selected, Selected::Single(0));
        state.move_selected_down();
        assert_eq!(state.selected, Selected::Single(1));
        state.move_selected_up();
        assert_eq!(state.selected, Selected::Single(0));
        state.move_selected_up();
        assert_eq!(state.selected, Selected::Single(2));
        state.move_selected_up();
        assert_eq!(state.selected, Selected::Single(1));
    }

    #[tokio::test]
    async fn test_search_state_movement_top_bottom() {
        let mut state = build_test_search_state(3);

        state.move_selected_top();
        assert_eq!(state.selected, Selected::Single(0));
        state.move_selected_bottom();
        assert_eq!(state.selected, Selected::Single(2));
        state.move_selected_bottom();
        assert_eq!(state.selected, Selected::Single(2));
        state.move_selected_top();
        assert_eq!(state.selected, Selected::Single(0));
    }

    #[tokio::test]
    async fn test_search_state_movement_half_page_increments() {
        let mut state = build_test_search_state(8);

        assert_eq!(state.selected, Selected::Single(0));
        state.move_selected_down_half_page();
        assert_eq!(state.selected, Selected::Single(3));
        state.move_selected_down_half_page();
        assert_eq!(state.selected, Selected::Single(6));
        state.move_selected_down_half_page();
        assert_eq!(state.selected, Selected::Single(7));
        state.move_selected_up_half_page();
        assert_eq!(state.selected, Selected::Single(4));
        state.move_selected_up_half_page();
        assert_eq!(state.selected, Selected::Single(1));
        state.move_selected_up_half_page();
        assert_eq!(state.selected, Selected::Single(0));
        state.move_selected_up_half_page();
        assert_eq!(state.selected, Selected::Single(7));
        state.move_selected_up_half_page();
        assert_eq!(state.selected, Selected::Single(4));
        state.move_selected_down_half_page();
        assert_eq!(state.selected, Selected::Single(7));
        state.move_selected_down_half_page();
        assert_eq!(state.selected, Selected::Single(0));
    }

    #[tokio::test]
    async fn test_search_state_movement_page_increments() {
        let mut state = build_test_search_state(12);

        assert_eq!(state.selected, Selected::Single(0));
        state.move_selected_down_full_page();
        assert_eq!(state.selected, Selected::Single(5));
        state.move_selected_down_full_page();
        assert_eq!(state.selected, Selected::Single(10));
        state.move_selected_down_full_page();
        assert_eq!(state.selected, Selected::Single(11));
        state.move_selected_down_full_page();
        assert_eq!(state.selected, Selected::Single(0));
        state.move_selected_up_full_page();
        assert_eq!(state.selected, Selected::Single(11));
        state.move_selected_up_full_page();
        assert_eq!(state.selected, Selected::Single(6));
        state.move_selected_up_full_page();
        assert_eq!(state.selected, Selected::Single(1));
        state.move_selected_up_full_page();
        assert_eq!(state.selected, Selected::Single(0));
        state.move_selected_up_full_page();
        assert_eq!(state.selected, Selected::Single(11));
        state.move_selected_up_full_page();
        assert_eq!(state.selected, Selected::Single(6));
        state.move_selected_up();
        assert_eq!(state.selected, Selected::Single(5));
        state.move_selected_up();
        assert_eq!(state.selected, Selected::Single(4));
        state.move_selected_up_full_page();
        assert_eq!(state.selected, Selected::Single(0));
    }

    #[test]
    fn test_selected_fields_movement() {
        let mut results = build_test_results(10);
        let mut state = build_test_search_state_with_results(results.clone());

        assert_eq!(state.selected, Selected::Single(0));
        assert_eq!(state.selected_fields(), &mut results[0..=0]);

        state.toggle_multiselect_mode();
        assert_eq!(
            state.selected,
            Selected::Multi(MultiSelected {
                anchor: 0,
                primary: 0,
            })
        );
        assert_eq!(state.selected_fields(), &mut results[0..=0]);

        state.move_selected_down();
        state.move_selected_down();
        assert_eq!(
            state.selected,
            Selected::Multi(MultiSelected {
                anchor: 0,
                primary: 2,
            })
        );
        assert_eq!(state.selected_fields(), &mut results[0..=2]);

        state.toggle_multiselect_mode();
        assert_eq!(state.selected, Selected::Single(2));
        assert_eq!(state.selected_fields(), &mut results[2..=2]);

        state.toggle_multiselect_mode();
        assert_eq!(
            state.selected,
            Selected::Multi(MultiSelected {
                anchor: 2,
                primary: 2,
            })
        );
        assert_eq!(state.selected_fields(), &mut results[2..=2]);
    }

    #[test]
    fn test_selected_fields_toggling() {
        let mut state = build_test_search_state(6);

        assert_eq!(state.selected, Selected::Single(0));
        state.move_selected_down();
        state.move_selected_down();
        state.move_selected_down();
        state.move_selected_down();
        assert_eq!(state.selected, Selected::Single(4));
        state.toggle_multiselect_mode();
        assert_eq!(
            state.selected,
            Selected::Multi(MultiSelected {
                anchor: 4,
                primary: 4,
            })
        );
        assert_eq!(state.selected_fields(), &state.results[4..=4]);
        state.move_selected_up();
        state.move_selected_up();
        assert_eq!(
            state.selected,
            Selected::Multi(MultiSelected {
                anchor: 4,
                primary: 2,
            })
        );
        assert_eq!(state.selected_fields(), &state.results[2..=4]);
        // All start unselected (opt-in model)
        assert_eq!(
            state
                .results
                .iter()
                .map(|res| res.search_result.included)
                .collect::<Vec<_>>(),
            vec![false, false, false, false, false, false]
        );
        // Toggle selected (indices 2-4): all unselected → selected, then auto-move
        state.toggle_selected_inclusion();
        assert_eq!(
            state
                .results
                .iter()
                .map(|res| res.search_result.included)
                .collect::<Vec<_>>(),
            vec![false, false, true, true, true, false]
        );
        // After toggle, moves to next: primary goes from 2 to 3 (within multiselect)
        assert_eq!(
            state.selected,
            Selected::Multi(MultiSelected {
                anchor: 4,
                primary: 3,
            })
        );
        // Selected range is now min(3,4)..=max(3,4) = 3..=4
        assert_eq!(state.selected_fields(), &state.results[3..=4]);
        state.toggle_multiselect_mode();
        assert_eq!(state.selected, Selected::Single(3));
        assert_eq!(state.selected_fields(), &state.results[3..=3]);
        state.move_selected_up();
        state.move_selected_up();
        assert_eq!(state.selected, Selected::Single(1));
        assert_eq!(state.selected_fields(), &state.results[1..=1]);
        state.toggle_selected_inclusion();
        // Toggle at index 1 (unselected → selected), auto-moves to 2
        assert_eq!(
            state
                .results
                .iter()
                .map(|res| res.search_result.included)
                .collect::<Vec<_>>(),
            vec![false, true, true, true, true, false]
        );
    }

    #[test]
    fn test_flip_multi_select_direction() {
        let mut state = build_test_search_state(10);
        assert_eq!(state.selected, Selected::Single(0));
        state.flip_multiselect_direction();
        assert_eq!(state.selected, Selected::Single(0));
        state.move_selected_down();
        assert_eq!(state.selected, Selected::Single(1));
        state.toggle_multiselect_mode();
        state.move_selected_down();
        state.move_selected_down();
        assert_eq!(
            state.selected,
            Selected::Multi(MultiSelected {
                anchor: 1,
                primary: 3,
            })
        );
        state.flip_multiselect_direction();
        assert_eq!(
            state.selected,
            Selected::Multi(MultiSelected {
                anchor: 3,
                primary: 1,
            })
        );
        state.move_selected_up();
        assert_eq!(
            state.selected,
            Selected::Multi(MultiSelected {
                anchor: 3,
                primary: 0,
            })
        );
        state.flip_multiselect_direction();
        assert_eq!(
            state.selected,
            Selected::Multi(MultiSelected {
                anchor: 0,
                primary: 3,
            })
        );
        state.move_selected_bottom();
        assert_eq!(
            state.selected,
            Selected::Multi(MultiSelected {
                anchor: 0,
                primary: 9,
            })
        );
        state.move_selected_down();
        assert_eq!(state.selected, Selected::Single(0));
    }

    #[test]
    fn test_key_handling_quit_takes_precedent() {
        let mut app = App::new(
            InputSource::Directory(std::env::current_dir().unwrap()),
            &SearchFieldValues::default(),
            AppRunConfig::default(),
            Config::default(),
        )
        .unwrap();
        app.set_popup(Popup::Text {
            title: "Error title".to_owned(),
            body: "some text in the body".to_owned(),
        });
        let res = app.handle_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(matches!(res, EventHandlingResult::Exit(None)));
    }

    #[test]
    fn test_key_handling_unmapped_key_closes_popup() {
        let mut app = App::new(
            InputSource::Directory(std::env::current_dir().unwrap()),
            &SearchFieldValues::default(),
            AppRunConfig::default(),
            Config::default(),
        )
        .unwrap();
        app.set_popup(Popup::Text {
            title: "Error title".to_owned(),
            body: "some text in the body".to_owned(),
        });
        let res = app.handle_key_event(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        assert!(matches!(res, EventHandlingResult::Rerender));
        assert!(app.popup().is_none());
    }
}
