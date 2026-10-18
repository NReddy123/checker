mod diff;
mod folder;

use diff::{CompareOptions, DiffKind, DiffLine, compare_text_with_options, merge_three_way};
use folder::{FolderEntry, FolderStatus, compare_folders, synchronize_folders, write_folder_file};
use gpui_kit::component::button::*;
use gpui_kit::component::{
    input::{Input, InputState, Textarea, TextareaState},
    scroll::ScrollableElement as _,
    *,
};
use gpui_kit::*;
use similar::{ChangeTag, TextDiff};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

struct Checker {
    left_path: Entity<InputState>,
    right_path: Entity<InputState>,
    base_path: Entity<InputState>,
    search: Entity<InputState>,
    include_patterns: Entity<InputState>,
    ignore_patterns: Entity<InputState>,
    editor: Entity<TextareaState>,
    rows: Vec<DiffLine>,
    folder_entries: Vec<FolderEntry>,
    left_content: String,
    right_content: String,
    folder_mode: bool,
    selected_folder_file: Option<String>,
    compared: bool,
    ignore_whitespace: bool,
    ignore_case: bool,
    ignore_comments: bool,
    bookmarks: BTreeSet<usize>,
    bookmark_cursor: usize,
    selected_bookmark: Option<usize>,
    status: String,
}

impl Checker {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
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
            editor: cx.new(|cx| TextareaState::new(window, cx)),
            rows: Vec::new(),
            folder_entries: Vec::new(),
            left_content: String::new(),
            right_content: String::new(),
            folder_mode: false,
            selected_folder_file: None,
            compared: false,
            ignore_whitespace: false,
            ignore_case: false,
            ignore_comments: false,
            bookmarks: BTreeSet::new(),
            bookmark_cursor: 0,
            selected_bookmark: None,
            status: "Compare two text files or folders.".into(),
        }
    }

    fn compare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let left_path = self.left_path.read(cx).value().to_string();
        let right_path = self.right_path.read(cx).value().to_string();
        if left_path.trim().is_empty() || right_path.trim().is_empty() {
            self.set_error("Enter both comparison paths.".into());
            cx.notify();
            return;
        }
        let left = PathBuf::from(&left_path);
        let right = PathBuf::from(&right_path);
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
                        self.rows.clear();
                        self.folder_mode = true;
                        self.selected_folder_file = None;
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
                self.rows = compare_text_with_options(
                    &self.left_content,
                    &self.right_content,
                    CompareOptions {
                        ignore_whitespace: self.ignore_whitespace,
                        ignore_case: self.ignore_case,
                        ignore_comments: self.ignore_comments,
                    },
                );
                let base_path = self.base_path.read(cx).value().to_string();
                if !base_path.trim().is_empty() {
                    match fs::read_to_string(&base_path) {
                        Ok(base) => {
                            let (merged, conflicts) =
                                merge_three_way(&base, &self.left_content, &self.right_content);
                            self.editor
                                .update(cx, |state, cx| state.set_value(merged, window, cx));
                            self.status = format!(
                                "3-way merge prepared · {conflicts} conflict(s); review the editable result below"
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
                    let right_content = self.right_content.clone();
                    self.editor
                        .update(cx, |state, cx| state.set_value(right_content, window, cx));
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
                self.bookmarks.clear();
                self.bookmark_cursor = 0;
                self.selected_bookmark = None;
            }
            _ => self.set_error("Both paths must be files or both must be folders.".into()),
        }
        cx.notify();
    }

    fn set_error(&mut self, message: String) {
        self.rows.clear();
        self.folder_entries.clear();
        self.folder_mode = false;
        self.selected_folder_file = None;
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
        }
        cx.notify();
    }

    fn sync_folders(&mut self, left_to_right: bool, cx: &mut Context<Self>) {
        if !self.folder_mode {
            self.status = "Enter and compare two folders before synchronizing.".into();
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

    fn copy_file(&mut self, left_to_right: bool, cx: &mut Context<Self>) {
        if self.folder_mode && self.selected_folder_file.is_none() {
            self.status = "Open a folder text file before copying file contents.".into();
            cx.notify();
            return;
        }
        let left_path = PathBuf::from(self.left_path.read(cx).value().to_string());
        let right_path = PathBuf::from(self.right_path.read(cx).value().to_string());
        let (contents, destination) = if left_to_right {
            (&self.left_content, right_path)
        } else {
            (&self.right_content, left_path)
        };
        let result = match &self.selected_folder_file {
            Some(relative) => write_folder_file(&destination, relative, contents),
            None => fs::write(destination, contents),
        };
        match result {
            Ok(()) => self.status = "Copied the full file. Compare again to refresh.".into(),
            Err(error) => self.status = format!("Could not copy file: {error}"),
        }
        cx.notify();
    }

    fn save_editor(&mut self, cx: &mut Context<Self>) {
        if self.folder_mode && self.selected_folder_file.is_none() {
            self.status = "Open a folder text file before saving the editor.".into();
            cx.notify();
            return;
        }
        let root = PathBuf::from(self.right_path.read(cx).value().to_string());
        let contents = self.editor.read(cx).value().to_string();
        let result = match &self.selected_folder_file {
            Some(relative) => write_folder_file(&root, relative, &contents),
            None => fs::write(root, contents),
        };
        match result {
            Ok(()) => self.status = "Saved the edited/merged text to the right-side file.".into(),
            Err(error) => self.status = format!("Could not save edited text: {error}"),
        }
        cx.notify();
    }

    fn load_editor(&mut self, left: bool, window: &mut Window, cx: &mut Context<Self>) {
        let contents = if left {
            self.left_content.clone()
        } else {
            self.right_content.clone()
        };
        self.editor
            .update(cx, |state, cx| state.set_value(contents, window, cx));
        cx.notify();
    }

    fn open_folder_file(&mut self, relative: String, window: &mut Window, cx: &mut Context<Self>) {
        let left_path = PathBuf::from(self.left_path.read(cx).value().to_string()).join(&relative);
        let right_path =
            PathBuf::from(self.right_path.read(cx).value().to_string()).join(&relative);
        let left = if left_path.exists() {
            match fs::read_to_string(&left_path) {
                Ok(contents) => contents,
                Err(error) => {
                    self.status =
                        format!("Could not open {} as text: {error}", left_path.display());
                    cx.notify();
                    return;
                }
            }
        } else {
            String::new()
        };
        let right = if right_path.exists() {
            match fs::read_to_string(&right_path) {
                Ok(contents) => contents,
                Err(error) => {
                    self.status =
                        format!("Could not open {} as text: {error}", right_path.display());
                    cx.notify();
                    return;
                }
            }
        } else {
            String::new()
        };
        self.left_content = left;
        self.right_content = right;
        self.rows = compare_text_with_options(
            &self.left_content,
            &self.right_content,
            CompareOptions {
                ignore_whitespace: self.ignore_whitespace,
                ignore_case: self.ignore_case,
                ignore_comments: self.ignore_comments,
            },
        );
        let right_content = self.right_content.clone();
        self.editor
            .update(cx, |state, cx| state.set_value(right_content, window, cx));
        self.selected_folder_file = Some(relative.clone());
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
}

impl Render for Checker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .v_flex()
            .size_full()
            .bg(rgb(0x101318))
            .text_color(rgb(0xe7eaf0))
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .px_8()
                    .py_4()
                    .border_b_1()
                    .border_color(rgb(0x272c34))
                    .child(
                        div()
                            .v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(22.))
                                    .font_weight(FontWeight::BOLD)
                                    .child("Checker"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(0x929baa))
                                    .child("Text and recursive folder comparison"),
                            ),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .rounded_full()
                            .bg(rgb(0x1c222a))
                            .text_size(px(11.))
                            .text_color(rgb(0x9ea8b7))
                            .child(if self.folder_mode { "FOLDER DIFF" } else { "TEXT DIFF" }),
                    ),
            )
            .child(
                div()
                    .v_flex()
                    .gap_3()
                    .p_5()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .h_flex()
                            .gap_3()
                            .child(path_input("01 · Original", &self.left_path))
                            .child(path_input("02 · Modified", &self.right_path)),
                    )
                    .child(Input::new(&self.base_path))
                    .child(
                        div()
                            .h_flex()
                            .gap_2()
                            .child(Input::new(&self.include_patterns))
                            .child(Input::new(&self.ignore_patterns)),
                    )
                    .child(
                        div()
                            .h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                Button::new("compare")
                                    .primary()
                                    .label("Compare")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.compare(window, cx)
                                    })),
                            )
                            .child(
                                Button::new("ignore-whitespace")
                                    .label(if self.ignore_whitespace {
                                        "✓ Ignore whitespace"
                                    } else {
                                        "Ignore whitespace"
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.toggle_rule(true, cx)
                                    })),
                            )
                            .child(
                                Button::new("ignore-case")
                                    .label(if self.ignore_case {
                                        "✓ Ignore case"
                                    } else {
                                        "Ignore case"
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.toggle_rule(false, cx)
                                    })),
                            )
                            .child(
                                Button::new("ignore-comments")
                                    .label(if self.ignore_comments {
                                        "✓ Ignore comments"
                                    } else {
                                        "Ignore comments"
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.toggle_comments(cx)
                                    })),
                            )
                            .child(Input::new(&self.search))
                            .child(
                                Button::new("bookmark-previous")
                                    .label("Previous bookmark")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.move_bookmark(false, cx)
                                    })),
                            )
                            .child(
                                Button::new("bookmark-next")
                                    .label("Next bookmark")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.move_bookmark(true, cx)
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .gap_2()
                            .child(
                                Button::new("sync-left-to-right")
                                    .label("Sync left → right")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.sync_folders(true, cx)
                                    })),
                            )
                            .child(
                                Button::new("sync-right-to-left")
                                    .label("Sync right → left")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.sync_folders(false, cx)
                                    })),
                            )
                            .child(
                                Button::new("copy-left-to-right")
                                    .label("Copy file left → right")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.copy_file(true, cx)
                                    })),
                            )
                            .child(
                                Button::new("copy-right-to-left")
                                    .label("Copy file right → left")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.copy_file(false, cx)
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(0xd1a96b))
                            .child(self.status.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scrollbar()
                            .child(self.render_results(cx)),
                    )
                    .child(
                        div()
                            .v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(0x929baa))
                                    .child("Editable result / merge preview (save writes to the right-side file)"),
                            )
                            .child(Textarea::new(&self.editor).h(px(120.)))
                            .child(
                                div()
                                    .h_flex()
                                    .gap_2()
                                    .child(
                                        Button::new("load-left")
                                            .label("Load left into editor")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.load_editor(true, window, cx)
                                            })),
                                    )
                                    .child(
                                        Button::new("load-right")
                                            .label("Load right into editor")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.load_editor(false, window, cx)
                                            })),
                                    )
                                    .child(
                                        Button::new("save-editor")
                                            .primary()
                                            .label("Save editor to right file")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.save_editor(cx)
                                            })),
                                    ),
                            ),
                    ),
            )
    }
}

impl Checker {
    fn render_results(&self, cx: &mut Context<Self>) -> impl IntoElement {
        if self.folder_mode {
            let mut list = div().v_flex().gap(px(1.));
            let query = self.search.read(cx).value().to_string().to_lowercase();
            for (index, entry) in self.folder_entries.iter().enumerate() {
                if !query.is_empty() && !entry.relative_path.to_lowercase().contains(&query) {
                    continue;
                }
                let (label, color) = match entry.status {
                    FolderStatus::LeftOnly => ("Left only", 0xd1a96b),
                    FolderStatus::RightOnly => ("Right only", 0x73a7ff),
                    FolderStatus::Different => ("Different", 0xe58e8e),
                    FolderStatus::Identical => ("Identical", 0x56d6ad),
                    FolderStatus::Unreadable => ("Unreadable", 0xf07178),
                };
                list = list.child(
                    div()
                        .h_flex()
                        .gap_3()
                        .px_3()
                        .py_1()
                        .bg(rgb(0x171b21))
                        .child(div().flex_1().child(entry.relative_path.clone()))
                        .child(div().text_color(rgb(color)).child(label)),
                );
                list = list.child(
                    Button::new(format!("open-folder-file-{index}"))
                        .label("Open text")
                        .on_click(cx.listener({
                            let relative = entry.relative_path.clone();
                            move |this, _, window, cx| {
                                this.open_folder_file(relative.clone(), window, cx)
                            }
                        })),
                );
            }
            let mut result = div().v_flex().gap_2().child(list);
            if self.selected_folder_file.is_some() {
                result = result.child(self.render_folder_detail(cx));
            }
            return result;
        }
        let query = self.search.read(cx).value().to_string().to_lowercase();
        let mut rows = div().v_flex().min_h(px(160.)).gap(px(1.));
        if self.rows.is_empty() {
            return div()
                .h_flex()
                .min_h(px(160.))
                .items_center()
                .justify_center()
                .text_color(rgb(0x778190))
                .child(if self.compared {
                    "No line changes to show."
                } else {
                    "Your comparison will appear here."
                });
        }
        for (index, line) in self.rows.iter().enumerate() {
            if self
                .selected_bookmark
                .is_some_and(|selected| selected != index)
            {
                continue;
            }
            if self.selected_bookmark.is_none()
                && !query.is_empty()
                && !line.left_text.to_lowercase().contains(&query)
                && !line.right_text.to_lowercase().contains(&query)
            {
                continue;
            }
            let (background, foreground, marker) = match line.kind {
                DiffKind::Equal => (0x171b21, 0xc8ced8, " "),
                DiffKind::Changed => (0x3a3022, 0xf2d19a, "±"),
                DiffKind::Added => (0x183027, 0xa9ebcb, "+"),
                DiffKind::Removed => (0x382326, 0xf0b1b1, "−"),
            };
            let left_number = line.left_number.map(|n| n.to_string()).unwrap_or_default();
            let right_number = line.right_number.map(|n| n.to_string()).unwrap_or_default();
            let left_text = line.left_text.clone();
            let right_text = line.right_text.clone();
            let (left_comparison, right_comparison) = if line.kind == DiffKind::Changed {
                (line.right_text.clone(), line.left_text.clone())
            } else {
                (left_text.clone(), right_text.clone())
            };
            let bookmarked = self.bookmarks.contains(&index);
            rows = rows.child(
                div()
                    .h_flex()
                    .gap(px(1.))
                    .child(diff_cell(
                        &left_number,
                        &left_text,
                        &left_comparison,
                        marker,
                        background,
                        foreground,
                        true,
                    ))
                    .child(diff_cell(
                        &right_number,
                        &right_text,
                        &right_comparison,
                        marker,
                        background,
                        foreground,
                        false,
                    ))
                    .child(
                        Button::new(format!("bookmark-{index}"))
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
                    ),
            );
        }
        div()
            .v_flex()
            .gap_2()
            .child(
                div()
                    .h_flex()
                    .px_3()
                    .py_2()
                    .text_size(px(11.))
                    .text_color(rgb(0x929baa))
                    .child(div().flex_1().child("ORIGINAL"))
                    .child(div().flex_1().child("MODIFIED"))
                    .child(div().w(px(40.))),
            )
            .child(rows)
    }

    fn render_folder_detail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.search.read(cx).value().to_string().to_lowercase();
        let mut rows = div().v_flex().gap(px(1.));
        for line in &self.rows {
            if !query.is_empty()
                && !line.left_text.to_lowercase().contains(&query)
                && !line.right_text.to_lowercase().contains(&query)
            {
                continue;
            }
            let (background, foreground, marker) = match line.kind {
                DiffKind::Equal => (0x171b21, 0xc8ced8, " "),
                DiffKind::Changed => (0x3a3022, 0xf2d19a, "±"),
                DiffKind::Added => (0x183027, 0xa9ebcb, "+"),
                DiffKind::Removed => (0x382326, 0xf0b1b1, "−"),
            };
            rows = rows.child(
                div()
                    .h_flex()
                    .gap(px(1.))
                    .child(diff_cell(
                        &line.left_number.map(|n| n.to_string()).unwrap_or_default(),
                        &line.left_text,
                        &line.right_text,
                        marker,
                        background,
                        foreground,
                        true,
                    ))
                    .child(diff_cell(
                        &line.right_number.map(|n| n.to_string()).unwrap_or_default(),
                        &line.right_text,
                        &line.left_text,
                        marker,
                        background,
                        foreground,
                        false,
                    )),
            );
        }
        div()
            .v_flex()
            .gap_2()
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0x929baa))
                    .child(self.selected_folder_file.clone().unwrap_or_default()),
            )
            .child(rows)
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

fn path_input(title: &str, state: &Entity<InputState>) -> impl IntoElement {
    div()
        .v_flex()
        .gap_2()
        .flex_1()
        .p_3()
        .rounded_lg()
        .bg(rgb(0x171b21))
        .border_1()
        .border_color(rgb(0x2b313a))
        .child(div().text_size(px(12.)).child(title.to_owned()))
        .child(Input::new(state))
}

fn diff_cell(
    number: &str,
    text: &str,
    counterpart: &str,
    marker: &str,
    background: u32,
    foreground: u32,
    left_side: bool,
) -> impl IntoElement {
    let mut cell = div()
        .h_flex()
        .flex_1()
        .min_w(px(0.))
        .px_2()
        .py_1()
        .gap_2()
        .bg(rgb(background))
        .child(
            div()
                .w(px(34.))
                .text_color(rgb(0x778190))
                .text_size(px(11.))
                .child(number.to_owned()),
        )
        .child(
            div()
                .w(px(12.))
                .text_color(rgb(foreground))
                .text_size(px(12.))
                .child(marker.to_owned()),
        );
    if text.is_empty() {
        cell = cell.child(div().flex_1().text_size(px(12.)).child(" "));
        return cell;
    }
    cell.child(highlighted_text(text, counterpart, foreground, left_side))
}

fn highlighted_text(
    text: &str,
    counterpart: &str,
    base_color: u32,
    left_side: bool,
) -> impl IntoElement {
    let mut changed = Vec::new();
    for change in TextDiff::from_chars(text, counterpart).iter_all_changes() {
        let visible = match change.tag() {
            ChangeTag::Insert => false,
            _ => true,
        };
        let is_changed = change.tag() != ChangeTag::Equal;
        changed.extend(
            change
                .value()
                .chars()
                .filter(|_| visible)
                .map(|c| (c, is_changed)),
        );
    }
    let syntax_colors = syntax_colors_for(text, base_color);
    let mut rendered = div().h_flex().flex_1().text_size(px(12.));
    let mut buffer = String::new();
    let mut active_color = None;
    for (index, (character, is_changed)) in changed.into_iter().enumerate() {
        let color = if is_changed {
            if left_side { 0xffc2a1 } else { 0xa7efc8 }
        } else {
            syntax_colors.get(index).copied().unwrap_or(base_color)
        };
        if active_color.is_some_and(|active| active != color) {
            let previous = active_color.unwrap();
            rendered = rendered.child(
                div()
                    .text_color(rgb(previous))
                    .child(std::mem::take(&mut buffer)),
            );
        }
        active_color = Some(color);
        buffer.push(character);
    }
    if let Some(color) = active_color {
        rendered = rendered.child(div().text_color(rgb(color)).child(buffer));
    }
    rendered
}

fn syntax_colors_for(text: &str, base_color: u32) -> Vec<u32> {
    let characters: Vec<char> = text.chars().collect();
    let mut colors = vec![base_color; characters.len()];
    if text.trim_start().starts_with("//")
        || text.trim_start().starts_with('#')
        || text.trim_start().starts_with("--")
    {
        colors.fill(0x82909e);
        return colors;
    }
    let keywords = [
        "as", "async", "await", "break", "class", "const", "continue", "def", "else", "enum",
        "false", "fn", "for", "func", "if", "import", "in", "let", "match", "mut", "new", "None",
        "null", "pub", "return", "self", "static", "struct", "True", "true", "type", "use", "var",
        "while",
    ];
    let mut index = 0;
    while index < characters.len() {
        if characters[index] == '"' || characters[index] == '\'' {
            let quote = characters[index];
            let start = index;
            index += 1;
            while index < characters.len() {
                if characters[index] == '\\' {
                    index = (index + 2).min(characters.len());
                } else if characters[index] == quote {
                    index += 1;
                    break;
                } else {
                    index += 1;
                }
            }
            colors[start..index].fill(0xd6a8f5);
        } else if characters[index..].starts_with(&['/', '/'])
            || characters[index] == '#'
            || characters[index..].starts_with(&['-', '-'])
        {
            colors[index..].fill(0x82909e);
            break;
        } else if characters[index].is_alphanumeric() || characters[index] == '_' {
            let start = index;
            while index < characters.len()
                && (characters[index].is_alphanumeric() || characters[index] == '_')
            {
                index += 1;
            }
            let word: String = characters[start..index].iter().collect();
            if keywords.contains(&word.as_str()) {
                colors[start..index].fill(0x83baff);
            } else if word.chars().all(|character| character.is_ascii_digit()) {
                colors[start..index].fill(0xe9c778);
            }
        } else {
            index += 1;
        }
    }
    colors
}

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::centered(size(px(1220.), px(900.)), cx)),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| Checker::new(window, cx))
            })
            .expect("failed to open Checker window");
        });
}
