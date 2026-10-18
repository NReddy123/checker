use similar::{ChangeTag, TextDiff};
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub left_number: Option<usize>,
    pub right_number: Option<usize>,
    pub left_span: Option<SourceSpan>,
    pub right_span: Option<SourceSpan>,
    pub text: String,
    pub left_text: String,
    pub right_text: String,
    pub kind: DiffKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSpan {
    pub content: Range<usize>,
    pub full_line: Range<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiffSide {
    Left,
    Right,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DiffProjection {
    left_by_row: Vec<Option<usize>>,
    right_by_row: Vec<Option<usize>>,
    left_row_by_line: Vec<Option<usize>>,
    right_row_by_line: Vec<Option<usize>>,
}

impl DiffProjection {
    pub(crate) fn from_rows(rows: &[DiffLine]) -> Self {
        let left_line_count = rows
            .iter()
            .filter_map(|row| row.left_number)
            .max()
            .unwrap_or_default();
        let right_line_count = rows
            .iter()
            .filter_map(|row| row.right_number)
            .max()
            .unwrap_or_default();
        let mut projection = Self {
            left_by_row: Vec::with_capacity(rows.len()),
            right_by_row: Vec::with_capacity(rows.len()),
            left_row_by_line: vec![None; left_line_count],
            right_row_by_line: vec![None; right_line_count],
        };

        for (row_index, row) in rows.iter().enumerate() {
            let left_line = row.left_number.map(|number| number - 1);
            let right_line = row.right_number.map(|number| number - 1);
            if let Some(line) = left_line {
                projection.left_row_by_line[line] = Some(row_index);
            }
            if let Some(line) = right_line {
                projection.right_row_by_line[line] = Some(row_index);
            }
            projection.left_by_row.push(left_line);
            projection.right_by_row.push(right_line);
        }
        projection
    }

    pub(crate) fn row_count(&self) -> usize {
        self.left_by_row.len()
    }

    pub(crate) fn source_line_for_row(&self, side: DiffSide, row: usize) -> Option<usize> {
        match side {
            DiffSide::Left => self.left_by_row.get(row).copied().flatten(),
            DiffSide::Right => self.right_by_row.get(row).copied().flatten(),
        }
    }

    pub(crate) fn display_row_for_source_line(&self, side: DiffSide, line: usize) -> Option<usize> {
        match side {
            DiffSide::Left => self.left_row_by_line.get(line).copied().flatten(),
            DiffSide::Right => self.right_row_by_line.get(line).copied().flatten(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChangeHunk {
    pub(crate) start_row: usize,
    pub(crate) end_row: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffKind {
    Equal,
    Changed,
    Added,
    Removed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompareOptions {
    pub ignore_whitespace: bool,
    pub ignore_case: bool,
    pub ignore_comments: bool,
}

#[cfg(test)]
pub fn compare_text(left: &str, right: &str) -> Vec<DiffLine> {
    compare_text_with_options(left, right, CompareOptions::default())
}

pub fn compare_text_with_options(
    left: &str,
    right: &str,
    options: CompareOptions,
) -> Vec<DiffLine> {
    let left_lines = lines(left);
    let right_lines = lines(right);
    let left_spans = source_spans(left);
    let right_spans = source_spans(right);
    let left_keys: Vec<_> = left_lines
        .iter()
        .map(|line| comparison_key(line, options))
        .collect();
    let right_keys: Vec<_> = right_lines
        .iter()
        .map(|line| comparison_key(line, options))
        .collect();
    let left_refs: Vec<_> = left_keys.iter().map(String::as_str).collect();
    let right_refs: Vec<_> = right_keys.iter().map(String::as_str).collect();

    let raw: Vec<_> = TextDiff::from_slices(&left_refs, &right_refs)
        .iter_all_changes()
        .map(|change| {
            let left_index = change.old_index();
            let right_index = change.new_index();
            let left_text = left_index
                .and_then(|index| left_lines.get(index))
                .cloned()
                .unwrap_or_default();
            let right_text = right_index
                .and_then(|index| right_lines.get(index))
                .cloned()
                .unwrap_or_default();
            let left_span = left_index.and_then(|index| left_spans.get(index)).cloned();
            let right_span = right_index
                .and_then(|index| right_spans.get(index))
                .cloned();
            let kind = match change.tag() {
                ChangeTag::Equal => DiffKind::Equal,
                ChangeTag::Insert => DiffKind::Added,
                ChangeTag::Delete => DiffKind::Removed,
            };
            DiffLine {
                left_number: left_index.map(|index| index + 1),
                right_number: right_index.map(|index| index + 1),
                left_span,
                right_span,
                text: if left_index.is_some() {
                    left_text.clone()
                } else {
                    right_text.clone()
                },
                left_text,
                right_text,
                kind,
            }
        })
        .collect();

    let mut paired = Vec::with_capacity(raw.len());
    let mut index = 0;
    while index < raw.len() {
        if raw[index].kind == DiffKind::Equal {
            paired.push(raw[index].clone());
            index += 1;
            continue;
        }
        let start = index;
        while index < raw.len() && raw[index].kind != DiffKind::Equal {
            index += 1;
        }
        let deletions: Vec<_> = raw[start..index]
            .iter()
            .filter(|line| line.kind == DiffKind::Removed)
            .cloned()
            .collect();
        let additions: Vec<_> = raw[start..index]
            .iter()
            .filter(|line| line.kind == DiffKind::Added)
            .cloned()
            .collect();
        let shared = deletions.len().min(additions.len());
        for (removed, added) in deletions.iter().zip(&additions).take(shared) {
            paired.push(DiffLine {
                left_number: removed.left_number,
                right_number: added.right_number,
                left_span: removed.left_span.clone(),
                right_span: added.right_span.clone(),
                text: removed.left_text.clone(),
                left_text: removed.left_text.clone(),
                right_text: added.right_text.clone(),
                kind: DiffKind::Changed,
            });
        }
        paired.extend(deletions.into_iter().skip(shared));
        paired.extend(additions.into_iter().skip(shared));
    }
    paired
}

pub(crate) fn change_hunks(rows: &[DiffLine]) -> Vec<ChangeHunk> {
    let mut hunks = Vec::new();
    let mut row = 0;
    while row < rows.len() {
        if rows[row].kind == DiffKind::Equal {
            row += 1;
            continue;
        }
        let start_row = row;
        while row < rows.len() && rows[row].kind != DiffKind::Equal {
            row += 1;
        }
        hunks.push(ChangeHunk {
            start_row,
            end_row: row,
        });
    }
    hunks
}

pub(crate) fn apply_hunk_copy(
    left: &str,
    right: &str,
    rows: &[DiffLine],
    projection: &DiffProjection,
    row_index: usize,
    left_to_right: bool,
) -> Option<String> {
    let hunk = change_hunks(rows)
        .into_iter()
        .find(|hunk| (hunk.start_row..hunk.end_row).contains(&row_index))?;

    let (source_text, destination_text) = if left_to_right {
        (left, right)
    } else {
        (right, left)
    };
    let (source_side, destination_side) = if left_to_right {
        (DiffSide::Left, DiffSide::Right)
    } else {
        (DiffSide::Right, DiffSide::Left)
    };
    let source_file_lines = split_lines(source_text);
    let mut source_lines = Vec::new();
    let mut destination_start = None;
    let mut destination_line_count = 0;
    for aligned_row in hunk.start_row..hunk.end_row {
        if let Some(line) = projection.source_line_for_row(source_side, aligned_row) {
            source_lines.push(*source_file_lines.get(line)?);
        }
        if let Some(line) = projection.source_line_for_row(destination_side, aligned_row) {
            destination_start.get_or_insert(line);
            destination_line_count += 1;
        }
    }
    let destination_start = destination_start.unwrap_or_else(|| {
        (0..hunk.start_row)
            .rev()
            .find_map(|row| {
                projection
                    .source_line_for_row(destination_side, row)
                    .map(|line| line + 1)
            })
            .unwrap_or_default()
    });
    let mut destination_lines = split_lines(destination_text);
    let end = destination_start
        .saturating_add(destination_line_count)
        .min(destination_lines.len());
    if destination_start > end {
        return None;
    }
    destination_lines.splice(destination_start..end, source_lines);

    let line_ending = line_ending(destination_text)
        .or_else(|| line_ending(source_text))
        .unwrap_or("\n");
    let preserve_trailing_newline = if destination_text.is_empty() {
        source_text.ends_with('\n')
    } else {
        destination_text.ends_with('\n')
    };
    let mut updated = destination_lines.join(line_ending);
    if preserve_trailing_newline && !destination_lines.is_empty() {
        updated.push_str(line_ending);
    }
    Some(updated)
}

fn split_lines(text: &str) -> Vec<&str> {
    text.split_terminator('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect()
}

fn line_ending(text: &str) -> Option<&'static str> {
    text.find('\n').map(|newline| {
        if newline > 0 && text.as_bytes()[newline - 1] == b'\r' {
            "\r\n"
        } else {
            "\n"
        }
    })
}

fn lines(text: &str) -> Vec<String> {
    text.split_inclusive('\n')
        .map(|line| line.trim_end_matches(['\r', '\n']).to_owned())
        .collect()
}

fn source_spans(text: &str) -> Vec<SourceSpan> {
    let mut start = 0;
    text.split_inclusive('\n')
        .map(|line| {
            let full_line = start..start + line.len();
            let content = start..start + line.trim_end_matches(['\r', '\n']).len();
            start = full_line.end;
            SourceSpan { content, full_line }
        })
        .collect()
}

fn comparison_key(line: &str, options: CompareOptions) -> String {
    let mut key = line.to_owned();
    if options.ignore_comments {
        let trimmed = key.trim_start();
        if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with("/*") {
            return String::new();
        }
    }
    if options.ignore_whitespace {
        key.retain(|character| !character.is_whitespace());
    }
    if options.ignore_case {
        key = key.to_lowercase();
    }
    key
}

pub fn merge_three_way(base: &str, left: &str, right: &str) -> (String, usize) {
    if left == right {
        return (left.to_owned(), 0);
    }
    if left == base {
        return (right.to_owned(), 0);
    }
    if right == base {
        return (left.to_owned(), 0);
    }

    #[derive(Clone)]
    struct Edit {
        start: usize,
        end: usize,
        replacement: Vec<String>,
    }
    fn edits(base: &[String], variant: &[String]) -> Vec<Edit> {
        let base_refs: Vec<_> = base.iter().map(String::as_str).collect();
        let variant_refs: Vec<_> = variant.iter().map(String::as_str).collect();
        TextDiff::from_slices(&base_refs, &variant_refs)
            .ops()
            .iter()
            .filter(|operation| operation.tag() != similar::DiffTag::Equal)
            .map(|operation| Edit {
                start: operation.old_range().start,
                end: operation.old_range().end,
                replacement: variant[operation.new_range()].to_vec(),
            })
            .collect()
    }
    fn overlaps(start: usize, end: usize, edit: &Edit) -> bool {
        if start == end {
            edit.start == start
        } else if edit.start == edit.end {
            edit.start >= start && edit.start < end
        } else {
            edit.start < end && edit.end > start
        }
    }
    fn apply_segment(base: &[String], start: usize, end: usize, edits: &[Edit]) -> Vec<String> {
        let mut output = Vec::new();
        let mut cursor = start;
        for edit in edits {
            output.extend_from_slice(&base[cursor..edit.start]);
            output.extend(edit.replacement.iter().cloned());
            cursor = edit.end;
        }
        output.extend_from_slice(&base[cursor..end]);
        output
    }

    let base_lines = merge_lines(base);
    let left_lines = merge_lines(left);
    let right_lines = merge_lines(right);
    let left_edits = edits(&base_lines, &left_lines);
    let right_edits = edits(&base_lines, &right_lines);
    let mut left_index = 0;
    let mut right_index = 0;
    let mut base_cursor = 0;
    let mut output = String::new();
    let mut conflicts = 0;

    while left_index < left_edits.len() || right_index < right_edits.len() {
        let next_left = left_edits.get(left_index);
        let next_right = right_edits.get(right_index);
        let start = match (next_left, next_right) {
            (Some(left), Some(right)) => left.start.min(right.start),
            (Some(left), None) => left.start,
            (None, Some(right)) => right.start,
            (None, None) => break,
        };
        for line in &base_lines[base_cursor..start] {
            output.push_str(line);
        }
        let mut group_start = start;
        let mut group_end = start;
        let mut group_left = Vec::new();
        let mut group_right = Vec::new();

        loop {
            let mut consumed = false;
            while left_index < left_edits.len()
                && overlaps(group_start, group_end, &left_edits[left_index])
            {
                let edit = left_edits[left_index].clone();
                group_start = group_start.min(edit.start);
                group_end = group_end.max(edit.end);
                group_left.push(edit);
                left_index += 1;
                consumed = true;
            }
            while right_index < right_edits.len()
                && overlaps(group_start, group_end, &right_edits[right_index])
            {
                let edit = right_edits[right_index].clone();
                group_start = group_start.min(edit.start);
                group_end = group_end.max(edit.end);
                group_right.push(edit);
                right_index += 1;
                consumed = true;
            }
            if !consumed {
                break;
            }
        }

        let left_result = apply_segment(&base_lines, group_start, group_end, &group_left);
        let right_result = apply_segment(&base_lines, group_start, group_end, &group_right);
        if group_left.is_empty() {
            append_lines(&mut output, &right_result);
        } else if group_right.is_empty() || left_result == right_result {
            append_lines(&mut output, &left_result);
        } else {
            conflicts += 1;
            append_marker(&mut output, "<<<<<<< LEFT\n");
            append_lines(&mut output, &left_result);
            append_marker(&mut output, "=======\n");
            append_lines(&mut output, &right_result);
            append_marker(&mut output, ">>>>>>> RIGHT\n");
        }
        base_cursor = group_end;
    }
    for line in &base_lines[base_cursor..] {
        output.push_str(line);
    }
    (output, conflicts)
}

fn append_marker(output: &mut String, marker: &str) {
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    output.push_str(marker);
}

fn append_lines(output: &mut String, lines: &[String]) {
    for line in lines {
        output.push_str(line);
    }
}

fn merge_lines(text: &str) -> Vec<String> {
    text.split_inclusive('\n').map(str::to_owned).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        CompareOptions, DiffKind, DiffProjection, DiffSide, apply_hunk_copy, change_hunks,
        compare_text, compare_text_with_options, merge_three_way,
    };

    #[test]
    fn aligns_insertions_without_marking_unchanged_lines_as_different() {
        let lines = compare_text("first\nlast\n", "first\nmiddle\nlast\n");

        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].kind, DiffKind::Equal);
        assert_eq!(lines[0].left_number, Some(1));
        assert_eq!(lines[0].right_number, Some(1));
        assert_eq!(lines[1].kind, DiffKind::Added);
        assert_eq!(lines[1].left_number, None);
        assert_eq!(lines[1].right_number, Some(2));
        assert_eq!(lines[2].text, "last");
        assert_eq!(lines[2].left_number, Some(2));
        assert_eq!(lines[2].right_number, Some(3));
    }

    #[test]
    fn handles_empty_files_and_crlf_line_endings() {
        let added = compare_text("", "new\r\n");
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].kind, DiffKind::Added);
        assert_eq!(added[0].text, "new");

        let unchanged = compare_text("same\r\n", "same\r\n");
        assert_eq!(unchanged.len(), 1);
        assert_eq!(unchanged[0].kind, DiffKind::Equal);
        assert_eq!(unchanged[0].text, "same");
    }

    #[test]
    fn diff_rows_keep_utf8_source_ranges_and_line_endings() {
        let left = "same\r\ncafé\r\n";
        let right = "same\r\ntea\r\n";
        let rows = compare_text(left, right);

        assert_eq!(rows[0].left_span.as_ref().unwrap().content, 0..4);
        assert_eq!(rows[0].left_span.as_ref().unwrap().full_line, 0..6);
        assert_eq!(rows[1].left_span.as_ref().unwrap().content, 6..11);
        assert_eq!(rows[1].left_span.as_ref().unwrap().full_line, 6..13);
        assert_eq!(rows[1].right_span.as_ref().unwrap().content, 6..9);
        assert_eq!(rows[1].right_span.as_ref().unwrap().full_line, 6..11);
    }

    #[test]
    fn diff_projection_maps_source_lines_to_aligned_rows() {
        let rows = compare_text("first\nlast\n", "first\nmiddle\nlast\n");
        let projection = DiffProjection::from_rows(&rows);

        assert_eq!(projection.row_count(), 3);
        assert_eq!(projection.source_line_for_row(DiffSide::Left, 1), None);
        assert_eq!(projection.source_line_for_row(DiffSide::Right, 1), Some(1));
        assert_eq!(
            projection.display_row_for_source_line(DiffSide::Left, 1),
            Some(2)
        );
        assert_eq!(
            projection.display_row_for_source_line(DiffSide::Right, 2),
            Some(2)
        );
    }

    #[test]
    fn copies_changed_hunks_in_both_directions() {
        let left = "first\nold\nlast";
        let right = "first\nnew\nlast";
        let rows = compare_text(left, right);
        let projection = DiffProjection::from_rows(&rows);

        assert_eq!(
            apply_hunk_copy(left, right, &rows, &projection, 1, true).as_deref(),
            Some("first\nold\nlast")
        );
        assert_eq!(
            apply_hunk_copy(left, right, &rows, &projection, 1, false).as_deref(),
            Some("first\nnew\nlast")
        );
    }

    #[test]
    fn copies_insertions_and_deletions_as_complete_hunks() {
        let left = "first\nlast";
        let right = "first\nmiddle\nlast";
        let rows = compare_text(left, right);
        let projection = DiffProjection::from_rows(&rows);
        assert_eq!(change_hunks(&rows).len(), 1);

        assert_eq!(
            apply_hunk_copy(left, right, &rows, &projection, 1, false).as_deref(),
            Some("first\nmiddle\nlast")
        );
        let reverse_rows = compare_text(right, left);
        let reverse_projection = DiffProjection::from_rows(&reverse_rows);
        assert_eq!(
            apply_hunk_copy(right, left, &reverse_rows, &reverse_projection, 1, true).as_deref(),
            Some("first\nmiddle\nlast")
        );
    }

    #[test]
    fn copies_asymmetric_replacements_and_empty_files() {
        let left = "first\nold one\nold two\nlast";
        let right = "first\nnew\nlast";
        let rows = compare_text(left, right);
        let projection = DiffProjection::from_rows(&rows);
        assert_eq!(
            apply_hunk_copy(left, right, &rows, &projection, 1, true).as_deref(),
            Some(left)
        );
        assert_eq!(
            apply_hunk_copy(left, right, &rows, &projection, 1, false).as_deref(),
            Some(right)
        );

        let empty_left = "";
        let populated_right = "new\n";
        let rows = compare_text(empty_left, populated_right);
        let projection = DiffProjection::from_rows(&rows);
        assert_eq!(
            apply_hunk_copy(empty_left, populated_right, &rows, &projection, 0, true).as_deref(),
            Some("")
        );
        assert_eq!(
            apply_hunk_copy(empty_left, populated_right, &rows, &projection, 0, false).as_deref(),
            Some("new\n")
        );
    }

    #[test]
    fn hunk_copy_preserves_destination_line_endings_and_trailing_newline() {
        let left = "first\r\nold\r\nlast\r\n";
        let right = "first\r\nnew\r\nlast\r\n";
        let rows = compare_text(left, right);
        let projection = DiffProjection::from_rows(&rows);
        assert_eq!(
            apply_hunk_copy(left, right, &rows, &projection, 1, true).as_deref(),
            Some(left)
        );
    }

    #[test]
    fn equal_rows_cannot_be_copied_as_hunks() {
        let left = "same\nold";
        let right = "same\nnew";
        let rows = compare_text(left, right);
        let projection = DiffProjection::from_rows(&rows);
        assert_eq!(
            apply_hunk_copy(left, right, &rows, &projection, 0, true),
            None
        );
    }

    #[test]
    fn supports_ignoring_whitespace_case_and_comment_lines() {
        let rows = compare_text_with_options(
            "Value = 1\n// old note\n",
            "value=1\n// new note\n",
            CompareOptions {
                ignore_whitespace: true,
                ignore_case: true,
                ignore_comments: true,
            },
        );
        assert!(rows.iter().all(|row| row.kind == DiffKind::Equal));
    }

    #[test]
    fn three_way_merge_resolves_unchanged_side_and_marks_conflicts() {
        assert_eq!(merge_three_way("base", "left", "base"), ("left".into(), 0));
        let (merged, conflicts) = merge_three_way("base", "left", "right");
        assert_eq!(conflicts, 1);
        assert!(merged.contains("<<<<<<< LEFT"));
        assert!(merged.contains(">>>>>>> RIGHT"));
    }

    #[test]
    fn three_way_merge_combines_non_overlapping_changes() {
        let (merged, conflicts) = merge_three_way(
            "first\nmiddle\nlast\n",
            "FIRST\nmiddle\nlast\n",
            "first\nmiddle\nLAST\n",
        );
        assert_eq!(conflicts, 0);
        assert_eq!(merged, "FIRST\nmiddle\nLAST\n");
    }
}
