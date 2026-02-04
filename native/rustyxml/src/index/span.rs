//! Span - offset and length into original input
//!
//! Zero-copy reference to a portion of the input document.
//! Used for element names, attribute names/values, and text content.

// Allow unused API methods - these are public for library consumers
#![allow(dead_code)]

/// A span referencing a portion of the input document.
///
/// Size: 6 bytes (offset: 4 bytes, len: 2 bytes)
/// Max string length: 64KB (u16::MAX)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub struct Span {
    /// Byte offset into the original input
    pub offset: u32,
    /// Length in bytes (max 64KB)
    pub len: u16,
}

impl Span {
    /// Create a new span
    #[inline]
    pub const fn new(offset: u32, len: u16) -> Self {
        Self { offset, len }
    }

    /// Create an empty span (used for "no value")
    #[inline]
    pub const fn empty() -> Self {
        Self { offset: 0, len: 0 }
    }

    /// Check if this span is empty
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Get the end offset (exclusive)
    #[inline]
    pub const fn end(&self) -> u32 {
        self.offset.saturating_add(self.len as u32)
    }

    /// Extract the byte slice from input
    #[inline]
    pub fn slice<'a>(&self, input: &'a [u8]) -> &'a [u8] {
        let start = self.offset as usize;
        let end = start.saturating_add(self.len as usize);
        if end <= input.len() {
            &input[start..end]
        } else {
            &[]
        }
    }

    /// Extract as UTF-8 string from input
    #[inline]
    pub fn as_str<'a>(&self, input: &'a [u8]) -> Option<&'a str> {
        std::str::from_utf8(self.slice(input)).ok()
    }

    /// Create a span from a slice that points into input
    ///
    /// # Safety
    /// The slice must be a subslice of the input buffer.
    pub fn from_slice(slice: &[u8], input: &[u8]) -> Option<Self> {
        let input_start = input.as_ptr() as usize;
        let slice_start = slice.as_ptr() as usize;

        if slice_start >= input_start && slice_start < input_start + input.len() {
            let offset = (slice_start - input_start) as u32;
            let len = slice.len().min(u16::MAX as usize) as u16;
            Some(Self::new(offset, len))
        } else {
            None
        }
    }
}

/// Extended span for text nodes that might exceed 64KB
///
/// For most text nodes, the regular len field is used.
/// For text > 64KB, extended_len is set and len is set to u16::MAX.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub struct ExtendedSpan {
    /// Base span (offset + short len)
    pub span: Span,
    /// Extended length for text > 64KB (0 means use span.len)
    pub extended_len: u32,
}

impl ExtendedSpan {
    /// Create a new extended span
    #[inline]
    pub fn new(offset: u32, len: usize) -> Self {
        if len <= u16::MAX as usize {
            Self {
                span: Span::new(offset, len as u16),
                extended_len: 0,
            }
        } else {
            Self {
                span: Span::new(offset, u16::MAX),
                extended_len: len as u32,
            }
        }
    }

    /// Get the actual length
    #[inline]
    pub const fn len(&self) -> usize {
        if self.extended_len > 0 {
            self.extended_len as usize
        } else {
            self.span.len as usize
        }
    }

    /// Check if empty
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Extract the byte slice from input
    #[inline]
    pub fn slice<'a>(&self, input: &'a [u8]) -> &'a [u8] {
        let start = self.span.offset as usize;
        let end = start.saturating_add(self.len());
        if end <= input.len() {
            &input[start..end]
        } else {
            &[]
        }
    }

    /// Extract as UTF-8 string from input
    #[inline]
    pub fn as_str<'a>(&self, input: &'a [u8]) -> Option<&'a str> {
        std::str::from_utf8(self.slice(input)).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_basic() {
        let span = Span::new(5, 10);
        assert_eq!(span.offset, 5);
        assert_eq!(span.len, 10);
        assert_eq!(span.end(), 15);
        assert!(!span.is_empty());
    }

    #[test]
    fn test_span_empty() {
        let span = Span::empty();
        assert!(span.is_empty());
        assert_eq!(span.offset, 0);
        assert_eq!(span.len, 0);
    }

    #[test]
    fn test_span_slice() {
        let input = b"hello world";
        let span = Span::new(6, 5);
        assert_eq!(span.slice(input), b"world");
    }

    #[test]
    fn test_span_as_str() {
        let input = b"hello world";
        let span = Span::new(0, 5);
        assert_eq!(span.as_str(input), Some("hello"));
    }

    #[test]
    fn test_span_from_slice() {
        let input = b"hello world";
        let slice = &input[6..11];
        let span = Span::from_slice(slice, input).unwrap();
        assert_eq!(span.offset, 6);
        assert_eq!(span.len, 5);
    }

    #[test]
    fn test_extended_span_short() {
        let span = ExtendedSpan::new(0, 100);
        assert_eq!(span.len(), 100);
        assert_eq!(span.extended_len, 0);
    }

    #[test]
    fn test_extended_span_long() {
        let span = ExtendedSpan::new(0, 100_000);
        assert_eq!(span.len(), 100_000);
        assert_eq!(span.span.len, u16::MAX);
        assert_eq!(span.extended_len, 100_000);
    }
}
