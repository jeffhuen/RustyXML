//! XML Document - Arena-based DOM representation
//!
//! Efficient DOM storage with:
//! - Arena allocation for nodes
//! - NodeId indices for traversal
//! - String interning for names
//! - Zero-copy text content via spans

use super::node::{XmlNode, XmlAttribute, NodeId, NodeKind};
use super::strings::StringPool;
use crate::reader::slice::SliceReader;
use crate::reader::events::XmlEvent;

/// An XML document stored in arena format
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

/// Owned version of XmlDocument that can be stored in a ResourceArc
/// without lifetime issues. All data is fully owned.
pub struct OwnedXmlDocument {
    /// Original input bytes (owned) - kept for reference
    pub input: Vec<u8>,
    /// Arena of nodes
    nodes: Vec<XmlNode>,
    /// Arena of attributes
    attributes: Vec<XmlAttribute>,
    /// Interned strings
    pub strings: StringPool,
    /// Root element node ID
    root_element: Option<NodeId>,
}

impl OwnedXmlDocument {
    /// Parse an XML document and take ownership of the input (lenient mode)
    pub fn parse(input: Vec<u8>) -> Self {
        Self::parse_with_options(input, false)
    }

    /// Parse an XML document in strict mode
    /// Returns Err if the document is not well-formed per XML 1.0
    pub fn parse_strict(input: Vec<u8>) -> Result<Self, String> {
        // Convert encoding if needed (UTF-16 to UTF-8)
        let input = crate::core::encoding::convert_to_utf8(input)?;

        // Use a helper to parse and check for errors
        let result = {
            let doc = XmlDocument::parse_strict(&input)?;
            (doc.nodes, doc.attributes, doc.strings, doc.root_element)
        };

        Ok(OwnedXmlDocument {
            input,
            nodes: result.0,
            attributes: result.1,
            strings: result.2,
            root_element: result.3,
        })
    }

    /// Parse with options
    fn parse_with_options(input: Vec<u8>, _strict: bool) -> Self {
        // Use a helper to parse and return owned components
        let (nodes, attributes, strings, root_element) = {
            let doc = XmlDocument::parse(&input);
            (doc.nodes, doc.attributes, doc.strings, doc.root_element)
        };
        // doc is dropped here, releasing the borrow on input

        OwnedXmlDocument {
            input,
            nodes,
            attributes,
            strings,
            root_element,
        }
    }

    /// Create a borrowed view for XPath evaluation
    /// This is O(1) - no re-parsing!
    pub fn as_borrowed(&self) -> XmlDocumentView<'_> {
        XmlDocumentView {
            input: &self.input,
            nodes: &self.nodes,
            attributes: &self.attributes,
            strings: &self.strings,
            root_element: self.root_element,
        }
    }

    /// Get node count
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get root element name
    pub fn root_name(&self) -> Option<&str> {
        self.root_element
            .and_then(|id| self.nodes.get(id as usize))
            .and_then(|node| self.strings.get_str(node.name_id))
    }
}

/// Borrowed view into an OwnedXmlDocument
/// Provides the same interface as XmlDocument but works with owned data
pub struct XmlDocumentView<'a> {
    input: &'a [u8],
    nodes: &'a [XmlNode],
    attributes: &'a [XmlAttribute],
    pub strings: &'a StringPool,
    root_element: Option<NodeId>,
}

impl<'a> XmlDocumentView<'a> {
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
        self.strings.get_str(node.name_id)
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
            self.strings.get_str(node.name_id)
        } else {
            None
        }
    }

    /// Get attributes for an element
    pub fn attributes(&self, id: NodeId) -> &[XmlAttribute] {
        if let Some(node) = self.get_node(id) {
            let start = node.attr_start as usize;
            let end = start + node.attr_count as usize;
            if end <= self.attributes.len() {
                &self.attributes[start..end]
            } else {
                &[]
            }
        } else {
            &[]
        }
    }

    /// Get attribute value by name
    pub fn get_attribute(&self, node_id: NodeId, name: &str) -> Option<&str> {
        for attr in self.attributes(node_id) {
            if self.strings.get_str(attr.name_id) == Some(name) {
                return self.strings.get_str(attr.value_id);
            }
        }
        None
    }

    /// Get all attribute names and values for a node
    pub fn get_attribute_values(&self, node_id: NodeId) -> Vec<(&str, &str)> {
        self.attributes(node_id)
            .iter()
            .filter_map(|attr| {
                let name = self.strings.get_str(attr.name_id)?;
                let value = self.strings.get_str(attr.value_id)?;
                Some((name, value))
            })
            .collect()
    }

    /// Iterate over children of a node
    pub fn children(&self, id: NodeId) -> ViewChildIter<'_> {
        let first = self.get_node(id).and_then(|n| n.first_child);
        ViewChildIter { view: self, next: first }
    }

    /// Iterate over all descendants of a node
    pub fn descendants(&self, id: NodeId) -> ViewDescendantIter<'_> {
        let mut stack = Vec::new();
        if let Some(node) = self.get_node(id) {
            let mut child_id = node.last_child;
            while let Some(cid) = child_id {
                stack.push(cid);
                child_id = self.get_node(cid).and_then(|n| n.prev_sibling);
            }
        }
        ViewDescendantIter { view: self, stack }
    }

    /// Get total number of nodes
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

/// Iterator over child nodes for XmlDocumentView
pub struct ViewChildIter<'a> {
    view: &'a XmlDocumentView<'a>,
    next: Option<NodeId>,
}

impl<'a> Iterator for ViewChildIter<'a> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next?;
        self.next = self.view.get_node(current).and_then(|n| n.next_sibling);
        Some(current)
    }
}

/// Iterator over descendant nodes for XmlDocumentView
pub struct ViewDescendantIter<'a> {
    view: &'a XmlDocumentView<'a>,
    stack: Vec<NodeId>,
}

impl<'a> Iterator for ViewDescendantIter<'a> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.stack.pop()?;
        if let Some(node) = self.view.get_node(current) {
            let mut child_id = node.last_child;
            while let Some(id) = child_id {
                self.stack.push(id);
                child_id = self.view.get_node(id).and_then(|n| n.prev_sibling);
            }
        }
        Some(current)
    }
}

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
    /// Returns Err if the document is not well-formed per XML 1.0
    pub fn parse_strict(input: &'a [u8]) -> Result<Self, String> {
        let mut doc = XmlDocument {
            input,
            nodes: Vec::with_capacity(256),
            attributes: Vec::with_capacity(128),
            strings: StringPool::new(),
            root_element: None,
        };

        // Create document root node
        doc.nodes.push(XmlNode::document());

        // Build DOM from events in strict mode
        doc.build_from_events(true)?;

        Ok(doc)
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

        // Track document structure for strict mode ordering validation
        let mut seen_xml_decl = false;
        let mut seen_doctype = false;
        let mut seen_root_element = false;

        while let Some(event) = reader.next_event() {
            match event {
                XmlEvent::StartElement(elem) => {
                    // Strict mode: check document structure
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

                    // Check for duplicate attributes in strict mode
                    if strict {
                        if let Some(dup) = Self::find_duplicate_attribute(&elem.attributes) {
                            return Err(format!("Duplicate attribute: {}", dup));
                        }
                    }

                    // Track tag name for matching
                    tag_stack.push(elem.name.as_ref().to_vec());

                    self.handle_element(elem, false, &mut stack, &mut ns_scopes, &mut default_ns);
                }

                XmlEvent::EmptyElement(elem) => {
                    // Strict mode: check document structure
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

                    // Check for duplicate attributes in strict mode
                    if strict {
                        if let Some(dup) = Self::find_duplicate_attribute(&elem.attributes) {
                            return Err(format!("Duplicate attribute: {}", dup));
                        }
                    }

                    self.handle_element(elem, true, &mut stack, &mut ns_scopes, &mut default_ns);
                }

                XmlEvent::EndElement(end_elem) => {
                    // Check tag matching in strict mode
                    if strict {
                        if let Some(start_name) = tag_stack.pop() {
                            if start_name != end_elem.name.as_ref() {
                                let start_str = String::from_utf8_lossy(&start_name);
                                let end_str = String::from_utf8_lossy(end_elem.name.as_ref());
                                return Err(format!("Tag mismatch: <{}> closed with </{}>", start_str, end_str));
                            }
                        } else {
                            let end_str = String::from_utf8_lossy(end_elem.name.as_ref());
                            return Err(format!("Unexpected end tag: </{}> without matching start tag", end_str));
                        }
                    }

                    stack.pop();
                    ns_scopes.pop();
                    default_ns.pop();
                }

                XmlEvent::Text(content) => {
                    // In strict mode, reject CDATA-like content outside elements
                    if strict && stack.len() == 1 {
                        // Check if text is non-whitespace at document level
                        let is_whitespace = content.as_ref().iter().all(|&b| {
                            b == b' ' || b == b'\t' || b == b'\n' || b == b'\r'
                        });
                        if !is_whitespace {
                            return Err("Text content not allowed at document level".to_string());
                        }
                    }

                    let parent_id = *stack.last().unwrap_or(&0);
                    let depth = stack.len() as u16;

                    let text_id = self.strings.intern(content.as_ref());
                    let mut node = XmlNode::text((0, 0), Some(parent_id), depth);
                    node.name_id = text_id;

                    let node_id = self.nodes.len() as NodeId;
                    self.nodes.push(node);
                    self.link_child(parent_id, node_id);
                }

                XmlEvent::CData(content) => {
                    // In strict mode, CDATA is not allowed at document level
                    if strict && stack.len() == 1 {
                        return Err("CDATA section not allowed at document level".to_string());
                    }

                    let parent_id = *stack.last().unwrap_or(&0);
                    let depth = stack.len() as u16;

                    let text_id = self.strings.intern(content.as_ref());
                    let mut node = XmlNode::cdata((0, 0), Some(parent_id), depth);
                    node.name_id = text_id;

                    let node_id = self.nodes.len() as NodeId;
                    self.nodes.push(node);
                    self.link_child(parent_id, node_id);
                }

                XmlEvent::Comment(content) => {
                    let parent_id = *stack.last().unwrap_or(&0);
                    let depth = stack.len() as u16;

                    let text_id = self.strings.intern(content.as_ref());
                    let mut node = XmlNode::comment((0, 0), Some(parent_id), depth);
                    node.name_id = text_id;

                    let node_id = self.nodes.len() as NodeId;
                    self.nodes.push(node);
                    self.link_child(parent_id, node_id);
                }

                XmlEvent::ProcessingInstruction { target, .. } => {
                    let parent_id = *stack.last().unwrap_or(&0);
                    let depth = stack.len() as u16;

                    let target_id = self.strings.intern(target.as_ref());
                    let node = XmlNode::processing_instruction(target_id, (0, 0), Some(parent_id), depth);

                    let node_id = self.nodes.len() as NodeId;
                    self.nodes.push(node);
                    self.link_child(parent_id, node_id);
                }

                XmlEvent::DocType(content) => {
                    // Strict mode: DOCTYPE must come after XMLDecl and before root
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
                    // Strict mode: XMLDecl must be first (before DOCTYPE or elements)
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

                XmlEvent::EndDocument => {
                    // Skip
                }
            }

            // Check for parse errors in strict mode
            if strict {
                if let Some(err) = reader.error() {
                    return Err(err.message.clone());
                }
            }
        }

        // Final error check
        if strict {
            if let Some(err) = reader.error() {
                return Err(err.message.clone());
            }

            // Check for unclosed tags
            if !tag_stack.is_empty() {
                let unclosed = String::from_utf8_lossy(&tag_stack[0]);
                return Err(format!("Unclosed tag: <{}>", unclosed));
            }

            // Post-parse DTD validation
            dtd_decls.validate()?;
        }

        Ok(())
    }

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

        // Intern element name
        let name_id = self.strings.intern(elem.name.as_ref());

        // Create element node
        let mut node = XmlNode::element(name_id, Some(parent_id), depth);

        // Handle namespace prefix if present
        if let Some(ref prefix) = elem.prefix {
            node.prefix_id = self.strings.intern(prefix.as_ref());
        }

        // Process namespace declarations and attributes
        let mut scope_ns: Vec<(u32, u32)> = vec![];
        let mut scope_default: Option<u32> = None;

        let attr_start = self.attributes.len() as u32;
        for attr in &elem.attributes {
            // Check for namespace declarations
            if attr.name.as_ref() == b"xmlns" {
                let uri_id = self.strings.intern(attr.value.as_ref());
                scope_default = Some(uri_id);
            } else if attr.name.as_ref().starts_with(b"xmlns:") {
                let prefix = &attr.name.as_ref()[6..];
                let prefix_id = self.strings.intern(prefix);
                let uri_id = self.strings.intern(attr.value.as_ref());
                scope_ns.push((prefix_id, uri_id));
            }

            // Store attribute
            let attr_name_id = self.strings.intern(attr.name.as_ref());
            let attr_value_id = self.strings.intern(attr.value.as_ref());
            let mut xml_attr = XmlAttribute::new(attr_name_id, attr_value_id, (0, 0));
            if let Some(ref prefix) = attr.prefix {
                xml_attr.prefix_id = self.strings.intern(prefix.as_ref());
            }
            self.attributes.push(xml_attr);
        }
        node.attr_start = attr_start;
        node.attr_count = elem.attributes.len().min(u16::MAX as usize) as u16;

        // Resolve namespace
        if node.prefix_id != 0 {
            // Look up prefix in current and ancestor scopes
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
            // Use default namespace
            node.namespace_id = scope_default.or_else(|| default_ns.last().and_then(|o| *o)).unwrap_or(0);
        }

        // Add node to arena and link to parent
        let node_id = self.nodes.len() as NodeId;
        self.nodes.push(node);
        self.link_child(parent_id, node_id);

        // Track root element
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
        // Get parent's last_child first to avoid borrow issues
        let last_child_opt = self.nodes[parent_id as usize].last_child;

        if let Some(last_child_id) = last_child_opt {
            // Link to previous sibling
            self.nodes[child_id as usize].prev_sibling = Some(last_child_id);
            self.nodes[last_child_id as usize].next_sibling = Some(child_id);
        } else {
            // First child
            self.nodes[parent_id as usize].first_child = Some(child_id);
        }
        self.nodes[parent_id as usize].last_child = Some(child_id);
    }

    /// Get the document root node (index 0)
    pub fn document_node(&self) -> &XmlNode {
        &self.nodes[0]
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

    /// Get a mutable node by ID
    pub fn get_node_mut(&mut self, id: NodeId) -> Option<&mut XmlNode> {
        self.nodes.get_mut(id as usize)
    }

    /// Get node name as string
    pub fn node_name(&self, id: NodeId) -> Option<&str> {
        let node = self.get_node(id)?;
        self.strings.get_str(node.name_id)
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
            self.strings.get_str(node.name_id) // We store text content ID in name_id
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
            if self.strings.get_str(attr.name_id) == Some(name) {
                return self.strings.get_str(attr.value_id);
            }
        }
        None
    }

    /// Get all attribute names and values for a node
    pub fn get_attribute_values(&self, node_id: NodeId) -> Vec<(&str, &str)> {
        self.attributes(node_id)
            .iter()
            .filter_map(|attr| {
                let name = self.strings.get_str(attr.name_id)?;
                let value = self.strings.get_str(attr.value_id)?;
                Some((name, value))
            })
            .collect()
    }

    /// Get attribute value ID by name (for efficient matching)
    pub fn get_attribute_value_id(&self, node_id: NodeId, name: &str) -> Option<u32> {
        for attr in self.attributes(node_id) {
            if self.strings.get_str(attr.name_id) == Some(name) {
                return Some(attr.value_id);
            }
        }
        None
    }

    /// Iterate over children of a node
    pub fn children(&self, id: NodeId) -> ChildIter<'_, 'a> {
        let first = self.get_node(id).and_then(|n| n.first_child);
        ChildIter { doc: self, next: first }
    }

    /// Iterate over all descendants of a node
    pub fn descendants(&self, id: NodeId) -> DescendantIter<'_, 'a> {
        // Initialize stack with all children in reverse order (so first is processed first)
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

    /// Get total number of nodes
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get original input
    pub fn input(&self) -> &'a [u8] {
        self.input
    }
}

/// Iterator over child nodes
pub struct ChildIter<'d, 'a> {
    doc: &'d XmlDocument<'a>,
    next: Option<NodeId>,
}

impl<'d, 'a> Iterator for ChildIter<'d, 'a> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next?;
        self.next = self.doc.get_node(current).and_then(|n| n.next_sibling);
        Some(current)
    }
}

/// Iterator over descendant nodes (depth-first)
pub struct DescendantIter<'d, 'a> {
    doc: &'d XmlDocument<'a>,
    stack: Vec<NodeId>,
}

impl<'d, 'a> Iterator for DescendantIter<'d, 'a> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.stack.pop()?;

        // Add children to stack in reverse order (so first child is processed first)
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
// DocumentAccess trait implementations
// =============================================================================

use super::DocumentAccess;

impl<'a> DocumentAccess for XmlDocument<'a> {
    fn root_element_id(&self) -> Option<NodeId> {
        self.root_element
    }

    fn get_node(&self, id: NodeId) -> Option<&XmlNode> {
        self.nodes.get(id as usize)
    }

    fn node_name(&self, id: NodeId) -> Option<&str> {
        let node = self.get_node(id)?;
        self.strings.get_str(node.name_id)
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
            self.strings.get_str(node.name_id)
        } else {
            None
        }
    }

    fn attributes(&self, id: NodeId) -> &[XmlAttribute] {
        if let Some(node) = self.get_node(id) {
            let start = node.attr_start as usize;
            let end = start + node.attr_count as usize;
            &self.attributes[start..end]
        } else {
            &[]
        }
    }

    fn get_attribute(&self, node_id: NodeId, name: &str) -> Option<&str> {
        for attr in self.attributes(node_id) {
            if self.strings.get_str(attr.name_id) == Some(name) {
                return self.strings.get_str(attr.value_id);
            }
        }
        None
    }

    fn get_attribute_values(&self, node_id: NodeId) -> Vec<(&str, &str)> {
        self.attributes(node_id)
            .iter()
            .filter_map(|attr| {
                let name = self.strings.get_str(attr.name_id)?;
                let value = self.strings.get_str(attr.value_id)?;
                Some((name, value))
            })
            .collect()
    }

    fn strings(&self) -> &StringPool {
        &self.strings
    }

    fn children_vec(&self, id: NodeId) -> Vec<NodeId> {
        self.children(id).collect()
    }

    fn descendants_vec(&self, id: NodeId) -> Vec<NodeId> {
        self.descendants(id).collect()
    }
}

impl<'a> DocumentAccess for XmlDocumentView<'a> {
    fn root_element_id(&self) -> Option<NodeId> {
        self.root_element
    }

    fn get_node(&self, id: NodeId) -> Option<&XmlNode> {
        self.nodes.get(id as usize)
    }

    fn node_name(&self, id: NodeId) -> Option<&str> {
        let node = self.get_node(id)?;
        self.strings.get_str(node.name_id)
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
            self.strings.get_str(node.name_id)
        } else {
            None
        }
    }

    fn attributes(&self, id: NodeId) -> &[XmlAttribute] {
        if let Some(node) = self.get_node(id) {
            let start = node.attr_start as usize;
            let end = start + node.attr_count as usize;
            if end <= self.attributes.len() {
                &self.attributes[start..end]
            } else {
                &[]
            }
        } else {
            &[]
        }
    }

    fn get_attribute(&self, node_id: NodeId, name: &str) -> Option<&str> {
        for attr in self.attributes(node_id) {
            if self.strings.get_str(attr.name_id) == Some(name) {
                return self.strings.get_str(attr.value_id);
            }
        }
        None
    }

    fn get_attribute_values(&self, node_id: NodeId) -> Vec<(&str, &str)> {
        self.attributes(node_id)
            .iter()
            .filter_map(|attr| {
                let name = self.strings.get_str(attr.name_id)?;
                let value = self.strings.get_str(attr.value_id)?;
                Some((name, value))
            })
            .collect()
    }

    fn strings(&self) -> &StringPool {
        self.strings
    }

    fn children_vec(&self, id: NodeId) -> Vec<NodeId> {
        self.children(id).collect()
    }

    fn descendants_vec(&self, id: NodeId) -> Vec<NodeId> {
        self.descendants(id).collect()
    }
}

/// Parse DTD declarations from DOCTYPE content for validation
fn parse_dtd_declarations(
    content: &[u8],
    decls: &mut crate::core::dtd::DtdDeclarations,
) -> Result<(), String> {
    use crate::core::dtd::{EntityDecl, extract_entity_references};

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

            // Skip whitespace
            while pos < len && matches!(content[pos], b' ' | b'\t' | b'\n' | b'\r') {
                pos += 1;
            }

            // Check for parameter entity (%)
            let is_pe = if pos < len && content[pos] == b'%' {
                pos += 1;
                while pos < len && matches!(content[pos], b' ' | b'\t' | b'\n' | b'\r') {
                    pos += 1;
                }
                true
            } else {
                false
            };

            // Read entity name
            let name_start = pos;
            while pos < len && is_name_char(content[pos]) {
                pos += 1;
            }
            if pos == name_start {
                pos += 1;
                continue;
            }
            let name = content[name_start..pos].to_vec();

            // Skip whitespace
            while pos < len && matches!(content[pos], b' ' | b'\t' | b'\n' | b'\r') {
                pos += 1;
            }

            // Determine if internal or external entity
            let mut entity_decl = EntityDecl {
                is_external: false,
                value: None,
                system_id: None,
                public_id: None,
                ndata: None,
                references: vec![],
            };

            if pos < len && (content[pos] == b'"' || content[pos] == b'\'') {
                // Internal entity - quoted value
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
                    pos += 1; // Skip closing quote
                }
            } else if pos + 6 <= len && (&content[pos..pos + 6] == b"SYSTEM" || &content[pos..pos + 6] == b"PUBLIC") {
                entity_decl.is_external = true;
                // Skip to end of declaration - external entities don't cause recursion issues
                // with general entities (only PE expansion would)
            }

            // Add entity declaration
            let _ = decls.add_entity(name, entity_decl, is_pe);

            // Skip to end of declaration
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
