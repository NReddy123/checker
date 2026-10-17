use similar::{ChangeTag, TextDiff};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub left_number: Option<usize>,
    pub right_number: Option<usize>,
    pub text: String,
    pub kind: DiffKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffKind {
    Equal,
    Added,
    Removed,
}

pub fn compare_text(left: &str, right: &str) -> Vec<DiffLine> {
    TextDiff::from_lines(left, right)
        .iter_all_changes()
        .map(|change| DiffLine {
            left_number: change.old_index().map(|line| line + 1),
            right_number: change.new_index().map(|line| line + 1),
            text: change.value().trim_end_matches(['\r', '\n']).to_owned(),
            kind: match change.tag() {
                ChangeTag::Equal => DiffKind::Equal,
                ChangeTag::Insert => DiffKind::Added,
                ChangeTag::Delete => DiffKind::Removed,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{DiffKind, compare_text};

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
}
