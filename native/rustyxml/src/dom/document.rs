//! XML Document - Arena-based DOM representation
//!
//! Production code: `validate_strict` for well-formedness validation.
//! Test code: `XmlDocument` DOM used by XPath unit tests.

use crate::reader::events::XmlEvent;
use crate::reader::slice::SliceReader;
use std::borrow::Cow;

// =============================================================================
// XmlDocument - Test only (used by XPath unit tests)
// =============================================================================

#[cfg(test)]
use super::node::{NodeId, NodeKind, XmlAttribute, XmlNode};
#[cfg(test)]
use super::strings::StringPool;

/// An XML document stored in arena format (test-only)
#[cfg(test)]
pub struct XmlDocument<'a> {
    /// Original input (for zero-copy text extraction)
    input: &'a [u8],
    /// Arena of nodes
    nodes: Vec<XmlNode>,
    /// Arena of attributes
    attributes: Vec<XmlAttribute>,
    /// Interned strings
    pub strings: StringPool,
    /// Root element node ID (not document node)
    root_element: Option<NodeId>,
}

#[cfg(test)]
impl<'a> XmlDocument<'a> {
    /// Parse an XML document from a byte slice (lenient mode)
    pub fn parse(input: &'a [u8]) -> Self {
        let mut doc = XmlDocument {
            input,
            nodes: Vec::with_capacity(256),
            attributes: Vec::with_capacity(128),
            strings: StringPool::new(),
            root_element: None,
        };

        // Create document root node
        doc.nodes.push(XmlNode::document());

        // Build DOM from events (lenient mode never fails)
        let _ = doc.build_from_events(false);

        doc
    }

    /// Parse an XML document in strict mode
    pub fn parse_strict(input: &'a [u8]) -> Result<Self, String> {
        let mut doc = XmlDocument {
            input,
            nodes: Vec::with_capacity(256),
            attributes: Vec::with_capacity(128),
            strings: StringPool::new(),
            root_element: None,
        };

        doc.nodes.push(XmlNode::document());
        doc.build_from_events(true)?;
        Ok(doc)
    }

    /// Intern a Cow<[u8]> intelligently:
    /// - If Borrowed (points into input): use intern_ref (zero-copy)
    /// - If Owned (entity-decoded): use intern (copies to pool)
    #[inline]
    fn intern_cow(&mut self, cow: &Cow<'_, [u8]>) -> u32 {
        match cow {
            Cow::Borrowed(slice) => {
                let input_start = self.input.as_ptr() as usize;
                let slice_start = slice.as_ptr() as usize;
                if slice_start >= input_start && slice_start < input_start + self.input.len() {
                    let offset = slice_start - input_start;
                    self.strings.intern_ref(slice, self.input, offset)
                } else {
                    self.strings.intern(slice)
                }
            }
            Cow::Owned(vec) => self.strings.intern(vec),
        }
    }

    /// Build DOM from XML events
    fn build_from_events(&mut self, strict: bool) -> Result<(), String> {
        let mut reader = if strict {
            SliceReader::new_strict(self.input)
        } else {
            SliceReader::new(self.input)
        };
        let mut stack: Vec<NodeId> = vec![0]; // Start with document node
        let mut tag_stack: Vec<Vec<u8>> = vec![]; // Track tag names for matching
        let mut ns_scopes: Vec<Vec<(u32, u32)>> = vec![vec![]]; // Track namespace scopes
        let mut default_ns: Vec<Option<u32>> = vec![None]; // Track default namespace
        let mut root_element_count = 0u32;
        let mut dtd_decls = crate::core::dtd::DtdDeclarations::new();

        let mut seen_xml_decl = false;
        let mut seen_doctype = false;
        let mut seen_root_element = false;
        let mut seen_anything = false;
        let mut first_event = true;

        while let Some(event) = reader.next_event() {
            if strict && first_event {
                first_event = false;
                match &event {
                    XmlEvent::XmlDeclaration { .. } => {}
                    XmlEvent::Comment(_) | XmlEvent::ProcessingInstruction { .. } => {
                        seen_anything = true;
                    }
                    _ => {}
                }
            } else if strict && !seen_xml_decl && seen_anything {
                if let XmlEvent::XmlDeclaration { .. } = &event {
                    return Err(
                        "XML declaration must be at the very beginning of the document".to_string(),
                    );
                }
            }

            match event {
                XmlEvent::StartElement(elem) => {
                    if strict && stack.len() == 1 {
                        if seen_root_element {
                            return Err("Content not allowed after root element".to_string());
                        }
                        root_element_count += 1;
                        if root_element_count > 1 {
                            return Err("Document has multiple root elements".to_string());
                        }
                        seen_root_element = true;
                    }

                    if strict {
                        if let Some(dup) = find_duplicate_attribute(&elem.attributes) {
                            return Err(format!("Duplicate attribute: {}", dup));
                        }
                    }

                    tag_stack.push(elem.name.as_ref().to_vec());
                    self.handle_element(elem, false, &mut stack, &mut ns_scopes, &mut default_ns);
                }

                XmlEvent::EmptyElement(elem) => {
                    if strict && stack.len() == 1 {
                        if seen_root_element {
                            return Err("Content not allowed after root element".to_string());
                        }
                        root_element_count += 1;
                        if root_element_count > 1 {
                            return Err("Document has multiple root elements".to_string());
                        }
                        seen_root_element = true;
                    }

                    if strict {
                        if let Some(dup) = find_duplicate_attribute(&elem.attributes) {
                            return Err(format!("Duplicate attribute: {}", dup));
                        }
                    }

                    self.handle_element(elem, true, &mut stack, &mut ns_scopes, &mut default_ns);
                }

                XmlEvent::EndElement(end_elem) => {
                    if strict {
                        if let Some(start_name) = tag_stack.pop() {
                            if start_name != end_elem.name.as_ref() {
                                let start_str = String::from_utf8_lossy(&start_name);
                                let end_str = String::from_utf8_lossy(end_elem.name.as_ref());
                                return Err(format!(
                                    "Tag mismatch: <{}> closed with </{}>",
                                    start_str, end_str
                                ));
                            }
                        } else {
                            let end_str = String::from_utf8_lossy(end_elem.name.as_ref());
                            return Err(format!(
                                "Unexpected end tag: </{}> without matching start tag",
                                end_str
                            ));
                        }
                    }

                    stack.pop();
                    ns_scopes.pop();
                    default_ns.pop();
                }

                XmlEvent::Text(content) => {
                    if strict && stack.len() == 1 {
                        if matches!(&content, Cow::Owned(_)) {
                            return Err(
                                "Entity/character references not allowed at document level"
                                    .to_string(),
                            );
                        }

                        let is_whitespace = content
                            .as_ref()
                            .iter()
                            .all(|&b| b == b' ' || b == b'\t' || b == b'\n' || b == b'\r');
                        if !is_whitespace {
                            if seen_root_element {
                                return Err("Content not allowed after root element".to_string());
                            } else {
                                return Err(
                                    "Text content not allowed before root element".to_string()
                                );
                            }
                        }
                    }

                    let parent_id = *stack.last().unwrap_or(&0);
                    let depth = stack.len() as u16;
                    let text_id = self.intern_cow(&content);
                    let mut node = XmlNode::text(Some(parent_id), depth);
                    node.name_id = text_id;

                    let node_id = self.nodes.len() as NodeId;
                    self.nodes.push(node);
                    self.link_child(parent_id, node_id);
                }

                XmlEvent::CData(content) => {
                    if strict && stack.len() == 1 {
                        return Err("CDATA section not allowed at document level".to_string());
                    }

                    let parent_id = *stack.last().unwrap_or(&0);
                    let depth = stack.len() as u16;
                    let text_id = self.intern_cow(&content);
                    let mut node = XmlNode::cdata(Some(parent_id), depth);
                    node.name_id = text_id;

                    let node_id = self.nodes.len() as NodeId;
                    self.nodes.push(node);
                    self.link_child(parent_id, node_id);
                }

                XmlEvent::Comment(content) => {
                    let parent_id = *stack.last().unwrap_or(&0);
                    let depth = stack.len() as u16;
                    let text_id = self.intern_cow(&content);
                    let mut node = XmlNode::comment(Some(parent_id), depth);
                    node.name_id = text_id;

                    let node_id = self.nodes.len() as NodeId;
                    self.nodes.push(node);
                    self.link_child(parent_id, node_id);
                }

                XmlEvent::ProcessingInstruction { target, .. } => {
                    let parent_id = *stack.last().unwrap_or(&0);
                    let depth = stack.len() as u16;
                    let target_id = self.intern_cow(&target);
                    let node = XmlNode::processing_instruction(target_id, Some(parent_id), depth);

                    let node_id = self.nodes.len() as NodeId;
                    self.nodes.push(node);
                    self.link_child(parent_id, node_id);
                }

                XmlEvent::DocType(content) => {
                    if strict {
                        if seen_doctype {
                            return Err("Multiple DOCTYPE declarations not allowed".to_string());
                        }
                        if seen_root_element {
                            return Err("DOCTYPE must come before root element".to_string());
                        }
                        seen_doctype = true;
                        parse_dtd_declarations(content.as_ref(), &mut dtd_decls)?;
                    }
                }

                XmlEvent::XmlDeclaration { .. } => {
                    if strict {
                        if seen_doctype {
                            return Err("XML declaration must come before DOCTYPE".to_string());
                        }
                        if seen_root_element {
                            return Err("XML declaration must come before root element".to_string());
                        }
                        seen_xml_decl = true;
                    }
                }

                XmlEvent::EndDocument => {}
            }

            if strict {
                if let Some(err) = reader.error() {
                    return Err(err.message.clone());
                }
            }
        }

        if strict {
            if let Some(err) = reader.error() {
                return Err(err.message.clone());
            }
            if !tag_stack.is_empty() {
                let unclosed = String::from_utf8_lossy(&tag_stack[0]);
                return Err(format!("Unclosed tag: <{}>", unclosed));
            }
            if root_element_count == 0 {
                return Err("Document has no root element".to_string());
            }
            dtd_decls.validate()?;
        }

        Ok(())
    }

    /// Handle start/empty element
    fn handle_element(
        &mut self,
        elem: crate::reader::events::StartElement<'_>,
        is_empty: bool,
        stack: &mut Vec<NodeId>,
        ns_scopes: &mut Vec<Vec<(u32, u32)>>,
        default_ns: &mut Vec<Option<u32>>,
    ) {
        let parent_id = *stack.last().unwrap_or(&0);
        let depth = stack.len() as u16;

        let name_id = self.intern_cow(&elem.name);
        let mut node = XmlNode::element(name_id, Some(parent_id), depth);

        if let Some(ref prefix) = elem.prefix {
            node.prefix_id = self.intern_cow(prefix);
        }

        let mut scope_ns: Vec<(u32, u32)> = vec![];
        let mut scope_default: Option<u32> = None;

        let attr_start = self.attributes.len() as u32;
        for attr in &elem.attributes {
            if attr.name.as_ref() == b"xmlns" {
                let uri_id = self.intern_cow(&attr.value);
                scope_default = Some(uri_id);
            } else if attr.name.as_ref().starts_with(b"xmlns:") {
                let prefix = &attr.name.as_ref()[6..];
                let prefix_id = self.strings.intern(prefix);
                let uri_id = self.intern_cow(&attr.value);
                scope_ns.push((prefix_id, uri_id));
            }

            let attr_name_id = self.intern_cow(&attr.name);
            let attr_value_id = self.intern_cow(&attr.value);
            let mut xml_attr = XmlAttribute::new(attr_name_id, attr_value_id);
            if let Some(ref prefix) = attr.prefix {
                xml_attr.prefix_id = self.intern_cow(prefix);
            }
            self.attributes.push(xml_attr);
        }
        node.attr_start = attr_start;
        node.attr_count = elem.attributes.len().min(u16::MAX as usize) as u16;

        if node.prefix_id != 0 {
            for scope in ns_scopes.iter().rev() {
                for &(p, u) in scope {
                    if p == node.prefix_id {
                        node.namespace_id = u;
                        break;
                    }
                }
                if node.namespace_id != 0 {
                    break;
                }
            }
        } else {
            node.namespace_id = scope_default
                .or_else(|| default_ns.last().and_then(|o| *o))
                .unwrap_or(0);
        }

        let node_id = self.nodes.len() as NodeId;
        self.nodes.push(node);
        self.link_child(parent_id, node_id);

        if self.root_element.is_none() && parent_id == 0 {
            self.root_element = Some(node_id);
        }

        if !is_empty {
            stack.push(node_id);
            ns_scopes.push(scope_ns);
            default_ns.push(scope_default);
        }
    }

    /// Link a child node to its parent
    fn link_child(&mut self, parent_id: NodeId, child_id: NodeId) {
        let last_child_opt = self.nodes[parent_id as usize].last_child;

        if let Some(last_child_id) = last_child_opt {
            self.nodes[child_id as usize].prev_sibling = Some(last_child_id);
            self.nodes[last_child_id as usize].next_sibling = Some(child_id);
        } else {
            self.nodes[parent_id as usize].first_child = Some(child_id);
        }
        self.nodes[parent_id as usize].last_child = Some(child_id);
    }

    /// Get the root element (first element child of document)
    pub fn root_element(&self) -> Option<&XmlNode> {
        self.root_element.map(|id| &self.nodes[id as usize])
    }

    /// Get root element ID
    pub fn root_element_id(&self) -> Option<NodeId> {
        self.root_element
    }

    /// Get a node by ID
    pub fn get_node(&self, id: NodeId) -> Option<&XmlNode> {
        self.nodes.get(id as usize)
    }

    /// Get node name as string
    pub fn node_name(&self, id: NodeId) -> Option<&str> {
        let node = self.get_node(id)?;
        self.strings.get_str_with_input(node.name_id, self.input)
    }

    /// Get node local name (without prefix)
    pub fn node_local_name(&self, id: NodeId) -> Option<&str> {
        let name = self.node_name(id)?;
        if let Some(pos) = name.find(':') {
            Some(&name[pos + 1..])
        } else {
            Some(name)
        }
    }

    /// Get text content of a text node
    pub fn text_content(&self, id: NodeId) -> Option<&str> {
        let node = self.get_node(id)?;
        if node.is_text() || node.kind == NodeKind::CData {
            self.strings.get_str_with_input(node.name_id, self.input)
        } else {
            None
        }
    }

    /// Get attributes for an element
    pub fn attributes(&self, id: NodeId) -> &[XmlAttribute] {
        if let Some(node) = self.get_node(id) {
            let start = node.attr_start as usize;
            let end = start + node.attr_count as usize;
            &self.attributes[start..end]
        } else {
            &[]
        }
    }

    /// Get attribute value by name
    pub fn get_attribute(&self, node_id: NodeId, name: &str) -> Option<&str> {
        for attr in self.attributes(node_id) {
            if self.strings.get_str_with_input(attr.name_id, self.input) == Some(name) {
                return self.strings.get_str_with_input(attr.value_id, self.input);
            }
        }
        None
    }

    /// Get all attribute names and values for a node
    pub fn get_attribute_values(&self, node_id: NodeId) -> Vec<(&str, &str)> {
        self.attributes(node_id)
            .iter()
            .filter_map(|attr| {
                let name = self.strings.get_str_with_input(attr.name_id, self.input)?;
                let value = self.strings.get_str_with_input(attr.value_id, self.input)?;
                Some((name, value))
            })
            .collect()
    }

    /// Iterate over children of a node
    pub fn children(&self, id: NodeId) -> ChildIter<'_, 'a> {
        let first = self.get_node(id).and_then(|n| n.first_child);
        ChildIter {
            doc: self,
            next: first,
        }
    }

    /// Iterate over all descendants of a node
    pub fn descendants(&self, id: NodeId) -> DescendantIter<'_, 'a> {
        let mut stack = Vec::new();
        if let Some(node) = self.get_node(id) {
            let mut child_id = node.last_child;
            while let Some(cid) = child_id {
                stack.push(cid);
                child_id = self.get_node(cid).and_then(|n| n.prev_sibling);
            }
        }
        DescendantIter { doc: self, stack }
    }
}

/// Iterator over child nodes
#[cfg(test)]
pub struct ChildIter<'d, 'a> {
    doc: &'d XmlDocument<'a>,
    next: Option<NodeId>,
}

#[cfg(test)]
impl<'d, 'a> Iterator for ChildIter<'d, 'a> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next?;
        self.next = self.doc.get_node(current).and_then(|n| n.next_sibling);
        Some(current)
    }
}

/// Iterator over descendant nodes (depth-first)
#[cfg(test)]
pub struct DescendantIter<'d, 'a> {
    doc: &'d XmlDocument<'a>,
    stack: Vec<NodeId>,
}

#[cfg(test)]
impl<'d, 'a> Iterator for DescendantIter<'d, 'a> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.stack.pop()?;
        if let Some(node) = self.doc.get_node(current) {
            let mut child_id = node.last_child;
            while let Some(id) = child_id {
                self.stack.push(id);
                child_id = self.doc.get_node(id).and_then(|n| n.prev_sibling);
            }
        }
        Some(current)
    }
}

// =============================================================================
// DocumentAccess trait implementation (test-only)
// =============================================================================

#[cfg(test)]
use super::DocumentAccess;

#[cfg(test)]
impl<'a> DocumentAccess for XmlDocument<'a> {
    fn root_element_id(&self) -> Option<NodeId> {
        self.root_element
    }

    fn get_node(&self, id: NodeId) -> Option<&XmlNode> {
        self.nodes.get(id as usize)
    }

    fn node_name(&self, id: NodeId) -> Option<&str> {
        let node = self.get_node(id)?;
        self.strings.get_str_with_input(node.name_id, self.input)
    }

    fn node_local_name(&self, id: NodeId) -> Option<&str> {
        let name = self.node_name(id)?;
        if let Some(pos) = name.find(':') {
            Some(&name[pos + 1..])
        } else {
            Some(name)
        }
    }

    fn text_content(&self, id: NodeId) -> Option<&str> {
        let node = self.get_node(id)?;
        if node.is_text() || node.kind == NodeKind::CData {
            self.strings.get_str_with_input(node.name_id, self.input)
        } else {
            None
        }
    }

    fn get_attribute(&self, node_id: NodeId, name: &str) -> Option<&str> {
        for attr in self.attributes(node_id) {
            if self.strings.get_str_with_input(attr.name_id, self.input) == Some(name) {
                return self.strings.get_str_with_input(attr.value_id, self.input);
            }
        }
        None
    }

    fn get_attribute_values(&self, node_id: NodeId) -> Vec<(&str, &str)> {
        self.attributes(node_id)
            .iter()
            .filter_map(|attr| {
                let name = self.strings.get_str_with_input(attr.name_id, self.input)?;
                let value = self.strings.get_str_with_input(attr.value_id, self.input)?;
                Some((name, value))
            })
            .collect()
    }

    fn children_vec(&self, id: NodeId) -> Vec<NodeId> {
        self.children(id).collect()
    }

    fn descendants_vec(&self, id: NodeId) -> Vec<NodeId> {
        self.descendants(id).collect()
    }
}

// =============================================================================
// Validation (production code)
// =============================================================================

/// Validate XML is well-formed without building a DOM
///
/// Performs all strict mode validation (tag matching, duplicate attributes,
/// document structure, DTD) without allocating nodes, attributes, or a
/// string pool. This is the memory-efficient validation path used by
/// `parse_strict` before building the structural index.
pub fn validate_strict(input: &[u8]) -> Result<(), String> {
    let mut reader = SliceReader::new_strict(input);
    let mut tag_stack: Vec<Vec<u8>> = vec![];
    let mut depth = 1usize; // 1 = document level (like stack starting with doc node)
    let mut root_element_count = 0u32;
    let mut dtd_decls = crate::core::dtd::DtdDeclarations::new();

    let mut seen_xml_decl = false;
    let mut seen_doctype = false;
    let mut seen_root_element = false;
    let mut seen_anything = false;
    let mut first_event = true;

    while let Some(event) = reader.next_event() {
        // Check ordering: XML decl must be first if present
        if first_event {
            first_event = false;
            match &event {
                XmlEvent::XmlDeclaration { .. } => {}
                XmlEvent::Comment(_) | XmlEvent::ProcessingInstruction { .. } => {
                    seen_anything = true;
                }
                _ => {}
            }
        } else if !seen_xml_decl && seen_anything {
            if let XmlEvent::XmlDeclaration { .. } = &event {
                return Err(
                    "XML declaration must be at the very beginning of the document".to_string(),
                );
            }
        }

        match event {
            XmlEvent::StartElement(elem) => {
                if depth == 1 {
                    if seen_root_element {
                        return Err("Content not allowed after root element".to_string());
                    }
                    root_element_count += 1;
                    if root_element_count > 1 {
                        return Err("Document has multiple root elements".to_string());
                    }
                    seen_root_element = true;
                }

                if let Some(dup) = find_duplicate_attribute(&elem.attributes) {
                    return Err(format!("Duplicate attribute: {}", dup));
                }

                tag_stack.push(elem.name.as_ref().to_vec());
                depth += 1;
            }

            XmlEvent::EmptyElement(elem) => {
                if depth == 1 {
                    if seen_root_element {
                        return Err("Content not allowed after root element".to_string());
                    }
                    root_element_count += 1;
                    if root_element_count > 1 {
                        return Err("Document has multiple root elements".to_string());
                    }
                    seen_root_element = true;
                }

                if let Some(dup) = find_duplicate_attribute(&elem.attributes) {
                    return Err(format!("Duplicate attribute: {}", dup));
                }
            }

            XmlEvent::EndElement(end_elem) => {
                if let Some(start_name) = tag_stack.pop() {
                    if start_name != end_elem.name.as_ref() {
                        let start_str = String::from_utf8_lossy(&start_name);
                        let end_str = String::from_utf8_lossy(end_elem.name.as_ref());
                        return Err(format!(
                            "Tag mismatch: <{}> closed with </{}>",
                            start_str, end_str
                        ));
                    }
                } else {
                    let end_str = String::from_utf8_lossy(end_elem.name.as_ref());
                    return Err(format!(
                        "Unexpected end tag: </{}> without matching start tag",
                        end_str
                    ));
                }
                depth -= 1;
            }

            XmlEvent::Text(content) => {
                if depth == 1 {
                    if matches!(&content, Cow::Owned(_)) {
                        return Err(
                            "Entity/character references not allowed at document level".to_string()
                        );
                    }
                    let is_whitespace = content
                        .as_ref()
                        .iter()
                        .all(|&b| b == b' ' || b == b'\t' || b == b'\n' || b == b'\r');
                    if !is_whitespace {
                        if seen_root_element {
                            return Err("Content not allowed after root element".to_string());
                        } else {
                            return Err("Text content not allowed before root element".to_string());
                        }
                    }
                }
            }

            XmlEvent::CData(_) => {
                if depth == 1 {
                    return Err("CDATA section not allowed at document level".to_string());
                }
            }

            XmlEvent::Comment(_) => {}
            XmlEvent::ProcessingInstruction { .. } => {}

            XmlEvent::DocType(content) => {
                if seen_doctype {
                    return Err("Multiple DOCTYPE declarations not allowed".to_string());
                }
                if seen_root_element {
                    return Err("DOCTYPE must come before root element".to_string());
                }
                seen_doctype = true;
                parse_dtd_declarations(content.as_ref(), &mut dtd_decls)?;
            }

            XmlEvent::XmlDeclaration { .. } => {
                if seen_doctype {
                    return Err("XML declaration must come before DOCTYPE".to_string());
                }
                if seen_root_element {
                    return Err("XML declaration must come before root element".to_string());
                }
                seen_xml_decl = true;
            }

            XmlEvent::EndDocument => {}
        }

        // Check for parse errors
        if let Some(err) = reader.error() {
            return Err(err.message.clone());
        }
    }

    // Final checks
    if let Some(err) = reader.error() {
        return Err(err.message.clone());
    }

    if !tag_stack.is_empty() {
        let unclosed = String::from_utf8_lossy(&tag_stack[0]);
        return Err(format!("Unclosed tag: <{}>", unclosed));
    }

    if root_element_count == 0 {
        return Err("Document has no root element".to_string());
    }

    dtd_decls.validate()?;

    Ok(())
}

// =============================================================================
// Helpers
// =============================================================================

/// Find duplicate attribute name (for strict mode validation)
fn find_duplicate_attribute(attrs: &[crate::core::attributes::Attribute<'_>]) -> Option<String> {
    if attrs.len() < 2 {
        return None;
    }
    for i in 0..attrs.len() {
        for j in (i + 1)..attrs.len() {
            if attrs[i].name.as_ref() == attrs[j].name.as_ref() {
                return Some(String::from_utf8_lossy(attrs[i].name.as_ref()).to_string());
            }
        }
    }
    None
}

/// Parse DTD declarations from DOCTYPE content for validation
fn parse_dtd_declarations(
    content: &[u8],
    decls: &mut crate::core::dtd::DtdDeclarations,
) -> Result<(), String> {
    use crate::core::dtd::{extract_entity_references, EntityDecl};

    let mut pos = 0;
    let len = content.len();

    while pos < len {
        // Skip whitespace
        while pos < len && matches!(content[pos], b' ' | b'\t' | b'\n' | b'\r') {
            pos += 1;
        }

        if pos >= len {
            break;
        }

        // Look for <!ENTITY
        if pos + 8 <= len && &content[pos..pos + 8] == b"<!ENTITY" {
            pos += 8;

            while pos < len && matches!(content[pos], b' ' | b'\t' | b'\n' | b'\r') {
                pos += 1;
            }

            let is_pe = if pos < len && content[pos] == b'%' {
                pos += 1;
                while pos < len && matches!(content[pos], b' ' | b'\t' | b'\n' | b'\r') {
                    pos += 1;
                }
                true
            } else {
                false
            };

            let name_start = pos;
            while pos < len && is_name_char(content[pos]) {
                pos += 1;
            }
            if pos == name_start {
                pos += 1;
                continue;
            }
            let name = content[name_start..pos].to_vec();

            while pos < len && matches!(content[pos], b' ' | b'\t' | b'\n' | b'\r') {
                pos += 1;
            }

            let mut entity_decl = EntityDecl {
                is_external: false,
                value: None,
                system_id: None,
                public_id: None,
                ndata: None,
                references: vec![],
            };

            if pos < len && (content[pos] == b'"' || content[pos] == b'\'') {
                let quote = content[pos];
                pos += 1;
                let value_start = pos;
                while pos < len && content[pos] != quote {
                    pos += 1;
                }
                let value = content[value_start..pos].to_vec();
                entity_decl.references = extract_entity_references(&value);
                entity_decl.value = Some(value);
                if pos < len {
                    pos += 1;
                }
            } else if pos + 6 <= len
                && (&content[pos..pos + 6] == b"SYSTEM" || &content[pos..pos + 6] == b"PUBLIC")
            {
                entity_decl.is_external = true;
            }

            let _ = decls.add_entity(name, entity_decl, is_pe);

            while pos < len && content[pos] != b'>' {
                pos += 1;
            }
            if pos < len {
                pos += 1;
            }
        } else {
            pos += 1;
        }
    }

    Ok(())
}

#[inline]
fn is_name_char(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b':') || b >= 0x80
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let doc = XmlDocument::parse(b"<root>hello</root>");
        assert!(doc.root_element().is_some());
        assert_eq!(doc.node_name(doc.root_element_id().unwrap()), Some("root"));
    }

    #[test]
    fn test_parse_nested() {
        let doc = XmlDocument::parse(b"<a><b><c/></b></a>");
        let root = doc.root_element_id().unwrap();
        let children: Vec<_> = doc.children(root).collect();
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn test_descendants() {
        let doc = XmlDocument::parse(b"<root><a/><b><c/></b></root>");
        let root = doc.root_element_id().unwrap();
        let descendants: Vec<_> = doc.descendants(root).collect();
        // a, b, c
        assert_eq!(descendants.len(), 3);
    }

    #[test]
    fn test_siblings() {
        let doc = XmlDocument::parse(b"<root><a/><b/><c/></root>");
        let root = doc.root_element_id().unwrap();
        let children: Vec<_> = doc.children(root).collect();
        assert_eq!(children.len(), 3);

        // Check sibling links
        let first = doc.get_node(children[0]).unwrap();
        assert!(first.prev_sibling.is_none());
        assert!(first.next_sibling.is_some());
    }
}
