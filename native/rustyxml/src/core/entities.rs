//! XML Entity Decoding
//!
//! Handles decoding of XML entities:
//! - Built-in entities: &lt; &gt; &amp; &quot; &apos;
//! - Numeric character references: &#123; &#x7B;
//!
//! Uses Cow for zero-copy when no entities are present.

use memchr::memchr;
use std::borrow::Cow;

/// Decode text content, handling entity references
///
/// Returns Borrowed if no entities present (zero-copy),
/// returns Owned if entities were decoded.
#[inline]
pub fn decode_text(input: &[u8]) -> Cow<'_, [u8]> {
    // Fast path: check if there are any entities using SIMD
    if memchr(b'&', input).is_none() {
        return Cow::Borrowed(input);
    }
    // Slow path: decode entities
    Cow::Owned(decode_entities(input))
}

/// Decode text content in strict mode
/// Returns Err if any character reference refers to an invalid XML character
pub fn decode_text_strict(input: &[u8]) -> Result<Cow<'_, [u8]>, &'static str> {
    // First validate that all bytes are valid XML characters
    for &b in input {
        if !is_valid_xml_byte(b) {
            return Err("Invalid XML character in content");
        }
    }

    // Fast path: check if there are any entities using SIMD
    if memchr(b'&', input).is_none() {
        return Ok(Cow::Borrowed(input));
    }
    // Slow path: decode entities with strict validation
    decode_entities_strict(input).map(Cow::Owned)
}

/// Decode all entity references with strict XML character validation
fn decode_entities_strict(input: &[u8]) -> Result<Vec<u8>, &'static str> {
    let mut result = Vec::with_capacity(input.len());
    let mut pos = 0;

    while pos < input.len() {
        if let Some(amp_pos) = memchr(b'&', &input[pos..]) {
            // Copy everything before the entity
            result.extend_from_slice(&input[pos..pos + amp_pos]);
            pos += amp_pos;

            // Find the semicolon
            if let Some(semi_offset) = memchr(b';', &input[pos..]) {
                let entity = &input[pos + 1..pos + semi_offset];

                if let Some(decoded) = decode_entity_strict(entity)? {
                    result.extend_from_slice(decoded.as_bytes());
                    pos += semi_offset + 1;
                } else {
                    // Unknown entity, keep as-is (for user-defined entities)
                    result.push(b'&');
                    pos += 1;
                }
            } else {
                // No semicolon found, keep the ampersand
                result.push(b'&');
                pos += 1;
            }
        } else {
            // No more entities, copy the rest
            result.extend_from_slice(&input[pos..]);
            break;
        }
    }

    Ok(result)
}

/// Decode a single entity with strict validation
fn decode_entity_strict(entity: &[u8]) -> Result<Option<String>, &'static str> {
    if entity.is_empty() {
        return Ok(None);
    }

    // Numeric character reference
    if entity[0] == b'#' {
        match decode_numeric_entity_impl(&entity[1..], true) {
            Some(s) => Ok(Some(s)),
            None => Err("Invalid character reference"),
        }
    } else {
        // Named entity
        Ok(match entity {
            b"lt" => Some("<".to_string()),
            b"gt" => Some(">".to_string()),
            b"amp" => Some("&".to_string()),
            b"quot" => Some("\"".to_string()),
            b"apos" => Some("'".to_string()),
            _ => None, // Unknown entity - leave for DTD processing
        })
    }
}

/// Decode all entity references in the input
pub fn decode_entities(input: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(input.len());
    let mut pos = 0;

    while pos < input.len() {
        if let Some(amp_pos) = memchr(b'&', &input[pos..]) {
            // Copy everything before the entity
            result.extend_from_slice(&input[pos..pos + amp_pos]);
            pos += amp_pos;

            // Find the semicolon
            if let Some(semi_offset) = memchr(b';', &input[pos..]) {
                let entity = &input[pos + 1..pos + semi_offset];

                if let Some(decoded) = decode_entity(entity) {
                    result.extend_from_slice(decoded.as_bytes());
                    pos += semi_offset + 1;
                } else {
                    // Unknown entity, keep as-is
                    result.push(b'&');
                    pos += 1;
                }
            } else {
                // No semicolon found, keep the ampersand
                result.push(b'&');
                pos += 1;
            }
        } else {
            // No more entities, copy the rest
            result.extend_from_slice(&input[pos..]);
            break;
        }
    }

    result
}

/// Decode a single entity (without & and ;)
fn decode_entity(entity: &[u8]) -> Option<String> {
    if entity.is_empty() {
        return None;
    }

    // Numeric character reference
    if entity[0] == b'#' {
        return decode_numeric_entity(&entity[1..]);
    }

    // Named entity
    match entity {
        b"lt" => Some("<".to_string()),
        b"gt" => Some(">".to_string()),
        b"amp" => Some("&".to_string()),
        b"quot" => Some("\"".to_string()),
        b"apos" => Some("'".to_string()),
        // HTML5 named entities (common ones)
        b"nbsp" => Some("\u{00A0}".to_string()),
        b"copy" => Some("\u{00A9}".to_string()),
        b"reg" => Some("\u{00AE}".to_string()),
        b"trade" => Some("\u{2122}".to_string()),
        b"mdash" => Some("\u{2014}".to_string()),
        b"ndash" => Some("\u{2013}".to_string()),
        b"lsquo" => Some("\u{2018}".to_string()),
        b"rsquo" => Some("\u{2019}".to_string()),
        b"ldquo" => Some("\u{201C}".to_string()),
        b"rdquo" => Some("\u{201D}".to_string()),
        b"hellip" => Some("\u{2026}".to_string()),
        _ => None,
    }
}

/// Decode a numeric character reference
fn decode_numeric_entity(entity: &[u8]) -> Option<String> {
    decode_numeric_entity_impl(entity, false)
}

/// Decode a numeric character reference with optional strict XML character validation
fn decode_numeric_entity_impl(entity: &[u8], strict: bool) -> Option<String> {
    if entity.is_empty() {
        return None;
    }

    let codepoint = if entity[0] == b'x' || entity[0] == b'X' {
        // Hexadecimal: &#xHHHH;
        let hex = std::str::from_utf8(&entity[1..]).ok()?;
        u32::from_str_radix(hex, 16).ok()?
    } else {
        // Decimal: &#DDDD;
        let dec = std::str::from_utf8(entity).ok()?;
        dec.parse::<u32>().ok()?
    };

    // In strict mode, validate against XML 1.0 Char production
    if strict && !is_valid_xml_char(codepoint) {
        return None;
    }

    // Convert codepoint to character
    char::from_u32(codepoint).map(|c| c.to_string())
}

/// Check if a code point is a valid XML 1.0 Char
/// Char ::= #x9 | #xA | #xD | [#x20-#xD7FF] | [#xE000-#xFFFD] | [#x10000-#x10FFFF]
#[inline]
pub fn is_valid_xml_char(codepoint: u32) -> bool {
    matches!(codepoint,
        0x9 | 0xA | 0xD |
        0x20..=0xD7FF |
        0xE000..=0xFFFD |
        0x10000..=0x10FFFF
    )
}

/// Check if a byte is a valid XML Char (for single-byte content validation)
#[inline]
pub fn is_valid_xml_byte(b: u8) -> bool {
    // Valid single-byte chars: Tab (0x9), LF (0xA), CR (0xD), and 0x20-0x7F
    // Bytes 0x80+ are potentially valid UTF-8 continuation bytes
    matches!(b, 0x9 | 0xA | 0xD | 0x20..=0x7F) || b >= 0x80
}

/// Validate UTF-8 content for invalid XML characters
/// Returns Err if content contains characters not allowed in XML 1.0
pub fn validate_xml_content(content: &[u8]) -> Result<(), &'static str> {
    let mut pos = 0;
    while pos < content.len() {
        let b = content[pos];

        if b < 0x80 {
            // ASCII - check for control characters
            if b < 0x20 && b != 0x09 && b != 0x0A && b != 0x0D {
                return Err("Invalid XML character: control character not allowed");
            }
            if b == 0x7F {
                return Err("Invalid XML character: DEL not allowed");
            }
            pos += 1;
        } else if b < 0xC0 {
            // Invalid UTF-8 start byte (continuation byte at start)
            return Err("Invalid UTF-8 encoding");
        } else if b < 0xE0 {
            // 2-byte sequence
            if pos + 1 >= content.len() {
                return Err("Invalid UTF-8 encoding: truncated sequence");
            }
            let c1 = content[pos + 1];
            if c1 & 0xC0 != 0x80 {
                return Err("Invalid UTF-8 encoding: bad continuation byte");
            }
            // Decode codepoint
            let cp = ((b as u32 & 0x1F) << 6) | (c1 as u32 & 0x3F);
            if !is_valid_xml_char(cp) {
                return Err("Invalid XML character");
            }
            pos += 2;
        } else if b < 0xF0 {
            // 3-byte sequence
            if pos + 2 >= content.len() {
                return Err("Invalid UTF-8 encoding: truncated sequence");
            }
            let c1 = content[pos + 1];
            let c2 = content[pos + 2];
            if c1 & 0xC0 != 0x80 || c2 & 0xC0 != 0x80 {
                return Err("Invalid UTF-8 encoding: bad continuation byte");
            }
            // Decode codepoint
            let cp = ((b as u32 & 0x0F) << 12) | ((c1 as u32 & 0x3F) << 6) | (c2 as u32 & 0x3F);
            if !is_valid_xml_char(cp) {
                return Err("Invalid XML character: U+FFFE/U+FFFF not allowed");
            }
            pos += 3;
        } else if b < 0xF8 {
            // 4-byte sequence
            if pos + 3 >= content.len() {
                return Err("Invalid UTF-8 encoding: truncated sequence");
            }
            let c1 = content[pos + 1];
            let c2 = content[pos + 2];
            let c3 = content[pos + 3];
            if c1 & 0xC0 != 0x80 || c2 & 0xC0 != 0x80 || c3 & 0xC0 != 0x80 {
                return Err("Invalid UTF-8 encoding: bad continuation byte");
            }
            // Decode codepoint
            let cp = ((b as u32 & 0x07) << 18) | ((c1 as u32 & 0x3F) << 12)
                   | ((c2 as u32 & 0x3F) << 6) | (c3 as u32 & 0x3F);
            if !is_valid_xml_char(cp) {
                return Err("Invalid XML character");
            }
            pos += 4;
        } else {
            return Err("Invalid UTF-8 encoding: byte too large");
        }
    }
    Ok(())
}

/// Encode text for XML output (escape special characters)
pub fn encode_text(input: &str) -> Cow<'_, str> {
    // Fast path: check if any escaping needed
    if !input.bytes().any(|b| matches!(b, b'<' | b'>' | b'&' | b'"' | b'\'')) {
        return Cow::Borrowed(input);
    }

    // Slow path: escape
    let mut result = String::with_capacity(input.len() + 16);
    for c in input.chars() {
        match c {
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '&' => result.push_str("&amp;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&apos;"),
            _ => result.push(c),
        }
    }
    Cow::Owned(result)
}

/// Encode text for use in XML attributes
pub fn encode_attribute(input: &str) -> Cow<'_, str> {
    encode_text(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_entities() {
        let input = b"Hello, World!";
        let result = decode_text(input);
        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(result.as_ref(), b"Hello, World!");
    }

    #[test]
    fn test_basic_entities() {
        let input = b"&lt;hello&gt; &amp; &quot;world&quot;";
        let result = decode_text(input);
        assert_eq!(result.as_ref(), b"<hello> & \"world\"");
    }

    #[test]
    fn test_numeric_decimal() {
        let input = b"&#65;&#66;&#67;";
        let result = decode_text(input);
        assert_eq!(result.as_ref(), b"ABC");
    }

    #[test]
    fn test_numeric_hex() {
        let input = b"&#x41;&#x42;&#x43;";
        let result = decode_text(input);
        assert_eq!(result.as_ref(), b"ABC");
    }

    #[test]
    fn test_unicode_entity() {
        let input = b"&#x1F600;"; // 😀
        let result = decode_text(input);
        assert_eq!(std::str::from_utf8(result.as_ref()).unwrap(), "😀");
    }

    #[test]
    fn test_unknown_entity() {
        let input = b"&unknown;";
        let result = decode_text(input);
        assert_eq!(result.as_ref(), b"&unknown;");
    }

    #[test]
    fn test_encode_text() {
        let input = "<hello> & \"world\"";
        let result = encode_text(input);
        assert_eq!(result.as_ref(), "&lt;hello&gt; &amp; &quot;world&quot;");
    }
}
