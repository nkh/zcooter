use std::collections::HashMap;

use crate::{
    app::{FocussedSection, Screen},
    config::KeysConfig,
    keyboard::{KeyCode, KeyEvent, KeyModifiers},
};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum Command {
    General(CommandGeneral),
    SearchFields(CommandSearchFields),
    PerformingReplacement(CommandPerformingReplacement),
    Results(CommandResults),
}

impl From<CommandGeneral> for Command {
    fn from(c: CommandGeneral) -> Self {
        Command::General(c)
    }
}

impl From<CommandSearchFields> for Command {
    fn from(c: CommandSearchFields) -> Self {
        Command::SearchFields(c)
    }
}

impl From<CommandSearchFocusFields> for Command {
    fn from(c: CommandSearchFocusFields) -> Self {
        Command::SearchFields(CommandSearchFields::SearchFocusFields(c))
    }
}

impl From<CommandSearchFocusResults> for Command {
    fn from(c: CommandSearchFocusResults) -> Self {
        Command::SearchFields(CommandSearchFields::SearchFocusResults(c))
    }
}

impl From<CommandPerformingReplacement> for Command {
    fn from(c: CommandPerformingReplacement) -> Self {
        Command::PerformingReplacement(c)
    }
}

impl From<CommandResults> for Command {
    fn from(c: CommandResults) -> Self {
        Command::Results(c)
    }
}

// Events applicable to all screens
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum CommandGeneral {
    Quit,
    Reset,
    ShowHelpMenu,
}

// Events applicable only to `SearchFields` screen
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum CommandSearchFields {
    TogglePreviewWrapping,
    ToggleHiddenFiles,
    ToggleMultiline,
    ToggleInterpretEscapeSequences,
    ResizeColumnShrink,
    ResizeColumnGrow,
    SearchFocusFields(CommandSearchFocusFields),
    SearchFocusResults(CommandSearchFocusResults),
}

// Events applicable only to `Screen::SearchFields` screen when focussed section is `FocussedSection::SearchFields`
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum CommandSearchFocusFields {
    UnlockPrepopulatedFields,
    TriggerSearch,
    FocusNextField,
    FocusPreviousField,
    OpenFileFinder,
    FocusSearchField,
    FocusReplaceField,
    FocusIncludeField,
    FocusExcludeField,
    FocusFixedField,
    FieldsToResults,
    EnterChars(KeyCode, KeyModifiers),
}

// Events applicable only to `Screen::SearchFields` screen when focussed section is `FocussedSection::SearchFields`
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum CommandSearchFocusResults {
    TriggerReplacement,
    BackToFields,
    OpenInEditor,

    MoveDown,
    MoveUp,
    MoveNextFile,
    MovePrevFile,
    MoveDownHalfPage,
    MoveDownFullPage,
    MoveUpHalfPage,
    MoveUpFullPage,
    MoveTop,
    MoveBottom,

    ToggleSelectedInclusion,
    ToggleAllSelected,
    ToggleMultiselectMode,

    FlipMultiselectDirection,
    ToggleCurrentFileSelected,
    EnterInsertMode,
    BackspaceToSearch,
}

// Events applicable only to `PerformingReplacement` screen
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum CommandPerformingReplacement {}

// Events applicable only to `Results` screen
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum CommandResults {
    ScrollErrorsDown,
    ScrollErrorsUp,
    Quit,
}

#[derive(Debug)]
pub(crate) struct KeyMap {
    general: HashMap<KeyEvent, CommandGeneral>,
    search_fields: HashMap<KeyEvent, CommandSearchFocusFields>,
    search_results: HashMap<KeyEvent, CommandSearchFocusResults>,
    search_common: HashMap<KeyEvent, CommandSearchFields>,
    #[allow(clippy::zero_sized_map_values)]
    performing_replacement: HashMap<KeyEvent, CommandPerformingReplacement>,
    results: HashMap<KeyEvent, CommandResults>,
    /// Map for prefix-key sequences: (prefix_key, second_key) -> Command
    prefix_map: HashMap<(KeyEvent, KeyEvent), Command>,
}

/// Represents a key binding conflict detected during `KeyMap` construction
#[derive(Debug)]
pub(crate) struct KeyConflict {
    pub(crate) key: KeyEvent,
    pub(crate) context: String,
    pub(crate) commands: Vec<String>,
}

impl KeyMap {
    /// Build a `KeyMap` from `KeysConfig`, detecting any conflicts
    #[allow(clippy::too_many_lines)]
    pub(crate) fn from_config(keys_config: &KeysConfig) -> Result<Self, Vec<KeyConflict>> {
        macro_rules! build_map {
            ($($path:tt).+, $conflicts:expr, [
                $(($field:ident, $command:expr)),* $(,)?
            ]) => {{
                let context = stringify!($($path).+);
                let config = &keys_config.$($path).+;
                let mut map = HashMap::new();
                let mut prefix_entries: Vec<(KeyEvent, KeyEvent, Command)> = Vec::new();
                $(
                    for key in &config.$field {
                        if key.prefix.is_some() {
                            // Prefix keys go into the prefix map
                            let prefix_key = KeyEvent::new(key.prefix.unwrap(), KeyModifiers::NONE);
                            let second_key = KeyEvent::new(key.code, key.modifiers);
                            prefix_entries.push((prefix_key, second_key, Command::from($command)));
                        } else {
                            Self::insert_and_detect(&mut map, *key, $command, context, $conflicts);
                        }
                    }
                )*
                (map, prefix_entries)
            }};
        }

        let mut conflicts = Vec::new();

        let general_map = build_map!(
            general,
            &mut conflicts,
            [
                (quit, CommandGeneral::Quit),
                (reset, CommandGeneral::Reset),
                (show_help_menu, CommandGeneral::ShowHelpMenu),
            ]
        );
        let general = general_map.0;

        let search_common_map = build_map!(
            search,
            &mut conflicts,
            [
                (
                    toggle_preview_wrapping,
                    CommandSearchFields::TogglePreviewWrapping
                ),
                (toggle_hidden_files, CommandSearchFields::ToggleHiddenFiles),
                (toggle_multiline, CommandSearchFields::ToggleMultiline),
                (
                    toggle_interpret_escape_sequences,
                    CommandSearchFields::ToggleInterpretEscapeSequences
                ),
                (resize_column_shrink, CommandSearchFields::ResizeColumnShrink),
                (resize_column_grow, CommandSearchFields::ResizeColumnGrow),
            ]
        );
        let search_common = search_common_map.0;

        let search_fields_map = build_map!(
            search.fields,
            &mut conflicts,
            [
                (
                    unlock_prepopulated_fields,
                    CommandSearchFocusFields::UnlockPrepopulatedFields
                ),
                (trigger_search, CommandSearchFocusFields::TriggerSearch),
                (focus_next_field, CommandSearchFocusFields::FocusNextField),
                (
                    focus_previous_field,
                    CommandSearchFocusFields::FocusPreviousField
                ),
                (open_file_finder, CommandSearchFocusFields::OpenFileFinder),
                (focus_search_field, CommandSearchFocusFields::FocusSearchField),
                (focus_replace_field, CommandSearchFocusFields::FocusReplaceField),
                (focus_include_field, CommandSearchFocusFields::FocusIncludeField),
                (focus_exclude_field, CommandSearchFocusFields::FocusExcludeField),
                (focus_fixed_field, CommandSearchFocusFields::FocusFixedField),
                (fields_to_results, CommandSearchFocusFields::FieldsToResults),
            ]
        );
        let search_fields = search_fields_map.0;

        let search_results_map = build_map!(
            search.results,
            &mut conflicts,
            [
                (
                    trigger_replacement,
                    CommandSearchFocusResults::TriggerReplacement
                ),
                (back_to_fields, CommandSearchFocusResults::BackToFields),
                (open_in_editor, CommandSearchFocusResults::OpenInEditor),
                (move_down, CommandSearchFocusResults::MoveDown),
                (move_up, CommandSearchFocusResults::MoveUp),
                (move_next_file, CommandSearchFocusResults::MoveNextFile),
                (move_prev_file, CommandSearchFocusResults::MovePrevFile),
                (
                    move_down_half_page,
                    CommandSearchFocusResults::MoveDownHalfPage
                ),
                (
                    move_down_full_page,
                    CommandSearchFocusResults::MoveDownFullPage
                ),
                (move_up_half_page, CommandSearchFocusResults::MoveUpHalfPage),
                (move_up_full_page, CommandSearchFocusResults::MoveUpFullPage),
                (move_top, CommandSearchFocusResults::MoveTop),
                (move_bottom, CommandSearchFocusResults::MoveBottom),
                (
                    toggle_selected_inclusion,
                    CommandSearchFocusResults::ToggleSelectedInclusion
                ),
                (
                    toggle_all_selected,
                    CommandSearchFocusResults::ToggleAllSelected
                ),
                (
                    toggle_multiselect_mode,
                    CommandSearchFocusResults::ToggleMultiselectMode
                ),
                (
                    flip_multiselect_direction,
                    CommandSearchFocusResults::FlipMultiselectDirection
                ),
                (
                    toggle_current_file_selected,
                    CommandSearchFocusResults::ToggleCurrentFileSelected
                ),
                (enter_insert_mode, CommandSearchFocusResults::EnterInsertMode),
                (backspace_to_search, CommandSearchFocusResults::BackspaceToSearch),
            ]
        );
        let search_results = search_results_map.0;

        let results_map = build_map!(
            results,
            &mut conflicts,
            [
                (scroll_errors_down, CommandResults::ScrollErrorsDown),
                (scroll_errors_up, CommandResults::ScrollErrorsUp),
                (quit, CommandResults::Quit),
            ]
        );
        let results = results_map.0;

        #[allow(clippy::zero_sized_map_values)]
        let performing_replacement = HashMap::new();

        // Collect all prefix entries into the prefix map
        let mut prefix_map: HashMap<(KeyEvent, KeyEvent), Command> = HashMap::new();
        for (prefix, second, cmd) in general_map.1 {
            Self::insert_prefix_and_detect(
                &mut prefix_map,
                prefix,
                second,
                cmd,
                &mut conflicts,
            );
        }
        for (prefix, second, cmd) in search_common_map.1 {
            Self::insert_prefix_and_detect(
                &mut prefix_map,
                prefix,
                second,
                cmd,
                &mut conflicts,
            );
        }
        for (prefix, second, cmd) in search_fields_map.1 {
            Self::insert_prefix_and_detect(
                &mut prefix_map,
                prefix,
                second,
                cmd,
                &mut conflicts,
            );
        }
        for (prefix, second, cmd) in search_results_map.1 {
            Self::insert_prefix_and_detect(
                &mut prefix_map,
                prefix,
                second,
                cmd,
                &mut conflicts,
            );
        }
        for (prefix, second, cmd) in results_map.1 {
            Self::insert_prefix_and_detect(
                &mut prefix_map,
                prefix,
                second,
                cmd,
                &mut conflicts,
            );
        }

        if conflicts.is_empty() {
            Ok(Self {
                general,
                search_fields,
                search_results,
                search_common,
                performing_replacement,
                results,
                prefix_map,
            })
        } else {
            Err(conflicts)
        }
    }

    /// Insert a prefix-key binding and detect conflicts
    fn insert_prefix_and_detect(
        map: &mut HashMap<(KeyEvent, KeyEvent), Command>,
        prefix: KeyEvent,
        second: KeyEvent,
        command: Command,
        conflicts: &mut Vec<KeyConflict>,
    ) {
        let key = (prefix, second);
        if let Some(existing) = map.insert(key, command) {
            let format_command = |cmd: &Command| -> String {
                format!("{cmd:?}").to_lowercase()
            };
            // Create a combined KeyEvent display for the conflict
            let combined = KeyEvent::with_prefix(prefix.code, second.code);
            conflicts.push(KeyConflict {
                key: combined,
                context: "prefix keys".to_string(),
                commands: vec![
                    format_command(&existing),
                    format_command(map.get(&key).unwrap()),
                ],
            });
        }
    }

    /// Insert a key binding and detect conflicts
    fn insert_and_detect<T: std::fmt::Debug>(
        map: &mut HashMap<KeyEvent, T>,
        key: KeyEvent,
        command: T,
        context: &str,
        conflicts: &mut Vec<KeyConflict>,
    ) {
        if let Some(existing) = map.insert(key, command) {
            // Convert snake_case Debug names to human-readable format
            let format_command = |cmd: &T| -> String {
                let debug_str = format!("{cmd:?}");
                // Convert PascalCase to snake_case
                debug_str
                    .chars()
                    .enumerate()
                    .flat_map(|(i, c)| {
                        if i > 0 && c.is_uppercase() {
                            vec!['_', c]
                        } else {
                            vec![c]
                        }
                    })
                    .collect::<String>()
                    .to_lowercase()
            };

            conflicts.push(KeyConflict {
                key,
                context: context.to_string(),
                commands: vec![
                    format_command(&existing),
                    format_command(map.get(&key).unwrap()),
                ],
            });
        }
    }

    /// Look up a prefix-key sequence command
    pub(crate) fn lookup_prefix(
        &self,
        prefix: KeyEvent,
        key: KeyEvent,
    ) -> Option<Command> {
        self.prefix_map.get(&(prefix, key)).copied()
    }

    /// Check if any command starts with the given prefix key
    #[allow(dead_code)]
    pub(crate) fn has_prefix_for(&self, prefix: KeyEvent) -> bool {
        self.prefix_map.keys().any(|(p, _)| *p == prefix)
    }

    /// Look up a command for the given key event and screen context
    pub(crate) fn lookup(&self, screen: &Screen, key_event: KeyEvent) -> Option<Command> {
        // Check screen-specific commands
        if let Some(cmd) = match screen {
            Screen::SearchFields(state) => {
                // Check common SearchFields commands first
                if let Some(cmd) = self.search_common.get(&key_event) {
                    return Some(Command::SearchFields(*cmd));
                }
                // Then check focus-specific commands
                match state.focussed_section {
                    FocussedSection::SearchFields => {
                        self.search_fields.get(&key_event).map(|cmd| {
                            Command::SearchFields(CommandSearchFields::SearchFocusFields(*cmd))
                        })
                    }
                    FocussedSection::SearchResults => {
                        self.search_results.get(&key_event).map(|cmd| {
                            Command::SearchFields(CommandSearchFields::SearchFocusResults(*cmd))
                        })
                    }
                }
            }
            Screen::PerformingReplacement(_) => self
                .performing_replacement
                .get(&key_event)
                .map(|cmd| Command::PerformingReplacement(*cmd)),
            Screen::Results(_) => self
                .results
                .get(&key_event)
                .map(|cmd| Command::Results(*cmd)),
        } {
            return Some(cmd);
        }

        // Check general commands - must happen after looking up screen-specific commands
        if let Some(cmd) = self.general.get(&key_event) {
            return Some(Command::General(*cmd));
        }
        None
    }
}

pub(crate) fn display_conflict_errors(conflicts: Vec<KeyConflict>) -> anyhow::Error {
    use std::fmt::Write;

    let mut error_msg = String::from("Key binding conflict detected!\n\n");
    for conflict in conflicts {
        writeln!(
            &mut error_msg,
            "The key '{}' is bound to multiple commands in [keys.{}]:",
            conflict.key, conflict.context
        )
        .unwrap();
        for (i, cmd) in conflict.commands.iter().enumerate() {
            writeln!(&mut error_msg, "  {}. {}", i + 1, cmd).unwrap();
        }
        error_msg.push_str("\nPlease update your config to use unique key bindings.");
    }
    anyhow::anyhow!(error_msg)
}
