//! String Interning Pool with Sub-Binary Support
//!
//! Efficient string storage with deduplication for element names,
//! attribute names, and namespace URIs.
//!
//! Two storage modes:
//! - Offset-based: (input_offset, len) referencing original input (zero-copy)
//! - Copied: strings that needed entity decoding, stored in pool buffer
//!
//! Uses hash-based lookup to avoid storing duplicate string data.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Entry type for string storage
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum StringEntry {
    /// References original input: (offset_in_input, length)
    /// Zero-copy - can use make_subbinary when building BEAM terms
    InputRef(u32, u16),
    /// Copied string: (offset_in_pool_data, length)
    /// For strings that needed entity decoding
    Copied(u32, u16),
}

/// String interning pool with zero-copy support
///
/// Memory layout:
/// - `entries`: StringEntry for each interned string ID
/// - `data`: buffer for copied strings only (entity-decoded)
/// - `hash_index`: hash -> list of IDs (handles rare collisions)
///
/// Strings that don't need entity decoding use InputRef entries
/// which reference the original input - zero additional memory.
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct StringPool {
    /// Entries indexed by string ID
    entries: Vec<StringEntry>,
    /// Buffer for copied strings only (those needing entity decoding)
    data: Vec<u8>,
    /// Hash of string content -> list of IDs with that hash
    hash_index: HashMap<u64, Vec<u32>>,
}

#[allow(dead_code)]
impl StringPool {
    /// Create a new empty string pool
    pub fn new() -> Self {
        let mut pool = StringPool {
            entries: Vec::with_capacity(256),
            data: Vec::with_capacity(4096),
            hash_index: HashMap::new(),
        };
        // Entry 0 is reserved for "no string" (empty InputRef)
        pool.entries.push(StringEntry::InputRef(0, 0));
        pool
    }

    /// Compute hash of byte slice
    #[inline]
    fn compute_hash(s: &[u8]) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        s.hash(&mut hasher);
        hasher.finish()
    }

    /// Intern a string with offset into original input (zero-copy path)
    ///
    /// Use this for strings that don't need entity decoding.
    /// Returns string ID that can be resolved via get_with_input().
    pub fn intern_ref(&mut self, s: &[u8], input: &[u8], input_offset: usize) -> u32 {
        if s.is_empty() {
            return 0;
        }

        let hash = Self::compute_hash(s);

        // Check for existing entry with same content
        if let Some(ids) = self.hash_index.get(&hash) {
            for &id in ids {
                if self.get_with_input(id, input) == Some(s) {
                    return id;
                }
            }
        }

        // Add new InputRef entry
        let len = s.len().min(u16::MAX as usize) as u16;
        let id = self.entries.len() as u32;
        self.entries
            .push(StringEntry::InputRef(input_offset as u32, len));
        self.hash_index.entry(hash).or_default().push(id);

        id
    }

    /// Intern a string by copying (for entity-decoded strings)
    ///
    /// Use this for strings that needed entity decoding.
    pub fn intern(&mut self, s: &[u8]) -> u32 {
        if s.is_empty() {
            return 0;
        }

        let hash = Self::compute_hash(s);

        // Check for existing entry (in data buffer)
        if let Some(ids) = self.hash_index.get(&hash) {
            for &id in ids {
                if self.get_from_data(id) == Some(s) {
                    return id;
                }
            }
        }

        // Copy to data buffer
        let offset = self.data.len() as u32;
        let len = s.len().min(u16::MAX as usize) as u16;
        self.data.extend_from_slice(s);

        let id = self.entries.len() as u32;
        self.entries.push(StringEntry::Copied(offset, len));
        self.hash_index.entry(hash).or_default().push(id);

        id
    }

    /// Get entry info for a string ID
    #[inline]
    pub fn get_entry(&self, id: u32) -> Option<StringEntry> {
        self.entries.get(id as usize).copied()
    }

    /// Get string from the copied data buffer only
    fn get_from_data(&self, id: u32) -> Option<&[u8]> {
        if id == 0 {
            return Some(b"");
        }
        match self.entries.get(id as usize)? {
            StringEntry::Copied(offset, len) => {
                let start = *offset as usize;
                let end = start + *len as usize;
                if end <= self.data.len() {
                    Some(&self.data[start..end])
                } else {
                    None
                }
            }
            StringEntry::InputRef(_, _) => None, // Can't get without input
        }
    }

    /// Get a string by ID, using the original input for InputRef entries
    pub fn get_with_input<'a>(&'a self, id: u32, input: &'a [u8]) -> Option<&'a [u8]> {
        if id == 0 {
            return Some(b"");
        }
        match self.entries.get(id as usize)? {
            StringEntry::InputRef(offset, len) => {
                let start = *offset as usize;
                let end = start + *len as usize;
                if end <= input.len() {
                    Some(&input[start..end])
                } else {
                    None
                }
            }
            StringEntry::Copied(offset, len) => {
                let start = *offset as usize;
                let end = start + *len as usize;
                if end <= self.data.len() {
                    Some(&self.data[start..end])
                } else {
                    None
                }
            }
        }
    }

    /// Get a string by ID (legacy API - only works for Copied strings)
    /// For InputRef strings, returns None - use get_with_input instead
    pub fn get(&self, id: u32) -> Option<&[u8]> {
        if id == 0 {
            return Some(b"");
        }
        match self.entries.get(id as usize)? {
            StringEntry::Copied(offset, len) => {
                let start = *offset as usize;
                let end = start + *len as usize;
                if end <= self.data.len() {
                    Some(&self.data[start..end])
                } else {
                    None
                }
            }
            // For InputRef, we need the input - caller should use get_with_input
            StringEntry::InputRef(_, _) => None,
        }
    }

    /// Get a string by ID as UTF-8 str (legacy API)
    pub fn get_str(&self, id: u32) -> Option<&str> {
        self.get(id).and_then(|b| std::str::from_utf8(b).ok())
    }

    /// Get a string by ID as UTF-8 str, using original input
    pub fn get_str_with_input<'a>(&'a self, id: u32, input: &'a [u8]) -> Option<&'a str> {
        self.get_with_input(id, input)
            .and_then(|b| std::str::from_utf8(b).ok())
    }

    /// Get the number of unique strings stored
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the pool is empty
    pub fn is_empty(&self) -> bool {
        self.entries.len() <= 1 // Entry 0 is reserved
    }

    /// Get total bytes used for copied string storage
    pub fn bytes_used(&self) -> usize {
        self.data.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intern_copied() {
        let mut pool = StringPool::new();
        let id = pool.intern(b"hello");
        assert!(id > 0);
        assert_eq!(pool.get(id), Some(b"hello" as &[u8]));
    }

    #[test]
    fn test_intern_ref() {
        let input = b"<root>hello</root>";
        let mut pool = StringPool::new();
        // "hello" starts at offset 6 in input
        let id = pool.intern_ref(b"hello", input, 6);
        assert!(id > 0);
        assert_eq!(pool.get_with_input(id, input), Some(b"hello" as &[u8]));
    }

    #[test]
    fn test_intern_duplicate_copied() {
        let mut pool = StringPool::new();
        let id1 = pool.intern(b"hello");
        let id2 = pool.intern(b"hello");
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_intern_different() {
        let mut pool = StringPool::new();
        let id1 = pool.intern(b"hello");
        let id2 = pool.intern(b"world");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_empty_string() {
        let input = b"test";
        let mut pool = StringPool::new();
        let id = pool.intern(b"");
        assert_eq!(id, 0);
        assert_eq!(pool.get_with_input(0, input), Some(b"" as &[u8]));
    }

    #[test]
    fn test_get_str_copied() {
        let mut pool = StringPool::new();
        let id = pool.intern(b"hello");
        assert_eq!(pool.get_str(id), Some("hello"));
    }

    #[test]
    fn test_get_str_with_input() {
        let input = b"<name>world</name>";
        let mut pool = StringPool::new();
        let id = pool.intern_ref(b"world", input, 6);
        assert_eq!(pool.get_str_with_input(id, input), Some("world"));
    }

    #[test]
    fn test_entry_types() {
        let input = b"hello world";
        let mut pool = StringPool::new();

        // Copied entry
        let id1 = pool.intern(b"copied");
        assert!(matches!(
            pool.get_entry(id1),
            Some(StringEntry::Copied(_, _))
        ));

        // InputRef entry
        let id2 = pool.intern_ref(b"hello", input, 0);
        assert!(matches!(
            pool.get_entry(id2),
            Some(StringEntry::InputRef(_, _))
        ));
    }
}
