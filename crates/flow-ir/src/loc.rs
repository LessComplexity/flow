//! Source locations: a byte-range span, defined **in this crate** so flow-ir
//! keeps zero dependencies on flow-syntax (DESIGN §2, D8 — the dependency
//! direction is "ir is depended on by 8 crates").
//!
//! `SourceLoc` is field-identical to `flow_syntax::SourceLoc` (half-open byte
//! range `[start, end)`, `u32` offsets); flow-lower converts trivially.

/// Half-open byte range `[start, end)` into the original source text.
///
/// `Copy`, totally ordered by `(start, end)`. Mirror of
/// `flow_syntax::SourceLoc` (D8); kept here so the IR depends on nothing.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SourceLoc {
    pub start: u32,
    pub end: u32,
}

impl SourceLoc {
    /// Construct a span. The caller guarantees `start <= end`.
    #[inline]
    pub fn new(start: u32, end: u32) -> Self {
        SourceLoc { start, end }
    }

    /// A zero-width span at `at` (handy for synthesized nodes with no surface
    /// text — e.g. the routes a composite primitive builds internally).
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
