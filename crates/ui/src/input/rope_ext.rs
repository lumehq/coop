use std::ops::Range;

use rope::{Point, Rope};

use super::cursor::Position;

/// An extension trait for `Rope` to provide additional utility methods.
pub trait RopeExt {
    /// Get the line at the given row (0-based) index, including the `\r` at the end, but not `\n`.
    ///
    /// Return empty rope if the row (0-based) is out of bounds.
    fn line(&self, row: usize) -> Rope;

    /// Start offset of the line at the given row (0-based) index.
    fn line_start_offset(&self, row: usize) -> usize;

    /// Line the end offset (including `\n`) of the line at the given row (0-based) index.
    ///
    /// Return the end of the rope if the row is out of bounds.
    fn line_end_offset(&self, row: usize) -> usize;

    /// Return the number of lines in the rope.
    fn lines_len(&self) -> usize;

    /// Return the lines iterator.
    ///
    /// Each line is including the `\r` at the end, but not `\n`.
    fn lines(&self) -> RopeLines;

    /// Check is equal to another rope.
    fn eq(&self, other: &Rope) -> bool;

    /// Total number of characters in the rope.
    fn chars_count(&self) -> usize;

    /// Get char at the given offset (byte).
    ///
    /// If the offset is in the middle of a multi-byte character will panic.
    ///
    /// If the offset is out of bounds, return None.
    fn char_at(&self, offset: usize) -> Option<char>;

    /// Get the byte offset from the given line, column [`Position`] (0-based).
    fn position_to_offset(&self, line_col: &Position) -> usize;

    /// Get the line, column [`Position`] (0-based) from the given byte offset.
    fn offset_to_position(&self, offset: usize) -> Position;

    /// Get the word byte range at the given offset (byte).
    #[allow(dead_code)]
    fn word_range(&self, offset: usize) -> Option<Range<usize>>;

    /// Get word at the given offset (byte).
    #[allow(dead_code)]
    fn word_at(&self, offset: usize) -> String;
}

/// An iterator over the lines of a `Rope`.
pub struct RopeLines {
    row: usize,
    end_row: usize,
    rope: Rope,
}

impl RopeLines {
    /// Create a new `RopeLines` iterator.
    pub fn new(rope: Rope) -> Self {
        let end_row = rope.lines_len();
        Self {
            row: 0,
            end_row,
            rope,
        }
    }
}

impl Iterator for RopeLines {
    type Item = Rope;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.row >= self.end_row {
            return None;
        }

        let line = self.rope.line(self.row);
        self.row += 1;
        Some(line)
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.row = self.row.saturating_add(n);
        self.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.end_row - self.row;
        (len, Some(len))
    }
}

impl std::iter::ExactSizeIterator for RopeLines {}
impl std::iter::FusedIterator for RopeLines {}

impl RopeExt for Rope {
    fn line(&self, row: usize) -> Rope {
        let start = self.line_start_offset(row);
        let end = start + self.line_len(row as u32) as usize;
        self.slice(start..end)
    }

    fn line_start_offset(&self, row: usize) -> usize {
        let row = row as u32;
        self.point_to_offset(Point::new(row, 0))
    }

    fn position_to_offset(&self, pos: &Position) -> usize {
        let line = self.line(pos.line as usize);
        self.line_start_offset(pos.line as usize)
            + line
                .chars()
                .take(pos.character as usize)
                .map(|c| c.len_utf8())
                .sum::<usize>()
    }

    fn offset_to_position(&self, offset: usize) -> Position {
        let point = self.offset_to_point(offset);
        let line = self.line(point.row as usize);
        let column = line.clip_offset(point.column as usize, sum_tree::Bias::Left);
        let character = line.slice(0..column).chars().count();
        Position::new(point.row, character as u32)
    }

    fn line_end_offset(&self, row: usize) -> usize {
        if row > self.max_point().row as usize {
            return self.len();
        }

        self.line_start_offset(row) + self.line_len(row as u32) as usize
    }

    fn lines_len(&self) -> usize {
        self.max_point().row as usize + 1
    }

    fn lines(&self) -> RopeLines {
        RopeLines::new(self.clone())
    }

    fn eq(&self, other: &Rope) -> bool {
        self.summary() == other.summary()
    }

    fn chars_count(&self) -> usize {
        self.chars().count()
    }

    fn char_at(&self, offset: usize) -> Option<char> {
        if offset > self.len() {
            return None;
        }

        let offset = self.clip_offset(offset, sum_tree::Bias::Left);
        self.slice(offset..self.len()).chars().next()
    }

    fn word_range(&self, offset: usize) -> Option<Range<usize>> {
        if offset >= self.len() {
            return None;
        }

        let offset = self.clip_offset(offset, sum_tree::Bias::Left);

        let mut left = String::new();
        for c in self.reversed_chars_at(offset) {
            if c.is_alphanumeric() || c == '_' {
                left.insert(0, c);
            } else {
                break;
            }
        }
        let start = offset.saturating_sub(left.len());

        let right = self
            .chars_at(offset)
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect::<String>();

        let end = offset + right.len();

        if start == end {
            None
        } else {
            Some(start..end)
        }
    }

    fn word_at(&self, offset: usize) -> String {
        if let Some(range) = self.word_range(offset) {
            self.slice(range).to_string()
        } else {
            String::new()
        }
    }
}
