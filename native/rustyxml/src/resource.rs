//! ResourceArc Wrappers
//!
//! Persistent state for streaming parsers and indexed documents.

use crate::index::{IndexedDocumentView, StructuralIndex};
use crate::strategy::StreamingParser;
use rustler::{Encoder, Env, ResourceArc, Term};
use std::sync::Mutex;

// ============================================================================
// Streaming SAX Parser Resource
// ============================================================================

/// Lightweight streaming SAX parser — just a buffer and depth counter.
///
/// Combined with `streaming_feed_sax` NIF: feed chunk → tokenize → encode
/// events into a compact binary → drain buffer. The binary crosses the NIF
/// boundary as a single allocation; Elixir decodes one event at a time via
/// pattern matching so only one event tuple is ever live on the BEAM heap.
pub struct StreamingSaxParser {
    pub buffer: Vec<u8>,
    pub depth: u32,
}

impl StreamingSaxParser {
    pub fn new() -> Self {
        StreamingSaxParser {
            buffer: Vec::with_capacity(64 * 1024),
            depth: 0,
        }
    }
}

impl Default for StreamingSaxParser {
    fn default() -> Self {
        Self::new()
    }
}

#[rustler::resource_impl]
impl rustler::Resource for StreamingSaxParser {}

/// Wrapper to hold StreamingSaxParser behind a Mutex for ResourceArc
pub struct StreamingSaxParserResource {
    pub inner: Mutex<StreamingSaxParser>,
}

impl StreamingSaxParserResource {
    pub fn new() -> Self {
        StreamingSaxParserResource {
            inner: Mutex::new(StreamingSaxParser::new()),
        }
    }
}

impl Default for StreamingSaxParserResource {
    fn default() -> Self {
        Self::new()
    }
}

#[rustler::resource_impl]
impl rustler::Resource for StreamingSaxParserResource {}

/// Type alias for streaming SAX parser ResourceArc
pub type StreamingSaxParserRef = ResourceArc<StreamingSaxParserResource>;

// ============================================================================
// Streaming Parser Resource
// ============================================================================

/// Wrapper for StreamingParser that can be stored in a ResourceArc
pub struct StreamingParserResource {
    pub inner: Mutex<StreamingParser>,
}

impl StreamingParserResource {
    pub fn new() -> Self {
        StreamingParserResource {
            inner: Mutex::new(StreamingParser::new()),
        }
    }

    pub fn with_filter(tag: &[u8]) -> Self {
        StreamingParserResource {
            inner: Mutex::new(StreamingParser::with_filter(tag)),
        }
    }
}

#[rustler::resource_impl]
impl rustler::Resource for StreamingParserResource {}

impl Default for StreamingParserResource {
    fn default() -> Self {
        Self::new()
    }
}

/// Type alias for the ResourceArc
pub type StreamingParserRef = ResourceArc<StreamingParserResource>;

// ============================================================================
// Structural Index Resource (main parse path)
// ============================================================================

/// Structural Index document resource
///
/// This is the DEFAULT parse path. Stores:
/// - The structural index (byte offsets into input)
/// - The input bytes (for string extraction)
///
/// Memory efficient: index is ~3x input size, strings are slices not copies.
pub struct IndexedDocumentResource {
    /// Structural index (offsets into input)
    pub index: StructuralIndex,
    /// Original input bytes (kept for string extraction)
    input: Vec<u8>,
}

impl IndexedDocumentResource {
    /// Create a new index from input binary
    pub fn new(input: Vec<u8>) -> Self {
        let index = crate::index::builder::build_index(&input);
        Self { index, input }
    }

    /// Create from borrowed input (copies the input)
    #[allow(dead_code)]
    pub fn new_from_slice(input: &[u8]) -> Self {
        Self::new(input.to_vec())
    }

    /// Get reference to stored input
    #[inline]
    #[allow(dead_code)]
    pub fn input(&self) -> &[u8] {
        &self.input
    }

    /// Get a view into the document for XPath evaluation
    #[inline]
    pub fn as_view(&self) -> IndexedDocumentView<'_> {
        IndexedDocumentView::new(&self.index, &self.input)
    }
}

/// Helper functions using stored input
#[allow(dead_code)]
impl IndexedDocumentResource {
    /// Get element name
    pub fn element_name_term<'a>(&self, env: Env<'a>, elem_idx: u32) -> Option<Term<'a>> {
        let elem = self.index.get_element(elem_idx)?;
        let slice = elem.name.slice(&self.input);
        Some(crate::term::bytes_to_binary(env, slice))
    }

    /// Get element name as string
    pub fn element_name_str(&self, elem_idx: u32) -> Option<&str> {
        let elem = self.index.get_element(elem_idx)?;
        elem.name.as_str(&self.input)
    }

    /// Get attribute value by name
    pub fn get_attribute_term<'a>(
        &self,
        env: Env<'a>,
        elem_idx: u32,
        name: &str,
    ) -> Option<Term<'a>> {
        let attrs = self.index.element_attributes(elem_idx);
        let name_bytes = name.as_bytes();

        for attr in attrs {
            if attr.name.slice(&self.input) == name_bytes {
                let raw = attr.value.slice(&self.input);
                let decoded = crate::core::entities::decode_text(raw);
                match decoded {
                    std::borrow::Cow::Borrowed(b) => {
                        return Some(crate::term::bytes_to_binary(env, b));
                    }
                    std::borrow::Cow::Owned(bytes) => {
                        return Some(crate::term::bytes_to_binary(env, &bytes));
                    }
                }
            }
        }
        None
    }

    /// Get attribute value as string
    pub fn get_attribute_str(&self, elem_idx: u32, name: &str) -> Option<String> {
        let attrs = self.index.element_attributes(elem_idx);
        let name_bytes = name.as_bytes();

        for attr in attrs {
            if attr.name.slice(&self.input) == name_bytes {
                let raw = attr.value.slice(&self.input);
                let decoded = crate::core::entities::decode_text(raw);
                return Some(String::from_utf8_lossy(&decoded).into_owned());
            }
        }
        None
    }

    /// Get all attributes as list of {name, value} tuples
    pub fn attributes_to_list<'a>(&self, env: Env<'a>, elem_idx: u32) -> Term<'a> {
        let attrs = self.index.element_attributes(elem_idx);

        let mut list = Term::list_new_empty(env);
        for attr in attrs.iter().rev() {
            let name = crate::term::bytes_to_binary(env, attr.name.slice(&self.input));
            let raw = attr.value.slice(&self.input);
            let decoded = crate::core::entities::decode_text(raw);
            let value = match decoded {
                std::borrow::Cow::Borrowed(b) => crate::term::bytes_to_binary(env, b),
                std::borrow::Cow::Owned(bytes) => crate::term::bytes_to_binary(env, &bytes),
            };
            let tuple = (name, value).encode(env);
            list = list.list_prepend(tuple);
        }
        list
    }

    /// Get text content by index
    pub fn text_to_term<'a>(&self, env: Env<'a>, text_idx: u32) -> Option<Term<'a>> {
        let text = self.index.get_text(text_idx)?;
        let raw = text.span.slice(&self.input);

        if text.needs_decode() {
            let decoded = crate::core::entities::decode_text(raw);
            match decoded {
                std::borrow::Cow::Borrowed(b) => Some(crate::term::bytes_to_binary(env, b)),
                std::borrow::Cow::Owned(bytes) => Some(crate::term::bytes_to_binary(env, &bytes)),
            }
        } else {
            Some(crate::term::bytes_to_binary(env, raw))
        }
    }

    /// Get text content as string
    pub fn text_content_str(&self, text_idx: u32) -> Option<String> {
        let text = self.index.get_text(text_idx)?;
        let raw = text.span.slice(&self.input);

        if text.needs_decode() {
            let decoded = crate::core::entities::decode_text(raw);
            Some(String::from_utf8_lossy(&decoded).into_owned())
        } else {
            Some(String::from_utf8_lossy(raw).into_owned())
        }
    }
}

#[rustler::resource_impl]
impl rustler::Resource for IndexedDocumentResource {}

/// Type alias for indexed document ResourceArc
pub type IndexedDocumentRef = ResourceArc<IndexedDocumentResource>;

// ============================================================================
// Document Accumulator (Streaming SimpleForm)
// ============================================================================

/// Accumulates XML chunks for streaming SimpleForm parsing.
///
/// Keeps all bytes in Rust until `to_simple_form()` is called,
/// minimizing BEAM heap usage during accumulation.
pub struct DocumentAccumulator {
    buffer: Mutex<Vec<u8>>,
}

impl DocumentAccumulator {
    pub fn new() -> Self {
        Self {
            buffer: Mutex::new(Vec::with_capacity(64 * 1024)),
        }
    }

    pub fn feed(&self, chunk: &[u8]) {
        if let Ok(mut buf) = self.buffer.lock() {
            buf.extend_from_slice(chunk);
        }
    }

    pub fn take_buffer(&self) -> Vec<u8> {
        if let Ok(mut buf) = self.buffer.lock() {
            std::mem::take(&mut buf)
        } else {
            Vec::new()
        }
    }
}

impl Default for DocumentAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

#[rustler::resource_impl]
impl rustler::Resource for DocumentAccumulator {}

/// Type alias for accumulator ResourceArc
pub type DocumentAccumulatorRef = ResourceArc<DocumentAccumulator>;
