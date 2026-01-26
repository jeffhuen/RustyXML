//! Zero-Copy Slice Parser (Strategy A)
//!
//! Parses XML from a byte slice with zero-copy semantics.
//! Input references are maintained directly in the output.

use crate::core::tokenizer::{Tokenizer, Token, TokenKind, ParseError};
use crate::core::attributes::{parse_attributes, parse_attributes_strict};
use super::events::{XmlEvent, StartElement, EndElement};

/// Zero-copy XML reader from a byte slice
pub struct SliceReader<'a> {
    input: &'a [u8],
    tokenizer: Tokenizer<'a>,
    strict: bool,
    attr_error: Option<&'static str>,
}

impl<'a> SliceReader<'a> {
    /// Create a new slice reader (lenient mode)
    pub fn new(input: &'a [u8]) -> Self {
        SliceReader {
            input,
            tokenizer: Tokenizer::new(input),
            strict: false,
            attr_error: None,
        }
    }

    /// Create a new slice reader in strict mode
    pub fn new_strict(input: &'a [u8]) -> Self {
        SliceReader {
            input,
            tokenizer: Tokenizer::new_strict(input),
            strict: true,
            attr_error: None,
        }
    }

    /// Get parse error (strict mode only)
    pub fn error(&self) -> Option<&ParseError> {
        // Check for attribute errors first
        if let Some(msg) = self.attr_error {
            // Return a temporary error - this is a bit of a hack but works for now
            static mut TEMP_ERROR: Option<ParseError> = None;
            unsafe {
                TEMP_ERROR = Some(ParseError::new(msg, 0));
                TEMP_ERROR.as_ref()
            }
        } else {
            self.tokenizer.error()
        }
    }

    /// Get the next XML event
    pub fn next_event(&mut self) -> Option<XmlEvent<'a>> {
        loop {
            let token = self.tokenizer.next_token()?;

            match token.kind {
                TokenKind::Eof => return Some(XmlEvent::EndDocument),

                TokenKind::StartTag => {
                    let attrs = self.parse_tag_attributes(&token);
                    let name = token.name?;
                    return Some(XmlEvent::StartElement(StartElement::from_cow(name, attrs)));
                }

                TokenKind::EndTag => {
                    let name = token.name?;
                    return Some(XmlEvent::EndElement(EndElement::from_cow(name)));
                }

                TokenKind::EmptyTag => {
                    let attrs = self.parse_tag_attributes(&token);
                    let name = token.name?;
                    return Some(XmlEvent::EmptyElement(StartElement::from_cow(name, attrs)));
                }

                TokenKind::Text => {
                    if let Some(content) = token.content {
                        // Preserve all text including whitespace-only for XML compliance
                        if !content.is_empty() {
                            return Some(XmlEvent::Text(content));
                        }
                    }
                }

                TokenKind::CData => {
                    if let Some(content) = token.content {
                        return Some(XmlEvent::CData(content));
                    }
                }

                TokenKind::Comment => {
                    if let Some(content) = token.content {
                        return Some(XmlEvent::Comment(content));
                    }
                }

                TokenKind::ProcessingInstruction => {
                    if let Some(name) = token.name {
                        return Some(XmlEvent::ProcessingInstruction {
                            target: name,
                            data: token.content,
                        });
                    }
                }

                TokenKind::XmlDeclaration => {
                    // Parse XML declaration attributes
                    let attrs = self.parse_tag_attributes(&token);
                    let version = attrs.iter()
                        .find(|a| a.name.as_ref() == b"version")
                        .map(|a| a.value.clone())
                        .unwrap_or_else(|| std::borrow::Cow::Borrowed(b"1.0" as &[u8]));
                    let encoding = attrs.iter()
                        .find(|a| a.name.as_ref() == b"encoding")
                        .map(|a| a.value.clone());
                    let standalone = attrs.iter()
                        .find(|a| a.name.as_ref() == b"standalone")
                        .map(|a| a.value.as_ref() == b"yes");

                    return Some(XmlEvent::XmlDeclaration { version, encoding, standalone });
                }

                TokenKind::DocType => {
                    let (start, end) = token.span;
                    let content = &self.input[start..end];
                    return Some(XmlEvent::DocType(std::borrow::Cow::Borrowed(content)));
                }
            }
        }
    }

    /// Parse attributes from a tag token
    fn parse_tag_attributes(&mut self, token: &Token<'a>) -> Vec<crate::core::attributes::Attribute<'a>> {
        let (start, end) = token.span;
        let tag_content = &self.input[start..end];

        // Find where the tag name ends (first whitespace after '<' and optional '/')
        let mut pos = 1; // Skip '<'
        if tag_content.get(1) == Some(&b'/') {
            pos = 2; // Skip '</'
        } else if tag_content.get(1) == Some(&b'?') {
            pos = 2; // Skip '<?'
        }

        // Skip the tag name
        while pos < tag_content.len() {
            let b = tag_content[pos];
            if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' || b == b'>' || b == b'/' || b == b'?' {
                break;
            }
            pos += 1;
        }

        // Find end of attributes (before '>' or '/>' or '?>')
        let mut attr_end = tag_content.len();
        if tag_content.ends_with(b"/>") || tag_content.ends_with(b"?>") {
            attr_end -= 2;
        } else if tag_content.ends_with(b">") {
            attr_end -= 1;
        }

        if pos >= attr_end {
            return Vec::new();
        }

        let attr_content = &tag_content[pos..attr_end];

        if self.strict {
            match parse_attributes_strict(attr_content) {
                Ok(attrs) => attrs,
                Err(msg) => {
                    self.attr_error = Some(msg);
                    Vec::new()
                }
            }
        } else {
            parse_attributes(attr_content)
        }
    }
}

impl<'a> Iterator for SliceReader<'a> {
    type Item = XmlEvent<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let event = self.next_event()?;
        if matches!(event, XmlEvent::EndDocument) {
            None
        } else {
            Some(event)
        }
    }
}

/// Parse XML from a byte slice and return all events
pub fn parse_events(input: &[u8]) -> Vec<XmlEvent<'_>> {
    SliceReader::new(input).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_element() {
        let events: Vec<_> = SliceReader::new(b"<root>hello</root>").collect();
        assert_eq!(events.len(), 3);

        assert!(matches!(&events[0], XmlEvent::StartElement(e) if e.name_str() == Some("root")));
        assert!(matches!(&events[1], XmlEvent::Text(t) if t.as_ref() == b"hello"));
        assert!(matches!(&events[2], XmlEvent::EndElement(e) if e.name_str() == Some("root")));
    }

    #[test]
    fn test_empty_element() {
        let events: Vec<_> = SliceReader::new(b"<br/>").collect();
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], XmlEvent::EmptyElement(e) if e.name_str() == Some("br")));
    }

    #[test]
    fn test_attributes() {
        let events: Vec<_> = SliceReader::new(b"<div id=\"main\" class=\"container\"/>").collect();
        assert_eq!(events.len(), 1);

        if let XmlEvent::EmptyElement(e) = &events[0] {
            assert_eq!(e.get_attribute_value("id"), Some("main"));
            assert_eq!(e.get_attribute_value("class"), Some("container"));
        } else {
            panic!("Expected EmptyElement");
        }
    }

    #[test]
    fn test_cdata() {
        let events: Vec<_> = SliceReader::new(b"<script><![CDATA[alert('hi')]]></script>").collect();
        assert_eq!(events.len(), 3);
        assert!(matches!(&events[1], XmlEvent::CData(c) if c.as_ref() == b"alert('hi')"));
    }

    #[test]
    fn test_comment() {
        let events: Vec<_> = SliceReader::new(b"<root><!-- comment --></root>").collect();
        assert!(events.iter().any(|e| matches!(e, XmlEvent::Comment(_))));
    }

    #[test]
    fn test_nested() {
        let events: Vec<_> = SliceReader::new(b"<a><b>text</b></a>").collect();
        assert_eq!(events.len(), 5);
    }
}
