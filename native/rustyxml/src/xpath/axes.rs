//! XPath Axes Implementation
//!
//! All 13 XPath 1.0 axes:
//! - child, parent, self
//! - descendant, descendant-or-self
//! - ancestor, ancestor-or-self
//! - following, following-sibling
//! - preceding, preceding-sibling
//! - attribute, namespace

use crate::dom::{DocumentAccess, NodeId, NodeKind};
#[cfg(test)]
use crate::dom::XmlDocument;
use super::parser::Axis;

/// Navigate along an axis from a context node
pub fn navigate<D: DocumentAccess>(
    doc: &D,
    context: NodeId,
    axis: Axis,
) -> Vec<NodeId> {
    match axis {
        Axis::Child => child_axis(doc, context),
        Axis::Descendant => descendant_axis(doc, context),
        Axis::DescendantOrSelf => descendant_or_self_axis(doc, context),
        Axis::Parent => parent_axis(doc, context),
        Axis::Ancestor => ancestor_axis(doc, context),
        Axis::AncestorOrSelf => ancestor_or_self_axis(doc, context),
        Axis::FollowingSibling => following_sibling_axis(doc, context),
        Axis::PrecedingSibling => preceding_sibling_axis(doc, context),
        Axis::Following => following_axis(doc, context),
        Axis::Preceding => preceding_axis(doc, context),
        Axis::Self_ => self_axis(context),
        Axis::Attribute => attribute_axis(doc, context),
        Axis::Namespace => namespace_axis(doc, context),
    }
}

/// child:: axis - all child nodes
fn child_axis<D: DocumentAccess>(doc: &D, context: NodeId) -> Vec<NodeId> {
    doc.children_vec(context)
}

/// descendant:: axis - all descendants (children, grandchildren, etc.)
fn descendant_axis<D: DocumentAccess>(doc: &D, context: NodeId) -> Vec<NodeId> {
    doc.descendants_vec(context)
}

/// descendant-or-self:: axis - context node plus all descendants
fn descendant_or_self_axis<D: DocumentAccess>(doc: &D, context: NodeId) -> Vec<NodeId> {
    let mut result = vec![context];
    result.extend(doc.descendants_vec(context));
    result
}

/// parent:: axis - parent node (at most one)
fn parent_axis<D: DocumentAccess>(doc: &D, context: NodeId) -> Vec<NodeId> {
    if let Some(node) = doc.get_node(context) {
        if let Some(parent) = node.parent {
            return vec![parent];
        }
    }
    Vec::new()
}

/// ancestor:: axis - all ancestors (parent, grandparent, etc.)
fn ancestor_axis<D: DocumentAccess>(doc: &D, context: NodeId) -> Vec<NodeId> {
    let mut result = Vec::new();
    let mut current = context;

    while let Some(node) = doc.get_node(current) {
        if let Some(parent) = node.parent {
            result.push(parent);
            current = parent;
        } else {
            break;
        }
    }

    result
}

/// ancestor-or-self:: axis - context node plus all ancestors
fn ancestor_or_self_axis<D: DocumentAccess>(doc: &D, context: NodeId) -> Vec<NodeId> {
    let mut result = vec![context];
    result.extend(ancestor_axis(doc, context));
    result
}

/// following-sibling:: axis - all following siblings
fn following_sibling_axis<D: DocumentAccess>(doc: &D, context: NodeId) -> Vec<NodeId> {
    let mut result = Vec::new();

    if let Some(node) = doc.get_node(context) {
        let mut sibling = node.next_sibling;
        while let Some(sib_id) = sibling {
            result.push(sib_id);
            if let Some(sib_node) = doc.get_node(sib_id) {
                sibling = sib_node.next_sibling;
            } else {
                break;
            }
        }
    }

    result
}

/// preceding-sibling:: axis - all preceding siblings (reverse order)
fn preceding_sibling_axis<D: DocumentAccess>(doc: &D, context: NodeId) -> Vec<NodeId> {
    let mut result = Vec::new();

    if let Some(node) = doc.get_node(context) {
        let mut sibling = node.prev_sibling;
        while let Some(sib_id) = sibling {
            result.push(sib_id);
            if let Some(sib_node) = doc.get_node(sib_id) {
                sibling = sib_node.prev_sibling;
            } else {
                break;
            }
        }
    }

    result
}

/// following:: axis - all nodes after in document order (not ancestors)
fn following_axis<D: DocumentAccess>(doc: &D, context: NodeId) -> Vec<NodeId> {
    let mut result = Vec::new();
    let context_node = match doc.get_node(context) {
        Some(n) => n.clone(),
        None => return result,
    };

    // Get all following siblings and their descendants
    let mut sibling = context_node.next_sibling;
    while let Some(sib_id) = sibling {
        result.push(sib_id);
        result.extend(doc.descendants_vec(sib_id));
        if let Some(sib_node) = doc.get_node(sib_id) {
            sibling = sib_node.next_sibling;
        } else {
            break;
        }
    }

    // Then do the same for ancestors' following siblings
    let mut ancestor = context_node.parent;
    while let Some(anc_id) = ancestor {
        if let Some(anc_node) = doc.get_node(anc_id) {
            let mut sibling = anc_node.next_sibling;
            while let Some(sib_id) = sibling {
                result.push(sib_id);
                result.extend(doc.descendants_vec(sib_id));
                if let Some(sib_node) = doc.get_node(sib_id) {
                    sibling = sib_node.next_sibling;
                } else {
                    break;
                }
            }
            ancestor = anc_node.parent;
        } else {
            break;
        }
    }

    result
}

/// preceding:: axis - all nodes before in document order (not ancestors)
fn preceding_axis<D: DocumentAccess>(doc: &D, context: NodeId) -> Vec<NodeId> {
    let mut result = Vec::new();
    let ancestors: std::collections::HashSet<NodeId> = ancestor_axis(doc, context).into_iter().collect();

    // Collect all nodes in document order that come before context
    // and are not ancestors
    fn collect_preceding<D: DocumentAccess>(
        doc: &D,
        node_id: NodeId,
        context: NodeId,
        ancestors: &std::collections::HashSet<NodeId>,
        result: &mut Vec<NodeId>,
        found_context: &mut bool,
    ) {
        if node_id == context {
            *found_context = true;
            return;
        }

        if *found_context {
            return;
        }

        if !ancestors.contains(&node_id) {
            result.push(node_id);
        }

        // Visit children
        for child in doc.children_vec(node_id) {
            collect_preceding(doc, child, context, ancestors, result, found_context);
            if *found_context {
                return;
            }
        }
    }

    let mut found_context = false;
    collect_preceding(doc, 0, context, &ancestors, &mut result, &mut found_context);

    // Reverse to get proper preceding order
    result.reverse();
    result
}

/// self:: axis - just the context node
fn self_axis(context: NodeId) -> Vec<NodeId> {
    vec![context]
}

/// attribute:: axis - attribute nodes of an element
fn attribute_axis<D: DocumentAccess>(doc: &D, _context: NodeId) -> Vec<NodeId> {
    // Attributes are not stored as nodes in our DOM implementation
    // They would need separate handling
    let _ = doc;
    Vec::new()
}

/// namespace:: axis - namespace nodes
fn namespace_axis<D: DocumentAccess>(doc: &D, _context: NodeId) -> Vec<NodeId> {
    // Namespace nodes are not commonly used and complex to implement
    // Return empty for now
    let _ = doc;
    Vec::new()
}

/// Check if a node matches a node test
pub fn matches_node_test<D: DocumentAccess>(
    doc: &D,
    node_id: NodeId,
    node_test: &super::compiler::CompiledNodeTest,
) -> bool {
    let node = match doc.get_node(node_id) {
        Some(n) => n,
        None => return false,
    };

    use super::compiler::CompiledNodeTest;

    match node_test {
        CompiledNodeTest::Any => {
            // * matches any element
            node.kind == NodeKind::Element
        }
        CompiledNodeTest::Name(name) => {
            if node.kind != NodeKind::Element {
                return false;
            }
            doc.node_local_name(node_id) == Some(name.as_str())
        }
        CompiledNodeTest::QName(ns, local) => {
            if node.kind != NodeKind::Element {
                return false;
            }
            // TODO: check namespace
            let _ = ns;
            doc.node_local_name(node_id) == Some(local.as_str())
        }
        CompiledNodeTest::NamespaceWildcard(ns) => {
            if node.kind != NodeKind::Element {
                return false;
            }
            // TODO: check namespace prefix matches
            let _ = ns;
            true
        }
        CompiledNodeTest::Node => {
            // node() matches any node type
            true
        }
        CompiledNodeTest::Text => {
            node.kind == NodeKind::Text || node.kind == NodeKind::CData
        }
        CompiledNodeTest::Comment => {
            node.kind == NodeKind::Comment
        }
        CompiledNodeTest::ProcessingInstruction(target) => {
            if node.kind != NodeKind::ProcessingInstruction {
                return false;
            }
            if let Some(expected_target) = target {
                doc.node_name(node_id) == Some(expected_target.as_str())
            } else {
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_child_axis() {
        let doc = XmlDocument::parse(b"<root><a/><b/></root>");
        let root = doc.root_element_id().unwrap();
        let children = child_axis(&doc, root);
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn test_descendant_axis() {
        let doc = XmlDocument::parse(b"<root><a><b/></a><c/></root>");
        let root = doc.root_element_id().unwrap();
        let descendants = descendant_axis(&doc, root);
        assert_eq!(descendants.len(), 3);  // a, b, c
    }

    #[test]
    fn test_ancestor_axis() {
        let doc = XmlDocument::parse(b"<root><a><b/></a></root>");
        let root = doc.root_element_id().unwrap();
        let children: Vec<_> = doc.children_vec(root);
        let a = children[0];
        let a_children: Vec<_> = doc.children_vec(a);
        let b = a_children[0];
        let ancestors = ancestor_axis(&doc, b);
        assert_eq!(ancestors.len(), 3);  // a, root, document
    }
}
