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
    Added,
    Removed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompareOptions {
    pub ignore_whitespace: bool,
    pub ignore_case: bool,
    pub ignore_comments: bool,
}

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

    TextDiff::from_slices(&left_keys, &right_keys)
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
        .collect()
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
    (
        format!(
            "<<<<<<< LEFT\n{}{}=======\n{}{}>>>>>>> RIGHT\n",
            ensure_trailing_newline(left),
            if left.is_empty() || left.ends_with('\n') {
                ""
            } else {
                "\n"
            },
            ensure_trailing_newline(right),
            if right.is_empty() || right.ends_with('\n') {
                ""
            } else {
                "\n"
            }
        ),
        1,
    )
}

fn ensure_trailing_newline(text: &str) -> &str {
    text
}

#[cfg(test)]
mod tests {
    use super::{CompareOptions, DiffKind, compare_text, compare_text_with_options, merge_three_way};

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
}
