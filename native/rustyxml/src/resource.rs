//! ResourceArc Wrappers
//!
//! Persistent state for streaming parsers and DOM documents.

use crate::dom::{OwnedXmlDocument, XmlDocumentView};
use crate::strategy::StreamingParser;
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

#[rustler::resource_impl]
impl rustler::Resource for StreamingParserResource {}

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

    /// Get a view into the document for XPath evaluation.
    /// This is O(1) - no re-parsing!
    ///
    /// # Errors
    ///
    /// Returns `"mutex_poisoned"` if the document mutex is poisoned,
    /// or `"no_document"` if no document is present.
    pub fn with_view<F, R>(&self, f: F) -> Result<R, &'static str>
    where
        F: FnOnce(XmlDocumentView<'_>) -> R,
    {
        let guard = self.doc.lock().map_err(|_| "mutex_poisoned")?;
        let owned = guard.as_ref().ok_or("no_document")?;
        Ok(f(owned.as_borrowed()))
    }
}

#[rustler::resource_impl]
impl rustler::Resource for DocumentResource {}

impl Default for DocumentResource {
    fn default() -> Self {
        DocumentResource {
            doc: Mutex::new(None),
        }
    }
}

/// Type alias for document ResourceArc
pub type DocumentRef = ResourceArc<DocumentResource>;

// ============================================================================
// XPath Result Set Resource (lazy evaluation)
// ============================================================================

use crate::dom::NodeId;

/// Stores XPath results without converting to BEAM terms
/// Allows lazy access to specific nodes/attributes
pub struct XPathResultResource {
    /// Reference to the source document
    pub doc: DocumentRef,
    /// Node IDs of matched nodes (stays in Rust memory)
    pub nodes: Vec<NodeId>,
}

impl XPathResultResource {
    pub fn new(doc: DocumentRef, nodes: Vec<NodeId>) -> Self {
        XPathResultResource { doc, nodes }
    }

    /// Get number of results
    pub fn count(&self) -> usize {
        self.nodes.len()
    }

    /// Get node ID at index
    pub fn get_node_id(&self, index: usize) -> Option<NodeId> {
        self.nodes.get(index).copied()
    }
}

#[rustler::resource_impl]
impl rustler::Resource for XPathResultResource {}

/// Type alias for result set ResourceArc
pub type XPathResultRef = ResourceArc<XPathResultResource>;
