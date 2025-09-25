use std::ops::Range;

use gpui::{App, Font, LineFragment, Pixels};
use rope::Rope;

use super::rope_ext::RopeExt;

/// A line with soft wrapped lines info.
#[derive(Clone)]
pub(super) struct LineItem {
    /// The original line text.
    line: Rope,
    /// The soft wrapped lines relative byte range (0..line.len) of this line (Include first line).
    ///
    /// FIXME: Here in somecase, the `line_wrapper.wrap_line` has returned different
    /// like the `window.text_system().shape_text`. So, this value may not equal
    /// the actual rendered lines.
    wrapped_lines: Vec<Range<usize>>,
}

impl LineItem {
    /// Get the bytes length of this line.
    #[inline]
    pub(super) fn len(&self) -> usize {
        self.line.len()
    }

    /// Get number of soft wrapped lines of this line (include the first line).
    #[inline]
    pub(super) fn lines_len(&self) -> usize {
        self.wrapped_lines.len()
    }

    /// Get the height of this line item with given line height.
    pub(super) fn height(&self, line_height: Pixels) -> Pixels {
        self.lines_len() as f32 * line_height
    }
}

/// Used to prepare the text with soft wrap to be get lines to displayed in the Editor.
///
/// After use lines to calculate the scroll size of the Editor.
pub(super) struct TextWrapper {
    text: Rope,
    /// Total wrapped lines (Inlucde the first line), value is start and end index of the line.
    soft_lines: usize,
    font: Font,
    font_size: Pixels,
    /// If is none, it means the text is not wrapped
    wrap_width: Option<Pixels>,
    /// The lines by split \n
    pub(super) lines: Vec<LineItem>,
}

#[allow(unused)]
impl TextWrapper {
    pub(super) fn new(font: Font, font_size: Pixels, wrap_width: Option<Pixels>) -> Self {
        Self {
            text: Rope::new(),
            font,
            font_size,
            wrap_width,
            soft_lines: 0,
            lines: Vec::new(),
        }
    }

    #[inline]
    pub(super) fn set_default_text(&mut self, text: &Rope) {
        self.text = text.clone();
    }

    /// Get the total number of lines including wrapped lines.
    #[inline]
    pub(super) fn len(&self) -> usize {
        self.soft_lines
    }

    /// Get the line item by row index.
    #[inline]
    pub(super) fn line(&self, row: usize) -> Option<&LineItem> {
        self.lines.get(row)
    }

    pub(super) fn set_wrap_width(&mut self, wrap_width: Option<Pixels>, cx: &mut App) {
        if wrap_width == self.wrap_width {
            return;
        }

        self.wrap_width = wrap_width;
        self.update_all(&self.text.clone(), true, cx);
    }

    pub(super) fn set_font(&mut self, font: Font, font_size: Pixels, cx: &mut App) {
        if self.font.eq(&font) && self.font_size == font_size {
            return;
        }

        self.font = font;
        self.font_size = font_size;
        self.update_all(&self.text.clone(), true, cx);
    }

    /// Update the text wrapper and recalculate the wrapped lines.
    ///
    /// If the `text` is the same as the current text, do nothing.
    ///
    /// - `changed_text`: The text [`Rope`] that has changed.
    /// - `range`: The `selected_range` before change.
    /// - `new_text`: The inserted text.
    /// - `force`: Whether to force the update, if false, the update will be skipped if the text is the same.
    /// - `cx`: The application context.
    pub(super) fn update(
        &mut self,
        changed_text: &Rope,
        range: &Range<usize>,
        new_text: &Rope,
        force: bool,
        cx: &mut App,
    ) {
        let mut line_wrapper = cx
            .text_system()
            .line_wrapper(self.font.clone(), self.font_size);
        self._update(
            changed_text,
            range,
            new_text,
            force,
            &mut |line_str, wrap_width| {
                line_wrapper
                    .wrap_line(&[LineFragment::text(line_str)], wrap_width)
                    .collect()
            },
        );
    }

    fn _update<F>(
        &mut self,
        changed_text: &Rope,
        range: &Range<usize>,
        new_text: &Rope,
        force: bool,
        wrap_line: &mut F,
    ) where
        F: FnMut(&str, Pixels) -> Vec<gpui::Boundary>,
    {
        if self.text.eq(changed_text) && !force {
            return;
        }

        // Remove the old changed lines.
        let start_row = self.text.offset_to_point(range.start).row as usize;
        let start_row = start_row.min(self.lines.len().saturating_sub(1));
        let end_row = self.text.offset_to_point(range.end).row as usize;
        let end_row = end_row.min(self.lines.len().saturating_sub(1));
        let rows_range = start_row..=end_row;

        // To add the new lines.
        let new_start_row = changed_text.offset_to_point(range.start).row as usize;
        let new_start_offset = changed_text.line_start_offset(new_start_row);
        let new_end_row = changed_text
            .offset_to_point(range.start + new_text.len())
            .row as usize;
        let new_end_offset = changed_text.line_end_offset(new_end_row);
        let new_range = new_start_offset..new_end_offset;

        let mut new_lines = vec![];

        let wrap_width = self.wrap_width;

        for line in changed_text.slice(new_range).lines() {
            let line_str = line.to_string();
            let mut wrapped_lines = vec![];
            let mut prev_boundary_ix = 0;

            // If wrap_width is Pixels::MAX, skip wrapping to disable word wrap
            if let Some(wrap_width) = wrap_width {
                // Here only have wrapped line, if there is no wrap meet, the `line_wraps` result will empty.
                for boundary in wrap_line(&line_str, wrap_width) {
                    wrapped_lines.push(prev_boundary_ix..boundary.ix);
                    prev_boundary_ix = boundary.ix;
                }
            }

            // Reset of the line
            if !line_str[prev_boundary_ix..].is_empty() || prev_boundary_ix == 0 {
                wrapped_lines.push(prev_boundary_ix..line.len());
            }

            new_lines.push(LineItem {
                line: line.clone(),
                wrapped_lines,
            });
        }

        // dbg!(&new_lines.len());
        // dbg!(self.lines.len());
        if self.lines.is_empty() {
            self.lines = new_lines;
        } else {
            self.lines.splice(rows_range, new_lines);
        }

        // dbg!(self.lines.len());
        self.text = changed_text.clone();
        self.soft_lines = self.lines.iter().map(|l| l.lines_len()).sum();
    }

    /// Update the text wrapper and recalculate the wrapped lines.
    ///
    /// If the `text` is the same as the current text, do nothing.
    pub(crate) fn update_all(&mut self, text: &Rope, force: bool, cx: &mut App) {
        self.update(text, &(0..text.len()), text, force, cx);
    }
}
