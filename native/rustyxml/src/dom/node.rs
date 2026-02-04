//! XML Node representation
//!
//! Uses NodeId (u32) for compact, cache-friendly node references.

/// Compact node identifier (index into arena)
pub type NodeId = u32;

/// Type of XML node
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// Document root
    Document,
    /// Element node
    Element,
    /// Text content
    Text,
    /// CDATA section
    CData,
    /// Comment
    Comment,
    /// Processing instruction
    ProcessingInstruction,
}

/// An XML node in the arena (used by DocumentAccess trait + XPath tests)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct XmlNode {
    /// Type of this node
    pub kind: NodeKind,
    /// Parent node (None for document root)
    pub parent: Option<NodeId>,
    /// First child node
    pub first_child: Option<NodeId>,
    /// Last child node
    pub last_child: Option<NodeId>,
    /// Previous sibling
    pub prev_sibling: Option<NodeId>,
    /// Next sibling
    pub next_sibling: Option<NodeId>,
    /// Index into string pool for name (elements, PIs) or text content (text nodes)
    pub name_id: u32,
    /// Index into string pool for namespace prefix, or 0
    pub prefix_id: u32,
    /// Index into string pool for namespace URI, or 0
    pub namespace_id: u32,
    /// Start of attributes in attribute arena (for elements)
    pub attr_start: u32,
    /// Number of attributes
    pub attr_count: u16,
    /// Depth in document tree
    pub depth: u16,
}

#[allow(dead_code)]
impl XmlNode {
    /// Create a new document root node
    pub fn document() -> Self {
        XmlNode {
            kind: NodeKind::Document,
            parent: None,
            first_child: None,
            last_child: None,
            prev_sibling: None,
            next_sibling: None,
            name_id: 0,
            prefix_id: 0,
            namespace_id: 0,
            attr_start: 0,
            attr_count: 0,
            depth: 0,
        }
    }

    /// Create a new element node
    pub fn element(name_id: u32, parent: Option<NodeId>, depth: u16) -> Self {
        XmlNode {
            kind: NodeKind::Element,
            parent,
            first_child: None,
            last_child: None,
            prev_sibling: None,
            next_sibling: None,
            name_id,
            prefix_id: 0,
            namespace_id: 0,
            attr_start: 0,
            attr_count: 0,
            depth,
        }
    }

    /// Create a new text node
    pub fn text(parent: Option<NodeId>, depth: u16) -> Self {
        XmlNode {
            kind: NodeKind::Text,
            parent,
            first_child: None,
            last_child: None,
            prev_sibling: None,
            next_sibling: None,
            name_id: 0,
            prefix_id: 0,
            namespace_id: 0,
            attr_start: 0,
            attr_count: 0,
            depth,
        }
    }

    /// Create a new comment node
    pub fn comment(parent: Option<NodeId>, depth: u16) -> Self {
        XmlNode {
            kind: NodeKind::Comment,
            parent,
            first_child: None,
            last_child: None,
            prev_sibling: None,
            next_sibling: None,
            name_id: 0,
            prefix_id: 0,
            namespace_id: 0,
            attr_start: 0,
            attr_count: 0,
            depth,
        }
    }

    /// Create a new CDATA node
    pub fn cdata(parent: Option<NodeId>, depth: u16) -> Self {
        XmlNode {
            kind: NodeKind::CData,
            parent,
            first_child: None,
            last_child: None,
            prev_sibling: None,
            next_sibling: None,
            name_id: 0,
            prefix_id: 0,
            namespace_id: 0,
            attr_start: 0,
            attr_count: 0,
            depth,
        }
    }

    /// Create a processing instruction node
    pub fn processing_instruction(name_id: u32, parent: Option<NodeId>, depth: u16) -> Self {
        XmlNode {
            kind: NodeKind::ProcessingInstruction,
            parent,
            first_child: None,
            last_child: None,
            prev_sibling: None,
            next_sibling: None,
            name_id,
            prefix_id: 0,
            namespace_id: 0,
            attr_start: 0,
            attr_count: 0,
            depth,
        }
    }

    /// Check if this is an element node
    #[inline]
    pub fn is_element(&self) -> bool {
        self.kind == NodeKind::Element
    }

    /// Check if this is a text node
    #[inline]
    pub fn is_text(&self) -> bool {
        self.kind == NodeKind::Text
    }

    /// Check if this node has children
    #[inline]
    pub fn has_children(&self) -> bool {
        self.first_child.is_some()
    }

    /// Check if this node has attributes
    #[inline]
    pub fn has_attributes(&self) -> bool {
        self.attr_count > 0
    }
}

/// Stored attribute (used by DocumentAccess trait + XPath tests)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct XmlAttribute {
    /// Index into string pool for attribute name
    pub name_id: u32,
    /// Index into string pool for namespace prefix
    pub prefix_id: u32,
    /// Index into string pool for attribute value
    pub value_id: u32,
}

#[allow(dead_code)]
impl XmlAttribute {
    pub fn new(name_id: u32, value_id: u32) -> Self {
        XmlAttribute {
            name_id,
            prefix_id: 0,
            value_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let doc = XmlNode::document();
        assert_eq!(doc.kind, NodeKind::Document);
        assert!(doc.parent.is_none());
        assert_eq!(doc.depth, 0);
    }

    #[test]
    fn test_element_node() {
        let elem = XmlNode::element(1, Some(0), 1);
        assert_eq!(elem.kind, NodeKind::Element);
        assert_eq!(elem.parent, Some(0));
        assert_eq!(elem.name_id, 1);
        assert_eq!(elem.depth, 1);
    }
}
