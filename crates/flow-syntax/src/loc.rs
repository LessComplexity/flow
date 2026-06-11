//! Source locations: byte-range spans, line/col display positions, and a
//! line index for offset→line/col conversion.
//!
//! Byte offsets are the canonical representation (DESIGN §1). `u32` suffices
//! because Flow-Core programs are single files.

/// Half-open byte range `[start, end)` into the source text.
///
/// Always on char boundaries (invariant I2). `Copy`, totally ordered by
/// `(start, end)`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SourceLoc {
    pub start: u32,
    pub end: u32,
}

impl SourceLoc {
    /// Construct a span. The caller guarantees `start <= end` and char
    /// alignment; debug assertions in the lexer verify both at emit time.
    #[inline]
    pub fn new(start: u32, end: u32) -> Self {
        SourceLoc { start, end }
    }

    /// A zero-width span at `at` (used for `Eof`).
    #[inline]
    pub fn empty_at(at: u32) -> Self {
        SourceLoc { start: at, end: at }
    }

    /// Length of the span in bytes.
    #[inline]
    pub fn len(self) -> u32 {
        self.end - self.start
    }

    /// Whether the span is zero-width.
    #[inline]
    pub fn is_empty(self) -> bool {
        self.start == self.end
    }
}

/// A 1-based line/column position, for human-facing display.
///
/// `col` counts Unicode scalar values from the line start (1-based), not
/// bytes — so multi-byte characters advance the column by one.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct LineCol {
    pub line: u32,
    pub col: u32,
}

/// Maps byte offsets to 1-based line/column positions. Built once per file.
///
/// Stores the byte offset of the start of each line; lookup is `O(log n)`
/// via binary search.
#[derive(Clone, Debug)]
pub struct LineIndex {
    /// Byte offset of the first character of each line. `line_starts[0] == 0`.
    line_starts: Vec<u32>,
    /// Total length of the source in bytes (for end-of-input lookups).
    len: u32,
    /// A copy of the source so that column counting can be char-aware.
    source: String,
}

impl LineIndex {
    /// Build the index from the full source text.
    pub fn new(source: &str) -> Self {
        let mut line_starts = Vec::with_capacity(16);
        line_starts.push(0u32);
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                // Next line starts right after the '\n'.
                line_starts.push((i as u32) + 1);
            }
        }
        LineIndex {
            line_starts,
            len: source.len() as u32,
            source: source.to_owned(),
        }
    }

    /// Convert a byte `offset` to a 1-based line/column.
    ///
    /// `offset` is clamped to the source length. The column counts Unicode
    /// scalar values from the line start. An offset that falls in the middle
    /// of a multi-byte char is rounded to the char it lands in by counting
    /// only char boundaries `< offset`.
    pub fn line_col(&self, offset: u32) -> LineCol {
        let offset = offset.min(self.len);
        // Largest line index whose start is <= offset.
        let line_idx = match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i - 1, // i >= 1 always, since line_starts[0] == 0.
        };
        let line_start = self.line_starts[line_idx];

        // Column: number of char boundaries strictly between line_start and
        // offset, plus 1 (1-based).
        let start = line_start as usize;
        let end = offset as usize;
        let slice = &self.source[start..end];
        let col = (slice.chars().count() as u32) + 1;

        LineCol {
            line: (line_idx as u32) + 1,
            col,
        }
    }
}
