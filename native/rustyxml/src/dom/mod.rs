//! DOM Module - Core types and traits for XML document access
//!
//! Provides:
//! - `DocumentAccess` trait: enables XPath on both DOM and indexed documents
//! - `NodeId`, `NodeKind`: compact node representation
//! - `XmlNode`, `XmlAttribute`: node types (used by trait interface)
//! - `validate_strict`: well-formedness validation
//! - `XmlDocument`: full DOM (test-only, used by XPath unit tests)

pub mod document;
pub mod node;
pub mod strings;

pub use document::validate_strict;
pub use node::{NodeId, NodeKind, XmlNode};

#[cfg(test)]
pub use document::XmlDocument;

/// Trait for document access - enables XPath to work with DOM and Index-based documents
pub trait DocumentAccess {
    /// Get root element ID
    fn root_element_id(&self) -> Option<NodeId>;

    /// Get a node by ID
    /// NOTE: Returns None for index-based views. Used by default navigation impls.
    fn get_node(&self, id: NodeId) -> Option<&XmlNode>;

    /// Get node name as string
    fn node_name(&self, id: NodeId) -> Option<&str>;

    /// Get node local name (without prefix)
    fn node_local_name(&self, id: NodeId) -> Option<&str>;

    /// Get namespace URI of a node (None if unavailable or no namespace)
    fn node_namespace_uri(&self, id: NodeId) -> Option<&str>;

    /// Get namespace prefix of a node (None if unavailable or no prefix)
    fn node_prefix(&self, id: NodeId) -> Option<&str>;

    /// Get text content of a text node
    fn text_content(&self, id: NodeId) -> Option<&str>;

    /// Get attribute value by name
    fn get_attribute(&self, node_id: NodeId, name: &str) -> Option<&str>;

    /// Get all attribute names and values
    fn get_attribute_values(&self, node_id: NodeId) -> Vec<(&str, &str)>;

    /// Iterate over children - returns collected Vec for trait object compatibility
    fn children_vec(&self, id: NodeId) -> Vec<NodeId>;

    /// Iterate over descendants - returns collected Vec for trait object compatibility
    fn descendants_vec(&self, id: NodeId) -> Vec<NodeId>;

    // === Navigation methods for XPath axes ===
    // These work with both DOM and Index-based views

    /// Get parent node ID
    fn parent_of(&self, id: NodeId) -> Option<NodeId> {
        self.get_node(id).and_then(|n| n.parent)
    }

    /// Get next sibling node ID
    fn next_sibling_of(&self, id: NodeId) -> Option<NodeId> {
        self.get_node(id).and_then(|n| n.next_sibling)
    }

    /// Get previous sibling node ID
    fn prev_sibling_of(&self, id: NodeId) -> Option<NodeId> {
        self.get_node(id).and_then(|n| n.prev_sibling)
    }

    /// Get node kind
    fn node_kind_of(&self, id: NodeId) -> NodeKind {
        self.get_node(id)
            .map(|n| n.kind)
            .unwrap_or(NodeKind::Element)
    }

    /// Get the document node ID (for XPath absolute paths)
    /// Default implementation returns 0 (for DOM-based documents)
    fn document_node_id(&self) -> NodeId {
        0
    }
}

/// Get the XPath string-value of a node per XPath 1.0 spec.
///
/// - For text/CDATA nodes: returns the text content
/// - For elements: concatenation of all descendant text nodes
/// - For other node types: empty string
pub fn node_string_value<D: DocumentAccess>(doc: &D, node_id: NodeId) -> String {
    let kind = doc.node_kind_of(node_id);

    match kind {
        NodeKind::Text | NodeKind::CData => doc.text_content(node_id).unwrap_or("").to_string(),
        NodeKind::Element => {
            let mut result = String::new();
            collect_descendant_text(doc, node_id, &mut result);
            result
        }
        _ => String::new(),
    }
}

/// Recursively collect text content from all descendant text nodes.
fn collect_descendant_text<D: DocumentAccess>(doc: &D, node_id: NodeId, result: &mut String) {
    for child_id in doc.children_vec(node_id) {
        let kind = doc.node_kind_of(child_id);

        match kind {
            NodeKind::Text | NodeKind::CData => {
                if let Some(text) = doc.text_content(child_id) {
                    result.push_str(text);
                }
            }
            NodeKind::Element => {
                collect_descendant_text(doc, child_id, result);
            }
            _ => {}
        }
    }
}
