//! Indexed Document View
//!
//! Provides a DocumentAccess implementation for StructuralIndex,
//! enabling XPath evaluation on memory-efficient indexed documents.

use super::element::{ChildRef, NO_NODE};
use super::structural::StructuralIndex;
use crate::dom::node::{NodeId, NodeKind, XmlNode};
use crate::dom::DocumentAccess;

/// Bit flag for encoding text node IDs
/// High bit = 1 means text node, high bit = 0 means element
const TEXT_NODE_FLAG: u32 = 0x8000_0000;

/// Special node ID for the virtual document node
/// This allows XPath absolute paths like /root to work correctly
const DOCUMENT_NODE_ID: u32 = 0x7FFF_FFFF;

/// Encode an element index as a NodeId
#[inline]
pub fn encode_element_id(idx: u32) -> NodeId {
    debug_assert!(idx < DOCUMENT_NODE_ID);
    idx
}

/// Encode a text node index as a NodeId
#[inline]
pub fn encode_text_id(idx: u32) -> NodeId {
    debug_assert!(idx < TEXT_NODE_FLAG);
    idx | TEXT_NODE_FLAG
}

/// Decode a NodeId into (is_text, index)
#[inline]
pub fn decode_node_id(id: NodeId) -> (bool, u32) {
    let is_text = (id & TEXT_NODE_FLAG) != 0;
    let idx = id & !TEXT_NODE_FLAG;
    (is_text, idx)
}

/// Check if a NodeId represents a text node
#[cfg(test)]
#[inline]
pub fn is_text_node_id(id: NodeId) -> bool {
    (id & TEXT_NODE_FLAG) != 0
}

/// Check if a NodeId represents the virtual document node
#[inline]
pub fn is_document_node_id(id: NodeId) -> bool {
    id == DOCUMENT_NODE_ID
}

/// A view into an indexed document for XPath evaluation
///
/// Combines a StructuralIndex reference with the input bytes to provide
/// zero-copy string access. Implements DocumentAccess for XPath compatibility.
pub struct IndexedDocumentView<'a> {
    pub index: &'a StructuralIndex,
    pub input: &'a [u8],
}

impl<'a> IndexedDocumentView<'a> {
    /// Create a new view
    pub fn new(index: &'a StructuralIndex, input: &'a [u8]) -> Self {
        Self { index, input }
    }

    /// Get the node kind for a given NodeId
    fn node_kind(&self, id: NodeId) -> NodeKind {
        // Handle virtual document node
        if is_document_node_id(id) {
            return NodeKind::Document;
        }

        let (is_text, idx) = decode_node_id(id);
        if is_text {
            if let Some(text) = self.index.get_text(idx) {
                if text.is_cdata() {
                    NodeKind::CData
                } else if text.is_comment() {
                    NodeKind::Comment
                } else if text.is_pi() {
                    NodeKind::ProcessingInstruction
                } else {
                    NodeKind::Text
                }
            } else {
                NodeKind::Text
            }
        } else {
            NodeKind::Element
        }
    }
}

impl<'a> DocumentAccess for IndexedDocumentView<'a> {
    fn root_element_id(&self) -> Option<NodeId> {
        self.index.root.map(encode_element_id)
    }

    /// Get a node by ID - returns None for indexed views
    /// Use the specific navigation methods instead
    fn get_node(&self, _id: NodeId) -> Option<&XmlNode> {
        // IndexedDocumentView doesn't store XmlNode structs
        // Use the navigation methods instead: parent_of(), next_sibling_of(), etc.
        None
    }

    fn node_name(&self, id: NodeId) -> Option<&str> {
        let (is_text, idx) = decode_node_id(id);
        if is_text {
            // Text nodes don't have names (return None or empty)
            // For PIs, the name is the target
            if let Some(text) = self.index.get_text(idx) {
                if text.is_pi() {
                    return text.span.as_str(self.input);
                }
            }
            None
        } else {
            self.index.element_name(idx, self.input)
        }
    }

    fn node_local_name(&self, id: NodeId) -> Option<&str> {
        let (is_text, idx) = decode_node_id(id);
        if is_text {
            None
        } else {
            // Get the full name and strip prefix
            let name = self.index.element_name(idx, self.input)?;
            if let Some(pos) = name.find(':') {
                Some(&name[pos + 1..])
            } else {
                Some(name)
            }
        }
    }

    fn node_namespace_uri(&self, _id: NodeId) -> Option<&str> {
        // Namespace resolution deferred for indexed documents
        None
    }

    fn text_content(&self, id: NodeId) -> Option<&str> {
        let (is_text, idx) = decode_node_id(id);
        if is_text {
            self.index.text_content(idx, self.input)
        } else {
            None
        }
    }

    fn get_attribute(&self, node_id: NodeId, name: &str) -> Option<&str> {
        let (is_text, idx) = decode_node_id(node_id);
        if is_text {
            None
        } else {
            self.index.get_attribute(idx, name, self.input)
        }
    }

    fn get_attribute_values(&self, node_id: NodeId) -> Vec<(&str, &str)> {
        let (is_text, idx) = decode_node_id(node_id);
        if is_text {
            Vec::new()
        } else {
            self.index.get_attribute_pairs(idx, self.input)
        }
    }

    fn children_vec(&self, id: NodeId) -> Vec<NodeId> {
        // Handle document node - its only child is the root element
        if is_document_node_id(id) {
            return self.root_element_id().into_iter().collect();
        }

        let (is_text, idx) = decode_node_id(id);
        if is_text {
            // Text nodes have no children
            Vec::new()
        } else {
            self.index
                .children(idx)
                .map(|child| {
                    if child.is_text() {
                        encode_text_id(child.index())
                    } else {
                        encode_element_id(child.index())
                    }
                })
                .collect()
        }
    }

    fn descendants_vec(&self, id: NodeId) -> Vec<NodeId> {
        // Handle document node - descendants include root and all its descendants
        if is_document_node_id(id) {
            if let Some(root_id) = self.root_element_id() {
                // Pre-allocate for all nodes: root + all elements + all texts
                let cap = 1 + self.index.element_count() + self.index.text_count();
                let mut result = Vec::with_capacity(cap);
                result.push(root_id);
                let (_, root_idx) = decode_node_id(root_id);
                for child in self.index.descendants(root_idx) {
                    if child.is_text() {
                        result.push(encode_text_id(child.index()));
                    } else {
                        result.push(encode_element_id(child.index()));
                    }
                }
                return result;
            }
            return Vec::new();
        }

        let (is_text, idx) = decode_node_id(id);
        if is_text {
            // Text nodes have no descendants
            Vec::new()
        } else {
            // Use child_count as lower bound estimate for pre-allocation
            let cap = self.index.child_count(idx);
            let mut result = Vec::with_capacity(cap.max(4));
            for child in self.index.descendants(idx) {
                if child.is_text() {
                    result.push(encode_text_id(child.index()));
                } else {
                    result.push(encode_element_id(child.index()));
                }
            }
            result
        }
    }

    // === Navigation methods for XPath axes ===

    fn parent_of(&self, id: NodeId) -> Option<NodeId> {
        // Document node has no parent
        if is_document_node_id(id) {
            return None;
        }

        let (is_text, idx) = decode_node_id(id);
        if is_text {
            // Text node's parent is stored in IndexText
            self.index.get_text(idx).and_then(|text| {
                if text.parent == NO_NODE {
                    None
                } else {
                    Some(encode_element_id(text.parent))
                }
            })
        } else {
            // Element's parent - root element's parent is the document node
            match self.index.parent(idx) {
                Some(parent_idx) => Some(encode_element_id(parent_idx)),
                None => {
                    // This element has no parent in the index - it's the root element
                    // Return the document node as its parent
                    if self.index.root == Some(idx) {
                        Some(DOCUMENT_NODE_ID)
                    } else {
                        None
                    }
                }
            }
        }
    }

    fn document_node_id(&self) -> NodeId {
        DOCUMENT_NODE_ID
    }

    fn next_sibling_of(&self, id: NodeId) -> Option<NodeId> {
        let (is_text, idx) = decode_node_id(id);
        if is_text {
            // Text nodes don't have direct sibling links in the index
            // We need to find the text node in its parent's children and get the next
            self.find_next_sibling_of_text(idx)
        } else {
            self.index.next_sibling(idx).map(encode_element_id)
        }
    }

    fn prev_sibling_of(&self, id: NodeId) -> Option<NodeId> {
        let (is_text, idx) = decode_node_id(id);
        if is_text {
            // Text nodes don't have direct sibling links
            self.find_prev_sibling_of_text(idx)
        } else {
            self.find_prev_sibling_of_element(idx)
        }
    }

    fn node_kind_of(&self, id: NodeId) -> NodeKind {
        self.node_kind(id)
    }
}

impl<'a> IndexedDocumentView<'a> {
    /// Find the next sibling of a text node
    fn find_next_sibling_of_text(&self, text_idx: u32) -> Option<NodeId> {
        let text = self.index.get_text(text_idx)?;
        if text.parent == NO_NODE {
            return None;
        }

        let children: Vec<_> = self.index.children(text.parent).collect();
        let target = ChildRef::text(text_idx);

        for (i, child) in children.iter().enumerate() {
            if child.raw() == target.raw() {
                // Found this text node, return the next sibling
                if i + 1 < children.len() {
                    let next = &children[i + 1];
                    return Some(if next.is_text() {
                        encode_text_id(next.index())
                    } else {
                        encode_element_id(next.index())
                    });
                }
            }
        }
        None
    }

    /// Find the previous sibling of a text node
    fn find_prev_sibling_of_text(&self, text_idx: u32) -> Option<NodeId> {
        let text = self.index.get_text(text_idx)?;
        if text.parent == NO_NODE {
            return None;
        }

        let children: Vec<_> = self.index.children(text.parent).collect();
        let target = ChildRef::text(text_idx);

        for (i, child) in children.iter().enumerate() {
            if child.raw() == target.raw() {
                // Found this text node, return the previous sibling
                if i > 0 {
                    let prev = &children[i - 1];
                    return Some(if prev.is_text() {
                        encode_text_id(prev.index())
                    } else {
                        encode_element_id(prev.index())
                    });
                }
            }
        }
        None
    }

    /// Find the previous sibling of an element
    /// Elements have next_sibling links but not prev_sibling links in the index
    fn find_prev_sibling_of_element(&self, elem_idx: u32) -> Option<NodeId> {
        let elem = self.index.get_element(elem_idx)?;
        if elem.parent == NO_NODE {
            return None;
        }

        let children: Vec<_> = self.index.children(elem.parent).collect();
        let target = ChildRef::element(elem_idx);

        for (i, child) in children.iter().enumerate() {
            if child.raw() == target.raw() {
                // Found this element, return the previous sibling
                if i > 0 {
                    let prev = &children[i - 1];
                    return Some(if prev.is_text() {
                        encode_text_id(prev.index())
                    } else {
                        encode_element_id(prev.index())
                    });
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::builder::build_index;

    #[test]
    fn test_node_id_encoding() {
        // Element IDs
        assert_eq!(encode_element_id(0), 0);
        assert_eq!(encode_element_id(100), 100);
        assert_eq!(decode_node_id(0), (false, 0));
        assert_eq!(decode_node_id(100), (false, 100));

        // Text IDs
        assert_eq!(decode_node_id(encode_text_id(0)), (true, 0));
        assert_eq!(decode_node_id(encode_text_id(100)), (true, 100));
    }

    #[test]
    fn test_basic_view() {
        let input = b"<root><child>text</child></root>";
        let index = build_index(input);
        let view = IndexedDocumentView::new(&index, input);

        // Root element should exist
        let root_id = view.root_element_id();
        assert!(root_id.is_some());

        // Root should have name "root"
        assert_eq!(view.node_name(root_id.unwrap()), Some("root"));

        // Root should have children
        let children = view.children_vec(root_id.unwrap());
        assert_eq!(children.len(), 1);

        // Child element should have name "child"
        let child_id = children[0];
        assert_eq!(view.node_name(child_id), Some("child"));
    }

    #[test]
    fn test_text_nodes() {
        let input = b"<root>hello</root>";
        let index = build_index(input);
        let view = IndexedDocumentView::new(&index, input);

        let root_id = view.root_element_id().unwrap();
        let children = view.children_vec(root_id);

        // Should have one text node child
        assert_eq!(children.len(), 1);
        let text_id = children[0];

        // Should be encoded as text node
        assert!(is_text_node_id(text_id));

        // Should have text content
        assert_eq!(view.text_content(text_id), Some("hello"));

        // Text node should have no name
        assert_eq!(view.node_name(text_id), None);

        // Text node's parent should be root
        assert_eq!(view.parent_of(text_id), Some(root_id));
    }

    #[test]
    fn test_attributes() {
        let input = b"<root id=\"1\" class=\"test\">content</root>";
        let index = build_index(input);
        let view = IndexedDocumentView::new(&index, input);

        let root_id = view.root_element_id().unwrap();

        // Should be able to get attributes
        assert_eq!(view.get_attribute(root_id, "id"), Some("1"));
        assert_eq!(view.get_attribute(root_id, "class"), Some("test"));
        assert_eq!(view.get_attribute(root_id, "missing"), None);

        // Should get all attribute pairs
        let attrs = view.get_attribute_values(root_id);
        assert_eq!(attrs.len(), 2);
    }

    #[test]
    fn test_descendants() {
        let input = b"<root><a><b>text</b></a><c/></root>";
        let index = build_index(input);
        let view = IndexedDocumentView::new(&index, input);

        let root_id = view.root_element_id().unwrap();
        let descendants = view.descendants_vec(root_id);

        // Should have a, b, text, c = 4 descendants
        assert_eq!(descendants.len(), 4);
    }

    #[test]
    fn test_navigation() {
        let input = b"<root><a/><b/><c/></root>";
        let index = build_index(input);
        let view = IndexedDocumentView::new(&index, input);

        let root_id = view.root_element_id().unwrap();
        let children = view.children_vec(root_id);

        assert_eq!(children.len(), 3);

        // Test parent navigation
        for &child in &children {
            assert_eq!(view.parent_of(child), Some(root_id));
        }

        // Test sibling navigation
        // a -> b
        let a_id = children[0];
        let b_id = children[1];
        let c_id = children[2];

        assert_eq!(view.next_sibling_of(a_id), Some(b_id));
        assert_eq!(view.next_sibling_of(b_id), Some(c_id));
        assert_eq!(view.next_sibling_of(c_id), None);

        assert_eq!(view.prev_sibling_of(c_id), Some(b_id));
        assert_eq!(view.prev_sibling_of(b_id), Some(a_id));
        assert_eq!(view.prev_sibling_of(a_id), None);
    }
}
