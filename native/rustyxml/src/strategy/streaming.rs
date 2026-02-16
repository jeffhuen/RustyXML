//! Streaming XML Parser (Strategy D)
//!
//! Stateful parser that processes XML in chunks with bounded memory.

use crate::core::attributes::parse_attributes;
use crate::core::tokenizer::{TokenKind, Tokenizer};
use memchr::memchr_iter;

/// Stateful streaming XML parser
pub struct StreamingParser {
    /// Accumulated buffer for incomplete input
    buffer: Vec<u8>,
    /// Parsed events ready to be consumed
    events: Vec<OwnedXmlEvent>,
    /// Complete elements ready to be consumed (faster path - no event rebuild)
    complete_elements: Vec<Vec<u8>>,
    /// Builder for current element being captured (start_pos in current chunk, accumulated bytes)
    element_builder: Option<ElementBuilder>,
    /// Whether we're inside an element (tracking quote state, etc.)
    in_quote: bool,
    /// Current depth in element tree
    depth: usize,
    /// Tag filter (only emit events for matching tags)
    tag_filter: Option<Vec<u8>>,
    /// Depth when we entered a target element (0 = not inside target)
    inside_target_depth: usize,
    /// Skip event generation (when only complete_elements are needed)
    elements_only: bool,
}

/// Builder for capturing complete elements
struct ElementBuilder {
    /// Accumulated bytes from previous chunks
    accumulated: Vec<u8>,
    /// Start position in current buffer (where element begins)
    start_in_buffer: usize,
    /// Depth when we started (to know when we're done)
    start_depth: usize,
}

/// Owned version of XmlEvent for storage
#[derive(Debug, Clone)]
pub enum OwnedXmlEvent {
    StartElement {
        name: Vec<u8>,
        attributes: Vec<(Vec<u8>, Vec<u8>)>,
    },
    EndElement {
        name: Vec<u8>,
    },
    EmptyElement {
        name: Vec<u8>,
        attributes: Vec<(Vec<u8>, Vec<u8>)>,
    },
    Text(Vec<u8>),
    CData(Vec<u8>),
    Comment(Vec<u8>),
    ProcessingInstruction {
        target: Vec<u8>,
        data: Vec<u8>,
    },
}

/// Find a safe boundary in a buffer (last `>` not inside quotes).
///
/// Returns the byte offset just past the last valid `>`, or 0 if none found.
/// Shared by `StreamingParser::process_buffer` and the streaming SAX NIFs.
pub fn find_safe_boundary(buf: &[u8]) -> usize {
    let len = buf.len();

    if len < 1024 {
        // Single-pass: scan once, track quotes, find last valid '>'
        let mut last_valid_gt = 0;
        let mut in_single = false;
        let mut in_double = false;

        for (i, &b) in buf.iter().enumerate() {
            match b {
                b'"' if !in_single => in_double = !in_double,
                b'\'' if !in_double => in_single = !in_single,
                b'>' if !in_single && !in_double => last_valid_gt = i + 1,
                _ => {}
            }
        }
        last_valid_gt
    } else {
        // For large buffers, use memchr SIMD to find '>' positions
        let mut last_valid_gt = 0;
        let mut in_single = false;
        let mut in_double = false;
        let mut pos = 0;

        for gt_pos in memchr_iter(b'>', buf) {
            for &b in &buf[pos..gt_pos] {
                match b {
                    b'"' if !in_single => in_double = !in_double,
                    b'\'' if !in_double => in_single = !in_single,
                    _ => {}
                }
            }
            pos = gt_pos + 1;

            if !in_single && !in_double {
                last_valid_gt = gt_pos + 1;
            }
        }
        last_valid_gt
    }
}

impl StreamingParser {
    /// Create a new streaming parser
    pub fn new() -> Self {
        StreamingParser {
            buffer: Vec::with_capacity(8192),
            events: Vec::with_capacity(64),
            complete_elements: Vec::with_capacity(16),
            element_builder: None,
            in_quote: false,
            depth: 0,
            tag_filter: None,
            inside_target_depth: 0,
            elements_only: false,
        }
    }

    /// Create a new streaming parser with a tag filter.
    /// Uses elements-only mode: skips event generation since the fast path
    /// (take_elements) provides complete XML strings without needing events.
    pub fn with_filter(tag: &[u8]) -> Self {
        StreamingParser {
            buffer: Vec::with_capacity(8192),
            events: Vec::new(),
            complete_elements: Vec::with_capacity(16),
            element_builder: None,
            in_quote: false,
            depth: 0,
            tag_filter: Some(tag.to_vec()),
            inside_target_depth: 0,
            elements_only: true,
        }
    }

    /// Feed a chunk of data to the parser
    pub fn feed(&mut self, chunk: &[u8]) {
        self.buffer.extend_from_slice(chunk);
        self.process_buffer();
    }

    /// Process the buffer to extract complete events
    fn process_buffer(&mut self) {
        // Find the last complete element boundary
        let boundary = self.find_safe_boundary();

        if boundary == 0 {
            return; // Not enough data
        }

        // Process directly from buffer slice - avoids allocation for the common case
        // We need to collect events with owned data, but we skip the intermediate Vec copy
        self.process_slice(boundary);

        // If we're building an element that spans chunks, save processed content
        if let Some(ref mut builder) = self.element_builder {
            // Accumulate everything from start to boundary
            builder
                .accumulated
                .extend_from_slice(&self.buffer[builder.start_in_buffer..boundary]);
            // Reset start to 0 since we're draining up to boundary
            builder.start_in_buffer = 0;
        }

        // Remove processed bytes efficiently using drain (no reallocation needed,
        // just moves remaining bytes to front)
        self.buffer.drain(..boundary);

        // Shrink buffer to avoid retaining excess capacity from large chunks.
        // Cap at 8KB to match initial capacity, preventing unbounded growth
        // in long-lived streaming parsers.
        self.buffer.shrink_to(8192);
    }

    /// Process a slice of the buffer up to the given boundary
    /// Builds complete elements directly (faster than event reconstruction)
    fn process_slice(&mut self, boundary: usize) {
        // Tokenize directly from buffer slice (zero-copy tokenization)
        let mut tokenizer = Tokenizer::new(&self.buffer[..boundary]);

        while let Some(token) = tokenizer.next_token() {
            match token.kind {
                TokenKind::Eof => break,

                TokenKind::StartTag => {
                    if let Some(name) = token.name {
                        let name_bytes = name.into_owned();
                        self.depth += 1;

                        // Check if this is a target tag (entering target element)
                        let is_entering_target =
                            self.is_target_tag(&name_bytes) && self.inside_target_depth == 0;
                        if is_entering_target {
                            self.inside_target_depth = self.depth;
                            // Start building element - capture from token start
                            let start_pos = token.span.0;
                            self.element_builder = Some(ElementBuilder {
                                accumulated: Vec::new(),
                                start_in_buffer: start_pos,
                                start_depth: self.depth,
                            });
                        }

                        // Emit event if we're inside a target element (skip in elements_only mode)
                        if !self.elements_only && self.inside_target_depth > 0 {
                            let attrs = self.extract_attributes_from_buffer(boundary, token.span);
                            self.events.push(OwnedXmlEvent::StartElement {
                                name: name_bytes,
                                attributes: attrs,
                            });
                        }
                    }
                }

                TokenKind::EndTag => {
                    if let Some(name) = token.name {
                        let name_bytes = name.into_owned();

                        // Emit event if we're inside a target element (skip in elements_only mode)
                        if !self.elements_only && self.inside_target_depth > 0 {
                            self.events
                                .push(OwnedXmlEvent::EndElement { name: name_bytes });
                        }

                        // Check if we're leaving the target element
                        if self.depth == self.inside_target_depth {
                            self.inside_target_depth = 0;

                            // Complete the element!
                            if let Some(builder) = self.element_builder.take() {
                                let end_pos = token.span.1;
                                let mut element = builder.accumulated;
                                element.extend_from_slice(
                                    &self.buffer[builder.start_in_buffer..end_pos],
                                );
                                self.complete_elements.push(element);
                            }
                        }

                        self.depth = self.depth.saturating_sub(1);
                    }
                }

                TokenKind::EmptyTag => {
                    if let Some(name) = token.name {
                        let name_bytes = name.into_owned();

                        // Check if this is a target tag at top level
                        let is_target_at_top =
                            self.is_target_tag(&name_bytes) && self.inside_target_depth == 0;

                        // If this is a target empty element, add it directly as complete
                        if is_target_at_top {
                            let start_pos = token.span.0;
                            let end_pos = token.span.1;
                            self.complete_elements
                                .push(self.buffer[start_pos..end_pos].to_vec());
                        }

                        // Emit event if inside target OR if this IS a target empty element
                        // (skip in elements_only mode)
                        if !self.elements_only && (self.inside_target_depth > 0 || is_target_at_top)
                        {
                            let attrs = self.extract_attributes_from_buffer(boundary, token.span);
                            self.events.push(OwnedXmlEvent::EmptyElement {
                                name: name_bytes,
                                attributes: attrs,
                            });
                        }
                    }
                }

                TokenKind::Text => {
                    // Only emit text if inside a target element (skip in elements_only mode)
                    if !self.elements_only && self.inside_target_depth > 0 {
                        if let Some(content) = token.content {
                            let bytes = content.into_owned();
                            // Preserve all text including whitespace-only for XML compliance
                            if !bytes.is_empty() {
                                self.events.push(OwnedXmlEvent::Text(bytes));
                            }
                        }
                    }
                }

                TokenKind::CData => {
                    if !self.elements_only && self.inside_target_depth > 0 {
                        if let Some(content) = token.content {
                            self.events.push(OwnedXmlEvent::CData(content.into_owned()));
                        }
                    }
                }

                TokenKind::Comment => {
                    if !self.elements_only && self.inside_target_depth > 0 {
                        if let Some(content) = token.content {
                            self.events
                                .push(OwnedXmlEvent::Comment(content.into_owned()));
                        }
                    }
                }

                _ => {}
            }
        }
    }

    /// Extract attributes from buffer directly (avoids copy)
    fn extract_attributes_from_buffer(
        &self,
        boundary: usize,
        span: (usize, usize),
    ) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.extract_attributes(&self.buffer[..boundary], span)
    }

    /// Find a safe boundary in the buffer (delegates to standalone fn)
    fn find_safe_boundary(&self) -> usize {
        find_safe_boundary(&self.buffer)
    }

    /// Check if this is a target tag we're looking for
    fn is_target_tag(&self, tag: &[u8]) -> bool {
        match &self.tag_filter {
            Some(filter) => tag == filter.as_slice(),
            None => true, // No filter means all tags are targets
        }
    }

    /// Extract attributes from a tag
    fn extract_attributes(&self, input: &[u8], span: (usize, usize)) -> Vec<(Vec<u8>, Vec<u8>)> {
        let (start, end) = span;
        if end <= start || end > input.len() {
            return Vec::new();
        }

        let tag_content = &input[start..end];

        // Skip '<' and optional '?' or '/'
        let mut pos = 1;
        if tag_content.get(1) == Some(&b'/') || tag_content.get(1) == Some(&b'?') {
            pos = 2;
        }

        // Skip tag name
        while pos < tag_content.len() {
            let b = tag_content[pos];
            if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' || b == b'>' || b == b'/' {
                break;
            }
            pos += 1;
        }

        // Find end of attributes
        let mut attr_end = tag_content.len();
        if tag_content.ends_with(b"/>") || tag_content.ends_with(b"?>") {
            attr_end -= 2;
        } else if tag_content.ends_with(b">") {
            attr_end -= 1;
        }

        if pos >= attr_end {
            return Vec::new();
        }

        let attrs = parse_attributes(&tag_content[pos..attr_end]);
        attrs
            .into_iter()
            .map(|a| (a.name.into_owned(), a.value.into_owned()))
            .collect()
    }

    /// Take up to `max` parsed events
    /// Returns events and leaves remaining events in place
    pub fn take_events(&mut self, max: usize) -> Vec<OwnedXmlEvent> {
        let count = max.min(self.events.len());
        if count == self.events.len() {
            // Take all events - swap with empty vec (no allocation)
            std::mem::take(&mut self.events)
        } else {
            // Partial take - use drain
            let taken: Vec<_> = self.events.drain(..count).collect();
            // Shrink remaining events vec to release excess capacity
            self.events.shrink_to(64);
            taken
        }
    }

    /// Take up to `max` complete elements (faster path - no event rebuild needed)
    /// Returns complete XML strings for target elements
    pub fn take_elements(&mut self, max: usize) -> Vec<Vec<u8>> {
        let count = max.min(self.complete_elements.len());
        if count == self.complete_elements.len() {
            std::mem::take(&mut self.complete_elements)
        } else {
            let taken: Vec<_> = self.complete_elements.drain(..count).collect();
            // Shrink remaining elements vec to release excess capacity
            self.complete_elements.shrink_to(16);
            taken
        }
    }

    /// Get number of available events
    pub fn available_events(&self) -> usize {
        self.events.len()
    }

    /// Get number of available complete elements
    pub fn available_elements(&self) -> usize {
        self.complete_elements.len()
    }

    /// Get buffer size
    pub fn buffer_size(&self) -> usize {
        self.buffer.len()
    }

    /// Check if there's unprocessed data
    pub fn has_pending(&self) -> bool {
        !self.buffer.is_empty()
    }

    /// Finalize parsing - process any remaining data
    pub fn finalize(&mut self) -> Vec<OwnedXmlEvent> {
        // Process remaining buffer as-is
        if !self.buffer.is_empty() {
            let remaining = std::mem::take(&mut self.buffer);
            let mut tokenizer = Tokenizer::new(&remaining);

            while let Some(token) = tokenizer.next_token() {
                match token.kind {
                    TokenKind::Eof => break,
                    TokenKind::Text => {
                        if let Some(content) = token.content {
                            let bytes = content.into_owned();
                            // Preserve all text including whitespace-only for XML compliance
                            if !bytes.is_empty() {
                                self.events.push(OwnedXmlEvent::Text(bytes));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        std::mem::take(&mut self.events)
    }
}

impl Default for StreamingParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_simple() {
        let mut parser = StreamingParser::new();
        parser.feed(b"<root>");
        parser.feed(b"<item/>");
        parser.feed(b"</root>");

        let events = parser.take_events(10);
        assert!(!events.is_empty());
    }

    #[test]
    fn test_streaming_chunks() {
        let mut parser = StreamingParser::new();
        parser.feed(b"<ro");
        parser.feed(b"ot><i");
        parser.feed(b"tem/></root>");

        let events = parser.take_events(10);
        assert!(!events.is_empty());
    }

    #[test]
    fn test_with_filter() {
        let mut parser = StreamingParser::with_filter(b"item");
        parser.feed(b"<root><item/><other/><item/></root>");

        // with_filter uses elements_only mode (no events generated)
        let elements = parser.take_elements(10);
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0], b"<item/>");
        assert_eq!(elements[1], b"<item/>");
        assert_eq!(parser.available_events(), 0);
    }
}
