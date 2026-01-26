//! String Interning Pool
//!
//! Efficient string storage with deduplication for element names,
//! attribute names, and namespace URIs.

use std::collections::HashMap;

/// String interning pool for efficient storage of repeated strings
#[derive(Debug, Default)]
pub struct StringPool {
    /// All strings stored contiguously
    data: Vec<u8>,
    /// Map from string content to (offset, length) in data
    index: HashMap<Vec<u8>, u32>,
    /// Entries: (offset, length) pairs
    entries: Vec<(u32, u16)>,
}

impl StringPool {
    /// Create a new empty string pool
    pub fn new() -> Self {
        let mut pool = StringPool {
            data: Vec::with_capacity(4096),
            index: HashMap::new(),
            entries: Vec::with_capacity(256),
        };
        // Entry 0 is reserved for "no string"
        pool.entries.push((0, 0));
        pool
    }

    /// Intern a string, returning its ID
    ///
    /// If the string already exists, returns the existing ID.
    /// Otherwise, adds it to the pool and returns the new ID.
    pub fn intern(&mut self, s: &[u8]) -> u32 {
        if s.is_empty() {
            return 0; // Reserved for empty/no string
        }

        // Check if already interned
        if let Some(&id) = self.index.get(s) {
            return id;
        }

        // Add to pool
        let offset = self.data.len() as u32;
        let len = s.len().min(u16::MAX as usize) as u16;
        self.data.extend_from_slice(s);

        let id = self.entries.len() as u32;
        self.entries.push((offset, len));
        self.index.insert(s.to_vec(), id);

        id
    }

    /// Get a string by ID
    pub fn get(&self, id: u32) -> Option<&[u8]> {
        if id == 0 {
            return Some(b"");
        }
        let (offset, len) = *self.entries.get(id as usize)?;
        let start = offset as usize;
        let end = start + len as usize;
        if end <= self.data.len() {
            Some(&self.data[start..end])
        } else {
            None
        }
    }

    /// Get a string by ID as UTF-8 str
    pub fn get_str(&self, id: u32) -> Option<&str> {
        self.get(id).and_then(|b| std::str::from_utf8(b).ok())
    }

    /// Get the number of unique strings stored
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the pool is empty
    pub fn is_empty(&self) -> bool {
        self.entries.len() <= 1 // Entry 0 is reserved
    }

    /// Get total bytes used for string storage
    pub fn bytes_used(&self) -> usize {
        self.data.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intern_new() {
        let mut pool = StringPool::new();
        let id = pool.intern(b"hello");
        assert!(id > 0);
        assert_eq!(pool.get(id), Some(b"hello" as &[u8]));
    }

    #[test]
    fn test_intern_duplicate() {
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
        let mut pool = StringPool::new();
        let id = pool.intern(b"");
        assert_eq!(id, 0);
        assert_eq!(pool.get(0), Some(b"" as &[u8]));
    }

    #[test]
    fn test_get_str() {
        let mut pool = StringPool::new();
        let id = pool.intern(b"hello");
        assert_eq!(pool.get_str(id), Some("hello"));
    }
}
