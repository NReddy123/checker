use crate::diff::{
    CompareOptions, DiffKind, DiffLine, DiffProjection, DiffSide, change_hunks,
    compare_text_with_options, merge_three_way,
};
use crate::diff_editor::DiffEditorCore;
use crate::folder::{
    FolderEntry, FolderStatus, compare_folders, synchronize_folders, write_folder_file,
};
use crate::session::{
    CompareMode, CompareSession, DiffFilter, FolderFilter, MainPanel, MergeContext,
};
use crate::theme::{AppTheme, ThemePalette};
use crate::ui::{
    comparison_mode_card, diff_cell, folder_side_cell, folder_side_header, labeled_input,
    merge_preview, path_input, source_label, text_source,
};
use gpui_kit::component::button::*;
use gpui_kit::component::{Sizable as _, Size as ComponentSize};
use gpui_kit::component::{
    input::{EditorState, Input, InputEvent, InputState},
    menu::{AppMenuBar, DropdownMenu},
    scroll::ScrollableElement as _,
    *,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

actions!(
    checker,
    [
        RunComparison,
        SaveLeft,
        SaveRight,
        DiscardSources,
        UndoDiffEdit,
        RedoDiffEdit,
        ToggleMergeContext,
        ToggleWhitespace,
        ToggleCase,
        ToggleComments,
        SyncLeftToRight,
        SyncRightToLeft,
        CopyLeftToRight,
        CopyRightToLeft,
        ToggleAdvanced,
        DecreaseFontSize,
        IncreaseFontSize,
        ResetFontSize,
        SetDarkTheme,
        SetLightTheme,
        ShowAbout,
        ShowHome,
        NewSession
    ]
);

struct Checker {
    menu_bar: Entity<AppMenuBar>,
    focus_handle: FocusHandle,
    left_path: Entity<InputState>,
    right_path: Entity<InputState>,
    base_path: Entity<InputState>,
    search: Entity<InputState>,
    include_patterns: Entity<InputState>,
    ignore_patterns: Entity<InputState>,
    left_text_editor: Entity<EditorState>,
    right_text_editor: Entity<EditorState>,
    sessions: Vec<CompareSession>,
    active_session: usize,
    show_home: bool,
    mode: Option<CompareMode>,
    rows: Vec<DiffLine>,
    diff_projection: DiffProjection,
    diff_editor: DiffEditorCore,
    left_diff_inputs: Vec<Entity<InputState>>,
    right_diff_inputs: Vec<Entity<InputState>>,
    active_diff_input: Option<(DiffSide, usize)>,
    last_diff_input: Option<(DiffSide, usize)>,
    pending_diff_focus: Option<(DiffSide, usize, usize)>,
    folder_entries: Vec<FolderEntry>,
    left_content: String,
    right_content: String,
    left_original: Option<String>,
    right_original: Option<String>,
    base_content: String,
    merge_context: Option<MergeContext>,
    show_merge_context: bool,
    folder_mode: bool,
    selected_folder_file: Option<String>,
    compared: bool,
    ignore_whitespace: bool,
    ignore_case: bool,
    ignore_comments: bool,
    bookmarks: BTreeSet<usize>,
    bookmark_cursor: usize,
    selected_bookmark: Option<usize>,
    active_panel: MainPanel,
    show_advanced: bool,
    folder_filter: FolderFilter,
    diff_filter: DiffFilter,
    editor_font_size: f32,
    theme: AppTheme,
    status: String,
}

impl Checker {
    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        theme: AppTheme,
        startup_status: Option<String>,
    ) -> Self {
        let left_text_editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("plaintext")
                .line_number(true)
        });
        let right_text_editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("plaintext")
                .line_number(true)
        });
        cx.subscribe(&left_text_editor, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.refresh_text_input(cx);
            }
        })
        .detach();
        cx.subscribe(&right_text_editor, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.refresh_text_input(cx);
            }
        })
        .detach();
        let checker = Self {
            menu_bar: AppMenuBar::new(cx),
            focus_handle: cx.focus_handle(),
            left_path: cx
                .new(|cx| InputState::new(window, cx).placeholder("First file or folder path")),
            right_path: cx
                .new(|cx| InputState::new(window, cx).placeholder("Second file or folder path")),
            base_path: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Optional base file for 3-way merge")
            }),
            search: cx.new(|cx| InputState::new(window, cx).placeholder("Find in diff")),
            include_patterns: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Include globs, comma separated")
            }),
            ignore_patterns: cx
                .new(|cx| InputState::new(window, cx).placeholder("Ignore globs, comma separated")),
            left_text_editor,
            right_text_editor,
            sessions: vec![CompareSession::blank(1)],
            active_session: 0,
            show_home: true,
            mode: None,
            rows: Vec::new(),
            diff_projection: DiffProjection::default(),
            diff_editor: DiffEditorCore::new(
                String::new(),
                String::new(),
                None,
                None,
                CompareOptions::default(),
            ),
            left_diff_inputs: Vec::new(),
            right_diff_inputs: Vec::new(),
            active_diff_input: None,
            last_diff_input: None,
            pending_diff_focus: None,
            folder_entries: Vec::new(),
            left_content: String::new(),
            right_content: String::new(),
            left_original: None,
            right_original: None,
            base_content: String::new(),
            merge_context: None,
            show_merge_context: false,
            folder_mode: false,
            selected_folder_file: None,
            compared: false,
            ignore_whitespace: false,
            ignore_case: false,
            ignore_comments: false,
            bookmarks: BTreeSet::new(),
            bookmark_cursor: 0,
            selected_bookmark: None,
            active_panel: MainPanel::Diff,
            show_advanced: false,
            folder_filter: FolderFilter::All,
            diff_filter: DiffFilter::All,
            editor_font_size: 12.,
            theme,
            status: startup_status
                .unwrap_or_else(|| "Choose a comparison type to get started.".into()),
        };
        checker
    }

    fn set_theme(&mut self, theme: AppTheme, cx: &mut Context<Self>) {
        let mode = match theme {
            AppTheme::Dark => gpui_kit::component::ThemeMode::Dark,
            AppTheme::Light => gpui_kit::component::ThemeMode::Light,
        };
        gpui_kit::component::Theme::change(mode, None, cx);
        self.theme = theme;
        self.status = match theme.save_preference() {
            Ok(()) => match theme {
                AppTheme::Dark => "Dark theme enabled.".into(),
                AppTheme::Light => "Light theme enabled.".into(),
            },
            Err(error) => format!("Theme changed but could not save preference: {error}"),
        };
        cx.notify();
    }

    fn snapshot_active_session(&self, cx: &Context<Self>) -> CompareSession {
        let mut session = CompareSession::blank(self.active_session + 1);
        session.name = self.sessions[self.active_session].name.clone();
        session.show_home = self.show_home;
        session.mode = self.mode;
        session.left_path = self.left_path.read(cx).value().to_string();
        session.right_path = self.right_path.read(cx).value().to_string();
        session.base_path = self.base_path.read(cx).value().to_string();
        session.search = self.search.read(cx).value().to_string();
        session.include_patterns = self.include_patterns.read(cx).value().to_string();
        session.ignore_patterns = self.ignore_patterns.read(cx).value().to_string();
        session.left_text = self.left_content.clone();
        session.right_text = self.right_content.clone();
        session.rows = self.rows.clone();
        session.folder_entries = self.folder_entries.clone();
        session.left_content = self.left_content.clone();
        session.right_content = self.right_content.clone();
        session.left_original = self.left_original.clone();
        session.right_original = self.right_original.clone();
        session.base_content = self.base_content.clone();
        session.merge_context = self.merge_context.clone();
        session.show_merge_context = self.show_merge_context;
        session.folder_mode = self.folder_mode;
        session.selected_folder_file = self.selected_folder_file.clone();
        session.compared = self.compared;
        session.ignore_whitespace = self.ignore_whitespace;
        session.ignore_case = self.ignore_case;
        session.ignore_comments = self.ignore_comments;
        session.bookmarks = self.bookmarks.clone();
        session.bookmark_cursor = self.bookmark_cursor;
        session.selected_bookmark = self.selected_bookmark;
        session.active_panel = self.active_panel;
        session.show_advanced = self.show_advanced;
        session.folder_filter = self.folder_filter;
        session.diff_filter = self.diff_filter;
        session.status = self.status.clone();
        session
    }

    fn set_rows(&mut self, rows: Vec<DiffLine>) {
        self.rows = rows;
        self.diff_projection = DiffProjection::from_rows(&self.rows);
    }

    fn compare_options(&self) -> CompareOptions {
        CompareOptions {
            ignore_whitespace: self.ignore_whitespace,
            ignore_case: self.ignore_case,
            ignore_comments: self.ignore_comments,
        }
    }

    fn reset_diff_editor(&mut self) {
        self.diff_editor = DiffEditorCore::new(
            self.left_content.clone(),
            self.right_content.clone(),
            self.left_original.clone(),
            self.right_original.clone(),
            self.compare_options(),
        );
        self.rows = self.diff_editor.rows().to_vec();
        self.diff_projection = self.diff_editor.projection().clone();
    }

    fn ensure_diff_editor(&mut self) {
        if !self.diff_editor.matches_inputs(
            &self.left_content,
            &self.right_content,
            &self.left_original,
            &self.right_original,
            self.compare_options(),
        ) {
            self.reset_diff_editor();
        } else {
            self.rows = self.diff_editor.rows().to_vec();
            self.diff_projection = self.diff_editor.projection().clone();
        }
    }

    fn sync_from_diff_editor(&mut self, stable_side: DiffSide, old_rows: &[DiffLine]) {
        self.left_content = self.diff_editor.text(DiffSide::Left).to_owned();
        self.right_content = self.diff_editor.text(DiffSide::Right).to_owned();
        let new_rows = self.diff_editor.rows().to_vec();
        let new_projection = self.diff_editor.projection().clone();
        let remap = |old_index: usize| {
            let old = old_rows.get(old_index)?;
            let (line_number, line_text) = match stable_side {
                DiffSide::Left => (old.left_number, old.left_text.as_str()),
                DiffSide::Right => (old.right_number, old.right_text.as_str()),
            };
            let line_number = line_number?;
            let source_line = line_number.checked_sub(1)?;
            let row_index = new_projection.display_row_for_source_line(stable_side, source_line)?;
            let row = new_rows.get(row_index)?;
            let (number, text) = match stable_side {
                DiffSide::Left => (row.left_number, row.left_text.as_str()),
                DiffSide::Right => (row.right_number, row.right_text.as_str()),
            };
            (row.kind != DiffKind::Equal && number == Some(line_number) && text == line_text)
                .then_some(row_index)
        };
        self.bookmarks = self
            .bookmarks
            .iter()
            .filter_map(|index| remap(*index))
            .collect();
        self.selected_bookmark = self.selected_bookmark.and_then(remap);
        self.bookmark_cursor = self
            .bookmark_cursor
            .min(self.bookmarks.len().saturating_sub(1));
        self.rows = new_rows;
        self.diff_projection = new_projection;
        self.compared = true;
        let changed = self
            .rows
            .iter()
            .filter(|line| line.kind != DiffKind::Equal)
            .count();
        self.status = if changed == 0 {
            "The source buffers are identical under the selected rules.".into()
        } else {
            format!("{changed} changed line(s) · unsaved edits remain in memory")
        };
    }

    fn new_diff_input(
        side: DiffSide,
        row: usize,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        let input = cx.new(|cx| InputState::new(window, cx).default_value(value));
        cx.subscribe(&input, move |this, input, event: &InputEvent, cx| {
            this.handle_diff_input_event(side, row, input, event, cx);
        })
        .detach();
        input
    }

    fn diff_editor_row_count(&self) -> usize {
        if !self.compared {
            return 0;
        }
        self.diff_projection.row_count().max(1)
            + usize::from(self.left_content.ends_with('\n') || self.right_content.ends_with('\n'))
    }

    fn diff_cell_text(&self, side: DiffSide, row: usize) -> String {
        match (side, self.rows.get(row)) {
            (DiffSide::Left, Some(line)) => line.left_text.clone(),
            (DiffSide::Right, Some(line)) => line.right_text.clone(),
            (_, None) => String::new(),
        }
    }

    fn ensure_diff_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let row_count = self.diff_editor_row_count();
        while self.left_diff_inputs.len() < row_count {
            let row = self.left_diff_inputs.len();
            let value = self.diff_cell_text(DiffSide::Left, row);
            self.left_diff_inputs.push(Self::new_diff_input(
                DiffSide::Left,
                row,
                value,
                window,
                cx,
            ));
        }
        while self.right_diff_inputs.len() < row_count {
            let row = self.right_diff_inputs.len();
            let value = self.diff_cell_text(DiffSide::Right, row);
            self.right_diff_inputs.push(Self::new_diff_input(
                DiffSide::Right,
                row,
                value,
                window,
                cx,
            ));
        }
        self.left_diff_inputs.truncate(row_count);
        self.right_diff_inputs.truncate(row_count);

        for row in 0..row_count {
            for (side, input, value) in [
                (
                    DiffSide::Left,
                    self.left_diff_inputs[row].clone(),
                    self.diff_cell_text(DiffSide::Left, row),
                ),
                (
                    DiffSide::Right,
                    self.right_diff_inputs[row].clone(),
                    self.diff_cell_text(DiffSide::Right, row),
                ),
            ] {
                if input.read(cx).value().as_ref() != value {
                    let previous_column = input.read(cx).selected_range().end;
                    input.update(cx, |state, cx| state.set_value(value, window, cx));
                    if self.active_diff_input == Some((side, row))
                        && self.pending_diff_focus.is_none()
                    {
                        self.pending_diff_focus = Some((side, row, previous_column));
                    }
                }
            }
        }

        if let Some((side, row, column)) = self.pending_diff_focus.take()
            && row < row_count
        {
            let input = match side {
                DiffSide::Left => &self.left_diff_inputs[row],
                DiffSide::Right => &self.right_diff_inputs[row],
            };
            input.update(cx, |state, cx| {
                let offset = column.min(state.value().len());
                state.set_selected_range(offset..offset, cx);
                state.focus(window, cx);
            });
            self.active_diff_input = Some((side, row));
            self.last_diff_input = Some((side, row));
        }
    }

    fn handle_diff_input_event(
        &mut self,
        side: DiffSide,
        row: usize,
        input: Entity<InputState>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Focus => {
                self.active_diff_input = Some((side, row));
                self.last_diff_input = Some((side, row));
            }
            InputEvent::Blur => {
                if self.active_diff_input == Some((side, row)) {
                    self.active_diff_input = None;
                }
            }
            InputEvent::Change => {
                self.ensure_diff_editor();
                let old_rows = self.rows.clone();
                let text = input.read(cx).value().to_string();
                if let Err(error) = self.diff_editor.replace_display_line(side, row, &text) {
                    self.status = format!("Could not edit this diff line: {error:?}");
                } else {
                    let selection = input.read(cx).selected_range();
                    let (current_row, _) = self.diff_editor.cursor_display_position(side);
                    if current_row != row {
                        self.pending_diff_focus = Some((side, current_row, selection.end));
                    }
                    if let Err(error) =
                        self.diff_editor
                            .place_selection(side, current_row, selection)
                    {
                        self.status = format!("Could not map the diff caret: {error:?}");
                    }
                    self.sync_from_diff_editor(opposite_side(side), &old_rows);
                }
                cx.notify();
            }
            InputEvent::PressEnter { .. } => {
                self.ensure_diff_editor();
                let old_rows = self.rows.clone();
                let selection = input.read(cx).selected_range();
                match self.diff_editor.split_display_line(side, row, selection) {
                    Ok(()) => {
                        self.sync_from_diff_editor(opposite_side(side), &old_rows);
                        let (next_row, column) = self.diff_editor.cursor_display_position(side);
                        self.pending_diff_focus = Some((side, next_row, column));
                    }
                    Err(error) => {
                        self.status = format!("Could not split this diff line: {error:?}");
                    }
                }
                cx.notify();
            }
        }
    }

    fn handle_diff_navigation(
        &mut self,
        side: DiffSide,
        row: usize,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.modifiers.modified() {
            return;
        }
        let input = match side {
            DiffSide::Left => self.left_diff_inputs.get(row),
            DiffSide::Right => self.right_diff_inputs.get(row),
        };
        let Some(input) = input.cloned() else {
            return;
        };
        let state = input.read(cx);
        let selection = state.selected_range();
        let value = state.value().to_string();
        if !selection.is_empty() {
            return;
        }
        let cursor = selection.end;
        let key = event.keystroke.key.as_str();
        let target = match key {
            "up" => self
                .next_visible_diff_row(row, -1, cx)
                .map(|row| (row, cursor)),
            "down" => self
                .next_visible_diff_row(row, 1, cx)
                .map(|row| (row, cursor)),
            "left" if cursor == 0 => self.next_visible_diff_row(row, -1, cx).map(|row| {
                let text = self.diff_cell_text(side, row);
                (row, text.len())
            }),
            "right" if cursor == value.len() => {
                self.next_visible_diff_row(row, 1, cx).map(|row| (row, 0))
            }
            "backspace" if cursor == 0 => {
                self.ensure_diff_editor();
                let old_rows = self.rows.clone();
                match self.diff_editor.merge_display_line_with_previous(side, row) {
                    Ok(true) => {
                        self.sync_from_diff_editor(opposite_side(side), &old_rows);
                        let (target_row, target_column) =
                            self.diff_editor.cursor_display_position(side);
                        self.pending_diff_focus = Some((side, target_row, target_column));
                    }
                    Ok(false) => {}
                    Err(error) => {
                        self.status = format!("Could not join diff lines: {error:?}");
                    }
                }
                cx.notify();
                None
            }
            "delete" if cursor == value.len() => {
                self.ensure_diff_editor();
                let old_rows = self.rows.clone();
                match self.diff_editor.merge_display_line_with_next(side, row) {
                    Ok(true) => {
                        self.sync_from_diff_editor(opposite_side(side), &old_rows);
                        let (target_row, target_column) =
                            self.diff_editor.cursor_display_position(side);
                        self.pending_diff_focus = Some((side, target_row, target_column));
                    }
                    Ok(false) => {}
                    Err(error) => {
                        self.status = format!("Could not join diff lines: {error:?}");
                    }
                }
                cx.notify();
                None
            }
            _ => return,
        };

        if let Some((target_row, target_column)) = target {
            let target_input = match side {
                DiffSide::Left => self.left_diff_inputs.get(target_row),
                DiffSide::Right => self.right_diff_inputs.get(target_row),
            };
            if let Some(target_input) = target_input.cloned() {
                target_input.update(cx, |state, cx| {
                    let value = state.value();
                    let mut offset = target_column.min(value.len());
                    while offset > 0 && !value.is_char_boundary(offset) {
                        offset -= 1;
                    }
                    state.set_selected_range(offset..offset, cx);
                    state.focus(window, cx);
                });
                self.active_diff_input = Some((side, target_row));
                self.last_diff_input = Some((side, target_row));
            }
            cx.notify();
        }
    }

    fn next_visible_diff_row(
        &self,
        row: usize,
        direction: isize,
        cx: &Context<Self>,
    ) -> Option<usize> {
        let query = self.search.read(cx).value().to_string().to_lowercase();
        let mut candidate = row as isize + direction;
        while candidate >= 0 && candidate < self.diff_editor_row_count() as isize {
            let index = candidate as usize;
            let kind = self
                .rows
                .get(index)
                .map(|line| line.kind)
                .unwrap_or(DiffKind::Equal);
            let left = self.diff_cell_text(DiffSide::Left, index);
            let right = self.diff_cell_text(DiffSide::Right, index);
            let visible = !(self.diff_filter == DiffFilter::Differences && kind == DiffKind::Equal
                || self.diff_filter == DiffFilter::Same && kind != DiffKind::Equal)
                && self
                    .selected_bookmark
                    .is_none_or(|selected| selected == index)
                && (query.is_empty()
                    || left.to_lowercase().contains(&query)
                    || right.to_lowercase().contains(&query));
            if visible {
                return Some(index);
            }
            candidate += direction;
        }
        None
    }

    fn persist_active_session(&mut self, cx: &Context<Self>) {
        let session = self.snapshot_active_session(cx);
        self.sessions[self.active_session] = session;
    }

    fn activate_session(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index == self.active_session {
            return;
        }
        self.persist_active_session(cx);
        let session = self.sessions[index].clone();
        self.active_session = index;
        self.show_home = session.show_home;
        self.mode = session.mode;
        self.left_path.update(cx, |state, cx| {
            state.set_value(session.left_path, window, cx)
        });
        self.right_path.update(cx, |state, cx| {
            state.set_value(session.right_path, window, cx)
        });
        self.base_path.update(cx, |state, cx| {
            state.set_value(session.base_path, window, cx)
        });
        self.search
            .update(cx, |state, cx| state.set_value(session.search, window, cx));
        self.include_patterns.update(cx, |state, cx| {
            state.set_value(session.include_patterns, window, cx)
        });
        self.ignore_patterns.update(cx, |state, cx| {
            state.set_value(session.ignore_patterns, window, cx)
        });
        self.left_text_editor.update(cx, |state, cx| {
            state.set_value(session.left_text, window, cx)
        });
        self.right_text_editor.update(cx, |state, cx| {
            state.set_value(session.right_text, window, cx)
        });
        self.rows = session.rows;
        self.diff_projection = DiffProjection::from_rows(&self.rows);
        self.folder_entries = session.folder_entries;
        self.left_content = session.left_content;
        self.right_content = session.right_content;
        self.left_original = session.left_original;
        self.right_original = session.right_original;
        self.base_content = session.base_content;
        self.merge_context = session.merge_context;
        self.show_merge_context = session.show_merge_context;
        self.folder_mode = session.folder_mode;
        self.selected_folder_file = session.selected_folder_file;
        self.compared = session.compared;
        self.ignore_whitespace = session.ignore_whitespace;
        self.ignore_case = session.ignore_case;
        self.ignore_comments = session.ignore_comments;
        self.bookmarks = session.bookmarks;
        self.bookmark_cursor = session.bookmark_cursor;
        self.selected_bookmark = session.selected_bookmark;
        self.active_panel = session.active_panel;
        self.show_advanced = session.show_advanced;
        self.folder_filter = session.folder_filter;
        self.diff_filter = session.diff_filter;
        self.status = session.status;
        self.active_diff_input = None;
        self.last_diff_input = None;
        self.pending_diff_focus = None;
        cx.notify();
    }

    fn new_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.persist_active_session(cx);
        let index = self.sessions.len();
        self.sessions.push(CompareSession::blank(index + 1));
        self.activate_session(index, window, cx);
    }

    fn select_mode(&mut self, mode: CompareMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != Some(mode) && self.has_unsaved_file_changes() {
            self.status =
                "Save or discard staged source edits before switching comparison modes.".into();
            cx.notify();
            return;
        }
        if self.mode != Some(mode) {
            self.left_path
                .update(cx, |state, cx| state.set_value("", window, cx));
            self.right_path
                .update(cx, |state, cx| state.set_value("", window, cx));
            self.left_text_editor
                .update(cx, |state, cx| state.set_value("", window, cx));
            self.right_text_editor
                .update(cx, |state, cx| state.set_value("", window, cx));
            self.base_path
                .update(cx, |state, cx| state.set_value("", window, cx));
            self.search
                .update(cx, |state, cx| state.set_value("", window, cx));
            self.set_rows(Vec::new());
            self.folder_entries.clear();
            self.left_content.clear();
            self.right_content.clear();
            self.left_original = None;
            self.right_original = None;
            self.base_content.clear();
            self.merge_context = None;
            self.show_merge_context = false;
            self.selected_folder_file = None;
            self.folder_mode = mode == CompareMode::Folder;
            self.compared = false;
            self.bookmarks.clear();
            self.bookmark_cursor = 0;
            self.selected_bookmark = None;
            self.active_diff_input = None;
            self.last_diff_input = None;
            self.pending_diff_focus = None;
            self.active_panel = if mode == CompareMode::Text {
                MainPanel::TextInput
            } else {
                MainPanel::Diff
            };
            self.folder_filter = FolderFilter::All;
            self.diff_filter = DiffFilter::All;
        }
        let returning_to_comparison = self.mode == Some(mode) && self.compared;
        self.mode = Some(mode);
        self.show_home = false;
        if !returning_to_comparison {
            self.status = match mode {
                CompareMode::Folder => "Choose two folders to compare.".into(),
                CompareMode::File => "Choose two files to compare.".into(),
                CompareMode::Text => "Enter or paste text into both panes.".into(),
            };
        }
        cx.notify();
    }

    fn browse_path(&mut self, left: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mode = self.mode;
        let window_handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let selection = match mode {
                Some(CompareMode::Folder) => rfd::AsyncFileDialog::new().pick_folder().await,
                Some(CompareMode::File) => rfd::AsyncFileDialog::new().pick_file().await,
                Some(CompareMode::Text) | None => return,
            };
            if let Some(path) = selection {
                let path = path.path().to_string_lossy().into_owned();
                let result = cx.update_window(window_handle, |_, window, cx| {
                    this.update(cx, |this, cx| {
                        let state = if left {
                            &this.left_path
                        } else {
                            &this.right_path
                        };
                        state.update(cx, |state, cx| state.set_value(path, window, cx));
                        cx.notify();
                    })
                });
                if let Err(error) = result.and_then(|result| result) {
                    eprintln!("Could not update the selected path: {error}");
                }
            }
        })
        .detach();
    }

    fn browse_base_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let window_handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            if let Some(path) = rfd::AsyncFileDialog::new().pick_file().await {
                let path = path.path().to_string_lossy().into_owned();
                let result = cx.update_window(window_handle, |_, window, cx| {
                    this.update(cx, |this, cx| {
                        this.base_path
                            .update(cx, |state, cx| state.set_value(path, window, cx));
                        cx.notify();
                    })
                });
                if let Err(error) = result.and_then(|result| result) {
                    eprintln!("Could not update the selected base path: {error}");
                }
            }
        })
        .detach();
    }

    fn compare_text_inputs(&mut self, cx: &mut Context<Self>) {
        if !self.compared {
            self.left_content = self.left_text_editor.read(cx).value().to_string();
            self.right_content = self.right_text_editor.read(cx).value().to_string();
        }
        self.left_original = None;
        self.right_original = None;
        self.rows = compare_text_with_options(
            &self.left_content,
            &self.right_content,
            CompareOptions {
                ignore_whitespace: self.ignore_whitespace,
                ignore_case: self.ignore_case,
                ignore_comments: self.ignore_comments,
            },
        );
        self.diff_projection = DiffProjection::from_rows(&self.rows);
        self.folder_mode = false;
        self.folder_entries.clear();
        self.selected_folder_file = None;
        self.compared = true;
        self.active_panel = MainPanel::Diff;
        self.merge_context = None;
        self.show_merge_context = false;
        let changed = self
            .rows
            .iter()
            .filter(|line| line.kind != DiffKind::Equal)
            .count();
        self.status = if changed == 0 {
            "The text panes are identical under the selected rules.".into()
        } else {
            format!("{changed} changed line(s)")
        };
        cx.notify();
    }

    fn refresh_text_input(&mut self, cx: &mut Context<Self>) {
        if self.mode != Some(CompareMode::Text) || self.compared {
            return;
        }
        self.left_content = self.left_text_editor.read(cx).value().to_string();
        self.right_content = self.right_text_editor.read(cx).value().to_string();
        self.status = "Text ready to compare.".into();
        cx.notify();
    }

    fn has_unsaved_file_changes(&self) -> bool {
        if self.mode == Some(CompareMode::Text) {
            return false;
        }
        self.is_side_dirty(true) || self.is_side_dirty(false)
    }

    fn is_side_dirty(&self, left: bool) -> bool {
        if self.mode == Some(CompareMode::Text) {
            return false;
        }
        if left {
            side_has_unsaved_changes(&self.left_content, &self.left_original)
        } else {
            side_has_unsaved_changes(&self.right_content, &self.right_original)
        }
    }

    fn discard_source_changes(&mut self, cx: &mut Context<Self>) {
        if self.mode == Some(CompareMode::Text) {
            self.status = "Text Compare buffers do not have file changes to discard.".into();
            cx.notify();
            return;
        }
        if !self.has_unsaved_file_changes() {
            self.status = "There are no unsaved source changes to discard.".into();
            cx.notify();
            return;
        }
        self.left_content = self.left_original.clone().unwrap_or_default();
        self.right_content = self.right_original.clone().unwrap_or_default();
        self.rows = compare_text_with_options(
            &self.left_content,
            &self.right_content,
            CompareOptions {
                ignore_whitespace: self.ignore_whitespace,
                ignore_case: self.ignore_case,
                ignore_comments: self.ignore_comments,
            },
        );
        self.diff_projection = DiffProjection::from_rows(&self.rows);
        self.reset_diff_editor();
        self.merge_context = None;
        self.show_merge_context = false;
        self.bookmarks.clear();
        self.selected_bookmark = None;
        self.bookmark_cursor = 0;
        self.status = "Unsaved changes discarded; the files on disk were not changed.".into();
        cx.notify();
    }

    fn save_source(&mut self, left: bool, cx: &mut Context<Self>) {
        if self.mode == Some(CompareMode::Text) {
            self.status = "Text Compare buffers are not saved to files.".into();
            cx.notify();
            return;
        }
        if self.folder_mode && self.selected_folder_file.is_none() {
            self.status = "Open a folder text file before saving a side.".into();
            cx.notify();
            return;
        }
        let root = PathBuf::from(if left {
            self.left_path.read(cx).value().to_string()
        } else {
            self.right_path.read(cx).value().to_string()
        });
        let relative = self.selected_folder_file.clone();
        let path = relative
            .as_ref()
            .map(|relative| root.join(relative))
            .unwrap_or_else(|| root.clone());
        let (contents, original) = if left {
            (self.left_content.clone(), self.left_original.clone())
        } else {
            (self.right_content.clone(), self.right_original.clone())
        };
        if !side_has_unsaved_changes(&contents, &original) {
            self.status = "That side has no unsaved changes.".into();
            cx.notify();
            return;
        }
        let current = match fs::read_to_string(&path) {
            Ok(contents) => Some(contents),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                self.status = format!("Could not verify {} before saving: {error}", path.display());
                cx.notify();
                return;
            }
        };
        if current != original {
            self.status = format!(
                "{} changed on disk since it was compared. Recompare before saving.",
                path.display()
            );
            cx.notify();
            return;
        }
        let result = if let Some(relative) = relative {
            write_folder_file(&root, &relative, &contents)
        } else {
            fs::write(&root, &contents)
        };
        match result {
            Ok(()) => {
                if left {
                    self.left_original = Some(contents);
                    self.diff_editor.mark_saved(DiffSide::Left);
                } else {
                    self.right_original = Some(contents);
                    self.diff_editor.mark_saved(DiffSide::Right);
                }
                self.status = format!("Saved {}.", path.display());
            }
            Err(error) => self.status = format!("Could not save {}: {error}", path.display()),
        }
        cx.notify();
    }

    fn compare(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == Some(CompareMode::Text) {
            self.compare_text_inputs(cx);
            return;
        }
        if self.has_unsaved_file_changes() {
            self.status = "Save or discard staged source edits before comparing again.".into();
            cx.notify();
            return;
        }
        let left_path = self.left_path.read(cx).value().to_string();
        let right_path = self.right_path.read(cx).value().to_string();
        if left_path.trim().is_empty() || right_path.trim().is_empty() {
            self.set_error("Enter both comparison paths.".into());
            cx.notify();
            return;
        }
        let left = PathBuf::from(&left_path);
        let right = PathBuf::from(&right_path);
        if self.mode == Some(CompareMode::Folder) && !(left.is_dir() && right.is_dir()) {
            self.set_error("Folder Compare requires two existing folders.".into());
            cx.notify();
            return;
        }
        if self.mode == Some(CompareMode::File) && (left.is_dir() || right.is_dir()) {
            self.set_error("File Compare requires two file paths, not folders.".into());
            cx.notify();
            return;
        }
        match (left.is_dir(), right.is_dir()) {
            (true, true) => {
                let includes = split_patterns(&self.include_patterns.read(cx).value());
                let ignores = split_patterns(&self.ignore_patterns.read(cx).value());
                match compare_folders(&left, &right, &includes, &ignores) {
                    Ok(entries) => {
                        let total = entries.len();
                        let changed = entries
                            .iter()
                            .filter(|entry| entry.status != FolderStatus::Identical)
                            .count();
                        self.folder_entries = entries;
                        self.set_rows(Vec::new());
                        self.left_content.clear();
                        self.right_content.clear();
                        self.left_original = None;
                        self.right_original = None;
                        self.base_content.clear();
                        self.merge_context = None;
                        self.show_merge_context = false;
                        self.folder_mode = true;
                        self.selected_folder_file = None;
                        self.active_panel = MainPanel::Diff;
                        self.compared = true;
                        self.status = format!("{changed} differing files · {total} files shown");
                    }
                    Err(error) => self.set_error(format!("Could not compare folders: {error}")),
                }
            }
            (false, false) => {
                self.folder_mode = false;
                self.folder_entries.clear();
                self.selected_folder_file = None;
                self.base_content.clear();
                let left_contents = match fs::read_to_string(&left) {
                    Ok(contents) => contents,
                    Err(error) => {
                        self.set_error(format!("Could not read {left_path}: {error}"));
                        cx.notify();
                        return;
                    }
                };
                let right_contents = match fs::read_to_string(&right) {
                    Ok(contents) => contents,
                    Err(error) => {
                        self.set_error(format!("Could not read {right_path}: {error}"));
                        cx.notify();
                        return;
                    }
                };
                self.left_content = left_contents;
                self.right_content = right_contents;
                self.left_original = Some(self.left_content.clone());
                self.right_original = Some(self.right_content.clone());
                self.merge_context = None;
                self.show_merge_context = false;
                let base_path = self.base_path.read(cx).value().to_string();
                if !base_path.trim().is_empty() {
                    match fs::read_to_string(&base_path) {
                        Ok(base) => {
                            self.base_content = base.clone();
                            let merge_left = self.left_content.clone();
                            let merge_right = self.right_content.clone();
                            let (merged, conflicts) =
                                merge_three_way(&base, &merge_left, &merge_right);
                            self.merge_context = Some(MergeContext {
                                base,
                                left: merge_left.clone(),
                                right: merge_right,
                                conflicts,
                            });
                            self.show_merge_context = true;
                            self.right_content = merged;
                            self.rows = compare_text_with_options(
                                &self.left_content,
                                &self.right_content,
                                self.compare_options(),
                            );
                            self.diff_projection = DiffProjection::from_rows(&self.rows);
                            self.status = format!(
                                "3-way result staged on the right · {conflicts} conflict(s)"
                            );
                        }
                        Err(error) => {
                            self.set_error(format!(
                                "Could not read base file {base_path}: {error}"
                            ));
                            cx.notify();
                            return;
                        }
                    }
                } else {
                    self.rows = compare_text_with_options(
                        &self.left_content,
                        &self.right_content,
                        self.compare_options(),
                    );
                    self.diff_projection = DiffProjection::from_rows(&self.rows);
                    let changed = self
                        .rows
                        .iter()
                        .filter(|line| line.kind != DiffKind::Equal)
                        .count();
                    self.status = if changed == 0 {
                        "The files are identical under the selected rules.".into()
                    } else {
                        format!("{changed} changed line(s)")
                    };
                }
                self.compared = true;
                self.active_panel = MainPanel::Diff;
                self.bookmarks.clear();
                self.bookmark_cursor = 0;
                self.selected_bookmark = None;
            }
            _ => self.set_error("Both paths must be files or both must be folders.".into()),
        }
        cx.notify();
    }

    fn set_error(&mut self, message: String) {
        self.set_rows(Vec::new());
        self.folder_entries.clear();
        self.folder_mode = false;
        self.selected_folder_file = None;
        self.base_content.clear();
        self.merge_context = None;
        self.show_merge_context = false;
        self.compared = false;
        self.status = message;
    }

    fn toggle_rule(&mut self, whitespace: bool, cx: &mut Context<Self>) {
        if whitespace {
            self.ignore_whitespace = !self.ignore_whitespace;
        } else {
            self.ignore_case = !self.ignore_case;
        }
        if !self.left_content.is_empty() || !self.right_content.is_empty() {
            self.rows = compare_text_with_options(
                &self.left_content,
                &self.right_content,
                CompareOptions {
                    ignore_whitespace: self.ignore_whitespace,
                    ignore_case: self.ignore_case,
                    ignore_comments: self.ignore_comments,
                },
            );
            self.diff_projection = DiffProjection::from_rows(&self.rows);
        }
        cx.notify();
    }

    fn toggle_comments(&mut self, cx: &mut Context<Self>) {
        self.ignore_comments = !self.ignore_comments;
        if !self.left_content.is_empty() || !self.right_content.is_empty() {
            self.rows = compare_text_with_options(
                &self.left_content,
                &self.right_content,
                CompareOptions {
                    ignore_whitespace: self.ignore_whitespace,
                    ignore_case: self.ignore_case,
                    ignore_comments: self.ignore_comments,
                },
            );
            self.diff_projection = DiffProjection::from_rows(&self.rows);
        }
        cx.notify();
    }

    fn sync_folders(&mut self, left_to_right: bool, cx: &mut Context<Self>) {
        if !self.folder_mode {
            self.status = "Enter and compare two folders before synchronizing.".into();
            cx.notify();
            return;
        }
        if self.has_unsaved_file_changes() {
            self.status =
                "Save or discard staged source edits before synchronizing folders.".into();
            cx.notify();
            return;
        }
        let left = PathBuf::from(self.left_path.read(cx).value().to_string());
        let right = PathBuf::from(self.right_path.read(cx).value().to_string());
        let (source, destination) = if left_to_right {
            (&left, &right)
        } else {
            (&right, &left)
        };
        match synchronize_folders(source, destination, &self.folder_entries) {
            Ok(count) => {
                self.status = format!("Copied {count} file(s); compare again to refresh.");
            }
            Err(error) => self.status = format!("Folder sync failed: {error}"),
        }
        cx.notify();
    }

    fn copy_file(&mut self, left_to_right: bool, _window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == Some(CompareMode::Text) {
            self.status = "Text Compare has no file destination.".into();
            cx.notify();
            return;
        }
        if self.folder_mode && self.selected_folder_file.is_none() {
            self.status = "Open a folder text file before copying file contents.".into();
            cx.notify();
            return;
        }
        let source_side = if left_to_right {
            DiffSide::Left
        } else {
            DiffSide::Right
        };
        let destination_is_dirty = if left_to_right {
            self.is_side_dirty(false)
        } else {
            self.is_side_dirty(true)
        };
        if destination_is_dirty {
            self.status = "The destination has unsaved edits. Save or discard them before staging a full-file copy.".into();
            cx.notify();
            return;
        }
        self.ensure_diff_editor();
        let old_rows = self.rows.clone();
        match self.diff_editor.copy_file_from(source_side) {
            Ok(true) => self.sync_from_diff_editor(source_side, &old_rows),
            Ok(false) => {
                self.status = "The source buffers already match; there is nothing to stage.".into();
                cx.notify();
                return;
            }
            Err(error) => {
                self.status = format!("Could not stage the full-file copy: {error:?}");
                cx.notify();
                return;
            }
        }
        self.bookmarks.clear();
        self.selected_bookmark = None;
        self.bookmark_cursor = 0;
        self.status = if left_to_right {
            "Entire left file staged on the right; save or discard the unsaved change.".into()
        } else {
            "Entire right file staged on the left; save or discard the unsaved change.".into()
        };
        cx.notify();
    }

    fn undo_diff_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_diff_editor();
        if !self.diff_editor.can_undo() {
            if let Some((side, row)) = self.last_diff_input
                && let Some(input) = match side {
                    DiffSide::Left => self.left_diff_inputs.get(row),
                    DiffSide::Right => self.right_diff_inputs.get(row),
                }
            {
                input.update(cx, |state, cx| state.focus(window, cx));
                window.dispatch_action(Box::new(gpui_kit::component::input::Undo), cx);
                return;
            }
            self.status = "There is no diff edit to undo.".into();
            cx.notify();
            return;
        }
        let old_left = self.left_content.clone();
        let old_right = self.right_content.clone();
        let old_rows = self.rows.clone();
        match self.diff_editor.undo() {
            Ok(true) => {
                let changed_side = if self.diff_editor.text(DiffSide::Left) != old_left {
                    DiffSide::Left
                } else if self.diff_editor.text(DiffSide::Right) != old_right {
                    DiffSide::Right
                } else {
                    self.status = "Could not identify the side changed by Undo.".into();
                    cx.notify();
                    return;
                };
                self.sync_from_diff_editor(opposite_side(changed_side), &old_rows);
                let (row, column) = self.diff_editor.cursor_display_position(changed_side);
                self.pending_diff_focus = Some((changed_side, row, column));
                self.status = "Undid the last diff edit.".into();
            }
            Ok(false) => self.status = "There is no diff edit to undo.".into(),
            Err(error) => self.status = format!("Could not undo the diff edit: {error:?}"),
        }
        cx.notify();
    }

    fn redo_diff_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_diff_editor();
        if !self.diff_editor.can_redo() {
            if let Some((side, row)) = self.last_diff_input
                && let Some(input) = match side {
                    DiffSide::Left => self.left_diff_inputs.get(row),
                    DiffSide::Right => self.right_diff_inputs.get(row),
                }
            {
                input.update(cx, |state, cx| state.focus(window, cx));
                window.dispatch_action(Box::new(gpui_kit::component::input::Redo), cx);
                return;
            }
            self.status = "There is no diff edit to redo.".into();
            cx.notify();
            return;
        }
        let old_left = self.left_content.clone();
        let old_right = self.right_content.clone();
        let old_rows = self.rows.clone();
        match self.diff_editor.redo() {
            Ok(true) => {
                let changed_side = if self.diff_editor.text(DiffSide::Left) != old_left {
                    DiffSide::Left
                } else if self.diff_editor.text(DiffSide::Right) != old_right {
                    DiffSide::Right
                } else {
                    self.status = "Could not identify the side changed by Redo.".into();
                    cx.notify();
                    return;
                };
                self.sync_from_diff_editor(opposite_side(changed_side), &old_rows);
                let (row, column) = self.diff_editor.cursor_display_position(changed_side);
                self.pending_diff_focus = Some((changed_side, row, column));
                self.status = "Redid the diff edit.".into();
            }
            Ok(false) => self.status = "There is no diff edit to redo.".into(),
            Err(error) => self.status = format!("Could not redo the diff edit: {error:?}"),
        }
        cx.notify();
    }

    fn toggle_merge_context(&mut self, cx: &mut Context<Self>) {
        if self.merge_context.is_some() {
            self.show_merge_context = !self.show_merge_context;
        } else {
            self.status = "No three-way merge context is available.".into();
        }
        cx.notify();
    }

    fn show_about(&mut self, cx: &mut Context<Self>) {
        self.status = "Checker · local file and folder comparison".into();
        cx.notify();
    }

    fn adjust_editor_font_size(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.editor_font_size = (self.editor_font_size + delta).clamp(9., 18.);
        cx.notify();
    }

    fn reset_editor_font_size(&mut self, cx: &mut Context<Self>) {
        self.editor_font_size = 12.;
        cx.notify();
    }

    fn open_folder_file(&mut self, relative: String, _window: &mut Window, cx: &mut Context<Self>) {
        if self.has_unsaved_file_changes() {
            self.status = "Save or discard staged edits before opening another folder file.".into();
            cx.notify();
            return;
        }
        let left_path = PathBuf::from(self.left_path.read(cx).value().to_string()).join(&relative);
        let right_path =
            PathBuf::from(self.right_path.read(cx).value().to_string()).join(&relative);
        let left_original = if left_path.exists() {
            match fs::read_to_string(&left_path) {
                Ok(contents) => Some(contents),
                Err(error) => {
                    self.status =
                        format!("Could not open {} as text: {error}", left_path.display());
                    cx.notify();
                    return;
                }
            }
        } else {
            None
        };
        let right_original = if right_path.exists() {
            match fs::read_to_string(&right_path) {
                Ok(contents) => Some(contents),
                Err(error) => {
                    self.status =
                        format!("Could not open {} as text: {error}", right_path.display());
                    cx.notify();
                    return;
                }
            }
        } else {
            None
        };
        self.left_content = left_original.clone().unwrap_or_default();
        self.right_content = right_original.clone().unwrap_or_default();
        self.left_original = left_original;
        self.right_original = right_original;
        self.base_content.clear();
        self.merge_context = None;
        self.show_merge_context = false;
        self.rows = compare_text_with_options(
            &self.left_content,
            &self.right_content,
            CompareOptions {
                ignore_whitespace: self.ignore_whitespace,
                ignore_case: self.ignore_case,
                ignore_comments: self.ignore_comments,
            },
        );
        self.diff_projection = DiffProjection::from_rows(&self.rows);
        self.selected_folder_file = Some(relative.clone());
        self.active_panel = MainPanel::Diff;
        self.status = format!("Comparing folder file: {relative}");
        cx.notify();
    }

    fn move_bookmark(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.bookmarks.is_empty() {
            self.status = "Bookmark a diff row first.".into();
        } else {
            let indices: Vec<_> = self.bookmarks.iter().copied().collect();
            self.bookmark_cursor = if forward {
                (self.bookmark_cursor + 1) % indices.len()
            } else {
                self.bookmark_cursor
                    .checked_sub(1)
                    .unwrap_or(indices.len() - 1)
            };
            self.selected_bookmark = Some(indices[self.bookmark_cursor]);
            self.status = format!(
                "Bookmark {} of {} · diff row {}",
                self.bookmark_cursor + 1,
                indices.len(),
                indices[self.bookmark_cursor] + 1
            );
        }
        cx.notify();
    }

    fn copy_hunk(
        &mut self,
        row_index: usize,
        left_to_right: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let source_side = if left_to_right {
            DiffSide::Left
        } else {
            DiffSide::Right
        };
        self.ensure_diff_editor();
        let old_rows = self.rows.clone();
        match self.diff_editor.copy_hunk_from(source_side, row_index) {
            Ok(true) => self.sync_from_diff_editor(source_side, &old_rows),
            Ok(false) => {
                self.status = "The destination already matches this diff hunk.".into();
                cx.notify();
                return;
            }
            Err(error) => {
                self.status = format!("Could not copy this diff hunk: {error:?}");
                cx.notify();
                return;
            }
        }
        self.status = if left_to_right {
            "Copied the hunk left to right; changes remain in memory.".into()
        } else {
            "Copied the hunk right to left; changes remain in memory.".into()
        };
        cx.notify();
    }

    fn render_session_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut tabs = div().h_flex().items_center().gap_1().px_2();
        for (index, session) in self.sessions.iter().enumerate() {
            let dirty = if index == self.active_session {
                self.has_unsaved_file_changes()
            } else {
                session.has_unsaved_file_changes()
            };
            let label = if dirty {
                format!("{} *", session.name)
            } else {
                session.name.clone()
            };
            tabs = tabs.child(
                Button::new(format!("session-tab-{index}"))
                    .compact()
                    .with_size(ComponentSize::Small)
                    .when(index == self.active_session, |button| button.secondary())
                    .when(index != self.active_session, |button| button.ghost())
                    .label(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.activate_session(index, window, cx)
                    })),
            );
        }
        tabs.child(
            Button::new("new-session")
                .compact()
                .with_size(ComponentSize::Small)
                .ghost()
                .label("+ New")
                .on_click(cx.listener(|this, _, window, cx| this.new_session(window, cx))),
        )
        .child(div().flex_1())
        .child(
            Button::new("home")
                .compact()
                .with_size(ComponentSize::Small)
                .ghost()
                .label("Home")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show_home = true;
                    cx.notify();
                })),
        )
    }

    fn render_home(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.theme.palette();
        div()
            .v_flex()
            .flex_1()
            .min_h_0()
            .items_center()
            .justify_center()
            .p_6()
            .child(
                div()
                    .v_flex()
                    .w(px(900.))
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(18.))
                            .font_weight(FontWeight::BOLD)
                            .child("Start a comparison"),
                    )
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(rgb(palette.secondary_text))
                            .child("Choose a workflow. Each comparison opens in its own session."),
                    )
                    .child(
                        div()
                            .h_flex()
                            .gap_3()
                            .mt_3()
                            .child(
                                comparison_mode_card(
                                    "Folder Compare",
                                    "Compare folder trees, review file metadata, and sync changes.",
                                    Button::new("start-folder-compare")
                                        .primary()
                                        .label("Compare folders")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.select_mode(CompareMode::Folder, window, cx)
                                        })),
                                    &palette,
                                ),
                            )
                            .child(
                                comparison_mode_card(
                                    "File Compare",
                                    "Choose two files in Explorer and review their line-by-line changes.",
                                    Button::new("start-file-compare")
                                        .primary()
                                        .label("Compare files")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.select_mode(CompareMode::File, window, cx)
                                        })),
                                    &palette,
                                ),
                            )
                            .child(
                                comparison_mode_card(
                                    "Text Compare",
                                    "Paste or type text directly into two side-by-side panes.",
                                    Button::new("start-text-compare")
                                        .primary()
                                        .label("Compare text")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.select_mode(CompareMode::Text, window, cx)
                                        })),
                                    &palette,
                                ),
                            ),
                    ),
            )
    }
}

impl Render for Checker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.theme.palette();
        let status_color = if self.status.starts_with("Could not")
            || self.status.starts_with("Folder sync failed")
            || self.status.starts_with("Enter ")
            || self.status.starts_with("Folder Compare requires")
            || self.status.starts_with("File Compare requires")
        {
            palette.status_error
        } else {
            palette.secondary_text
        };
        let difference_count = self
            .rows
            .iter()
            .filter(|line| line.kind != DiffKind::Equal)
            .count();
        let same_count = self.rows.len().saturating_sub(difference_count);
        let merge_context_preview = self
            .merge_context
            .as_ref()
            .filter(|_| self.show_merge_context)
            .map(|context| {
                (
                    context.base.clone(),
                    context.left.clone(),
                    context.right.clone(),
                )
            });
        div()
            .v_flex()
            .size_full()
            .track_focus(&self.focus_handle)
            .text_size(px(10.5))
            .bg(rgb(palette.window))
            .text_color(rgb(palette.foreground))
            .on_action(cx.listener(|this, _: &RunComparison, window, cx| this.compare(window, cx)))
            .on_action(cx.listener(|this, _: &SaveLeft, _, cx| this.save_source(true, cx)))
            .on_action(cx.listener(|this, _: &SaveRight, _, cx| this.save_source(false, cx)))
            .on_action(
                cx.listener(|this, _: &DiscardSources, _, cx| this.discard_source_changes(cx)),
            )
            .on_action(
                cx.listener(|this, _: &UndoDiffEdit, window, cx| this.undo_diff_edit(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &RedoDiffEdit, window, cx| this.redo_diff_edit(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ToggleMergeContext, _, cx| this.toggle_merge_context(cx)),
            )
            .on_action(cx.listener(|this, _: &ToggleWhitespace, _, cx| this.toggle_rule(true, cx)))
            .on_action(cx.listener(|this, _: &ToggleCase, _, cx| this.toggle_rule(false, cx)))
            .on_action(cx.listener(|this, _: &ToggleComments, _, cx| this.toggle_comments(cx)))
            .on_action(cx.listener(|this, _: &SyncLeftToRight, _, cx| this.sync_folders(true, cx)))
            .on_action(cx.listener(|this, _: &SyncRightToLeft, _, cx| this.sync_folders(false, cx)))
            .on_action(
                cx.listener(|this, _: &CopyLeftToRight, window, cx| {
                    this.copy_file(true, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &CopyRightToLeft, window, cx| {
                this.copy_file(false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleAdvanced, _, cx| {
                this.show_advanced = !this.show_advanced;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &DecreaseFontSize, _, cx| {
                this.adjust_editor_font_size(-1., cx)
            }))
            .on_action(
                cx.listener(|this, _: &IncreaseFontSize, _, cx| {
                    this.adjust_editor_font_size(1., cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &ResetFontSize, _, cx| this.reset_editor_font_size(cx)),
            )
            .on_action(
                cx.listener(|this, _: &SetDarkTheme, _, cx| this.set_theme(AppTheme::Dark, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SetLightTheme, _, cx| this.set_theme(AppTheme::Light, cx)),
            )
            .on_action(cx.listener(|this, _: &ShowAbout, _, cx| this.show_about(cx)))
            .on_action(cx.listener(|this, _: &ShowHome, _, cx| {
                this.show_home = true;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &NewSession, window, cx| this.new_session(window, cx)))
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .h(px(26.))
                    .bg(rgb(palette.header))
                    .border_b_1()
                    .border_color(rgb(palette.border))
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_2()
                            .text_size(px(10.5))
                            .font_weight(FontWeight::BOLD)
                            .child(
                                div()
                                    .size(px(18.))
                                    .h_flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .bg(rgb(palette.accent))
                                    .text_color(rgb(0xffffff))
                                    .child("C"),
                            )
                            .child("Checker"),
                    )
                    .child(div().h(px(26.)).child(self.menu_bar.clone()))
                    .child(self.render_session_tabs(cx))
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(palette.muted_text))
                            .child(match self.mode {
                                Some(CompareMode::Folder) => "FOLDER COMPARE",
                                Some(CompareMode::File) => "FILE COMPARE",
                                Some(CompareMode::Text) => "TEXT COMPARE",
                                None => "WORKBENCH",
                            }),
                    ),
            )
            .when(self.show_home, |this| this.child(self.render_home(cx)))
            .when(!self.show_home, |this| {
                this.child(
                    div()
                        .v_flex()
                        .gap_1()
                        .p_1()
                        .flex_1()
                        .min_h_0()
                        .when(self.mode != Some(CompareMode::Text), |this| {
                            this.child(
                                div()
                                    .v_flex()
                                    .gap_1()
                                    .px_1()
                                    .py(px(2.))
                                    .child(
                                        div()
                                            .h_flex()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .h_flex()
                                                    .items_center()
                                                    .flex_1()
                                                    .min_w(px(0.))
                                                    .gap(px(3.))
                                                    .child(path_input(
                                                        "LEFT",
                                                        &self.left_path,
                                                        &palette,
                                                    ))
                                                    .child(
                                                        Button::new("browse-left")
                                                            .ghost()
                                                            .compact()
                                                            .with_size(ComponentSize::XSmall)
                                                            .label("Browse")
                                                            .on_click(cx.listener(
                                                                |this, _, window, cx| {
                                                                    this.browse_path(
                                                                        true, window, cx,
                                                                    )
                                                                },
                                                            )),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .h_flex()
                                                    .items_center()
                                                    .flex_1()
                                                    .min_w(px(0.))
                                                    .gap(px(3.))
                                                    .child(path_input(
                                                        "RIGHT",
                                                        &self.right_path,
                                                        &palette,
                                                    ))
                                                    .child(
                                                        Button::new("browse-right")
                                                            .ghost()
                                                            .compact()
                                                            .with_size(ComponentSize::XSmall)
                                                            .label("Browse")
                                                            .on_click(cx.listener(
                                                                |this, _, window, cx| {
                                                                    this.browse_path(
                                                                        false, window, cx,
                                                                    )
                                                                },
                                                            )),
                                                    ),
                                            )
                                            .child(
                                                Button::new("advanced-options")
                                                    .ghost()
                                                    .compact()
                                                    .with_size(ComponentSize::XSmall)
                                                    .label("Advanced")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.show_advanced = !this.show_advanced;
                                                        cx.notify();
                                                    })),
                                            ),
                                    )
                                    .when(self.show_advanced, |this| {
                                        this.child(
                                            div()
                                                .h_flex()
                                                .gap_3()
                                                .child(
                                                    div()
                                                        .h_flex()
                                                        .items_end()
                                                        .flex_1()
                                                        .gap_1()
                                                        .child(labeled_input(
                                                            "BASE FILE",
                                                            &self.base_path,
                                                            &palette,
                                                        ))
                                                        .child(
                                                            Button::new("browse-base")
                                                                .ghost()
                                                                .label("Browse")
                                                                .on_click(cx.listener(
                                                                    |this, _, window, cx| {
                                                                        this.browse_base_file(
                                                                            window, cx,
                                                                        )
                                                                    },
                                                                )),
                                                        ),
                                                )
                                                .child(labeled_input(
                                                    "INCLUDE PATTERNS",
                                                    &self.include_patterns,
                                                    &palette,
                                                ))
                                                .child(labeled_input(
                                                    "IGNORE PATTERNS",
                                                    &self.ignore_patterns,
                                                    &palette,
                                                )),
                                        )
                                    }),
                            )
                        })
                        .child(
                            div()
                                .h_flex()
                                .items_center()
                                .gap_1()
                                .h(px(26.))
                                .px_2()
                                .bg(rgb(palette.toolbar))
                                .child(
                                    Button::new("compare")
                                        .compact()
                                        .with_size(ComponentSize::XSmall)
                                        .primary()
                                        .label("Compare")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.compare(window, cx)
                                        })),
                                )
                                .child(
                                    Button::new("compare-options")
                                        .ghost()
                                        .compact()
                                        .with_size(ComponentSize::XSmall)
                                        .label(
                                            if self.ignore_whitespace
                                                || self.ignore_case
                                                || self.ignore_comments
                                            {
                                                "Options •"
                                            } else {
                                                "Options"
                                            },
                                        )
                                        .dropdown_menu(|menu, _, _| {
                                            menu.menu(
                                                "Ignore whitespace",
                                                Box::new(ToggleWhitespace),
                                            )
                                            .menu("Ignore case", Box::new(ToggleCase))
                                            .menu("Ignore comment lines", Box::new(ToggleComments))
                                        }),
                                )
                                .when(
                                    self.active_panel == MainPanel::Diff
                                        && self.compared
                                        && (self.mode != Some(CompareMode::Folder)
                                            || self.selected_folder_file.is_some()),
                                    |this| {
                                        this.child(
                                            Button::new("diff-filter-all")
                                                .compact()
                                                .with_size(ComponentSize::XSmall)
                                                .when(
                                                    self.diff_filter == DiffFilter::All,
                                                    |button| button.secondary(),
                                                )
                                                .when(
                                                    self.diff_filter != DiffFilter::All,
                                                    |button| button.ghost(),
                                                )
                                                .label(format!("All {}", self.rows.len()))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.diff_filter = DiffFilter::All;
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            Button::new("diff-filter-differences")
                                                .compact()
                                                .with_size(ComponentSize::XSmall)
                                                .when(
                                                    self.diff_filter == DiffFilter::Differences,
                                                    |button| button.secondary(),
                                                )
                                                .when(
                                                    self.diff_filter != DiffFilter::Differences,
                                                    |button| button.ghost(),
                                                )
                                                .label(format!("Diffs {difference_count}"))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.diff_filter = DiffFilter::Differences;
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            Button::new("diff-filter-same")
                                                .compact()
                                                .with_size(ComponentSize::XSmall)
                                                .when(
                                                    self.diff_filter == DiffFilter::Same,
                                                    |button| button.secondary(),
                                                )
                                                .when(
                                                    self.diff_filter != DiffFilter::Same,
                                                    |button| button.ghost(),
                                                )
                                                .label(format!("Same {same_count}"))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.diff_filter = DiffFilter::Same;
                                                    cx.notify();
                                                })),
                                        )
                                    },
                                )
                                .when(
                                    self.mode != Some(CompareMode::Text) && self.compared,
                                    |this| {
                                        this.child(
                                            Button::new("file-actions")
                                                .ghost()
                                                .compact()
                                                .with_size(ComponentSize::XSmall)
                                                .label("Actions")
                                                .dropdown_menu(|menu, _, _| {
                                                    menu.menu(
                                                        "Sync left to right",
                                                        Box::new(SyncLeftToRight),
                                                    )
                                                    .menu(
                                                        "Sync right to left",
                                                        Box::new(SyncRightToLeft),
                                                    )
                                                    .separator()
                                                    .menu(
                                                        "Stage entire left file on the right",
                                                        Box::new(CopyLeftToRight),
                                                    )
                                                    .menu(
                                                        "Stage entire right file on the left",
                                                        Box::new(CopyRightToLeft),
                                                    )
                                                }),
                                        )
                                    },
                                )
                                .when(self.merge_context.is_some(), |this| {
                                    this.child(
                                        Button::new("merge-context")
                                            .compact()
                                            .with_size(ComponentSize::XSmall)
                                            .when(self.show_merge_context, |button| {
                                                button.secondary()
                                            })
                                            .when(!self.show_merge_context, |button| button.ghost())
                                            .label(format!(
                                                "Merge inputs · {} conflict(s)",
                                                self.merge_context
                                                    .as_ref()
                                                    .map(|context| context.conflicts)
                                                    .unwrap_or_default()
                                            ))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.toggle_merge_context(cx);
                                            })),
                                    )
                                })
                                .when(
                                    self.active_panel == MainPanel::Diff && self.compared,
                                    |this| {
                                        this.child(
                                            div().w(px(120.)).child(
                                                Input::new(&self.search)
                                                    .with_size(ComponentSize::Small),
                                            ),
                                        )
                                        .child(
                                            Button::new("bookmark-previous")
                                                .ghost()
                                                .compact()
                                                .with_size(ComponentSize::XSmall)
                                                .label("‹")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.move_bookmark(false, cx)
                                                })),
                                        )
                                        .child(
                                            Button::new("bookmark-next")
                                                .ghost()
                                                .compact()
                                                .with_size(ComponentSize::XSmall)
                                                .label("›")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.move_bookmark(true, cx)
                                                })),
                                        )
                                    },
                                )
                                .when(
                                    self.active_panel == MainPanel::Diff
                                        && self.compared
                                        && self.mode != Some(CompareMode::Text)
                                        && (!self.folder_mode
                                            || self.selected_folder_file.is_some())
                                        && self.is_side_dirty(true),
                                    |this| {
                                        this.child(
                                            Button::new("save-left-source")
                                                .primary()
                                                .compact()
                                                .with_size(ComponentSize::XSmall)
                                                .label("Save left")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.save_source(true, cx)
                                                })),
                                        )
                                    },
                                )
                                .when(
                                    self.active_panel == MainPanel::Diff
                                        && self.compared
                                        && self.mode != Some(CompareMode::Text)
                                        && (!self.folder_mode
                                            || self.selected_folder_file.is_some())
                                        && self.is_side_dirty(false),
                                    |this| {
                                        this.child(
                                            Button::new("save-right-source")
                                                .primary()
                                                .compact()
                                                .with_size(ComponentSize::XSmall)
                                                .label(if self.merge_context.is_some() {
                                                    "Save result"
                                                } else {
                                                    "Save right"
                                                })
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.save_source(false, cx)
                                                })),
                                        )
                                    },
                                )
                                .when(
                                    self.active_panel == MainPanel::Diff
                                        && self.has_unsaved_file_changes(),
                                    |this| {
                                        this.child(
                                            Button::new("discard-source-changes")
                                                .ghost()
                                                .compact()
                                                .with_size(ComponentSize::XSmall)
                                                .label("Discard")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.discard_source_changes(cx)
                                                })),
                                        )
                                    },
                                )
                                .when(
                                    self.active_panel == MainPanel::Diff && self.compared,
                                    |this| {
                                        this.child(
                                            Button::new("diff-undo")
                                                .ghost()
                                                .compact()
                                                .with_size(ComponentSize::XSmall)
                                                .label("Undo")
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.undo_diff_edit(window, cx)
                                                })),
                                        )
                                        .child(
                                            Button::new("diff-redo")
                                                .ghost()
                                                .compact()
                                                .with_size(ComponentSize::XSmall)
                                                .label("Redo")
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.redo_diff_edit(window, cx)
                                                })),
                                        )
                                    },
                                )
                                .child(
                                    Button::new("editor-font-smaller")
                                        .ghost()
                                        .compact()
                                        .with_size(ComponentSize::XSmall)
                                        .label("A-")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.adjust_editor_font_size(-1., cx)
                                        })),
                                )
                                .child(
                                    Button::new("editor-font-reset")
                                        .ghost()
                                        .compact()
                                        .with_size(ComponentSize::XSmall)
                                        .label(format!("{:.0}px", self.editor_font_size))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.reset_editor_font_size(cx)
                                        })),
                                )
                                .child(
                                    Button::new("editor-font-larger")
                                        .ghost()
                                        .compact()
                                        .with_size(ComponentSize::XSmall)
                                        .label("A+")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.adjust_editor_font_size(1., cx)
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .v_flex()
                                .flex_1()
                                .min_h_0()
                                .bg(rgb(palette.panel))
                                .border_1()
                                .border_color(rgb(palette.border))
                                .when(self.active_panel == MainPanel::Diff, |this| {
                                    let mut content = div().v_flex().flex_1().min_h_0();
                                    if let Some((base, left, right)) = merge_context_preview {
                                        content = content.child(
                                            div()
                                                .h_flex()
                                                .h(px(110.))
                                                .gap_2()
                                                .p_1()
                                                .child(merge_preview(
                                                    "BASE",
                                                    &base,
                                                    self.editor_font_size,
                                                    &palette,
                                                ))
                                                .child(merge_preview(
                                                    "LEFT SOURCE",
                                                    &left,
                                                    self.editor_font_size,
                                                    &palette,
                                                ))
                                                .child(merge_preview(
                                                    "RIGHT SOURCE",
                                                    &right,
                                                    self.editor_font_size,
                                                    &palette,
                                                )),
                                        );
                                    }
                                    this.child(
                                        content.child(
                                            div()
                                                .flex_1()
                                                .min_h_0()
                                                .overflow_y_scrollbar()
                                                .child(self.render_results(window, &palette, cx)),
                                        ),
                                    )
                                })
                                .when(self.active_panel == MainPanel::TextInput, |this| {
                                    this.child(self.render_text_inputs(&palette))
                                })
                                .when(!self.show_home, |this| {
                                    this.child(
                                        div()
                                            .h_flex()
                                            .items_center()
                                            .h(px(20.))
                                            .px_2()
                                            .bg(rgb(palette.toolbar))
                                            .border_t_1()
                                            .border_color(rgb(palette.border))
                                            .text_size(px(9.))
                                            .text_color(rgb(status_color))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .text_ellipsis()
                                                    .child(self.status.clone()),
                                            )
                                            .when(self.has_unsaved_file_changes(), |bar| {
                                                bar.child(" · UNSAVED")
                                            }),
                                    )
                                }),
                        ),
                )
            })
    }
}

impl Checker {
    fn render_text_inputs(&self, palette: &ThemePalette) -> impl IntoElement {
        div()
            .v_flex()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .px_2()
                    .py_1()
                    .text_size(px(9.))
                    .text_color(rgb(palette.secondary_text))
                    .child("Enter or paste both texts, then Compare. Editing after comparison happens inline."),
            )
            .child(
                div()
                    .h_flex()
                    .flex_1()
                    .min_h_0()
                    .gap_1()
                    .p_1()
                    .child(text_source(
                        "LEFT TEXT",
                        &self.left_text_editor,
                        self.editor_font_size,
                        palette,
                    ))
                    .child(text_source(
                        "RIGHT TEXT",
                        &self.right_text_editor,
                        self.editor_font_size,
                        palette,
                    )),
            )
    }

    fn render_results(
        &mut self,
        window: &mut Window,
        palette: &ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        if self.folder_mode && self.selected_folder_file.is_none() {
            let mut list = div().v_flex().gap(px(1.));
            let query = self.search.read(cx).value().to_string().to_lowercase();
            let different = self
                .folder_entries
                .iter()
                .filter(|entry| entry.status != FolderStatus::Identical)
                .count();
            for (index, entry) in self.folder_entries.iter().enumerate() {
                if !query.is_empty() && !entry.relative_path.to_lowercase().contains(&query) {
                    continue;
                }
                if self.folder_filter == FolderFilter::Differences
                    && entry.status == FolderStatus::Identical
                    || self.folder_filter == FolderFilter::Identical
                        && entry.status != FolderStatus::Identical
                {
                    continue;
                }
                let (label, color) = match entry.status {
                    FolderStatus::LeftOnly => ("Left only", palette.left_only),
                    FolderStatus::RightOnly => ("Right only", palette.right_only),
                    FolderStatus::Different => ("Different", palette.diff_removed_text),
                    FolderStatus::Identical => ("Same", palette.identical),
                    FolderStatus::Unreadable => ("Unreadable", palette.status_error),
                };
                let left_path = (entry.status != FolderStatus::RightOnly)
                    .then_some(entry.relative_path.as_str());
                let right_path = (entry.status != FolderStatus::LeftOnly)
                    .then_some(entry.relative_path.as_str());
                list = list.child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap_1()
                        .bg(rgb(palette.header))
                        .child(folder_side_cell(left_path, entry.left.as_ref(), palette))
                        .child(
                            div()
                                .w(px(92.))
                                .h_flex()
                                .justify_center()
                                .text_size(px(10.))
                                .text_color(rgb(color))
                                .child(label),
                        )
                        .child(folder_side_cell(right_path, entry.right.as_ref(), palette))
                        .child(
                            Button::new(format!("open-folder-file-{index}"))
                                .ghost()
                                .label("Open")
                                .on_click(cx.listener({
                                    let relative = entry.relative_path.clone();
                                    move |this, _, window, cx| {
                                        this.open_folder_file(relative.clone(), window, cx)
                                    }
                                })),
                        ),
                );
            }
            return div()
                .v_flex()
                .flex_1()
                .min_h_0()
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap_1()
                        .px_3()
                        .py_2()
                        .bg(rgb(palette.toolbar))
                        .border_b_1()
                        .border_color(rgb(palette.border))
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(12.))
                                .font_weight(FontWeight::BOLD)
                                .child(format!("Files · {}", self.folder_entries.len())),
                        )
                        .child(
                            Button::new("folder-filter-all")
                                .when(self.folder_filter == FolderFilter::All, |button| {
                                    button.secondary()
                                })
                                .when(self.folder_filter != FolderFilter::All, |button| {
                                    button.ghost()
                                })
                                .label(format!("All {}", self.folder_entries.len()))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.folder_filter = FolderFilter::All;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("folder-filter-differences")
                                .when(self.folder_filter == FolderFilter::Differences, |button| {
                                    button.secondary()
                                })
                                .when(self.folder_filter != FolderFilter::Differences, |button| {
                                    button.ghost()
                                })
                                .label(format!("Diffs {different}"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.folder_filter = FolderFilter::Differences;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("folder-filter-identical")
                                .when(self.folder_filter == FolderFilter::Identical, |button| {
                                    button.secondary()
                                })
                                .when(self.folder_filter != FolderFilter::Identical, |button| {
                                    button.ghost()
                                })
                                .label(format!(
                                    "Same {}",
                                    self.folder_entries.len().saturating_sub(different)
                                ))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.folder_filter = FolderFilter::Identical;
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .bg(rgb(palette.panel))
                        .border_b_1()
                        .border_color(rgb(palette.border))
                        .child(folder_side_header(
                            &source_label(&self.left_path.read(cx).value(), None, "Left side"),
                            palette,
                        ))
                        .child(
                            div()
                                .w(px(92.))
                                .text_size(px(9.))
                                .text_color(rgb(palette.muted_text))
                                .child("STATUS"),
                        )
                        .child(folder_side_header(
                            &source_label(&self.right_path.read(cx).value(), None, "Right side"),
                            palette,
                        ))
                        .child(div().w(px(58.)).child("")),
                )
                .child(div().flex_1().min_h_0().overflow_y_scrollbar().child(list));
        }
        self.ensure_diff_editor();
        let query = self.search.read(cx).value().to_string().to_lowercase();
        let mut rows = div().v_flex().min_h(px(160.)).gap(px(1.));
        let mut visible_lines = 0;
        let row_count = self.diff_editor_row_count();
        let mut hunk_starts = vec![None; row_count];
        for hunk in change_hunks(&self.rows) {
            for row in hunk.start_row..hunk.end_row {
                hunk_starts[row] = Some(hunk.start_row);
            }
        }
        let mut shown_hunks = BTreeSet::new();
        if self.rows.is_empty() && !self.compared {
            return div()
                .h_flex()
                .min_h(px(160.))
                .items_center()
                .justify_center()
                .text_color(rgb(palette.secondary_text))
                .child("Your comparison will appear here.");
        }
        self.ensure_diff_inputs(window, cx);
        let left_label = if self.mode == Some(CompareMode::Text) {
            "LEFT TEXT".to_owned()
        } else {
            source_label(
                &self.left_path.read(cx).value(),
                self.selected_folder_file.as_deref(),
                "LEFT",
            )
        };
        let right_label = if self.merge_context.is_some() {
            "MERGED RESULT".to_owned()
        } else if self.mode == Some(CompareMode::Text) {
            "RIGHT TEXT".to_owned()
        } else {
            source_label(
                &self.right_path.read(cx).value(),
                self.selected_folder_file.as_deref(),
                "RIGHT",
            )
        };
        rows = rows.child(
            div()
                .h_flex()
                .items_center()
                .px_2()
                .h(px(22.))
                .bg(rgb(palette.toolbar))
                .text_size(px(9.))
                .text_color(rgb(palette.secondary_text))
                .child(div().flex_1().child(left_label))
                .child(div().w(px(82.)).justify_center().child("DIFF"))
                .child(div().flex_1().child(right_label)),
        );
        for index in 0..row_count {
            let line = self.rows.get(index);
            let line_kind = line.map(|line| line.kind).unwrap_or(DiffKind::Equal);
            let left_text = line.map(|line| line.left_text.as_str()).unwrap_or("");
            let right_text = line.map(|line| line.right_text.as_str()).unwrap_or("");
            if self.diff_filter == DiffFilter::Differences && line_kind == DiffKind::Equal
                || self.diff_filter == DiffFilter::Same && line_kind != DiffKind::Equal
            {
                continue;
            }
            if self
                .selected_bookmark
                .is_some_and(|selected| selected != index)
            {
                continue;
            }
            if self.selected_bookmark.is_none()
                && !query.is_empty()
                && !left_text.to_lowercase().contains(&query)
                && !right_text.to_lowercase().contains(&query)
            {
                continue;
            }
            visible_lines += 1;
            let first_visible_hunk_row =
                hunk_starts[index].is_some_and(|start| shown_hunks.insert(start));
            let (background, foreground, marker) = match line_kind {
                DiffKind::Equal => (palette.diff_equal, palette.foreground, ""),
                DiffKind::Changed => (palette.diff_changed, palette.diff_changed_text, "≠"),
                DiffKind::Added => (palette.diff_added, palette.diff_added_text, "+"),
                DiffKind::Removed => (palette.diff_removed, palette.diff_removed_text, "−"),
            };
            let left_number = self
                .diff_projection
                .source_line_for_row(DiffSide::Left, index)
                .map(|line| (line + 1).to_string())
                .unwrap_or_default();
            let right_number = self
                .diff_projection
                .source_line_for_row(DiffSide::Right, index)
                .map(|line| (line + 1).to_string())
                .unwrap_or_default();
            let left_input = self.left_diff_inputs[index].clone();
            let right_input = self.right_diff_inputs[index].clone();
            let bookmarked = self.bookmarks.contains(&index);
            rows = rows.child(
                div()
                    .h_flex()
                    .gap(px(1.))
                    .child(diff_cell(
                        &left_number,
                        &left_input,
                        background,
                        foreground,
                        self.editor_font_size,
                        palette,
                        cx.listener(move |this, event, window, cx| {
                            this.handle_diff_navigation(DiffSide::Left, index, event, window, cx)
                        }),
                    ))
                    .child(
                        div()
                            .w(px(82.))
                            .h_flex()
                            .items_center()
                            .justify_center()
                            .gap(px(1.))
                            .bg(rgb(palette.toolbar))
                            .child(
                                div()
                                    .w(px(12.))
                                    .text_size(px((self.editor_font_size - 1.).max(8.)))
                                    .text_color(rgb(foreground))
                                    .child(marker),
                            )
                            .when(first_visible_hunk_row, |gutter| {
                                gutter
                                    .child(
                                        Button::new(format!("copy-hunk-right-{index}"))
                                            .ghost()
                                            .compact()
                                            .with_size(ComponentSize::XSmall)
                                            .label("←")
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.copy_hunk(index, false, window, cx)
                                            })),
                                    )
                                    .child(
                                        Button::new(format!("copy-hunk-left-{index}"))
                                            .ghost()
                                            .compact()
                                            .with_size(ComponentSize::XSmall)
                                            .label("→")
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.copy_hunk(index, true, window, cx)
                                            })),
                                    )
                            })
                            .when(line_kind != DiffKind::Equal, |gutter| {
                                gutter.child(
                                    Button::new(format!("bookmark-{index}"))
                                        .ghost()
                                        .compact()
                                        .with_size(ComponentSize::XSmall)
                                        .label(if bookmarked { "★" } else { "☆" })
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if !this.bookmarks.insert(index) {
                                                this.bookmarks.remove(&index);
                                                if this.selected_bookmark == Some(index) {
                                                    this.selected_bookmark = None;
                                                }
                                            }
                                            cx.notify();
                                        })),
                                )
                            }),
                    )
                    .child(diff_cell(
                        &right_number,
                        &right_input,
                        background,
                        foreground,
                        self.editor_font_size,
                        palette,
                        cx.listener(move |this, event, window, cx| {
                            this.handle_diff_navigation(DiffSide::Right, index, event, window, cx)
                        }),
                    )),
            );
        }
        if visible_lines == 0 {
            return div()
                .h_flex()
                .min_h(px(160.))
                .items_center()
                .justify_center()
                .text_color(rgb(palette.secondary_text))
                .child("No lines match this filter or search.");
        }
        rows
    }
}

fn split_patterns(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|pattern| !pattern.is_empty())
        .map(str::to_owned)
        .collect()
}

fn side_has_unsaved_changes(contents: &str, original: &Option<String>) -> bool {
    match original {
        Some(original) => contents != original,
        None => !contents.is_empty(),
    }
}

fn opposite_side(side: DiffSide) -> DiffSide {
    match side {
        DiffSide::Left => DiffSide::Right,
        DiffSide::Right => DiffSide::Left,
    }
}

fn build_app_menus() -> Vec<Menu> {
    vec![
        Menu::new("File").items([
            MenuItem::action("New session", NewSession),
            MenuItem::action("Home", ShowHome),
            MenuItem::separator(),
            MenuItem::action("Compare sources", RunComparison),
            MenuItem::action("Save left source", SaveLeft),
            MenuItem::action("Save right source", SaveRight),
        ]),
        Menu::new("Edit").items([
            MenuItem::action("Undo diff edit", UndoDiffEdit),
            MenuItem::action("Redo diff edit", RedoDiffEdit),
            MenuItem::separator(),
            MenuItem::action("Discard unsaved source edits", DiscardSources),
        ]),
        Menu::new("Compare").items([
            MenuItem::action("Compare now", RunComparison),
            MenuItem::separator(),
            MenuItem::action("Ignore whitespace", ToggleWhitespace),
            MenuItem::action("Ignore case", ToggleCase),
            MenuItem::action("Ignore comment lines", ToggleComments),
            MenuItem::separator(),
            MenuItem::action("Sync left to right", SyncLeftToRight),
            MenuItem::action("Sync right to left", SyncRightToLeft),
            MenuItem::action("Overwrite right with entire left file", CopyLeftToRight),
            MenuItem::action("Overwrite left with entire right file", CopyRightToLeft),
        ]),
        Menu::new("View").items([
            MenuItem::action("Toggle three-way source context", ToggleMergeContext),
            MenuItem::action("Toggle advanced options", ToggleAdvanced),
            MenuItem::separator(),
            MenuItem::action("Dark theme", SetDarkTheme),
            MenuItem::action("Light theme", SetLightTheme),
            MenuItem::separator(),
            MenuItem::action("Decrease editor font size", DecreaseFontSize),
            MenuItem::action("Increase editor font size", IncreaseFontSize),
            MenuItem::action("Reset editor font size", ResetFontSize),
        ]),
        Menu::new("Help").items([MenuItem::action("About Checker", ShowAbout)]),
    ]
}

pub(crate) fn run() {
    let (theme, startup_status) = match AppTheme::load_preference() {
        Ok(Some(theme)) => (theme, None),
        Ok(None) => (AppTheme::Dark, None),
        Err(error) => {
            eprintln!("Could not load theme preference; using Dark theme: {error}");
            (
                AppTheme::Dark,
                Some(format!(
                    "Could not load theme preference; using Dark theme: {error}"
                )),
            )
        }
    };
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            gpui_kit::component::Theme::change(
                match theme {
                    AppTheme::Dark => gpui_kit::component::ThemeMode::Dark,
                    AppTheme::Light => gpui_kit::component::ThemeMode::Light,
                },
                None,
                cx,
            );
            cx.set_menus(build_app_menus());
            GlobalState::global_mut(cx)
                .set_app_menus(build_app_menus().into_iter().map(Menu::owned).collect());
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::centered(size(px(1220.), px(900.)), cx)),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| Checker::new(window, cx, theme, startup_status))
            })
            .expect("failed to open Checker window");
        });
}
