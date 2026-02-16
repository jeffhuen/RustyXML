//! Unified Scanner with ScanHandler Trait
//!
//! Provides a trait-based interface for XML scanning that enables both:
//! - Index building (for DOM/XPath queries)
//! - SAX event collection (for streaming APIs)
//!
//! The scanner uses the existing memchr-based Scanner for byte searching.

use super::scanner::Scanner;
use crate::index::Span;

/// Trait for handling scan events
///
/// Implement this trait to receive XML parsing events. The scanner calls
/// these methods as it tokenizes the input, passing spans (byte offsets)
/// instead of string copies for zero-copy efficiency.
pub trait ScanHandler {
    /// Called when an element starts
    ///
    /// # Arguments
    /// * `name` - Span of the element name in the input
    /// * `attrs` - Slice of (name_span, value_span) pairs
    /// * `is_empty` - True if this is a self-closing element (e.g., `<br/>`)
    fn start_element(&mut self, name: Span, attrs: &[(Span, Span)], is_empty: bool);

    /// Called when an element ends
    ///
    /// # Arguments
    /// * `name` - Span of the element name in the input
    fn end_element(&mut self, name: Span);

    /// Called for text content
    ///
    /// # Arguments
    /// * `span` - Span of the text in the input
    /// * `needs_entity_decode` - True if the text contains entity references
    fn text(&mut self, span: Span, needs_entity_decode: bool);

    /// Called for CDATA sections
    ///
    /// # Arguments
    /// * `span` - Span of the CDATA content (excluding `<![CDATA[` and `]]>`)
    fn cdata(&mut self, span: Span);

    /// Called for comments
    ///
    /// # Arguments
    /// * `span` - Span of the comment content (excluding `<!--` and `-->`)
    fn comment(&mut self, span: Span);

    /// Called for processing instructions
    ///
    /// # Arguments
    /// * `target` - Span of the PI target
    /// * `data` - Optional span of the PI data
    fn processing_instruction(&mut self, target: Span, data: Option<Span>);

    /// Called for XML declaration (optional, default does nothing)
    fn xml_declaration(
        &mut self,
        _version: Option<Span>,
        _encoding: Option<Span>,
        _standalone: Option<Span>,
    ) {
    }

    /// Called for DOCTYPE (optional, default does nothing)
    fn doctype(&mut self, _content: Span) {}
}

/// Unified scanner that uses ScanHandler for event dispatch
pub struct UnifiedScanner<'a> {
    input: &'a [u8],
    scanner: Scanner<'a>,
    /// Reusable attribute buffer to avoid per-element allocations
    attrs_buf: Vec<(Span, Span)>,
}

impl<'a> UnifiedScanner<'a> {
    /// Create a new unified scanner for the input
    pub fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            scanner: Scanner::new(input),
            attrs_buf: Vec::with_capacity(8), // Most elements have < 8 attrs
        }
    }

    /// Scan the entire document, calling handler methods for each token
    pub fn scan<H: ScanHandler>(&mut self, handler: &mut H) {
        while !self.scanner.is_eof() {
            // Look for next interesting character
            match self.scanner.peek() {
                Some(b'<') => {
                    self.scan_markup(handler);
                }
                Some(_) => {
                    self.scan_text(handler);
                }
                None => break,
            }
        }
    }

    /// Scan markup starting with '<'
    fn scan_markup<H: ScanHandler>(&mut self, handler: &mut H) {
        let start = self.scanner.position();
        self.scanner.advance(1); // Skip '<'

        match self.scanner.peek() {
            Some(b'/') => {
                // End tag
                self.scanner.advance(1);
                self.scan_end_tag(handler);
            }
            Some(b'!') => {
                // Comment, CDATA, or DOCTYPE
                self.scanner.advance(1);
                match (self.scanner.peek(), self.scanner.peek_at(1)) {
                    (Some(b'-'), Some(b'-')) => {
                        self.scanner.advance(2);
                        self.scan_comment(handler);
                    }
                    (Some(b'['), _) => {
                        // CDATA or conditional
                        if self.scanner.starts_with(b"[CDATA[") {
                            self.scanner.advance(7);
                            self.scan_cdata(handler);
                        } else {
                            // Skip unknown
                            self.skip_to_tag_end();
                        }
                    }
                    (Some(b'D'), _) | (Some(b'd'), _) => {
                        // DOCTYPE
                        self.scan_doctype(handler);
                    }
                    _ => {
                        self.skip_to_tag_end();
                    }
                }
            }
            Some(b'?') => {
                // Processing instruction
                self.scanner.advance(1);
                self.scan_pi(handler);
            }
            Some(c) if is_name_start_char(c) => {
                // Start tag
                self.scanner.set_position(start); // Reset to '<'
                self.scan_start_tag(handler);
            }
            _ => {
                // Invalid markup (e.g., "<1invalid/>"), treat '<' as text
                // Do NOT reset to start - stay at position after '<' so scan_text
                // handles remaining content, while we emit '<' as literal text
                let lt_span = Span::new(start as u32, 1);
                handler.text(lt_span, false);
                // Continue scanning from current position (after '<')
            }
        }
    }

    /// Scan a start tag
    fn scan_start_tag<H: ScanHandler>(&mut self, handler: &mut H) {
        self.scanner.advance(1); // Skip '<'

        // Read element name
        let name_start = self.scanner.position();
        if self.scanner.read_name().is_none() {
            return;
        }
        let name_end = self.scanner.position();
        let name_span = Span::new(
            name_start as u32,
            (name_end - name_start).min(u16::MAX as usize) as u16,
        );

        // Parse attributes - reuse buffer to avoid per-element allocations
        self.attrs_buf.clear();
        self.scanner.skip_whitespace();

        while !self.scanner.is_eof() {
            match self.scanner.peek() {
                Some(b'>') => {
                    self.scanner.advance(1);
                    handler.start_element(name_span, &self.attrs_buf, false);
                    return;
                }
                Some(b'/') => {
                    self.scanner.advance(1);
                    if self.scanner.peek() == Some(b'>') {
                        self.scanner.advance(1);
                        handler.start_element(name_span, &self.attrs_buf, true);
                        return;
                    }
                }
                Some(c) if is_name_start_char(c) => {
                    if let Some((name, value)) = self.scan_attribute() {
                        self.attrs_buf.push((name, value));
                    }
                }
                _ => {
                    self.scanner.advance(1);
                }
            }
            self.scanner.skip_whitespace();
        }
    }

    /// Scan an attribute, returning (name_span, value_span)
    fn scan_attribute(&mut self) -> Option<(Span, Span)> {
        let name_start = self.scanner.position();
        self.scanner.read_name()?;
        let name_end = self.scanner.position();

        self.scanner.skip_whitespace();

        // Expect '='
        if self.scanner.peek() != Some(b'=') {
            return None;
        }
        self.scanner.advance(1);
        self.scanner.skip_whitespace();

        // Read quoted value
        let quote = self.scanner.peek()?;
        if quote != b'"' && quote != b'\'' {
            return None;
        }
        self.scanner.advance(1);

        let value_start = self.scanner.position();
        // Find closing quote
        while let Some(c) = self.scanner.peek() {
            if c == quote {
                break;
            }
            self.scanner.advance(1);
        }
        let value_end = self.scanner.position();

        if self.scanner.peek() == Some(quote) {
            self.scanner.advance(1);
        }

        let name_span = Span::new(
            name_start as u32,
            (name_end - name_start).min(u16::MAX as usize) as u16,
        );
        let value_span = Span::new(
            value_start as u32,
            (value_end - value_start).min(u16::MAX as usize) as u16,
        );

        Some((name_span, value_span))
    }

    /// Scan an end tag
    fn scan_end_tag<H: ScanHandler>(&mut self, handler: &mut H) {
        self.scanner.skip_whitespace();

        let name_start = self.scanner.position();
        if self.scanner.read_name().is_none() {
            self.skip_to_tag_end();
            return;
        }
        let name_end = self.scanner.position();

        self.scanner.skip_whitespace();

        // Expect '>'
        if self.scanner.peek() == Some(b'>') {
            self.scanner.advance(1);
        }

        let name_span = Span::new(
            name_start as u32,
            (name_end - name_start).min(u16::MAX as usize) as u16,
        );
        handler.end_element(name_span);
    }

    /// Scan text content
    fn scan_text<H: ScanHandler>(&mut self, handler: &mut H) {
        let start = self.scanner.position();
        let mut needs_decode = false;

        while let Some(c) = self.scanner.peek() {
            match c {
                b'<' => break,
                b'&' => {
                    needs_decode = true;
                    self.scanner.advance(1);
                }
                _ => {
                    self.scanner.advance(1);
                }
            }
        }

        let end = self.scanner.position();
        if end > start {
            let span = Span::new(start as u32, (end - start).min(u16::MAX as usize) as u16);
            handler.text(span, needs_decode);
        }
    }

    /// Scan a comment
    fn scan_comment<H: ScanHandler>(&mut self, handler: &mut H) {
        let content_start = self.scanner.position();

        // Find "-->"
        loop {
            match self.scanner.find_byte(b'-') {
                Some(pos) => {
                    self.scanner.set_position(pos);
                    if self.scanner.starts_with(b"-->") {
                        let content_end = pos;
                        let span = Span::new(
                            content_start as u32,
                            (content_end - content_start).min(u16::MAX as usize) as u16,
                        );
                        self.scanner.advance(3);
                        handler.comment(span);
                        return;
                    }
                    self.scanner.advance(1);
                }
                None => {
                    // Unterminated comment
                    let content_end = self.input.len();
                    let span = Span::new(
                        content_start as u32,
                        (content_end - content_start).min(u16::MAX as usize) as u16,
                    );
                    self.scanner.set_position(content_end);
                    handler.comment(span);
                    return;
                }
            }
        }
    }

    /// Scan a CDATA section
    fn scan_cdata<H: ScanHandler>(&mut self, handler: &mut H) {
        let content_start = self.scanner.position();

        // Find "]]>"
        loop {
            match self.scanner.find_byte(b']') {
                Some(pos) => {
                    self.scanner.set_position(pos);
                    if self.scanner.starts_with(b"]]>") {
                        let content_end = pos;
                        let span = Span::new(
                            content_start as u32,
                            (content_end - content_start).min(u16::MAX as usize) as u16,
                        );
                        self.scanner.advance(3);
                        handler.cdata(span);
                        return;
                    }
                    self.scanner.advance(1);
                }
                None => {
                    // Unterminated CDATA
                    let content_end = self.input.len();
                    let span = Span::new(
                        content_start as u32,
                        (content_end - content_start).min(u16::MAX as usize) as u16,
                    );
                    self.scanner.set_position(content_end);
                    handler.cdata(span);
                    return;
                }
            }
        }
    }

    /// Scan a processing instruction
    fn scan_pi<H: ScanHandler>(&mut self, handler: &mut H) {
        let target_start = self.scanner.position();
        if self.scanner.read_name().is_none() {
            self.skip_to_pi_end();
            return;
        }
        let target_end = self.scanner.position();

        self.scanner.skip_whitespace();

        let data_start = self.scanner.position();

        // Find "?>"
        loop {
            match self.scanner.find_byte(b'?') {
                Some(pos) => {
                    self.scanner.set_position(pos);
                    if self.scanner.peek_at(1) == Some(b'>') {
                        let data_end = pos;
                        let target_span = Span::new(
                            target_start as u32,
                            (target_end - target_start).min(u16::MAX as usize) as u16,
                        );
                        let data_span = if data_end > data_start {
                            Some(Span::new(
                                data_start as u32,
                                (data_end - data_start).min(u16::MAX as usize) as u16,
                            ))
                        } else {
                            None
                        };
                        self.scanner.advance(2);
                        handler.processing_instruction(target_span, data_span);
                        return;
                    }
                    self.scanner.advance(1);
                }
                None => {
                    // Unterminated PI
                    let target_span = Span::new(
                        target_start as u32,
                        (target_end - target_start).min(u16::MAX as usize) as u16,
                    );
                    handler.processing_instruction(target_span, None);
                    self.scanner.set_position(self.input.len());
                    return;
                }
            }
        }
    }

    /// Scan DOCTYPE
    fn scan_doctype<H: ScanHandler>(&mut self, handler: &mut H) {
        let start = self.scanner.position();

        // Skip "DOCTYPE"
        if self.scanner.starts_with(b"DOCTYPE") || self.scanner.starts_with(b"doctype") {
            self.scanner.advance(7);
        }

        // Find end of DOCTYPE, handling internal subset
        let mut depth = 0;

        while let Some(c) = self.scanner.peek() {
            match c {
                b'[' => {
                    depth += 1;
                    self.scanner.advance(1);
                }
                b']' => {
                    depth -= 1;
                    self.scanner.advance(1);
                }
                b'>' if depth == 0 => {
                    let end = self.scanner.position();
                    let span = Span::new(start as u32, (end - start).min(u16::MAX as usize) as u16);
                    self.scanner.advance(1);
                    handler.doctype(span);
                    return;
                }
                _ => {
                    self.scanner.advance(1);
                }
            }
        }
    }

    /// Skip to the end of a tag (find '>')
    fn skip_to_tag_end(&mut self) {
        if let Some(pos) = self.scanner.find_tag_end_quoted() {
            self.scanner.set_position(pos + 1);
        } else {
            self.scanner.set_position(self.input.len());
        }
    }

    /// Skip to end of PI
    fn skip_to_pi_end(&mut self) {
        loop {
            match self.scanner.find_byte(b'?') {
                Some(pos) => {
                    self.scanner.set_position(pos);
                    if self.scanner.peek_at(1) == Some(b'>') {
                        self.scanner.advance(2);
                        return;
                    }
                    self.scanner.advance(1);
                }
                None => {
                    self.scanner.set_position(self.input.len());
                    return;
                }
            }
        }
    }
}

/// Check if byte is valid XML name start character
#[inline]
fn is_name_start_char(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'_' | b':') || b >= 0x80
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test handler that collects events
    struct TestHandler {
        elements: Vec<(String, bool)>, // (name, is_empty)
        end_elements: Vec<String>,
        texts: Vec<(String, bool)>, // (content, needs_decode)
        comments: Vec<String>,
        cdatas: Vec<String>,
    }

    impl TestHandler {
        fn new() -> Self {
            Self {
                elements: Vec::new(),
                end_elements: Vec::new(),
                texts: Vec::new(),
                comments: Vec::new(),
                cdatas: Vec::new(),
            }
        }
    }

    impl ScanHandler for TestHandler {
        fn start_element(&mut self, name: Span, _attrs: &[(Span, Span)], is_empty: bool) {
            // We can't get the actual name without the input, so just store the span info
            self.elements
                .push((format!("@{}:{}", name.offset, name.len), is_empty));
        }

        fn end_element(&mut self, name: Span) {
            self.end_elements
                .push(format!("@{}:{}", name.offset, name.len));
        }

        fn text(&mut self, span: Span, needs_decode: bool) {
            self.texts
                .push((format!("@{}:{}", span.offset, span.len), needs_decode));
        }

        fn cdata(&mut self, span: Span) {
            self.cdatas.push(format!("@{}:{}", span.offset, span.len));
        }

        fn comment(&mut self, span: Span) {
            self.comments.push(format!("@{}:{}", span.offset, span.len));
        }

        fn processing_instruction(&mut self, _target: Span, _data: Option<Span>) {}
    }

    #[test]
    fn test_simple_element() {
        let input = b"<root/>";
        let mut scanner = UnifiedScanner::new(input);
        let mut handler = TestHandler::new();
        scanner.scan(&mut handler);

        assert_eq!(handler.elements.len(), 1);
        assert!(handler.elements[0].1); // is_empty = true
    }

    #[test]
    fn test_nested_elements() {
        let input = b"<a><b/></a>";
        let mut scanner = UnifiedScanner::new(input);
        let mut handler = TestHandler::new();
        scanner.scan(&mut handler);

        assert_eq!(handler.elements.len(), 2);
        assert_eq!(handler.end_elements.len(), 1);
    }

    #[test]
    fn test_text_content() {
        let input = b"<a>hello</a>";
        let mut scanner = UnifiedScanner::new(input);
        let mut handler = TestHandler::new();
        scanner.scan(&mut handler);

        assert_eq!(handler.texts.len(), 1);
        assert!(!handler.texts[0].1); // no entity decode needed
    }

    #[test]
    fn test_entity_detection() {
        let input = b"<a>hello &amp; world</a>";
        let mut scanner = UnifiedScanner::new(input);
        let mut handler = TestHandler::new();
        scanner.scan(&mut handler);

        assert_eq!(handler.texts.len(), 1);
        assert!(handler.texts[0].1); // entity decode needed
    }

    #[test]
    fn test_comment() {
        let input = b"<!-- comment --><a/>";
        let mut scanner = UnifiedScanner::new(input);
        let mut handler = TestHandler::new();
        scanner.scan(&mut handler);

        assert_eq!(handler.comments.len(), 1);
        assert_eq!(handler.elements.len(), 1);
    }

    #[test]
    fn test_cdata() {
        let input = b"<a><![CDATA[content]]></a>";
        let mut scanner = UnifiedScanner::new(input);
        let mut handler = TestHandler::new();
        scanner.scan(&mut handler);

        assert_eq!(handler.cdatas.len(), 1);
    }

    #[test]
    fn test_invalid_markup_as_text() {
        // Invalid markup like "<1invalid/>" should be treated as text, not cause infinite loop
        let input = b"<1invalid/>";
        let mut scanner = UnifiedScanner::new(input);
        let mut handler = TestHandler::new();
        scanner.scan(&mut handler);

        // The '<' should be emitted as text, then "1invalid/>" as more text
        assert!(
            !handler.texts.is_empty(),
            "Invalid markup should produce text events"
        );
        assert_eq!(
            handler.elements.len(),
            0,
            "No valid elements in invalid markup"
        );
    }

    #[test]
    fn test_invalid_markup_mixed_with_valid() {
        // Mix of invalid and valid markup
        let input = b"<1bad/><good/>";
        let mut scanner = UnifiedScanner::new(input);
        let mut handler = TestHandler::new();
        scanner.scan(&mut handler);

        // Should have text for "<1bad/>" and one valid element
        assert!(
            !handler.texts.is_empty(),
            "Invalid markup should produce text"
        );
        assert_eq!(
            handler.elements.len(),
            1,
            "One valid element should be found"
        );
    }
}
