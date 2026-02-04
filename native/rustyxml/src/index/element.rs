//! Structural Index Element Types
//!
//! Compact, cache-aligned structures for storing XML structure
//! as offsets into the original input.

// Allow unused API methods - these are public for library consumers
#![allow(dead_code)]

use super::span::{ExtendedSpan, Span};

/// Flags for IndexElement
pub mod element_flags {
    /// Element has namespace prefix
    pub const HAS_PREFIX: u16 = 0x0001;
    /// Element has namespace URI
    pub const HAS_NAMESPACE: u16 = 0x0002;
    /// Element is empty (self-closing)
    pub const IS_EMPTY: u16 = 0x0004;
}

/// Flags for IndexText
pub mod text_flags {
    /// Text needs entity decoding (contains &amp; etc.)
    pub const NEEDS_ENTITY_DECODE: u16 = 0x0001;
    /// Text is CDATA section
    pub const IS_CDATA: u16 = 0x0002;
    /// Text is a comment
    pub const IS_COMMENT: u16 = 0x0004;
    /// Text is a processing instruction
    pub const IS_PI: u16 = 0x0008;
}

/// Sentinel value for "no node"
pub const NO_NODE: u32 = u32::MAX;

/// Index of an element in the structural index
///
/// Size: ~32 bytes
/// Stores only offsets into the original input - zero string allocation.
/// Note: Not using repr(C) to allow Rust to pack fields efficiently.
#[derive(Debug, Clone, Copy)]
pub struct IndexElement {
    /// Element name span (tag name in input)
    pub name: Span, // 6 bytes
    /// Parent element index (NO_NODE for root)
    pub parent: u32, // 4 bytes
    /// First child index (NO_NODE if no children)
    pub first_child: u32, // 4 bytes
    /// Next sibling index (NO_NODE if last child)
    pub next_sibling: u32, // 4 bytes
    /// Start index in attributes array
    pub attr_start: u32, // 4 bytes
    /// Number of attributes
    pub attr_count: u16, // 2 bytes
    /// Depth in document tree (0 = root element)
    pub depth: u16, // 2 bytes
    /// Flags (see element_flags)
    pub flags: u16, // 2 bytes
    /// Padding for alignment
    _pad: u16, // 2 bytes
    /// Last child index (for efficient appendChild)
    pub last_child: u32, // 4 bytes
}

impl IndexElement {
    /// Create a new element
    #[inline]
    pub fn new(name: Span, parent: u32, depth: u16) -> Self {
        Self {
            name,
            parent,
            first_child: NO_NODE,
            next_sibling: NO_NODE,
            last_child: NO_NODE,
            attr_start: 0,
            attr_count: 0,
            depth,
            flags: 0,
            _pad: 0,
        }
    }

    /// Check if this is the root element
    #[inline]
    pub fn is_root(&self) -> bool {
        self.parent == NO_NODE
    }

    /// Check if this element has children
    #[inline]
    pub fn has_children(&self) -> bool {
        self.first_child != NO_NODE
    }

    /// Check if this element has attributes
    #[inline]
    pub fn has_attributes(&self) -> bool {
        self.attr_count > 0
    }

    /// Check if this is an empty/self-closing element
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.flags & element_flags::IS_EMPTY != 0
    }

    /// Check if this element has a namespace prefix
    #[inline]
    pub fn has_prefix(&self) -> bool {
        self.flags & element_flags::HAS_PREFIX != 0
    }
}

impl Default for IndexElement {
    fn default() -> Self {
        Self {
            name: Span::empty(),
            parent: NO_NODE,
            first_child: NO_NODE,
            next_sibling: NO_NODE,
            last_child: NO_NODE,
            attr_start: 0,
            attr_count: 0,
            depth: 0,
            flags: 0,
            _pad: 0,
        }
    }
}

/// Index of a text node (text, CDATA, comment, PI)
///
/// Size: ~16 bytes
/// Uses ExtendedSpan to support text > 64KB.
/// Note: Not using repr(C) to allow Rust to pack fields efficiently.
#[derive(Debug, Clone, Copy)]
pub struct IndexText {
    /// Text content span
    pub span: ExtendedSpan, // 10 bytes
    /// Parent element index
    pub parent: u32, // 4 bytes
    /// Flags (see text_flags)
    pub flags: u16, // 2 bytes
}

impl IndexText {
    /// Create a new text node
    #[inline]
    pub fn new(offset: u32, len: usize, parent: u32) -> Self {
        Self {
            span: ExtendedSpan::new(offset, len),
            parent,
            flags: 0,
        }
    }

    /// Create a text node that needs entity decoding
    #[inline]
    pub fn new_with_entities(offset: u32, len: usize, parent: u32) -> Self {
        Self {
            span: ExtendedSpan::new(offset, len),
            parent,
            flags: text_flags::NEEDS_ENTITY_DECODE,
        }
    }

    /// Create a CDATA section
    #[inline]
    pub fn cdata(offset: u32, len: usize, parent: u32) -> Self {
        Self {
            span: ExtendedSpan::new(offset, len),
            parent,
            flags: text_flags::IS_CDATA,
        }
    }

    /// Create a comment
    #[inline]
    pub fn comment(offset: u32, len: usize, parent: u32) -> Self {
        Self {
            span: ExtendedSpan::new(offset, len),
            parent,
            flags: text_flags::IS_COMMENT,
        }
    }

    /// Create a processing instruction
    #[inline]
    pub fn pi(target_offset: u32, target_len: usize, parent: u32) -> Self {
        Self {
            span: ExtendedSpan::new(target_offset, target_len),
            parent,
            flags: text_flags::IS_PI,
        }
    }

    /// Check if this text needs entity decoding
    #[inline]
    pub fn needs_decode(&self) -> bool {
        self.flags & text_flags::NEEDS_ENTITY_DECODE != 0
    }

    /// Check if this is a CDATA section
    #[inline]
    pub fn is_cdata(&self) -> bool {
        self.flags & text_flags::IS_CDATA != 0
    }

    /// Check if this is a comment
    #[inline]
    pub fn is_comment(&self) -> bool {
        self.flags & text_flags::IS_COMMENT != 0
    }

    /// Check if this is a processing instruction
    #[inline]
    pub fn is_pi(&self) -> bool {
        self.flags & text_flags::IS_PI != 0
    }

    /// Check if this is regular text (not CDATA/comment/PI)
    #[inline]
    pub fn is_text(&self) -> bool {
        self.flags & (text_flags::IS_CDATA | text_flags::IS_COMMENT | text_flags::IS_PI) == 0
    }
}

impl Default for IndexText {
    fn default() -> Self {
        Self {
            span: ExtendedSpan::new(0, 0),
            parent: NO_NODE,
            flags: 0,
        }
    }
}

/// An attribute in the structural index
///
/// Size: ~12 bytes
/// Note: Not using repr(C) to allow Rust to pack fields efficiently.
#[derive(Debug, Clone, Copy)]
pub struct IndexAttribute {
    /// Attribute name span
    pub name: Span, // 6 bytes
    /// Attribute value span
    pub value: Span, // 6 bytes
}

impl IndexAttribute {
    /// Create a new attribute
    #[inline]
    pub fn new(name: Span, value: Span) -> Self {
        Self { name, value }
    }
}

impl Default for IndexAttribute {
    fn default() -> Self {
        Self {
            name: Span::empty(),
            value: Span::empty(),
        }
    }
}

/// A child reference - can be either an element or a text node
///
/// We use a discriminated union approach with the high bit of the index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChildRef(u32);

impl ChildRef {
    /// Bit flag indicating this is a text node reference
    const TEXT_BIT: u32 = 0x8000_0000;

    /// Create a reference to an element
    #[inline]
    pub const fn element(idx: u32) -> Self {
        debug_assert!(idx < Self::TEXT_BIT);
        Self(idx)
    }

    /// Create a reference to a text node
    #[inline]
    pub const fn text(idx: u32) -> Self {
        debug_assert!(idx < Self::TEXT_BIT);
        Self(idx | Self::TEXT_BIT)
    }

    /// Check if this is a text node reference
    #[inline]
    pub const fn is_text(&self) -> bool {
        self.0 & Self::TEXT_BIT != 0
    }

    /// Check if this is an element reference
    #[inline]
    pub const fn is_element(&self) -> bool {
        self.0 & Self::TEXT_BIT == 0
    }

    /// Get the index (strips the type bit)
    #[inline]
    pub const fn index(&self) -> u32 {
        self.0 & !Self::TEXT_BIT
    }

    /// Get the raw value
    #[inline]
    pub const fn raw(&self) -> u32 {
        self.0
    }

    /// Create from raw value
    #[inline]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_element_size() {
        // Verify the struct is reasonably sized (padding may vary by platform)
        let size = std::mem::size_of::<IndexElement>();
        // Should be at most 40 bytes (compact representation)
        assert!(size <= 40, "IndexElement too large: {} bytes", size);
    }

    #[test]
    fn test_index_text_size() {
        let size = std::mem::size_of::<IndexText>();
        // Should be at most 24 bytes
        assert!(size <= 24, "IndexText too large: {} bytes", size);
    }

    #[test]
    fn test_index_attribute_size() {
        let size = std::mem::size_of::<IndexAttribute>();
        // Should be at most 16 bytes
        assert!(size <= 16, "IndexAttribute too large: {} bytes", size);
    }

    #[test]
    fn test_child_ref() {
        let elem = ChildRef::element(42);
        assert!(elem.is_element());
        assert!(!elem.is_text());
        assert_eq!(elem.index(), 42);

        let text = ChildRef::text(100);
        assert!(text.is_text());
        assert!(!text.is_element());
        assert_eq!(text.index(), 100);
    }

    #[test]
    fn test_element_flags() {
        let mut elem = IndexElement::default();
        assert!(!elem.is_empty());

        elem.flags |= element_flags::IS_EMPTY;
        assert!(elem.is_empty());
    }

    #[test]
    fn test_text_flags() {
        let text = IndexText::new(0, 10, 0);
        assert!(text.is_text());
        assert!(!text.needs_decode());

        let text_with_entities = IndexText::new_with_entities(0, 10, 0);
        assert!(text_with_entities.needs_decode());

        let cdata = IndexText::cdata(0, 10, 0);
        assert!(cdata.is_cdata());
        assert!(!cdata.is_text());
    }
}
