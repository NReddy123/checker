use crate::diff::{
    CompareOptions, DiffLine, DiffProjection, DiffSide, apply_hunk_copy, compare_text_with_options,
};
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
struct TextBuffer {
    text: String,
    saved: Option<String>,
    cursor: usize,
    selection: Option<Range<usize>>,
}

impl TextBuffer {
    fn new(text: String, saved: Option<String>) -> Self {
        Self {
            text,
            saved,
            cursor: 0,
            selection: None,
        }
    }

    fn is_dirty(&self) -> bool {
        match &self.saved {
            Some(saved) => saved != &self.text,
            None => !self.text.is_empty(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EditOperation {
    side: DiffSide,
    start: usize,
    removed: String,
    inserted: String,
    cursor_before: usize,
    cursor_after: usize,
    selection_before: Option<Range<usize>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DiffEditorError {
    InvalidRange,
    InvalidCursor,
    InvalidSelection,
    NoDiffHunkAtRow,
    HistoryMismatch,
}

pub(crate) struct DiffEditorCore {
    left: TextBuffer,
    right: TextBuffer,
    rows: Vec<DiffLine>,
    projection: DiffProjection,
    options: CompareOptions,
    undo: Vec<EditOperation>,
    redo: Vec<EditOperation>,
}

impl DiffEditorCore {
    pub(crate) fn new(
        left: String,
        right: String,
        left_saved: Option<String>,
        right_saved: Option<String>,
        options: CompareOptions,
    ) -> Self {
        let mut editor = Self {
            left: TextBuffer::new(left, left_saved),
            right: TextBuffer::new(right, right_saved),
            rows: Vec::new(),
            projection: DiffProjection::default(),
            options,
            undo: Vec::new(),
            redo: Vec::new(),
        };
        editor.recompute();
        editor
    }

    pub(crate) fn text(&self, side: DiffSide) -> &str {
        &self.buffer(side).text
    }

    pub(crate) fn rows(&self) -> &[DiffLine] {
        &self.rows
    }

    pub(crate) fn projection(&self) -> &DiffProjection {
        &self.projection
    }

    pub(crate) fn cursor(&self, side: DiffSide) -> usize {
        self.buffer(side).cursor
    }

    pub(crate) fn selection(&self, side: DiffSide) -> Option<Range<usize>> {
        self.buffer(side).selection.clone()
    }

    pub(crate) fn is_dirty(&self, side: DiffSide) -> bool {
        self.buffer(side).is_dirty()
    }

    pub(crate) fn matches_inputs(
        &self,
        left: &str,
        right: &str,
        left_saved: &Option<String>,
        right_saved: &Option<String>,
        options: CompareOptions,
    ) -> bool {
        self.left.text == left
            && self.right.text == right
            && &self.left.saved == left_saved
            && &self.right.saved == right_saved
            && self.options == options
    }

    pub(crate) fn set_options(&mut self, options: CompareOptions) {
        if self.options != options {
            self.options = options;
            self.recompute();
        }
    }

    pub(crate) fn set_cursor(
        &mut self,
        side: DiffSide,
        offset: usize,
    ) -> Result<(), DiffEditorError> {
        let buffer = self.buffer_mut(side);
        if !buffer.text.is_char_boundary(offset) {
            return Err(DiffEditorError::InvalidCursor);
        }
        buffer.cursor = offset;
        buffer.selection = None;
        Ok(())
    }

    fn replace_range_untracked(
        &mut self,
        side: DiffSide,
        range: Range<usize>,
        inserted: &str,
    ) -> Result<(), DiffEditorError> {
        let buffer = self.buffer(side);
        if range.start > range.end
            || !buffer.text.is_char_boundary(range.start)
            || !buffer.text.is_char_boundary(range.end)
        {
            return Err(DiffEditorError::InvalidRange);
        }
        let removed = buffer
            .text
            .get(range.clone())
            .ok_or(DiffEditorError::InvalidRange)?;
        if removed == inserted {
            self.set_cursor(side, range.start + inserted.len())?;
            return Ok(());
        }

        let buffer = self.buffer_mut(side);
        buffer.text.replace_range(range.clone(), inserted);
        buffer.cursor = range.start + inserted.len();
        buffer.selection = None;
        self.undo.clear();
        self.redo.clear();
        self.recompute();
        Ok(())
    }

    pub(crate) fn set_selection(
        &mut self,
        side: DiffSide,
        range: Option<Range<usize>>,
    ) -> Result<(), DiffEditorError> {
        let buffer = self.buffer_mut(side);
        if let Some(range) = &range {
            if range.start > range.end
                || !buffer.text.is_char_boundary(range.start)
                || !buffer.text.is_char_boundary(range.end)
            {
                return Err(DiffEditorError::InvalidSelection);
            }
            buffer.cursor = range.end;
        }
        buffer.selection = range;
        Ok(())
    }

    pub(crate) fn place_cursor(
        &mut self,
        side: DiffSide,
        display_row: usize,
        byte_column: usize,
    ) -> Result<(), DiffEditorError> {
        let source_line = self.projection.source_line_for_row(side, display_row);
        let span = source_line.and_then(|_| self.span_for_row(side, display_row).cloned());
        let offset = if let Some(span) = span {
            let target = span.content.start + byte_column.min(span.content.len());
            nearest_char_boundary(self.text(side), target, span.content.end)
        } else {
            self.insertion_offset_for_virtual_row(side, display_row)
        };
        self.set_cursor(side, offset)
    }

    pub(crate) fn place_selection(
        &mut self,
        side: DiffSide,
        display_row: usize,
        selection: Range<usize>,
    ) -> Result<(), DiffEditorError> {
        if let Some(span) = self.span_for_row(side, display_row).cloned() {
            let content = self
                .text(side)
                .get(span.content.clone())
                .ok_or(DiffEditorError::InvalidSelection)?;
            if selection.start > selection.end
                || !content.is_char_boundary(selection.start)
                || !content.is_char_boundary(selection.end)
            {
                return Err(DiffEditorError::InvalidSelection);
            }
            self.set_selection(
                side,
                Some(span.content.start + selection.start..span.content.start + selection.end),
            )
        } else {
            let offset = self.insertion_offset_for_virtual_row(side, display_row);
            self.set_cursor(side, offset)
        }
    }

    pub(crate) fn cursor_display_position(&self, side: DiffSide) -> (usize, usize) {
        let cursor = self.cursor(side);
        if cursor == self.text(side).len() && self.text(side).ends_with('\n') {
            return (self.rows.len(), 0);
        }
        self.rows
            .iter()
            .enumerate()
            .filter_map(|(row_index, row)| {
                let span = match side {
                    DiffSide::Left => row.left_span.as_ref(),
                    DiffSide::Right => row.right_span.as_ref(),
                }?;
                (span.full_line.start <= cursor && cursor <= span.full_line.end).then_some((
                    row_index,
                    cursor
                        .saturating_sub(span.content.start)
                        .min(span.content.len()),
                ))
            })
            .last()
            .unwrap_or((0, 0))
    }

    pub(crate) fn replace_range(
        &mut self,
        side: DiffSide,
        range: Range<usize>,
        inserted: &str,
    ) -> Result<(), DiffEditorError> {
        let buffer = self.buffer(side);
        if range.start > range.end
            || !buffer.text.is_char_boundary(range.start)
            || !buffer.text.is_char_boundary(range.end)
        {
            return Err(DiffEditorError::InvalidRange);
        }
        let removed = buffer
            .text
            .get(range.clone())
            .ok_or(DiffEditorError::InvalidRange)?
            .to_owned();
        if removed == inserted {
            self.set_cursor(side, range.start + inserted.len())?;
            return Ok(());
        }

        let cursor_before = buffer.cursor;
        let operation = EditOperation {
            side,
            start: range.start,
            removed,
            inserted: inserted.to_owned(),
            cursor_before,
            cursor_after: range.start + inserted.len(),
            selection_before: buffer.selection.clone(),
        };
        self.apply_operation(&operation, true)?;
        self.undo.push(operation);
        self.redo.clear();
        self.recompute();
        Ok(())
    }

    pub(crate) fn replace_display_line(
        &mut self,
        side: DiffSide,
        display_row: usize,
        text: &str,
    ) -> Result<(), DiffEditorError> {
        if let Some(span) = self.span_for_row(side, display_row).cloned() {
            if self.text(side).get(span.content.clone()) == Some(text) {
                return Ok(());
            }
            self.replace_range_untracked(side, span.content, text)
        } else if text.is_empty() {
            Ok(())
        } else {
            self.insert_virtual_line(side, display_row, text)
        }
    }

    pub(crate) fn split_display_line(
        &mut self,
        side: DiffSide,
        display_row: usize,
        selection: Range<usize>,
    ) -> Result<(), DiffEditorError> {
        if let Some(span) = self.span_for_row(side, display_row).cloned() {
            let content = self
                .text(side)
                .get(span.content.clone())
                .ok_or(DiffEditorError::InvalidRange)?;
            if selection.start > selection.end
                || !content.is_char_boundary(selection.start)
                || !content.is_char_boundary(selection.end)
            {
                return Err(DiffEditorError::InvalidRange);
            }
            let eol = line_ending_for(self.text(side), span.full_line.end);
            let range = span.content.start + selection.start..span.content.start + selection.end;
            self.replace_range(side, range, eol)
        } else {
            let offset = self.insertion_offset_for_virtual_row(side, display_row);
            let eol = line_ending_for(self.text(side), offset);
            self.replace_range(side, offset..offset, eol)
        }
    }

    pub(crate) fn merge_display_line_with_previous(
        &mut self,
        side: DiffSide,
        display_row: usize,
    ) -> Result<bool, DiffEditorError> {
        let Some(span) = self.span_for_row(side, display_row).cloned() else {
            return Ok(false);
        };
        let Some(previous_row) = (0..display_row)
            .rev()
            .find(|row| self.span_for_row(side, *row).is_some())
        else {
            return Ok(false);
        };
        let previous = self
            .span_for_row(side, previous_row)
            .ok_or(DiffEditorError::InvalidRange)?;
        if previous.full_line.end > span.full_line.start {
            return Err(DiffEditorError::InvalidRange);
        }
        self.replace_range(side, previous.content.end..span.content.start, "")?;
        Ok(true)
    }

    pub(crate) fn merge_display_line_with_next(
        &mut self,
        side: DiffSide,
        display_row: usize,
    ) -> Result<bool, DiffEditorError> {
        let Some(span) = self.span_for_row(side, display_row).cloned() else {
            return Ok(false);
        };
        let Some(next_row) =
            (display_row + 1..self.rows.len()).find(|row| self.span_for_row(side, *row).is_some())
        else {
            return Ok(false);
        };
        let next = self
            .span_for_row(side, next_row)
            .ok_or(DiffEditorError::InvalidRange)?;
        if span.full_line.end > next.full_line.start {
            return Err(DiffEditorError::InvalidRange);
        }
        self.replace_range(side, span.content.end..next.content.start, "")?;
        Ok(true)
    }

    pub(crate) fn insert_text(
        &mut self,
        side: DiffSide,
        inserted: &str,
    ) -> Result<(), DiffEditorError> {
        let buffer = self.buffer(side);
        let range = buffer
            .selection
            .clone()
            .unwrap_or(buffer.cursor..buffer.cursor);
        self.replace_range(side, range, inserted)
    }

    pub(crate) fn delete_backward(&mut self, side: DiffSide) -> Result<bool, DiffEditorError> {
        let buffer = self.buffer(side);
        let range = if let Some(selection) = buffer.selection.clone() {
            selection
        } else {
            let Some((start, _)) = buffer.text[..buffer.cursor].char_indices().next_back() else {
                return Ok(false);
            };
            start..buffer.cursor
        };
        self.replace_range(side, range, "").map(|()| true)
    }

    pub(crate) fn delete_forward(&mut self, side: DiffSide) -> Result<bool, DiffEditorError> {
        let buffer = self.buffer(side);
        let range = if let Some(selection) = buffer.selection.clone() {
            selection
        } else {
            let Some(next) = buffer.text[buffer.cursor..].chars().next() else {
                return Ok(false);
            };
            buffer.cursor..buffer.cursor + next.len_utf8()
        };
        self.replace_range(side, range, "").map(|()| true)
    }

    pub(crate) fn copy_hunk_from(
        &mut self,
        source_side: DiffSide,
        display_row: usize,
    ) -> Result<bool, DiffEditorError> {
        let left_to_right = source_side == DiffSide::Left;
        let updated = apply_hunk_copy(
            &self.left.text,
            &self.right.text,
            &self.rows,
            &self.projection,
            display_row,
            left_to_right,
        )
        .ok_or(DiffEditorError::NoDiffHunkAtRow)?;
        let destination_side = opposite_side(source_side);
        self.stage_replacement(destination_side, updated)
    }

    pub(crate) fn copy_file_from(
        &mut self,
        source_side: DiffSide,
    ) -> Result<bool, DiffEditorError> {
        let destination_side = opposite_side(source_side);
        let source = self.text(source_side).to_owned();
        self.stage_replacement(destination_side, source)
    }

    pub(crate) fn mark_saved(&mut self, side: DiffSide) {
        let buffer = self.buffer_mut(side);
        buffer.saved = Some(buffer.text.clone());
    }

    pub(crate) fn discard(&mut self, side: DiffSide) {
        let buffer = self.buffer_mut(side);
        buffer.text = buffer.saved.clone().unwrap_or_default();
        buffer.cursor = 0;
        buffer.selection = None;
        self.undo.retain(|operation| operation.side != side);
        self.redo.retain(|operation| operation.side != side);
        self.recompute();
    }

    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub(crate) fn undo(&mut self) -> Result<bool, DiffEditorError> {
        let Some(operation) = self.undo.last().cloned() else {
            return Ok(false);
        };
        self.apply_operation(&operation, false)?;
        self.undo.pop();
        self.redo.push(operation);
        self.recompute();
        Ok(true)
    }

    pub(crate) fn redo(&mut self) -> Result<bool, DiffEditorError> {
        let Some(operation) = self.redo.last().cloned() else {
            return Ok(false);
        };
        self.apply_operation(&operation, true)?;
        self.redo.pop();
        self.undo.push(operation);
        self.recompute();
        Ok(true)
    }

    fn stage_replacement(
        &mut self,
        side: DiffSide,
        updated: String,
    ) -> Result<bool, DiffEditorError> {
        let current = self.text(side);
        let (range, replacement) = minimal_replacement(current, &updated);
        if range.is_empty() && replacement.is_empty() {
            return Ok(false);
        }
        self.replace_range(side, range, replacement).map(|()| true)
    }

    fn apply_operation(
        &mut self,
        operation: &EditOperation,
        forward: bool,
    ) -> Result<(), DiffEditorError> {
        let (expected, replacement, cursor, selection) = if forward {
            (
                &operation.removed,
                &operation.inserted,
                operation.cursor_after,
                None,
            )
        } else {
            (
                &operation.inserted,
                &operation.removed,
                operation.cursor_before,
                operation.selection_before.clone(),
            )
        };
        let buffer = self.buffer_mut(operation.side);
        let end = operation.start + expected.len();
        let current = buffer
            .text
            .get(operation.start..end)
            .ok_or(DiffEditorError::HistoryMismatch)?;
        if current != expected {
            return Err(DiffEditorError::HistoryMismatch);
        }
        buffer.text.replace_range(operation.start..end, replacement);
        buffer.cursor = cursor.min(buffer.text.len());
        buffer.selection = selection;
        Ok(())
    }

    fn recompute(&mut self) {
        self.rows = compare_text_with_options(&self.left.text, &self.right.text, self.options);
        self.projection = DiffProjection::from_rows(&self.rows);
    }

    fn insertion_offset_for_virtual_row(&self, side: DiffSide, display_row: usize) -> usize {
        for row_index in display_row..self.rows.len() {
            if let Some(span) = self.span_for_row(side, row_index) {
                return span.full_line.start;
            }
        }
        self.text(side).len()
    }

    fn insert_virtual_line(
        &mut self,
        side: DiffSide,
        display_row: usize,
        text: &str,
    ) -> Result<(), DiffEditorError> {
        let source = self.text(side);
        let offset = self.insertion_offset_for_virtual_row(side, display_row);
        let eol = line_ending_for(source, offset);
        let insertion = if source.is_empty() {
            text.to_owned()
        } else if offset == 0 || offset < source.len() || source.ends_with(['\n', '\r']) {
            format!("{text}{eol}")
        } else {
            format!("{eol}{text}")
        };
        self.replace_range_untracked(side, offset..offset, &insertion)
    }

    fn span_for_row(&self, side: DiffSide, row: usize) -> Option<&crate::diff::SourceSpan> {
        let row = self.rows.get(row)?;
        match side {
            DiffSide::Left => row.left_span.as_ref(),
            DiffSide::Right => row.right_span.as_ref(),
        }
    }

    fn buffer(&self, side: DiffSide) -> &TextBuffer {
        match side {
            DiffSide::Left => &self.left,
            DiffSide::Right => &self.right,
        }
    }

    fn buffer_mut(&mut self, side: DiffSide) -> &mut TextBuffer {
        match side {
            DiffSide::Left => &mut self.left,
            DiffSide::Right => &mut self.right,
        }
    }
}

fn opposite_side(side: DiffSide) -> DiffSide {
    match side {
        DiffSide::Left => DiffSide::Right,
        DiffSide::Right => DiffSide::Left,
    }
}

fn nearest_char_boundary(text: &str, offset: usize, upper_bound: usize) -> usize {
    let mut candidate = offset.min(upper_bound);
    while candidate > 0 && !text.is_char_boundary(candidate) {
        candidate -= 1;
    }
    candidate
}

fn line_ending_for(text: &str, offset: usize) -> &'static str {
    let offset = offset.min(text.len());
    if let Some(index) = text[..offset].rfind('\n') {
        return if index > 0 && text.as_bytes()[index - 1] == b'\r' {
            "\r\n"
        } else {
            "\n"
        };
    }
    if let Some(relative_index) = text[offset..].find('\n') {
        let index = offset + relative_index;
        return if index > 0 && text.as_bytes()[index - 1] == b'\r' {
            "\r\n"
        } else {
            "\n"
        };
    }
    "\n"
}

fn minimal_replacement<'a>(current: &str, updated: &'a str) -> (Range<usize>, &'a str) {
    let mut prefix = 0;
    for ((current_index, current_char), (updated_index, updated_char)) in
        current.char_indices().zip(updated.char_indices())
    {
        if current_index != updated_index || current_char != updated_char {
            break;
        }
        prefix = current_index + current_char.len_utf8();
    }

    let current_tail = &current[prefix..];
    let updated_tail = &updated[prefix..];
    let mut common_suffix = 0;
    for (current_char, updated_char) in current_tail.chars().rev().zip(updated_tail.chars().rev()) {
        if current_char != updated_char {
            break;
        }
        common_suffix += current_char.len_utf8();
    }

    let current_end = current.len() - common_suffix;
    let updated_end = updated.len() - common_suffix;
    (prefix..current_end, &updated[prefix..updated_end])
}

#[cfg(test)]
mod tests {
    use super::{DiffEditorCore, DiffEditorError};
    use crate::diff::{CompareOptions, DiffSide};

    fn file_editor(left: &str, right: &str) -> DiffEditorCore {
        DiffEditorCore::new(
            left.into(),
            right.into(),
            Some(left.into()),
            Some(right.into()),
            CompareOptions::default(),
        )
    }

    #[test]
    fn typing_updates_projection_and_preserves_saved_baseline() {
        let mut editor = file_editor("same\nold\nlast\n", "same\nnew\nlast\n");
        editor
            .replace_range(DiffSide::Left, 5..8, "updated")
            .unwrap();

        assert_eq!(editor.text(DiffSide::Left), "same\nupdated\nlast\n");
        assert_eq!(editor.rows()[1].left_text, "updated");
        assert!(editor.is_dirty(DiffSide::Left));
        assert!(!editor.is_dirty(DiffSide::Right));
    }

    #[test]
    fn virtual_row_cursor_targets_the_insertion_point() {
        let mut editor = file_editor("first\nlast\n", "first\nmiddle\nlast\n");
        editor.place_cursor(DiffSide::Left, 1, 0).unwrap();
        assert_eq!(editor.cursor(DiffSide::Left), 6);

        editor
            .replace_display_line(DiffSide::Left, 1, "local")
            .unwrap();
        assert_eq!(editor.text(DiffSide::Left), "first\nlocal\nlast\n");
    }

    #[test]
    fn line_split_preserves_crlf_and_places_cursor_on_new_line() {
        let mut editor = file_editor("aBc\r\nlast\r\n", "other\n");
        editor.split_display_line(DiffSide::Left, 0, 1..1).unwrap();
        assert_eq!(editor.text(DiffSide::Left), "a\r\nBc\r\nlast\r\n");
        assert_eq!(editor.cursor(DiffSide::Left), 3);
    }

    #[test]
    fn virtual_row_insertion_preserves_trailing_newline() {
        let mut editor = file_editor("first\n", "first\nmiddle\n");
        let row = editor.rows().len() - 1;
        editor
            .replace_display_line(DiffSide::Left, row, "last")
            .unwrap();
        assert_eq!(editor.text(DiffSide::Left), "first\nlast\n");
    }

    #[test]
    fn end_of_file_cursor_uses_a_virtual_row_after_trailing_newline() {
        let mut editor = file_editor("last", "last\n");
        editor.split_display_line(DiffSide::Left, 0, 4..4).unwrap();
        assert_eq!(editor.text(DiffSide::Left), "last\n");
        assert_eq!(
            editor.cursor_display_position(DiffSide::Left),
            (editor.rows().len(), 0)
        );

        editor
            .replace_display_line(DiffSide::Left, editor.rows().len(), "next")
            .unwrap();
        assert_eq!(editor.text(DiffSide::Left), "last\nnext\n");
    }

    #[test]
    fn hunk_copy_is_an_undoable_staged_edit() {
        let left = "first\nleft\nlast\n";
        let right = "first\nright\nlast\n";
        let mut editor = file_editor(left, right);

        assert!(editor.copy_hunk_from(DiffSide::Left, 1).unwrap());
        assert!(editor.can_undo());
        assert_eq!(editor.text(DiffSide::Right), left);
        assert!(editor.is_dirty(DiffSide::Right));

        assert!(editor.undo().unwrap());
        assert_eq!(editor.text(DiffSide::Right), right);
        assert!(!editor.is_dirty(DiffSide::Right));

        assert!(editor.redo().unwrap());
        assert_eq!(editor.text(DiffSide::Right), left);
        assert!(editor.is_dirty(DiffSide::Right));
    }

    #[test]
    fn inline_cell_edits_use_native_input_undo_history() {
        let mut editor = file_editor("first\nleft\nlast\n", "first\nright\nlast\n");
        editor.copy_hunk_from(DiffSide::Left, 1).unwrap();
        assert!(editor.can_undo());

        editor
            .replace_display_line(DiffSide::Left, 1, "inline edit")
            .unwrap();

        assert!(!editor.can_undo());
        assert!(!editor.can_redo());
        assert_eq!(editor.text(DiffSide::Left), "first\ninline edit\nlast\n");
    }

    #[test]
    fn discard_restores_saved_text_and_clears_history_for_that_side() {
        let mut editor = file_editor("before\n", "other\n");
        editor
            .replace_range(DiffSide::Left, 0..6, "edited")
            .unwrap();
        editor.discard(DiffSide::Left);

        assert_eq!(editor.text(DiffSide::Left), "before\n");
        assert!(!editor.is_dirty(DiffSide::Left));
        assert!(!editor.undo().unwrap());
        assert!(editor.replace_range(DiffSide::Left, 1..0, "x").is_err());
        assert_eq!(editor.selection(DiffSide::Left), None);
    }

    #[test]
    fn invalid_utf8_ranges_do_not_mutate_either_buffer() {
        let mut editor = file_editor("café\n", "other\n");
        assert_eq!(
            editor.replace_range(DiffSide::Left, 4..4, "x"),
            Err(DiffEditorError::InvalidRange)
        );
        assert_eq!(editor.text(DiffSide::Left), "café\n");
    }

    #[test]
    fn undo_restores_a_selection_replaced_by_an_edit() {
        let mut editor = file_editor("select me\n", "");
        editor.set_selection(DiffSide::Left, Some(0..6)).unwrap();
        editor
            .replace_range(DiffSide::Left, 0..6, "replace")
            .unwrap();
        assert_eq!(editor.selection(DiffSide::Left), None);

        editor.undo().unwrap();
        assert_eq!(editor.selection(DiffSide::Left), Some(0..6));
    }

    #[test]
    fn text_input_and_backspace_replace_unicode_selection_atomically() {
        let mut editor = file_editor("aéz\n", "");
        editor.set_cursor(DiffSide::Left, 3).unwrap();
        editor.delete_backward(DiffSide::Left).unwrap();
        assert_eq!(editor.text(DiffSide::Left), "az\n");
        editor.undo().unwrap();
        assert_eq!(editor.text(DiffSide::Left), "aéz\n");

        editor.set_selection(DiffSide::Left, Some(1..3)).unwrap();
        editor.insert_text(DiffSide::Left, "Ω").unwrap();
        assert_eq!(editor.text(DiffSide::Left), "aΩz\n");
        assert_eq!(editor.cursor(DiffSide::Left), 3);
    }

    #[test]
    fn saved_buffers_and_comparison_options_are_independent_of_edits() {
        let mut editor = file_editor("Name \n", "name\n");
        editor.set_options(CompareOptions {
            ignore_whitespace: true,
            ignore_case: true,
            ignore_comments: false,
        });
        assert!(
            editor
                .rows()
                .iter()
                .all(|row| row.kind == crate::diff::DiffKind::Equal)
        );
        assert_eq!(editor.projection().row_count(), editor.rows().len());

        editor.copy_file_from(DiffSide::Left).unwrap();
        assert_eq!(editor.text(DiffSide::Right), "Name \n");
        assert!(editor.is_dirty(DiffSide::Right));
        editor.mark_saved(DiffSide::Right);
        assert!(!editor.is_dirty(DiffSide::Right));
    }
}
