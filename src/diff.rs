use similar::{ChangeTag, TextDiff};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub left_number: Option<usize>,
    pub right_number: Option<usize>,
    pub text: String,
    pub left_text: String,
    pub right_text: String,
    pub kind: DiffKind,
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
            let kind = match change.tag() {
                ChangeTag::Equal => DiffKind::Equal,
                ChangeTag::Insert => DiffKind::Added,
                ChangeTag::Delete => DiffKind::Removed,
            };
            DiffLine {
                left_number: left_index.map(|index| index + 1),
                right_number: right_index.map(|index| index + 1),
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

fn lines(text: &str) -> Vec<String> {
    text.split_inclusive('\n')
        .map(|line| line.trim_end_matches(['\r', '\n']).to_owned())
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
        CompareOptions, DiffKind, compare_text, compare_text_with_options, merge_three_way,
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
