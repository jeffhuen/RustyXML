//! SAX Event Types
//!
//! Defines the events emitted during SAX-style XML parsing.

use crate::index::Span;

/// A SAX parsing event
///
/// Events use Spans for zero-copy access to the input.
/// When returning to Elixir, spans are converted to sub-binaries.
#[derive(Debug, Clone)]
pub enum SaxEvent {
    /// Start of an element
    StartElement {
        /// Element name span
        name: Span,
        /// Attributes as (name_span, value_span) pairs
        attributes: Vec<(Span, Span)>,
    },

    /// End of an element
    EndElement {
        /// Element name span
        name: Span,
    },

    /// Text content
    Text {
        /// Text content span
        span: Span,
        /// Whether the text contains entity references that need decoding
        needs_decode: bool,
    },

    /// CDATA section
    CData {
        /// CDATA content span (excluding markers)
        span: Span,
    },

    /// Comment
    Comment {
        /// Comment content span (excluding markers)
        span: Span,
    },

    /// Processing instruction
    ProcessingInstruction {
        /// Target name span
        target: Span,
        /// Optional data span
        data: Option<Span>,
    },

    /// XML declaration (rarely needed in SAX)
    XmlDeclaration {
        /// Version span
        version: Option<Span>,
        /// Encoding span
        encoding: Option<Span>,
        /// Standalone span
        standalone: Option<Span>,
    },

    /// DOCTYPE declaration
    DocType {
        /// DOCTYPE content span
        content: Span,
    },
}

impl SaxEvent {
    /// Check if this is a start element event
    #[inline]
    pub fn is_start_element(&self) -> bool {
        matches!(self, SaxEvent::StartElement { .. })
    }

    /// Check if this is an end element event
    #[inline]
    pub fn is_end_element(&self) -> bool {
        matches!(self, SaxEvent::EndElement { .. })
    }

    /// Check if this is a text event
    #[inline]
    pub fn is_text(&self) -> bool {
        matches!(self, SaxEvent::Text { .. })
    }

    /// Get the element name span if this is a start or end element
    pub fn element_name(&self) -> Option<Span> {
        match self {
            SaxEvent::StartElement { name, .. } => Some(*name),
            SaxEvent::EndElement { name } => Some(*name),
            _ => None,
        }
    }
}

/// Compact event for memory-efficient storage
///
/// Uses a tag byte and inline data to minimize memory per event.
/// Total size: 24 bytes per event (vs 48+ for the enum)
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct CompactSaxEvent {
    /// Event type tag
    pub tag: u8,
    /// Flags (e.g., needs_decode for text)
    pub flags: u8,
    /// Padding for alignment
    _pad: u16,
    /// Primary span offset
    pub offset: u32,
    /// Primary span length
    pub len: u32,
    /// Secondary data (attribute count, or data span for PI)
    pub secondary: u32,
    /// Tertiary data (attribute start index)
    pub tertiary: u32,
}

impl CompactSaxEvent {
    /// Tag values
    pub const TAG_START_ELEMENT: u8 = 1;
    pub const TAG_END_ELEMENT: u8 = 2;
    pub const TAG_TEXT: u8 = 3;
    pub const TAG_CDATA: u8 = 4;
    pub const TAG_COMMENT: u8 = 5;
    pub const TAG_PI: u8 = 6;
    pub const TAG_XML_DECL: u8 = 7;
    pub const TAG_DOCTYPE: u8 = 8;

    /// Flag: text needs entity decoding
    pub const FLAG_NEEDS_DECODE: u8 = 0x01;

    /// Create a start element event
    #[inline]
    pub fn start_element(
        name_offset: u32,
        name_len: u32,
        attr_start: u32,
        attr_count: u32,
    ) -> Self {
        Self {
            tag: Self::TAG_START_ELEMENT,
            flags: 0,
            _pad: 0,
            offset: name_offset,
            len: name_len,
            secondary: attr_count,
            tertiary: attr_start,
        }
    }

    /// Create an end element event
    #[inline]
    pub fn end_element(name_offset: u32, name_len: u32) -> Self {
        Self {
            tag: Self::TAG_END_ELEMENT,
            flags: 0,
            _pad: 0,
            offset: name_offset,
            len: name_len,
            secondary: 0,
            tertiary: 0,
        }
    }

    /// Create a text event
    #[inline]
    pub fn text(offset: u32, len: u32, needs_decode: bool) -> Self {
        Self {
            tag: Self::TAG_TEXT,
            flags: if needs_decode {
                Self::FLAG_NEEDS_DECODE
            } else {
                0
            },
            _pad: 0,
            offset,
            len,
            secondary: 0,
            tertiary: 0,
        }
    }

    /// Create a CDATA event
    #[inline]
    pub fn cdata(offset: u32, len: u32) -> Self {
        Self {
            tag: Self::TAG_CDATA,
            flags: 0,
            _pad: 0,
            offset,
            len,
            secondary: 0,
            tertiary: 0,
        }
    }

    /// Create a comment event
    #[inline]
    pub fn comment(offset: u32, len: u32) -> Self {
        Self {
            tag: Self::TAG_COMMENT,
            flags: 0,
            _pad: 0,
            offset,
            len,
            secondary: 0,
            tertiary: 0,
        }
    }

    /// Create a PI event
    #[inline]
    pub fn pi(target_offset: u32, target_len: u32, data_offset: u32, data_len: u32) -> Self {
        Self {
            tag: Self::TAG_PI,
            flags: 0,
            _pad: 0,
            offset: target_offset,
            len: target_len,
            secondary: data_offset,
            tertiary: data_len,
        }
    }

    /// Get the primary span
    #[inline]
    pub fn span(&self) -> Span {
        Span::new(self.offset, self.len.min(u16::MAX as u32) as u16)
    }

    /// Check if this text event needs decoding
    #[inline]
    pub fn needs_decode(&self) -> bool {
        self.flags & Self::FLAG_NEEDS_DECODE != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compact_event_size() {
        let size = std::mem::size_of::<CompactSaxEvent>();
        // Should be reasonably compact (at most 24 bytes)
        assert!(size <= 24, "CompactSaxEvent too large: {} bytes", size);
    }

    #[test]
    fn test_start_element_event() {
        let event = CompactSaxEvent::start_element(10, 5, 0, 2);
        assert_eq!(event.tag, CompactSaxEvent::TAG_START_ELEMENT);
        assert_eq!(event.offset, 10);
        assert_eq!(event.len, 5);
        assert_eq!(event.secondary, 2); // attr_count
        assert_eq!(event.tertiary, 0); // attr_start
    }

    #[test]
    fn test_text_event_decode_flag() {
        let plain = CompactSaxEvent::text(0, 10, false);
        assert!(!plain.needs_decode());

        let with_entities = CompactSaxEvent::text(0, 10, true);
        assert!(with_entities.needs_decode());
    }
}
