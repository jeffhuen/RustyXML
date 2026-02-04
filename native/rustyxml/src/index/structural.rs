//! Structural Index - Main index structure
//!
//! Stores the entire XML document structure as offsets into the original input.
//! Memory efficient: approximately 32 bytes per element + 16 bytes per text node.

// Allow unused API methods - these are public for library consumers
#![allow(dead_code)]

use super::element::{ChildRef, IndexAttribute, IndexElement, IndexText, NO_NODE};

/// The structural index of an XML document
///
/// Memory layout optimized for cache efficiency:
/// - Elements stored contiguously for fast traversal
/// - Attributes stored contiguously, referenced by (start, count)
/// - Text nodes stored separately, linked via ChildRef
/// - Children stored as a flat list for each element
#[derive(Debug, Default)]
pub struct StructuralIndex {
    /// Element nodes (index 0 is always the root element)
    pub elements: Vec<IndexElement>,
    /// Text nodes (includes text, CDATA, comments, PIs)
    pub texts: Vec<IndexText>,
    /// Attributes (referenced by elements via attr_start/attr_count)
    pub attributes: Vec<IndexAttribute>,
    /// Children for each element - list of ChildRef
    /// Indexed by element index, stores range into children_data
    children_ranges: Vec<(u32, u32)>, // (start, count) for each element
    /// Flat storage of all children references
    children_data: Vec<ChildRef>,
    /// Root element index (None if document is empty)
    pub root: Option<u32>,
}

impl StructuralIndex {
    /// Create a new empty structural index
    pub fn new() -> Self {
        Self {
            elements: Vec::with_capacity(256),
            texts: Vec::with_capacity(128),
            attributes: Vec::with_capacity(256),
            children_ranges: Vec::with_capacity(256),
            children_data: Vec::with_capacity(512),
            root: None,
        }
    }

    /// Create with estimated capacity
    pub fn with_capacity(elements: usize, texts: usize, attributes: usize) -> Self {
        Self {
            elements: Vec::with_capacity(elements),
            texts: Vec::with_capacity(texts),
            attributes: Vec::with_capacity(attributes),
            // children_ranges is rebuilt in build_children_from_parents, start empty
            children_ranges: Vec::new(),
            // children_data is rebuilt in build_children_from_parents, start empty
            children_data: Vec::new(),
            root: None,
        }
    }

    /// Get the root element
    #[inline]
    pub fn root_element(&self) -> Option<&IndexElement> {
        self.root.and_then(|idx| self.elements.get(idx as usize))
    }

    /// Get an element by index
    #[inline]
    pub fn get_element(&self, idx: u32) -> Option<&IndexElement> {
        self.elements.get(idx as usize)
    }

    /// Get a mutable element by index
    #[inline]
    pub fn get_element_mut(&mut self, idx: u32) -> Option<&mut IndexElement> {
        self.elements.get_mut(idx as usize)
    }

    /// Get a text node by index
    #[inline]
    pub fn get_text(&self, idx: u32) -> Option<&IndexText> {
        self.texts.get(idx as usize)
    }

    /// Get element name from input
    #[inline]
    pub fn element_name<'a>(&self, idx: u32, input: &'a [u8]) -> Option<&'a str> {
        self.get_element(idx)?.name.as_str(input)
    }

    /// Get element name bytes from input
    #[inline]
    pub fn element_name_bytes<'a>(&self, idx: u32, input: &'a [u8]) -> Option<&'a [u8]> {
        Some(self.get_element(idx)?.name.slice(input))
    }

    /// Get text content from input
    #[inline]
    pub fn text_content<'a>(&self, idx: u32, input: &'a [u8]) -> Option<&'a str> {
        self.get_text(idx)?.span.as_str(input)
    }

    /// Get text content bytes from input
    #[inline]
    pub fn text_content_bytes<'a>(&self, idx: u32, input: &'a [u8]) -> Option<&'a [u8]> {
        Some(self.get_text(idx)?.span.slice(input))
    }

    /// Get attributes for an element
    #[inline]
    pub fn element_attributes(&self, idx: u32) -> &[IndexAttribute] {
        if let Some(elem) = self.get_element(idx) {
            let start = elem.attr_start as usize;
            let end = start + elem.attr_count as usize;
            if end <= self.attributes.len() {
                return &self.attributes[start..end];
            }
        }
        &[]
    }

    /// Get attribute value by name
    pub fn get_attribute<'a>(&self, elem_idx: u32, name: &str, input: &'a [u8]) -> Option<&'a str> {
        let name_bytes = name.as_bytes();
        for attr in self.element_attributes(elem_idx) {
            if attr.name.slice(input) == name_bytes {
                return attr.value.as_str(input);
            }
        }
        None
    }

    /// Get attribute value bytes by name
    pub fn get_attribute_bytes<'a>(
        &self,
        elem_idx: u32,
        name: &str,
        input: &'a [u8],
    ) -> Option<&'a [u8]> {
        let name_bytes = name.as_bytes();
        for attr in self.element_attributes(elem_idx) {
            if attr.name.slice(input) == name_bytes {
                return Some(attr.value.slice(input));
            }
        }
        None
    }

    /// Get all attribute name-value pairs for an element
    pub fn get_attribute_pairs<'a>(
        &self,
        elem_idx: u32,
        input: &'a [u8],
    ) -> Vec<(&'a str, &'a str)> {
        self.element_attributes(elem_idx)
            .iter()
            .filter_map(|attr| {
                let name = attr.name.as_str(input)?;
                let value = attr.value.as_str(input)?;
                Some((name, value))
            })
            .collect()
    }

    /// Iterate over children of an element
    pub fn children(&self, elem_idx: u32) -> ChildIter<'_> {
        let (start, count) = self
            .children_ranges
            .get(elem_idx as usize)
            .copied()
            .unwrap_or((0, 0));
        let end = start.saturating_add(count);
        ChildIter {
            index: self,
            data_idx: start as usize,
            end_idx: end as usize,
        }
    }

    /// Get number of children for an element
    pub fn child_count(&self, elem_idx: u32) -> usize {
        self.children_ranges
            .get(elem_idx as usize)
            .map(|(_, count)| *count as usize)
            .unwrap_or(0)
    }

    /// Iterate over element children only (skip text nodes)
    pub fn element_children(&self, elem_idx: u32) -> impl Iterator<Item = u32> + '_ {
        self.children(elem_idx).filter_map(|child| {
            if child.is_element() {
                Some(child.index())
            } else {
                None
            }
        })
    }

    /// Iterate over text children only (skip elements)
    pub fn text_children(&self, elem_idx: u32) -> impl Iterator<Item = u32> + '_ {
        self.children(elem_idx).filter_map(|child| {
            if child.is_text() {
                Some(child.index())
            } else {
                None
            }
        })
    }

    /// Iterate over all descendants of an element (depth-first)
    pub fn descendants(&self, elem_idx: u32) -> DescendantIter<'_> {
        let mut stack = Vec::with_capacity(32);
        // Add children in reverse order so first child is processed first
        let (start, count) = self
            .children_ranges
            .get(elem_idx as usize)
            .copied()
            .unwrap_or((0, 0));
        let end = start + count;
        for i in (start..end).rev() {
            if let Some(child) = self.children_data.get(i as usize) {
                stack.push(*child);
            }
        }
        DescendantIter { index: self, stack }
    }

    /// Get parent element of an element
    #[inline]
    pub fn parent(&self, elem_idx: u32) -> Option<u32> {
        let elem = self.get_element(elem_idx)?;
        if elem.parent == NO_NODE {
            None
        } else {
            Some(elem.parent)
        }
    }

    /// Get next sibling of an element
    #[inline]
    pub fn next_sibling(&self, elem_idx: u32) -> Option<u32> {
        let elem = self.get_element(elem_idx)?;
        if elem.next_sibling == NO_NODE {
            None
        } else {
            Some(elem.next_sibling)
        }
    }

    /// Get total number of elements
    #[inline]
    pub fn element_count(&self) -> usize {
        self.elements.len()
    }

    /// Get total number of text nodes
    #[inline]
    pub fn text_count(&self) -> usize {
        self.texts.len()
    }

    /// Get total number of attributes
    #[inline]
    pub fn attribute_count(&self) -> usize {
        self.attributes.len()
    }

    /// Get length of children_data (for builder)
    #[inline]
    pub(crate) fn children_data_len(&self) -> usize {
        self.children_data.len()
    }

    // === Builder methods (used by IndexBuilder) ===

    /// Add an element and return its index
    pub(crate) fn add_element(&mut self, elem: IndexElement) -> u32 {
        let idx = self.elements.len() as u32;
        self.elements.push(elem);
        // Initialize empty children range
        self.children_ranges.push((0, 0));
        idx
    }

    /// Add a text node and return its index
    pub(crate) fn add_text(&mut self, text: IndexText) -> u32 {
        let idx = self.texts.len() as u32;
        self.texts.push(text);
        idx
    }

    /// Add an attribute and return its index
    pub(crate) fn add_attribute(&mut self, attr: IndexAttribute) -> u32 {
        let idx = self.attributes.len() as u32;
        self.attributes.push(attr);
        idx
    }

    /// Set the children for an element (finalizes the element's children list)
    pub(crate) fn set_children(&mut self, elem_idx: u32, children: Vec<ChildRef>) {
        if let Some(range) = self.children_ranges.get_mut(elem_idx as usize) {
            let start = self.children_data.len() as u32;
            let count = children.len() as u32;
            self.children_data.extend(children);
            *range = (start, count);
        }
    }

    /// Link sibling elements
    pub(crate) fn link_sibling(&mut self, prev_idx: u32, next_idx: u32) {
        if let Some(prev) = self.elements.get_mut(prev_idx as usize) {
            prev.next_sibling = next_idx;
        }
    }

    /// Set first and last child of an element
    pub(crate) fn set_first_child(&mut self, parent_idx: u32, child_idx: u32) {
        if let Some(parent) = self.elements.get_mut(parent_idx as usize) {
            parent.first_child = child_idx;
        }
    }

    pub(crate) fn set_last_child(&mut self, parent_idx: u32, child_idx: u32) {
        if let Some(parent) = self.elements.get_mut(parent_idx as usize) {
            parent.last_child = child_idx;
        }
    }

    /// Shrink all internal vectors to fit their contents
    /// Called after building to release unused capacity
    pub(crate) fn shrink_to_fit(&mut self) {
        self.elements.shrink_to_fit();
        self.texts.shrink_to_fit();
        self.attributes.shrink_to_fit();
        self.children_ranges.shrink_to_fit();
        self.children_data.shrink_to_fit();
    }

    /// Build children from parent links
    ///
    /// This method uses the parent field of elements and texts to build
    /// the children_data and children_ranges. Memory efficient because
    /// it processes data already in the index rather than using temporary buffers.
    pub(crate) fn build_children_from_parents(&mut self) {
        let num_elements = self.elements.len();
        if num_elements == 0 {
            return;
        }

        // Count children per element
        let mut counts = vec![0u32; num_elements];

        // Count element children
        for elem in &self.elements {
            if elem.parent != NO_NODE && (elem.parent as usize) < num_elements {
                counts[elem.parent as usize] += 1;
            }
        }

        // Count text children
        for text in &self.texts {
            if text.parent != NO_NODE && (text.parent as usize) < num_elements {
                counts[text.parent as usize] += 1;
            }
        }

        // Compute offsets and reserve space
        let total: u32 = counts.iter().sum();
        self.children_data = Vec::with_capacity(total as usize);
        self.children_ranges = vec![(0, 0); num_elements];

        let mut offset = 0u32;
        for (i, &count) in counts.iter().enumerate() {
            self.children_ranges[i] = (offset, count);
            offset += count;
        }

        // Fill children_data with placeholders
        self.children_data
            .resize(total as usize, ChildRef::element(0));

        // Reset counts for placement tracking
        let mut placed = vec![0u32; num_elements];

        // Place element children (skip root)
        for (elem_idx, elem) in self.elements.iter().enumerate() {
            if elem_idx == 0 {
                continue; // Root has no parent
            }
            if elem.parent != NO_NODE && (elem.parent as usize) < num_elements {
                let parent = elem.parent as usize;
                let (start, _) = self.children_ranges[parent];
                let pos = start + placed[parent];
                self.children_data[pos as usize] = ChildRef::element(elem_idx as u32);
                placed[parent] += 1;
            }
        }

        // Place text children
        for (text_idx, text) in self.texts.iter().enumerate() {
            if text.parent != NO_NODE && (text.parent as usize) < num_elements {
                let parent = text.parent as usize;
                let (start, _) = self.children_ranges[parent];
                let pos = start + placed[parent];
                self.children_data[pos as usize] = ChildRef::text(text_idx as u32);
                placed[parent] += 1;
            }
        }

        // Sort each parent's children by document position to preserve
        // document order for mixed content (e.g. <p>A<b/>C</p>)
        for i in 0..num_elements {
            let (start, count) = self.children_ranges[i];
            if count > 1 {
                let slice = &mut self.children_data[start as usize..(start + count) as usize];
                slice.sort_by_key(|child| {
                    if child.is_text() {
                        self.texts[child.index() as usize].span.span.offset
                    } else {
                        self.elements[child.index() as usize].name.offset
                    }
                });
            }
        }
    }
}

/// Iterator over children of an element
pub struct ChildIter<'a> {
    index: &'a StructuralIndex,
    data_idx: usize,
    end_idx: usize,
}

impl<'a> Iterator for ChildIter<'a> {
    type Item = ChildRef;

    fn next(&mut self) -> Option<Self::Item> {
        if self.data_idx >= self.end_idx {
            return None;
        }
        let child = self.index.children_data.get(self.data_idx)?.to_owned();
        self.data_idx += 1;
        Some(child)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.end_idx.saturating_sub(self.data_idx);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for ChildIter<'_> {}

/// Iterator over descendants (depth-first)
pub struct DescendantIter<'a> {
    index: &'a StructuralIndex,
    stack: Vec<ChildRef>,
}

impl<'a> Iterator for DescendantIter<'a> {
    type Item = ChildRef;

    fn next(&mut self) -> Option<Self::Item> {
        let child = self.stack.pop()?;

        // If this is an element, add its children to the stack
        if child.is_element() {
            let elem_idx = child.index();
            let (start, count) = self
                .index
                .children_ranges
                .get(elem_idx as usize)
                .copied()
                .unwrap_or((0, 0));
            let end = start + count;
            // Add in reverse order for depth-first traversal
            for i in (start..end).rev() {
                if let Some(c) = self.index.children_data.get(i as usize) {
                    self.stack.push(*c);
                }
            }
        }

        Some(child)
    }
}

/// Find elements by name (simple query)
impl StructuralIndex {
    /// Find all elements with the given name
    pub fn find_elements_by_name<'a>(
        &'a self,
        name: &'a str,
        input: &'a [u8],
    ) -> impl Iterator<Item = u32> + 'a {
        let name_bytes = name.as_bytes();
        self.elements
            .iter()
            .enumerate()
            .filter_map(move |(idx, elem)| {
                if elem.name.slice(input) == name_bytes {
                    Some(idx as u32)
                } else {
                    None
                }
            })
    }

    /// Find all elements with the given local name (ignoring prefix)
    pub fn find_elements_by_local_name<'a>(
        &'a self,
        local_name: &'a str,
        input: &'a [u8],
    ) -> impl Iterator<Item = u32> + 'a {
        let local_bytes = local_name.as_bytes();
        self.elements
            .iter()
            .enumerate()
            .filter_map(move |(idx, elem)| {
                let name = elem.name.slice(input);
                // Check for prefix:localname or just localname
                let local = if let Some(pos) = name.iter().position(|&b| b == b':') {
                    &name[pos + 1..]
                } else {
                    name
                };
                if local == local_bytes {
                    Some(idx as u32)
                } else {
                    None
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::span::Span;

    fn make_test_index() -> StructuralIndex {
        let mut index = StructuralIndex::new();

        // Add root element "root" at offset 1 (after '<')
        let root_name = Span::new(1, 4); // "root"
        let root = IndexElement::new(root_name, NO_NODE, 0);
        let root_idx = index.add_element(root);
        index.root = Some(root_idx);

        // Add child element "child" at offset 7
        let child_name = Span::new(7, 5); // "child"
        let mut child = IndexElement::new(child_name, root_idx, 1);
        child.attr_start = 0;
        child.attr_count = 1;
        let child_idx = index.add_element(child);

        // Add attribute for child
        let attr = IndexAttribute::new(Span::new(13, 2), Span::new(17, 5)); // id="value"
        index.add_attribute(attr);

        // Add text node
        let text = IndexText::new(24, 11, child_idx); // "hello world"
        let text_idx = index.add_text(text);

        // Link structure
        index.set_first_child(root_idx, child_idx);
        index.set_last_child(root_idx, child_idx);
        index.set_children(root_idx, vec![ChildRef::element(child_idx)]);
        index.set_children(child_idx, vec![ChildRef::text(text_idx)]);

        index
    }

    #[test]
    fn test_structural_index_basic() {
        let index = make_test_index();

        assert_eq!(index.element_count(), 2);
        assert_eq!(index.text_count(), 1);
        assert_eq!(index.attribute_count(), 1);
        assert!(index.root.is_some());
    }

    #[test]
    fn test_element_name() {
        let index = make_test_index();
        let input = b"<root><child id=\"value\">hello world</child></root>";

        assert_eq!(index.element_name(0, input), Some("root"));
        assert_eq!(index.element_name(1, input), Some("child"));
    }

    #[test]
    fn test_children_iteration() {
        let index = make_test_index();

        let root_children: Vec<_> = index.children(0).collect();
        assert_eq!(root_children.len(), 1);
        assert!(root_children[0].is_element());
        assert_eq!(root_children[0].index(), 1);

        let child_children: Vec<_> = index.children(1).collect();
        assert_eq!(child_children.len(), 1);
        assert!(child_children[0].is_text());
        assert_eq!(child_children[0].index(), 0);
    }

    #[test]
    fn test_attribute_access() {
        let index = make_test_index();
        let input = b"<root><child id=\"value\">hello world</child></root>";

        let attrs = index.element_attributes(1);
        assert_eq!(attrs.len(), 1);

        assert_eq!(index.get_attribute(1, "id", input), Some("value"));
        assert_eq!(index.get_attribute(1, "missing", input), None);
    }
}
