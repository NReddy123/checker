use crate::diff::{DiffLine, DiffProjection};
use crate::folder::FolderEntry;
use std::collections::BTreeSet;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MainPanel {
    Diff,
    TextInput,
}

#[derive(Clone)]
pub(crate) struct MergeContext {
    pub(crate) base: String,
    pub(crate) left: String,
    pub(crate) right: String,
    pub(crate) conflicts: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FolderFilter {
    All,
    Differences,
    Identical,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiffFilter {
    All,
    Differences,
    Same,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompareMode {
    Folder,
    File,
    Text,
}

#[derive(Clone)]
pub(crate) struct CompareSession {
    pub(crate) name: String,
    pub(crate) show_home: bool,
    pub(crate) mode: Option<CompareMode>,
    pub(crate) left_path: String,
    pub(crate) right_path: String,
    pub(crate) base_path: String,
    pub(crate) search: String,
    pub(crate) include_patterns: String,
    pub(crate) ignore_patterns: String,
    pub(crate) left_text: String,
    pub(crate) right_text: String,
    pub(crate) rows: Arc<[DiffLine]>,
    pub(crate) diff_projection: Arc<DiffProjection>,
    pub(crate) folder_entries: Vec<FolderEntry>,
    pub(crate) left_content: String,
    pub(crate) right_content: String,
    pub(crate) left_original: Option<String>,
    pub(crate) right_original: Option<String>,
    pub(crate) base_content: String,
    pub(crate) merge_context: Option<MergeContext>,
    pub(crate) show_merge_context: bool,
    pub(crate) folder_mode: bool,
    pub(crate) selected_folder_file: Option<String>,
    pub(crate) compared: bool,
    pub(crate) ignore_whitespace: bool,
    pub(crate) ignore_case: bool,
    pub(crate) ignore_comments: bool,
    pub(crate) bookmarks: BTreeSet<usize>,
    pub(crate) bookmark_cursor: usize,
    pub(crate) selected_bookmark: Option<usize>,
    pub(crate) active_panel: MainPanel,
    pub(crate) show_advanced: bool,
    pub(crate) folder_filter: FolderFilter,
    pub(crate) diff_filter: DiffFilter,
    pub(crate) status: String,
}

impl CompareSession {
    pub(crate) fn blank(number: usize) -> Self {
        Self {
            name: format!("Session {number}"),
            show_home: true,
            mode: None,
            left_path: String::new(),
            right_path: String::new(),
            base_path: String::new(),
            search: String::new(),
            include_patterns: String::new(),
            ignore_patterns: String::new(),
            left_text: String::new(),
            right_text: String::new(),
            rows: Arc::from([]),
            diff_projection: Arc::new(DiffProjection::default()),
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
            status: "Choose a comparison type to get started.".into(),
        }
    }

    pub(crate) fn has_unsaved_file_changes(&self) -> bool {
        if self.mode == Some(CompareMode::Text) {
            return false;
        }
        side_has_unsaved_changes(&self.left_content, &self.left_original)
            || side_has_unsaved_changes(&self.right_content, &self.right_original)
    }
}

fn side_has_unsaved_changes(content: &str, original: &Option<String>) -> bool {
    match original {
        Some(original) => content != original,
        None => !content.is_empty(),
    }
}
