//! Elixir Term Conversion Utilities
//!
//! Converts Rust XML structures to Elixir terms.

use rustler::{Encoder, Env, NewBinary, Term};
use crate::dom::{DocumentAccess, NodeId, NodeKind};
use crate::xpath::XPathValue;
use crate::strategy::streaming::OwnedXmlEvent;

// Pre-defined atoms for efficiency - created once at compile time
rustler::atoms! {
    element,
    comment,
    pi,
    start_element,
    end_element,
    empty_element,
    text,
    cdata,
    processing_instruction,
}

/// Convert an XPath value to an Elixir term
pub fn xpath_value_to_term<'a, D: DocumentAccess>(env: Env<'a>, value: XPathValue, doc: &D) -> Term<'a> {
    match value {
        XPathValue::NodeSet(nodes) => {
            // Convert node set to list of node terms
            let mut list = Term::list_new_empty(env);
            for &id in nodes.iter().rev() {
                let node_term = node_to_term(env, doc, id);
                list = list.list_prepend(node_term);
            }
            list
        }
        XPathValue::Boolean(b) => b.encode(env),
        XPathValue::Number(n) => n.encode(env),
        XPathValue::String(s) => s.encode(env),
        XPathValue::StringList(list) => {
            // Convert list of strings to Elixir list
            let mut term_list = Term::list_new_empty(env);
            for s in list.into_iter().rev() {
                term_list = term_list.list_prepend(s.encode(env));
            }
            term_list
        }
    }
}

/// Convert a node to an Elixir term (simplified representation)
pub fn node_to_term<'a, D: DocumentAccess>(env: Env<'a>, doc: &D, node_id: NodeId) -> Term<'a> {
    let node = match doc.get_node(node_id) {
        Some(n) => n,
        None => return rustler::types::atom::nil().encode(env),
    };

    match node.kind {
        NodeKind::Element => {
            // Return as {:element, name, attrs, children}
            let name = doc.node_name(node_id).unwrap_or("");
            let name_term = str_to_binary(env, name);

            // Get attributes as list of {name, value} tuples - build in reverse order
            let attrs_slice = doc.attributes(node_id);
            let mut attrs = Term::list_new_empty(env);
            for attr in attrs_slice.iter().rev() {
                let attr_name = doc.strings().get_str(attr.name_id).unwrap_or("");
                let attr_value = doc.strings().get_str(attr.value_id).unwrap_or("");
                let attr_tuple = (str_to_binary(env, attr_name), str_to_binary(env, attr_value));
                attrs = attrs.list_prepend(attr_tuple.encode(env));
            }

            // Build children directly by traversing last_child->prev_sibling chain
            // This avoids allocating an intermediate Vec
            let mut children = Term::list_new_empty(env);
            let mut child_id = node.last_child;
            while let Some(cid) = child_id {
                let child_term = node_to_term(env, doc, cid);
                children = children.list_prepend(child_term);
                child_id = doc.get_node(cid).and_then(|n| n.prev_sibling);
            }

            (element(), name_term, attrs, children).encode(env)
        }
        NodeKind::Text | NodeKind::CData => {
            let content = doc.text_content(node_id).unwrap_or("");
            str_to_binary(env, content)
        }
        NodeKind::Comment => {
            let content = doc.text_content(node_id).unwrap_or("");
            (comment(), str_to_binary(env, content)).encode(env)
        }
        NodeKind::ProcessingInstruction => {
            let target = doc.node_name(node_id).unwrap_or("");
            (pi(), str_to_binary(env, target)).encode(env)
        }
        NodeKind::Document => {
            // Return root element
            if let Some(root_id) = doc.root_element_id() {
                node_to_term(env, doc, root_id)
            } else {
                rustler::types::atom::nil().encode(env)
            }
        }
        NodeKind::Attribute => {
            rustler::types::atom::nil().encode(env)
        }
    }
}

/// Convert a string to a binary term (more efficient than .encode())
#[inline]
fn str_to_binary<'a>(env: Env<'a>, s: &str) -> Term<'a> {
    let bytes = s.as_bytes();
    let mut binary = NewBinary::new(env, bytes.len());
    binary.as_mut_slice().copy_from_slice(bytes);
    binary.into()
}

/// Convert streaming events to Elixir terms
pub fn events_to_term<'a>(env: Env<'a>, events: Vec<OwnedXmlEvent>) -> Term<'a> {
    let mut list = Term::list_new_empty(env);
    for event in events.into_iter().rev() {
        let event_term = event_to_term(env, event);
        list = list.list_prepend(event_term);
    }
    list
}

/// Convert a single event to an Elixir term
pub fn event_to_term<'a>(env: Env<'a>, event: OwnedXmlEvent) -> Term<'a> {
    match event {
        OwnedXmlEvent::StartElement { name, attributes } => {
            let name_term = bytes_to_binary(env, &name);

            let mut attrs = Term::list_new_empty(env);
            for (k, v) in attributes.into_iter().rev() {
                let tuple = (bytes_to_binary(env, &k), bytes_to_binary(env, &v));
                attrs = attrs.list_prepend(tuple.encode(env));
            }

            (start_element(), name_term, attrs).encode(env)
        }
        OwnedXmlEvent::EndElement { name } => {
            let name_term = bytes_to_binary(env, &name);
            (end_element(), name_term).encode(env)
        }
        OwnedXmlEvent::EmptyElement { name, attributes } => {
            let name_term = bytes_to_binary(env, &name);

            let mut attrs = Term::list_new_empty(env);
            for (k, v) in attributes.into_iter().rev() {
                let tuple = (bytes_to_binary(env, &k), bytes_to_binary(env, &v));
                attrs = attrs.list_prepend(tuple.encode(env));
            }

            (empty_element(), name_term, attrs).encode(env)
        }
        OwnedXmlEvent::Text(content) => {
            (text(), bytes_to_binary(env, &content)).encode(env)
        }
        OwnedXmlEvent::CData(content) => {
            (cdata(), bytes_to_binary(env, &content)).encode(env)
        }
        OwnedXmlEvent::Comment(content) => {
            (comment(), bytes_to_binary(env, &content)).encode(env)
        }
        OwnedXmlEvent::ProcessingInstruction { target, data } => {
            (processing_instruction(), bytes_to_binary(env, &target), bytes_to_binary(env, &data)).encode(env)
        }
    }
}

/// Create a binary from bytes
pub fn bytes_to_binary<'a>(env: Env<'a>, bytes: &[u8]) -> Term<'a> {
    let mut binary = NewBinary::new(env, bytes.len());
    binary.as_mut_slice().copy_from_slice(bytes);
    binary.into()
}

/// Convert an XPath node set to a list of XML binaries (fast path)
/// Returns each element as its serialized XML string - bypasses BEAM term construction
pub fn nodeset_to_xml_binaries<'a, D: DocumentAccess>(env: Env<'a>, nodes: &[NodeId], doc: &D) -> Term<'a> {
    let mut list = Term::list_new_empty(env);
    for &id in nodes.iter().rev() {
        let xml = serialize_node_to_xml(doc, id);
        let binary = bytes_to_binary(env, xml.as_bytes());
        list = list.list_prepend(binary);
    }
    list
}

/// Serialize a node to XML string
/// Uses iterative approach with explicit stack to avoid stack overflow on deep XML
fn serialize_node_to_xml<D: DocumentAccess>(doc: &D, node_id: NodeId) -> String {
    // Estimate buffer size based on typical element size
    let mut buf = String::with_capacity(1024);

    // Stack entries: Either entering a node or need to write closing tag
    enum StackEntry {
        Enter(NodeId),
        Close(NodeId),
    }

    let mut stack: Vec<StackEntry> = Vec::with_capacity(64);
    stack.push(StackEntry::Enter(node_id));

    while let Some(entry) = stack.pop() {
        match entry {
            StackEntry::Close(id) => {
                // Write closing tag
                if let Some(name) = doc.node_name(id) {
                    buf.push_str("</");
                    buf.push_str(name);
                    buf.push('>');
                }
            }
            StackEntry::Enter(current_id) => {
                let node = match doc.get_node(current_id) {
                    Some(n) => n,
                    None => continue,
                };

                match node.kind {
                    NodeKind::Element => {
                        let name = doc.node_name(current_id).unwrap_or("");
                        buf.push('<');
                        buf.push_str(name);

                        // Add attributes
                        for attr in doc.attributes(current_id) {
                            let attr_name = doc.strings().get_str(attr.name_id).unwrap_or("");
                            let attr_value = doc.strings().get_str(attr.value_id).unwrap_or("");
                            buf.push(' ');
                            buf.push_str(attr_name);
                            buf.push_str("=\"");
                            escape_xml_to_buf(attr_value, &mut buf);
                            buf.push('"');
                        }

                        if node.first_child.is_none() {
                            buf.push_str("/>");
                        } else {
                            buf.push('>');

                            // Push closing tag first (processed after children)
                            stack.push(StackEntry::Close(current_id));

                            // Push children in reverse order using last_child->prev_sibling
                            // This avoids allocating a Vec for children
                            let mut child_id = node.last_child;
                            while let Some(cid) = child_id {
                                stack.push(StackEntry::Enter(cid));
                                child_id = doc.get_node(cid).and_then(|n| n.prev_sibling);
                            }
                        }
                    }
                    NodeKind::Text => {
                        let content = doc.text_content(current_id).unwrap_or("");
                        escape_xml_to_buf(content, &mut buf);
                    }
                    NodeKind::CData => {
                        let content = doc.text_content(current_id).unwrap_or("");
                        buf.push_str("<![CDATA[");
                        buf.push_str(content);
                        buf.push_str("]]>");
                    }
                    NodeKind::Comment => {
                        let content = doc.text_content(current_id).unwrap_or("");
                        buf.push_str("<!--");
                        buf.push_str(content);
                        buf.push_str("-->");
                    }
                    NodeKind::ProcessingInstruction => {
                        let target = doc.node_name(current_id).unwrap_or("");
                        buf.push_str("<?");
                        buf.push_str(target);
                        buf.push_str("?>");
                    }
                    NodeKind::Document => {
                        if let Some(root_id) = doc.root_element_id() {
                            stack.push(StackEntry::Enter(root_id));
                        }
                    }
                    NodeKind::Attribute => {}
                }
            }
        }
    }

    buf
}

/// Escape XML special characters to buffer
#[inline]
fn escape_xml_to_buf(s: &str, buf: &mut String) {
    for c in s.chars() {
        match c {
            '&' => buf.push_str("&amp;"),
            '<' => buf.push_str("&lt;"),
            '>' => buf.push_str("&gt;"),
            '"' => buf.push_str("&quot;"),
            _ => buf.push(c),
        }
    }
}
