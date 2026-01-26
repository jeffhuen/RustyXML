//! XML Attribute Parsing
//!
//! Parses XML attributes from tag content.

use super::entities::{decode_text, decode_text_strict};
use memchr::memchr;
use std::borrow::Cow;

/// A parsed XML attribute
#[derive(Debug, Clone)]
pub struct Attribute<'a> {
    /// Attribute name (may include namespace prefix)
    pub name: Cow<'a, [u8]>,
    /// Attribute value (entities decoded)
    pub value: Cow<'a, [u8]>,
    /// Local name (after colon, if namespaced)
    pub local_name: Cow<'a, [u8]>,
    /// Namespace prefix (before colon), if any
    pub prefix: Option<Cow<'a, [u8]>>,
}

impl<'a> Attribute<'a> {
    /// Create a new attribute
    pub fn new(name: &'a [u8], value: Cow<'a, [u8]>) -> Self {
        let (prefix, local_name) = split_name(name);
        Attribute {
            name: Cow::Borrowed(name),
            value,
            local_name: Cow::Borrowed(local_name),
            prefix: prefix.map(Cow::Borrowed),
        }
    }

    /// Get the name as a string
    pub fn name_str(&self) -> Option<&str> {
        std::str::from_utf8(self.name.as_ref()).ok()
    }

    /// Get the value as a string
    pub fn value_str(&self) -> Option<&str> {
        std::str::from_utf8(self.value.as_ref()).ok()
    }

    /// Get the local name as a string
    pub fn local_name_str(&self) -> Option<&str> {
        std::str::from_utf8(self.local_name.as_ref()).ok()
    }

    /// Get the prefix as a string
    pub fn prefix_str(&self) -> Option<&str> {
        self.prefix.as_ref().and_then(|p| std::str::from_utf8(p.as_ref()).ok())
    }
}

/// Split a name into prefix and local name at the colon
fn split_name(name: &[u8]) -> (Option<&[u8]>, &[u8]) {
    if let Some(colon_pos) = memchr(b':', name) {
        (Some(&name[..colon_pos]), &name[colon_pos + 1..])
    } else {
        (None, name)
    }
}

/// Parse attributes from raw tag content (after the element name)
///
/// Input should be the content between element name and '>' or '/>'
pub fn parse_attributes(input: &[u8]) -> Vec<Attribute<'_>> {
    parse_attributes_with_validation(input, false).0
}

/// Parse attributes with optional strict validation
/// Returns (attributes, error_message if any)
pub fn parse_attributes_strict(input: &[u8]) -> Result<Vec<Attribute<'_>>, &'static str> {
    let (attrs, error) = parse_attributes_with_validation(input, true);
    match error {
        Some(msg) => Err(msg),
        None => Ok(attrs),
    }
}

fn parse_attributes_with_validation(input: &[u8], strict: bool) -> (Vec<Attribute<'_>>, Option<&'static str>) {
    let mut attrs = Vec::new();
    let mut pos = 0;

    while pos < input.len() {
        // Skip whitespace
        while pos < input.len() && is_whitespace(input[pos]) {
            pos += 1;
        }

        if pos >= input.len() {
            break;
        }

        // Check for end of attributes (/ or >)
        if input[pos] == b'/' || input[pos] == b'>' {
            break;
        }

        // Parse attribute name
        let name_start = pos;

        // First character must be NameStartChar
        if pos < input.len() {
            let first = input[pos];
            if !is_name_start_char(first) {
                if strict {
                    return (attrs, Some("Attribute name must start with letter, underscore, or colon"));
                }
                pos += 1;
                continue;
            }
        }

        while pos < input.len() && is_name_char(input[pos]) {
            pos += 1;
        }

        if pos == name_start {
            // No valid name character found
            pos += 1;
            continue;
        }

        let name = &input[name_start..pos];

        // Strict mode: validate attribute name
        if strict {
            if let Err(msg) = super::unicode::validate_name_fast(name) {
                return (attrs, Some(msg));
            }
        }

        // Skip whitespace around '='
        while pos < input.len() && is_whitespace(input[pos]) {
            pos += 1;
        }

        if pos >= input.len() || input[pos] != b'=' {
            if strict {
                return (attrs, Some("Attribute value required"));
            }
            // Attribute without value (like HTML boolean attributes)
            attrs.push(Attribute::new(name, Cow::Borrowed(b"")));
            continue;
        }

        pos += 1; // Skip '='

        // Skip whitespace
        while pos < input.len() && is_whitespace(input[pos]) {
            pos += 1;
        }

        if pos >= input.len() {
            break;
        }

        // Parse attribute value
        let quote = input[pos];
        if quote != b'"' && quote != b'\'' {
            if strict {
                return (attrs, Some("Attribute value must be quoted"));
            }
            // Unquoted value (non-standard but handle it)
            let value_start = pos;
            while pos < input.len() && !is_whitespace(input[pos]) && input[pos] != b'/' && input[pos] != b'>' {
                pos += 1;
            }
            let value = decode_text(&input[value_start..pos]);
            attrs.push(Attribute::new(name, value));
            continue;
        }

        pos += 1; // Skip opening quote
        let value_start = pos;

        // Find closing quote
        while pos < input.len() && input[pos] != quote {
            // Strict mode: check for invalid characters in attribute value
            if strict {
                if input[pos] == b'<' {
                    return (attrs, Some("Attribute value cannot contain '<'"));
                }
                // Check for unescaped & (must be followed by entity name or #)
                if input[pos] == b'&' {
                    let amp_pos = pos;
                    pos += 1;
                    if pos < input.len() {
                        let next = input[pos];
                        if next == b'#' {
                            // Character reference - skip to ;
                            while pos < input.len() && input[pos] != b';' && input[pos] != quote {
                                pos += 1;
                            }
                        } else if is_name_start_char(next) {
                            // Named entity reference - skip to ;
                            while pos < input.len() && input[pos] != b';' && input[pos] != quote {
                                pos += 1;
                            }
                        } else {
                            return (attrs, Some("Bare '&' not allowed in attribute value"));
                        }
                    } else {
                        return (attrs, Some("Bare '&' not allowed in attribute value"));
                    }
                    pos = amp_pos + 1;
                    continue;
                }
            }
            pos += 1;
        }

        // Check for mismatched quotes
        if strict && pos >= input.len() {
            return (attrs, Some("Attribute value has mismatched quotes"));
        }

        let value_bytes = &input[value_start..pos];
        let value = if strict {
            match decode_text_strict(value_bytes) {
                Ok(v) => v,
                Err(msg) => return (attrs, Some(msg)),
            }
        } else {
            decode_text(value_bytes)
        };
        attrs.push(Attribute::new(name, value));

        if pos < input.len() {
            pos += 1; // Skip closing quote
        }
    }

    (attrs, None)
}

/// Check if byte is a valid XML NameStartChar (ASCII only, non-ASCII handled elsewhere)
#[inline]
fn is_name_start_char(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'_' | b':') || b >= 0x80
}

/// Check if byte is whitespace
#[inline]
fn is_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

/// Check if byte is valid in XML name
#[inline]
fn is_name_char(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b':')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_attributes() {
        let attrs = parse_attributes(b" id=\"test\" class=\"foo\"");
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0].name_str(), Some("id"));
        assert_eq!(attrs[0].value_str(), Some("test"));
        assert_eq!(attrs[1].name_str(), Some("class"));
        assert_eq!(attrs[1].value_str(), Some("foo"));
    }

    #[test]
    fn test_single_quoted() {
        let attrs = parse_attributes(b" id='test'");
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0].value_str(), Some("test"));
    }

    #[test]
    fn test_namespaced_attribute() {
        let attrs = parse_attributes(b" xmlns:xlink=\"http://www.w3.org/1999/xlink\"");
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0].name_str(), Some("xmlns:xlink"));
        assert_eq!(attrs[0].prefix_str(), Some("xmlns"));
        assert_eq!(attrs[0].local_name_str(), Some("xlink"));
    }

    #[test]
    fn test_entity_in_value() {
        let attrs = parse_attributes(b" title=\"&lt;hello&gt;\"");
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0].value_str(), Some("<hello>"));
    }

    #[test]
    fn test_empty_attributes() {
        let attrs = parse_attributes(b"");
        assert_eq!(attrs.len(), 0);
    }

    #[test]
    fn test_whitespace_handling() {
        let attrs = parse_attributes(b"  id  =  \"test\"  ");
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0].name_str(), Some("id"));
        assert_eq!(attrs[0].value_str(), Some("test"));
    }
}
