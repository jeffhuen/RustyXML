//! DOM Module - Arena-based XML Document
//!
//! Implements an efficient DOM representation using:
//! - Arena allocation for nodes
//! - NodeId (u32) indices for cache-friendly traversal
//! - String interning for element/attribute names
//! - Namespace resolution stack

pub mod document;
pub mod node;
pub mod strings;
pub mod namespace;

pub use document::{XmlDocument, OwnedXmlDocument, XmlDocumentView};
pub use node::{NodeId, NodeKind, XmlNode, XmlAttribute};
pub use strings::StringPool;

/// Trait for document access - enables XPath to work with both XmlDocument and XmlDocumentView
pub trait DocumentAccess {
    /// Get root element ID
    fn root_element_id(&self) -> Option<NodeId>;

    /// Get a node by ID
    fn get_node(&self, id: NodeId) -> Option<&XmlNode>;

    /// Get node name as string
    fn node_name(&self, id: NodeId) -> Option<&str>;

    /// Get node local name (without prefix)
    fn node_local_name(&self, id: NodeId) -> Option<&str>;

    /// Get text content of a text node
    fn text_content(&self, id: NodeId) -> Option<&str>;

    /// Get attributes for an element
    fn attributes(&self, id: NodeId) -> &[XmlAttribute];

    /// Get attribute value by name
    fn get_attribute(&self, node_id: NodeId, name: &str) -> Option<&str>;

    /// Get all attribute names and values
    fn get_attribute_values(&self, node_id: NodeId) -> Vec<(&str, &str)>;

    /// Get the string pool for direct access
    fn strings(&self) -> &StringPool;

    /// Iterate over children - returns collected Vec for trait object compatibility
    fn children_vec(&self, id: NodeId) -> Vec<NodeId>;

    /// Iterate over descendants - returns collected Vec for trait object compatibility
    fn descendants_vec(&self, id: NodeId) -> Vec<NodeId>;
}
