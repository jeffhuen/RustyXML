//! Index Builder
//!
//! Builds a StructuralIndex from XML parsing events.
//! Implements the ScanHandler trait for use with UnifiedScanner.
//!
//! Memory-efficient: builds children from parent links after scan,
//! avoiding temporary buffers during construction.

use super::element::{element_flags, IndexAttribute, IndexElement, IndexText, NO_NODE};
use super::span::Span;
use super::structural::StructuralIndex;
use crate::core::unified_scanner::ScanHandler;

/// Builder state for constructing a StructuralIndex
///
/// Uses a memory-efficient design:
/// - Stack stores only element indices during scan
/// - Children are built from parent links in finish()
pub struct IndexBuilder<'a> {
    /// The index being built
    index: StructuralIndex,
    /// Original input bytes (unused but kept for potential future use)
    #[allow(dead_code)]
    input: &'a [u8],
    /// Stack of open element indices
    stack: Vec<u32>,
    /// Previous sibling element at each depth (for linking siblings)
    prev_sibling_at_depth: Vec<Option<u32>>,
}

impl<'a> IndexBuilder<'a> {
    /// Create a new builder for the given input
    pub fn new(input: &'a [u8]) -> Self {
        // Estimate capacity based on input size
        // Complex XML with nested elements: ~1 element per 35 bytes
        // Whitespace between tags creates many text nodes: ~2 texts per element
        // Attributes: ~0.4 per element on average
        let estimated_elements = (input.len() / 35).max(16);
        let estimated_texts = estimated_elements * 2; // Whitespace creates lots of text nodes
        let estimated_attrs = (estimated_elements * 2) / 5; // ~0.4 attrs per element

        Self {
            index: StructuralIndex::with_capacity(
                estimated_elements,
                estimated_texts,
                estimated_attrs,
            ),
            input,
            stack: Vec::with_capacity(32),
            prev_sibling_at_depth: Vec::with_capacity(32),
        }
    }

    /// Finish building and return the index
    pub fn finish(mut self) -> StructuralIndex {
        // Build children from parent links
        self.index.build_children_from_parents();

        // Release over-allocated capacity from initial estimates.
        // Estimates are based on input size heuristics and often over-allocate
        // by 2-3x. This reclaims significant memory for long-lived documents.
        self.index.shrink_to_fit();

        // Debug output for structural index sizing (disabled by default)
        // Enable by setting RUSTYXML_DEBUG_INDEX=1 environment variable
        #[cfg(feature = "memory_tracking")]
        if std::env::var("RUSTYXML_DEBUG_INDEX").is_ok() {
            eprintln!(
                "[INDEX] counts - elem:{}, text:{}, attr:{}, child:{}",
                self.index.element_count(),
                self.index.text_count(),
                self.index.attribute_count(),
                self.index.children_data_len()
            );
            eprintln!(
                "[INDEX] capacity - elem:{}, text:{}, attr:{}",
                self.index.elements.capacity(),
                self.index.texts.capacity(),
                self.index.attributes.capacity()
            );
            eprintln!(
                "[INDEX] sizes - elem={}B text={}B attr={}B child={}B",
                std::mem::size_of::<super::element::IndexElement>(),
                std::mem::size_of::<super::element::IndexText>(),
                std::mem::size_of::<super::element::IndexAttribute>(),
                std::mem::size_of::<super::element::ChildRef>()
            );
            let elem_cap_bytes = self.index.elements.capacity()
                * std::mem::size_of::<super::element::IndexElement>();
            let text_cap_bytes =
                self.index.texts.capacity() * std::mem::size_of::<super::element::IndexText>();
            let attr_cap_bytes = self.index.attributes.capacity()
                * std::mem::size_of::<super::element::IndexAttribute>();
            let child_cap_bytes =
                self.index.children_data_len() * std::mem::size_of::<super::element::ChildRef>();
            let range_cap_bytes =
                self.index.elements.capacity() * std::mem::size_of::<(u32, u32)>();
            let total_cap = elem_cap_bytes
                + text_cap_bytes
                + attr_cap_bytes
                + child_cap_bytes
                + range_cap_bytes;
            eprintln!(
                "[INDEX] capacity bytes: {:.2} MB",
                total_cap as f64 / 1_000_000.0
            );
        }

        self.index
    }

    /// Get the current depth (number of open elements)
    #[inline]
    fn current_depth(&self) -> u16 {
        self.stack.len() as u16
    }

    /// Get the current parent element index
    #[inline]
    fn current_parent(&self) -> u32 {
        self.stack.last().copied().unwrap_or(NO_NODE)
    }

    /// Handle start of an element
    pub fn start_element(&mut self, name: Span, attrs: &[(Span, Span)], is_empty: bool) {
        let depth = self.current_depth();
        let parent = self.current_parent();

        // Create the element
        let mut elem = IndexElement::new(name, parent, depth);

        // Add attributes
        if !attrs.is_empty() {
            elem.attr_start = self.index.attributes.len() as u32;
            elem.attr_count = attrs.len().min(u16::MAX as usize) as u16;

            for (attr_name, attr_value) in attrs {
                self.index
                    .add_attribute(IndexAttribute::new(*attr_name, *attr_value));
            }
        }

        if is_empty {
            elem.flags |= element_flags::IS_EMPTY;
        }

        // Add to index
        let elem_idx = self.index.add_element(elem);

        // Set as root if this is the first element
        if self.index.root.is_none() {
            self.index.root = Some(elem_idx);
        }

        // Link siblings at this depth
        self.ensure_depth(depth as usize);
        if let Some(prev_idx) = self.prev_sibling_at_depth[depth as usize] {
            self.index.link_sibling(prev_idx, elem_idx);
        }

        // Update first_child of parent if this is the first child element
        if parent != NO_NODE {
            if let Some(parent_elem) = self.index.get_element(parent) {
                if parent_elem.first_child == NO_NODE {
                    self.index.set_first_child(parent, elem_idx);
                }
            }
        }

        // Track this as the previous sibling at this depth
        self.prev_sibling_at_depth[depth as usize] = Some(elem_idx);

        // Clear previous siblings at deeper levels
        for d in (depth as usize + 1)..self.prev_sibling_at_depth.len() {
            self.prev_sibling_at_depth[d] = None;
        }

        if !is_empty {
            // Push onto stack for children
            self.stack.push(elem_idx);
        } else {
            // Update last_child of parent
            if parent != NO_NODE {
                self.index.set_last_child(parent, elem_idx);
            }
        }
    }

    /// Handle end of an element
    pub fn end_element(&mut self, _name: Span) {
        if let Some(elem_idx) = self.stack.pop() {
            // Update last_child of parent
            let parent = self
                .index
                .get_element(elem_idx)
                .map(|e| e.parent)
                .unwrap_or(NO_NODE);
            if parent != NO_NODE {
                self.index.set_last_child(parent, elem_idx);
            }

            // Update last_child to actual last child element
            if let Some(elem) = self.index.get_element(elem_idx) {
                let depth = elem.depth;
                if let Some(last_elem) = self
                    .prev_sibling_at_depth
                    .get(depth as usize + 1)
                    .and_then(|x| *x)
                {
                    self.index.set_last_child(elem_idx, last_elem);
                }
            }
        }
    }

    /// Handle text content
    pub fn text(&mut self, span: Span, needs_entity_decode: bool) {
        let parent = self.current_parent();

        let text = if needs_entity_decode {
            IndexText::new_with_entities(span.offset, span.len as usize, parent)
        } else {
            IndexText::new(span.offset, span.len as usize, parent)
        };

        self.index.add_text(text);
    }

    /// Handle CDATA section
    pub fn cdata(&mut self, span: Span) {
        let parent = self.current_parent();
        let text = IndexText::cdata(span.offset, span.len as usize, parent);
        self.index.add_text(text);
    }

    /// Handle comment
    pub fn comment(&mut self, span: Span) {
        let parent = self.current_parent();
        let text = IndexText::comment(span.offset, span.len as usize, parent);
        self.index.add_text(text);
    }

    /// Handle processing instruction
    pub fn processing_instruction(&mut self, target: Span, _data: Option<Span>) {
        let parent = self.current_parent();
        let text = IndexText::pi(target.offset, target.len as usize, parent);
        self.index.add_text(text);
    }

    /// Ensure prev_sibling_at_depth has capacity for the given depth
    fn ensure_depth(&mut self, depth: usize) {
        while self.prev_sibling_at_depth.len() <= depth {
            self.prev_sibling_at_depth.push(None);
        }
    }
}

// ============================================================================
// ScanHandler Implementation for Zero-Copy Index Building
// ============================================================================

impl<'a> ScanHandler for IndexBuilder<'a> {
    fn start_element(&mut self, name: Span, attrs: &[(Span, Span)], is_empty: bool) {
        IndexBuilder::start_element(self, name, attrs, is_empty);
    }

    fn end_element(&mut self, name: Span) {
        IndexBuilder::end_element(self, name);
    }

    fn text(&mut self, span: Span, needs_entity_decode: bool) {
        IndexBuilder::text(self, span, needs_entity_decode);
    }

    fn cdata(&mut self, span: Span) {
        IndexBuilder::cdata(self, span);
    }

    fn comment(&mut self, span: Span) {
        IndexBuilder::comment(self, span);
    }

    fn processing_instruction(&mut self, target: Span, data: Option<Span>) {
        IndexBuilder::processing_instruction(self, target, data);
    }
}

// ============================================================================
// Build Functions
// ============================================================================

/// Build a StructuralIndex using UnifiedScanner (zero-copy)
///
/// This uses the UnifiedScanner which directly produces Spans without
/// intermediate string allocations, achieving true zero-copy parsing.
pub fn build_index(input: &[u8]) -> StructuralIndex {
    use crate::core::unified_scanner::UnifiedScanner;

    let mut builder = IndexBuilder::new(input);
    let mut scanner = UnifiedScanner::new(input);
    scanner.scan(&mut builder);
    builder.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_simple() {
        let xml = b"<root><child>text</child></root>";
        let index = build_index(xml);

        assert_eq!(index.element_count(), 2);
        assert!(index.root.is_some());
    }

    #[test]
    fn test_build_with_attributes() {
        let xml = b"<root id=\"1\" name=\"test\"><child/></root>";
        let index = build_index(xml);

        assert_eq!(index.element_count(), 2);
        assert_eq!(index.attribute_count(), 2);
    }

    #[test]
    fn test_build_nested() {
        let xml = b"<a><b><c/></b><d/></a>";
        let index = build_index(xml);

        assert_eq!(index.element_count(), 4);

        // Check structure - root has 2 children (b and d)
        let root_children: Vec<_> = index.children(0).collect();
        assert_eq!(root_children.len(), 2);
    }

    #[test]
    fn test_build_with_text() {
        let xml = b"<root>hello<child>world</child>!</root>";
        let index = build_index(xml);

        assert_eq!(index.element_count(), 2);
        assert!(index.text_count() >= 2);
    }
}
