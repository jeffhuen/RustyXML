//! SIMD-accelerated XML scanning using memchr
//!
//! Uses memchr crate for fast byte searching with SIMD acceleration:
//! - SSE2 (default x86_64)
//! - AVX2 (runtime detection)
//! - NEON (aarch64)

use memchr::{memchr, memchr2, memchr3};

/// Scanner for XML delimiter detection
pub struct Scanner<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Scanner<'a> {
    /// Create a new scanner for the given input
    #[inline]
    pub fn new(input: &'a [u8]) -> Self {
        Scanner { input, pos: 0 }
    }

    /// Get the current position
    #[inline]
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Set the current position
    #[inline]
    pub fn set_position(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// Check if we've reached the end
    #[inline]
    pub fn is_eof(&self) -> bool {
        self.pos >= self.input.len()
    }

    /// Get remaining bytes
    #[inline]
    pub fn remaining(&self) -> &'a [u8] {
        &self.input[self.pos..]
    }

    /// Get a slice from start to end positions
    #[inline]
    pub fn slice(&self, start: usize, end: usize) -> &'a [u8] {
        &self.input[start..end]
    }

    /// Peek at current byte without advancing
    #[inline]
    pub fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    /// Peek at byte at offset from current position
    #[inline]
    pub fn peek_at(&self, offset: usize) -> Option<u8> {
        self.input.get(self.pos + offset).copied()
    }

    /// Advance by n bytes
    #[inline]
    pub fn advance(&mut self, n: usize) {
        self.pos += n;
    }

    /// Skip whitespace characters (space, tab, newline, carriage return)
    #[inline]
    pub fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            match self.input[self.pos] {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                _ => break,
            }
        }
    }

    /// Find next '<' (tag start) using SIMD
    #[inline]
    pub fn find_tag_start(&self) -> Option<usize> {
        memchr(b'<', &self.input[self.pos..]).map(|i| self.pos + i)
    }

    /// Find next '>' (tag end) using SIMD
    /// Note: Does not handle '>' inside quotes - use find_tag_end_quoted for that
    #[inline]
    pub fn find_tag_end(&self) -> Option<usize> {
        memchr(b'>', &self.input[self.pos..]).map(|i| self.pos + i)
    }

    /// Find tag end while handling quotes properly
    /// Returns the position of '>' that is not inside quotes
    pub fn find_tag_end_quoted(&self) -> Option<usize> {
        let mut pos = self.pos;
        let mut in_single_quote = false;
        let mut in_double_quote = false;

        while pos < self.input.len() {
            match self.input[pos] {
                b'"' if !in_single_quote => in_double_quote = !in_double_quote,
                b'\'' if !in_double_quote => in_single_quote = !in_single_quote,
                b'>' if !in_single_quote && !in_double_quote => return Some(pos),
                _ => {}
            }
            pos += 1;
        }
        None
    }

    /// Find next entity reference start '&' using SIMD
    #[inline]
    pub fn find_entity_start(&self) -> Option<usize> {
        memchr(b'&', &self.input[self.pos..]).map(|i| self.pos + i)
    }

    /// Find content break: '<', '&', or ']' (for CDATA end detection)
    #[inline]
    pub fn find_content_break(&self) -> Option<usize> {
        memchr3(b'<', b'&', b']', &self.input[self.pos..]).map(|i| self.pos + i)
    }

    /// Find next '<' or '&' (text content boundaries)
    #[inline]
    pub fn find_text_boundary(&self) -> Option<usize> {
        memchr2(b'<', b'&', &self.input[self.pos..]).map(|i| self.pos + i)
    }

    /// Find next occurrence of a specific byte
    #[inline]
    pub fn find_byte(&self, byte: u8) -> Option<usize> {
        memchr(byte, &self.input[self.pos..]).map(|i| self.pos + i)
    }

    /// Find next occurrence of either of two bytes
    #[inline]
    pub fn find_byte2(&self, b1: u8, b2: u8) -> Option<usize> {
        memchr2(b1, b2, &self.input[self.pos..]).map(|i| self.pos + i)
    }

    /// Check if input starts with a byte sequence at current position
    #[inline]
    pub fn starts_with(&self, needle: &[u8]) -> bool {
        self.input[self.pos..].starts_with(needle)
    }

    /// Check if we have at least n bytes remaining
    #[inline]
    pub fn has_remaining(&self, n: usize) -> bool {
        self.pos + n <= self.input.len()
    }

    /// Read bytes until a delimiter, returning the slice and advancing past delimiter
    pub fn read_until(&mut self, delimiter: u8) -> Option<&'a [u8]> {
        let start = self.pos;
        if let Some(end) = memchr(delimiter, &self.input[start..]) {
            let slice = &self.input[start..start + end];
            self.pos = start + end + 1;
            Some(slice)
        } else {
            None
        }
    }

    /// Read an XML name (starts with letter/underscore, continues with letters/digits/hyphens/underscores/periods)
    pub fn read_name(&mut self) -> Option<&'a [u8]> {
        let start = self.pos;

        // First character must be a name start char
        if start >= self.input.len() {
            return None;
        }

        let first = self.input[start];
        if !is_name_start_char(first) {
            return None;
        }

        self.pos += 1;

        // Continue with name chars
        while self.pos < self.input.len() && is_name_char(self.input[self.pos]) {
            self.pos += 1;
        }

        Some(&self.input[start..self.pos])
    }
}

/// Check if byte is valid XML name start character
/// Allows ASCII letters, underscore, colon, and non-ASCII (UTF-8 Unicode)
#[inline]
fn is_name_start_char(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'_' | b':') || b >= 0x80
}

/// Check if byte is valid XML name character
/// Allows ASCII alphanumeric, punctuation, and non-ASCII (UTF-8 Unicode)
#[inline]
fn is_name_char(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b':') || b >= 0x80
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_tag_start() {
        let scanner = Scanner::new(b"hello <world>");
        assert_eq!(scanner.find_tag_start(), Some(6));
    }

    #[test]
    fn test_find_tag_end_quoted() {
        let scanner = Scanner::new(b"<a attr=\">test\">content");
        assert_eq!(scanner.find_tag_end_quoted(), Some(15));
    }

    #[test]
    fn test_read_name() {
        let mut scanner = Scanner::new(b"element-name>");
        assert_eq!(scanner.read_name(), Some(b"element-name" as &[u8]));
        assert_eq!(scanner.position(), 12);
    }

    #[test]
    fn test_skip_whitespace() {
        let mut scanner = Scanner::new(b"  \t\n hello");
        scanner.skip_whitespace();
        assert_eq!(scanner.position(), 5);
    }
}
