//! ResourceArc Wrappers
//!
//! Persistent state for streaming parsers and DOM documents.

use crate::strategy::StreamingParser;
use crate::dom::{XmlDocument, OwnedXmlDocument, XmlDocumentView};
use rustler::ResourceArc;
use std::sync::Mutex;

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

impl Default for StreamingParserResource {
    fn default() -> Self {
        Self::new()
    }
}

/// Type alias for the ResourceArc
pub type StreamingParserRef = ResourceArc<StreamingParserResource>;

/// Wrapper for XmlDocument that can be stored in a ResourceArc
/// Stores the fully parsed document to avoid re-parsing on each query.
pub struct DocumentResource {
    /// The fully parsed document - O(1) access for XPath queries
    pub doc: Mutex<Option<OwnedXmlDocument>>,
}

impl DocumentResource {
    pub fn new(input: Vec<u8>) -> Self {
        // Parse ONCE and store the result
        let owned_doc = OwnedXmlDocument::parse(input);

        DocumentResource {
            doc: Mutex::new(Some(owned_doc)),
        }
    }

    /// Create from an already-parsed document (used by parse_strict)
    pub fn from_owned(doc: OwnedXmlDocument) -> Self {
        DocumentResource {
            doc: Mutex::new(Some(doc)),
        }
    }

    /// Get a view into the document for XPath evaluation
    /// This is O(1) - no re-parsing!
    pub fn with_view<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(XmlDocumentView<'_>) -> R,
    {
        let guard = self.doc.lock().ok()?;
        let owned = guard.as_ref()?;
        Some(f(owned.as_borrowed()))
    }

    /// Legacy: Get document for parsing (still re-parses for XmlDocument API)
    /// TODO: Phase this out in favor of with_view
    pub fn with_doc<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&XmlDocument) -> R,
    {
        let guard = self.doc.lock().ok()?;
        let owned = guard.as_ref()?;
        // For now, re-parse to get XmlDocument API
        // This will be optimized when XPath uses XmlDocumentView
        let doc = XmlDocument::parse(&owned.input);
        Some(f(&doc))
    }

    /// Get node count without parsing
    pub fn node_count(&self) -> Option<usize> {
        let guard = self.doc.lock().ok()?;
        guard.as_ref().map(|d| d.node_count())
    }

    /// Get root element name without parsing
    pub fn root_name(&self) -> Option<String> {
        let guard = self.doc.lock().ok()?;
        guard.as_ref().and_then(|d| d.root_name().map(|s| s.to_string()))
    }
}

impl Default for DocumentResource {
    fn default() -> Self {
        DocumentResource {
            doc: Mutex::new(None),
        }
    }
}

/// Type alias for document ResourceArc
pub type DocumentRef = ResourceArc<DocumentResource>;
