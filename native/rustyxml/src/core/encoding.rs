//! XML Encoding Detection and Conversion
//!
//! Handles detection of UTF-16 and other encodings based on BOM and XML declaration.
//! Converts non-UTF-8 encodings to UTF-8 for parsing.

/// Detect the encoding of XML input based on BOM or byte patterns
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XmlEncoding {
    Utf8,
    Utf16Le,
    Utf16Be,
}

impl XmlEncoding {
    /// Detect encoding from byte order mark or initial bytes
    pub fn detect(input: &[u8]) -> Self {
        if input.len() < 2 {
            return XmlEncoding::Utf8;
        }

        // Check for BOM
        match (input[0], input[1]) {
            // UTF-16 LE BOM: 0xFF 0xFE
            (0xFF, 0xFE) => XmlEncoding::Utf16Le,
            // UTF-16 BE BOM: 0xFE 0xFF
            (0xFE, 0xFF) => XmlEncoding::Utf16Be,
            // UTF-8 BOM: 0xEF 0xBB 0xBF (detected but treated as UTF-8)
            (0xEF, 0xBB) if input.len() >= 3 && input[2] == 0xBF => XmlEncoding::Utf8,
            // No BOM - check for UTF-16 pattern (< followed by null or null followed by <)
            (0x00, b'<') => XmlEncoding::Utf16Be,
            (b'<', 0x00) => XmlEncoding::Utf16Le,
            _ => XmlEncoding::Utf8,
        }
    }
}

/// Convert UTF-16 bytes to UTF-8
///
/// Takes raw bytes that may be UTF-16 LE or BE encoded and converts to UTF-8.
/// Returns the original bytes if already UTF-8 or conversion fails.
pub fn convert_to_utf8(input: Vec<u8>) -> Result<Vec<u8>, String> {
    let encoding = XmlEncoding::detect(&input);

    match encoding {
        XmlEncoding::Utf8 => {
            // Skip UTF-8 BOM if present
            if input.starts_with(&[0xEF, 0xBB, 0xBF]) {
                Ok(input[3..].to_vec())
            } else {
                Ok(input)
            }
        }
        XmlEncoding::Utf16Le => convert_utf16_le_to_utf8(&input),
        XmlEncoding::Utf16Be => convert_utf16_be_to_utf8(&input),
    }
}

/// Convert UTF-16 LE to UTF-8
fn convert_utf16_le_to_utf8(input: &[u8]) -> Result<Vec<u8>, String> {
    // Skip BOM if present
    let start = if input.starts_with(&[0xFF, 0xFE]) { 2 } else { 0 };
    let bytes = &input[start..];

    // Ensure even number of bytes
    if bytes.len() % 2 != 0 {
        return Err("Invalid UTF-16 LE: odd number of bytes".to_string());
    }

    // Convert pairs of bytes to u16 code units (little endian)
    let code_units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();

    // Decode UTF-16 to String
    String::from_utf16(&code_units)
        .map(|s| s.into_bytes())
        .map_err(|e| format!("Invalid UTF-16 LE: {}", e))
}

/// Convert UTF-16 BE to UTF-8
fn convert_utf16_be_to_utf8(input: &[u8]) -> Result<Vec<u8>, String> {
    // Skip BOM if present
    let start = if input.starts_with(&[0xFE, 0xFF]) { 2 } else { 0 };
    let bytes = &input[start..];

    // Ensure even number of bytes
    if bytes.len() % 2 != 0 {
        return Err("Invalid UTF-16 BE: odd number of bytes".to_string());
    }

    // Convert pairs of bytes to u16 code units (big endian)
    let code_units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
        .collect();

    // Decode UTF-16 to String
    String::from_utf16(&code_units)
        .map(|s| s.into_bytes())
        .map_err(|e| format!("Invalid UTF-16 BE: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_utf8() {
        assert_eq!(XmlEncoding::detect(b"<root/>"), XmlEncoding::Utf8);
        assert_eq!(XmlEncoding::detect(b"<?xml"), XmlEncoding::Utf8);
    }

    #[test]
    fn test_detect_utf8_bom() {
        assert_eq!(XmlEncoding::detect(&[0xEF, 0xBB, 0xBF, b'<']), XmlEncoding::Utf8);
    }

    #[test]
    fn test_detect_utf16_le_bom() {
        assert_eq!(XmlEncoding::detect(&[0xFF, 0xFE, b'<', 0x00]), XmlEncoding::Utf16Le);
    }

    #[test]
    fn test_detect_utf16_be_bom() {
        assert_eq!(XmlEncoding::detect(&[0xFE, 0xFF, 0x00, b'<']), XmlEncoding::Utf16Be);
    }

    #[test]
    fn test_convert_utf16_le() {
        // "<r/>" in UTF-16 LE with BOM
        let utf16_le = vec![
            0xFF, 0xFE,  // BOM
            b'<', 0x00,
            b'r', 0x00,
            b'/', 0x00,
            b'>', 0x00,
        ];
        let result = convert_to_utf8(utf16_le).unwrap();
        assert_eq!(result, b"<r/>");
    }

    #[test]
    fn test_convert_utf16_be() {
        // "<r/>" in UTF-16 BE with BOM
        let utf16_be = vec![
            0xFE, 0xFF,  // BOM
            0x00, b'<',
            0x00, b'r',
            0x00, b'/',
            0x00, b'>',
        ];
        let result = convert_to_utf8(utf16_be).unwrap();
        assert_eq!(result, b"<r/>");
    }

    #[test]
    fn test_utf8_passthrough() {
        let utf8 = b"<root>hello</root>".to_vec();
        let result = convert_to_utf8(utf8.clone()).unwrap();
        assert_eq!(result, utf8);
    }
}
