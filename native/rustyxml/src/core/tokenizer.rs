//! XML Tokenizer - State machine for XML token extraction
//!
//! Implements a pull-parser style tokenizer that extracts XML tokens:
//! - Element start/end tags
//! - Text content
//! - CDATA sections
//! - Comments
//! - Processing instructions
//! - Entity references

use super::scanner::Scanner;
use std::borrow::Cow;

/// Current parsing state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseState {
    /// Initial state before parsing starts
    Init,
    /// Inside text content between tags
    InsideText,
    /// Inside a markup construct (<...>)
    InsideMarkup,
    /// Inside an entity reference (&...;)
    InsideRef,
    /// End of input reached
    Done,
}

/// Type of XML token
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// Element start tag: <element>
    StartTag,
    /// Element end tag: </element>
    EndTag,
    /// Empty element: <element/>
    EmptyTag,
    /// Text content
    Text,
    /// CDATA section: <![CDATA[...]]>
    CData,
    /// Comment: <!--...-->
    Comment,
    /// Processing instruction: <?target ...?>
    ProcessingInstruction,
    /// XML declaration: <?xml ...?>
    XmlDeclaration,
    /// DOCTYPE declaration
    DocType,
    /// End of file
    Eof,
}

/// A parsed XML token
#[derive(Debug, Clone)]
pub struct Token<'a> {
    pub kind: TokenKind,
    /// Raw span in input (start, end)
    pub span: (usize, usize),
    /// For tags: the element name
    pub name: Option<Cow<'a, [u8]>>,
    /// For text/cdata: the content (may be borrowed or owned if entities decoded)
    pub content: Option<Cow<'a, [u8]>>,
}

impl<'a> Token<'a> {
    fn new(kind: TokenKind, span: (usize, usize)) -> Self {
        Token {
            kind,
            span,
            name: None,
            content: None,
        }
    }

    fn with_name(mut self, name: &'a [u8]) -> Self {
        self.name = Some(Cow::Borrowed(name));
        self
    }

    fn with_content(mut self, content: Cow<'a, [u8]>) -> Self {
        self.content = Some(content);
        self
    }
}

/// Error type for strict mode validation failures
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub position: usize,
}

impl ParseError {
    pub fn new(message: impl Into<String>, position: usize) -> Self {
        ParseError {
            message: message.into(),
            position,
        }
    }
}

/// XML tokenizer implementing a pull-parser pattern
pub struct Tokenizer<'a> {
    scanner: Scanner<'a>,
    state: ParseState,
    strict: bool,
    error: Option<ParseError>,
}

impl<'a> Tokenizer<'a> {
    /// Create a new tokenizer for the given input (lenient mode)
    pub fn new(input: &'a [u8]) -> Self {
        Tokenizer {
            scanner: Scanner::new(input),
            state: ParseState::Init,
            strict: false,
            error: None,
        }
    }

    /// Create a new tokenizer in strict mode
    pub fn new_strict(input: &'a [u8]) -> Self {
        Tokenizer {
            scanner: Scanner::new(input),
            state: ParseState::Init,
            strict: true,
            error: None,
        }
    }

    /// Get any parse error (strict mode only)
    pub fn error(&self) -> Option<&ParseError> {
        self.error.as_ref()
    }

    /// Set an error if in strict mode
    fn set_error(&mut self, message: impl Into<String>) {
        if self.strict && self.error.is_none() {
            self.error = Some(ParseError::new(message, self.scanner.position()));
        }
    }

    /// Get the current parse state
    pub fn state(&self) -> ParseState {
        self.state
    }

    /// Get the current position in the input
    pub fn position(&self) -> usize {
        self.scanner.position()
    }

    /// Get the next token, or None if at end of input
    pub fn next_token(&mut self) -> Option<Token<'a>> {
        if self.state == ParseState::Done {
            return None;
        }

        // Skip leading whitespace in Init state
        if self.state == ParseState::Init {
            self.scanner.skip_whitespace();
            self.state = ParseState::InsideText;
        }

        if self.scanner.is_eof() {
            self.state = ParseState::Done;
            return Some(Token::new(TokenKind::Eof, (self.scanner.position(), self.scanner.position())));
        }

        // Check what's next
        match self.scanner.peek() {
            Some(b'<') => self.parse_markup(),
            Some(_) => self.parse_text(),
            None => {
                self.state = ParseState::Done;
                Some(Token::new(TokenKind::Eof, (self.scanner.position(), self.scanner.position())))
            }
        }
    }

    /// Parse markup starting with '<'
    fn parse_markup(&mut self) -> Option<Token<'a>> {
        let start = self.scanner.position();
        self.scanner.advance(1); // Skip '<'
        self.state = ParseState::InsideMarkup;

        match self.scanner.peek() {
            Some(b'/') => self.parse_end_tag(start),
            Some(b'!') => self.parse_bang_markup(start),
            Some(b'?') => self.parse_pi(start),
            Some(_) => self.parse_start_tag(start),
            None => None,
        }
    }

    /// Parse a start tag or empty element tag
    fn parse_start_tag(&mut self, start: usize) -> Option<Token<'a>> {
        // Read element name
        let name = match self.scanner.read_name() {
            Some(name) => name,
            None => {
                // In strict mode, set an error for invalid names
                if self.strict {
                    self.set_error("Invalid element name: must start with letter, underscore, or colon");
                }
                return None;
            }
        };

        // Strict mode: validate element name (additional checks)
        if !self.validate_name_strict(name) {
            return None;
        }

        // Find the end of the tag, handling quoted attributes
        let end = self.scanner.find_tag_end_quoted()?;

        // Strict mode: validate all characters in tag content
        if self.strict {
            let tag_content = self.scanner.slice(start + 1, end);
            if let Err(msg) = validate_tag_chars(tag_content) {
                self.set_error(msg);
                return None;
            }
        }

        // Check if it's an empty element tag
        let is_empty = end > 0 && self.scanner.slice(end - 1, end) == b"/";

        self.scanner.set_position(end + 1);
        self.state = ParseState::InsideText;

        let kind = if is_empty { TokenKind::EmptyTag } else { TokenKind::StartTag };
        Some(Token::new(kind, (start, end + 1)).with_name(name))
    }

    /// Parse an end tag
    fn parse_end_tag(&mut self, start: usize) -> Option<Token<'a>> {
        self.scanner.advance(1); // Skip '/'

        // Read element name
        let name = match self.scanner.read_name() {
            Some(name) => name,
            None => {
                if self.strict {
                    self.set_error("Invalid element name in end tag: must start with letter, underscore, or colon");
                }
                return None;
            }
        };

        // Find '>'
        let end = self.scanner.find_tag_end()?;

        // Strict mode: validate all characters in tag content
        if self.strict {
            let tag_content = self.scanner.slice(start + 2, end); // +2 to skip "</"
            if let Err(msg) = validate_tag_chars(tag_content) {
                self.set_error(msg);
                return None;
            }
        }

        self.scanner.set_position(end + 1);
        self.state = ParseState::InsideText;

        Some(Token::new(TokenKind::EndTag, (start, end + 1)).with_name(name))
    }

    /// Parse markup starting with '!' (comment, CDATA, DOCTYPE)
    fn parse_bang_markup(&mut self, start: usize) -> Option<Token<'a>> {
        self.scanner.advance(1); // Skip '!'

        if self.scanner.starts_with(b"--") {
            // Comment
            self.parse_comment(start)
        } else if self.scanner.starts_with(b"[CDATA[") {
            // CDATA section
            self.parse_cdata(start)
        } else if self.scanner.starts_with(b"DOCTYPE") {
            // DOCTYPE
            self.parse_doctype(start)
        } else {
            None
        }
    }

    /// Parse a comment <!--...-->
    fn parse_comment(&mut self, start: usize) -> Option<Token<'a>> {
        self.scanner.advance(2); // Skip '--'

        let content_start = self.scanner.position();

        // Find '-->'
        loop {
            let pos = self.scanner.find_byte(b'-')?;
            self.scanner.set_position(pos);

            if self.scanner.starts_with(b"-->") {
                let content = self.scanner.slice(content_start, pos);

                // Strict mode: validate comment content
                if self.strict {
                    if let Err(msg) = validate_xml_chars(content) {
                        self.set_error(msg);
                        return None;
                    }
                    if let Err(msg) = validate_comment(content) {
                        self.set_error(msg);
                        return None;
                    }
                }

                self.scanner.advance(3); // Skip '-->'
                self.state = ParseState::InsideText;
                return Some(Token::new(TokenKind::Comment, (start, self.scanner.position()))
                    .with_content(Cow::Borrowed(content)));
            }
            self.scanner.advance(1);
        }
    }

    /// Parse a CDATA section <![CDATA[...]]>
    fn parse_cdata(&mut self, start: usize) -> Option<Token<'a>> {
        self.scanner.advance(7); // Skip '[CDATA['

        let content_start = self.scanner.position();

        // Find ']]>'
        loop {
            let pos = self.scanner.find_byte(b']')?;
            self.scanner.set_position(pos);

            if self.scanner.starts_with(b"]]>") {
                let content = self.scanner.slice(content_start, pos);

                // Strict mode: validate characters in CDATA
                if self.strict {
                    if let Err(msg) = validate_xml_chars(content) {
                        self.set_error(msg);
                        return None;
                    }
                }

                self.scanner.advance(3); // Skip ']]>'
                self.state = ParseState::InsideText;
                return Some(Token::new(TokenKind::CData, (start, self.scanner.position()))
                    .with_content(Cow::Borrowed(content)));
            }
            self.scanner.advance(1);
        }
    }

    /// Parse a DOCTYPE declaration
    fn parse_doctype(&mut self, start: usize) -> Option<Token<'a>> {
        // DOCTYPE parsing with proper internal subset handling
        // Format: <!DOCTYPE name [internal subset]> or <!DOCTYPE name SYSTEM "uri">
        let mut in_internal_subset = false;
        let mut in_string = false;
        let mut string_char: u8 = 0;

        while !self.scanner.is_eof() {
            let b = self.scanner.peek()?;

            // Handle strings (quoted sections in DTD)
            if in_string {
                if b == string_char {
                    in_string = false;
                }
                self.scanner.advance(1);
                continue;
            }

            match b {
                b'"' | b'\'' => {
                    in_string = true;
                    string_char = b;
                    self.scanner.advance(1);
                }
                b'[' => {
                    in_internal_subset = true;
                    self.scanner.advance(1);
                }
                b']' => {
                    in_internal_subset = false;
                    self.scanner.advance(1);
                }
                b'>' if !in_internal_subset => {
                    self.scanner.advance(1);
                    self.state = ParseState::InsideText;
                    return Some(Token::new(TokenKind::DocType, (start, self.scanner.position())));
                }
                // In strict mode, validate content inside internal subset
                b'<' if in_internal_subset && self.strict => {
                    self.scanner.advance(1);
                    if let Some(next) = self.scanner.peek() {
                        match next {
                            b'?' => {
                                // Validate PI target name
                                self.scanner.advance(1);
                                if let Some(name) = self.scanner.read_name() {
                                    // XML declaration is not allowed in DTD internal subset
                                    if name.eq_ignore_ascii_case(b"xml") {
                                        self.set_error("XML declaration not allowed in DTD internal subset");
                                        return None;
                                    }
                                    if let Err(msg) = validate_name(name) {
                                        self.set_error(msg);
                                        return None;
                                    }
                                    // After PI target name, must have whitespace or ?>
                                    match self.scanner.peek() {
                                        Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') => {}
                                        Some(b'?') if self.scanner.peek_at(1) == Some(b'>') => {}
                                        Some(_) => {
                                            self.set_error("Invalid character after PI target name");
                                            return None;
                                        }
                                        None => {}
                                    }
                                    // Skip to end of PI
                                    loop {
                                        if let Some(pos) = self.scanner.find_byte(b'?') {
                                            self.scanner.set_position(pos);
                                            if self.scanner.starts_with(b"?>") {
                                                self.scanner.advance(2);
                                                break;
                                            }
                                            self.scanner.advance(1);
                                        } else {
                                            break;
                                        }
                                    }
                                }
                            }
                            b'!' => {
                                self.scanner.advance(1);
                                if self.scanner.starts_with(b"--") {
                                    // Comment - validate and skip
                                    self.scanner.advance(2);
                                    let content_start = self.scanner.position();
                                    loop {
                                        if let Some(pos) = self.scanner.find_byte(b'-') {
                                            self.scanner.set_position(pos);
                                            if self.scanner.starts_with(b"-->") {
                                                let content = self.scanner.slice(content_start, pos);
                                                if let Err(msg) = validate_comment(content) {
                                                    self.set_error(msg);
                                                    return None;
                                                }
                                                self.scanner.advance(3);
                                                break;
                                            }
                                            self.scanner.advance(1);
                                        } else {
                                            break;
                                        }
                                    }
                                } else {
                                    // Element/Attlist/Entity/Notation declaration
                                    // Read the declaration keyword
                                    let keyword_start = self.scanner.position();
                                    while !self.scanner.is_eof() {
                                        match self.scanner.peek() {
                                            Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') => {
                                                break;
                                            }
                                            Some(_) => self.scanner.advance(1),
                                            None => break,
                                        }
                                    }
                                    let keyword = self.scanner.slice(keyword_start, self.scanner.position());

                                    // Validate declaration keyword is uppercase
                                    if let Err(msg) = validate_dtd_keyword(keyword) {
                                        self.set_error(msg);
                                        return None;
                                    }

                                    let is_element_decl = keyword == b"ELEMENT";
                                    let is_attlist_decl = keyword == b"ATTLIST";
                                    let is_entity_decl = keyword == b"ENTITY";

                                    self.scanner.skip_whitespace();

                                    // For ENTITY, check for % (parameter entity)
                                    if is_entity_decl && self.scanner.peek() == Some(b'%') {
                                        self.scanner.advance(1);
                                        // Must have whitespace after %
                                        if !matches!(self.scanner.peek(), Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')) {
                                            self.set_error("Missing whitespace after '%' in entity declaration");
                                            return None;
                                        }
                                        self.scanner.skip_whitespace();
                                    }

                                    // Read and validate the declared name
                                    if let Some(name) = self.scanner.read_name() {
                                        if let Err(msg) = validate_name(name) {
                                            self.set_error(msg);
                                            return None;
                                        }
                                    } else {
                                        self.set_error("Missing name in DTD declaration");
                                        return None;
                                    }

                                    // Capture the rest of the declaration for validation
                                    let content_start = self.scanner.position();

                                    // Skip to end of declaration (handle nested parens and strings)
                                    let mut decl_depth = 1;
                                    let mut paren_depth: usize = 0;
                                    let mut decl_in_string = false;
                                    let mut decl_string_char: u8 = 0;
                                    while !self.scanner.is_eof() && decl_depth > 0 {
                                        let db = self.scanner.peek().unwrap_or(0);
                                        if decl_in_string {
                                            if db == decl_string_char {
                                                decl_in_string = false;
                                            }
                                        } else {
                                            match db {
                                                b'"' | b'\'' => {
                                                    decl_in_string = true;
                                                    decl_string_char = db;
                                                }
                                                b'(' => paren_depth += 1,
                                                b')' => paren_depth = paren_depth.saturating_sub(1),
                                                b'<' => decl_depth += 1,
                                                b'>' => decl_depth -= 1,
                                                _ => {}
                                            }
                                        }
                                        self.scanner.advance(1);
                                    }

                                    // Get declaration content (before the final >)
                                    let content_end = self.scanner.position().saturating_sub(1);
                                    let content = self.scanner.slice(content_start, content_end);

                                    // Validate ELEMENT declaration content
                                    if is_element_decl {
                                        if let Err(msg) = validate_element_content(content) {
                                            self.set_error(msg);
                                            return None;
                                        }
                                    }

                                    // Validate ATTLIST declaration content
                                    if is_attlist_decl {
                                        if let Err(msg) = validate_attlist_content(content) {
                                            self.set_error(msg);
                                            return None;
                                        }
                                    }

                                    // Validate ENTITY declaration content
                                    if is_entity_decl {
                                        if let Err(msg) = validate_entity_content(content) {
                                            self.set_error(msg);
                                            return None;
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => self.scanner.advance(1),
            }
        }
        None
    }

    /// Parse a processing instruction <?...?>
    fn parse_pi(&mut self, start: usize) -> Option<Token<'a>> {
        self.scanner.advance(1); // Skip '?'

        // Check for empty PI target (e.g., "<? ?>")
        if self.strict {
            let first_byte = self.scanner.peek();
            if first_byte == Some(b' ') || first_byte == Some(b'\t') ||
               first_byte == Some(b'\n') || first_byte == Some(b'\r') ||
               first_byte == Some(b'?') {
                self.set_error("Processing instruction target cannot be empty");
                return None;
            }
        }

        // Read target name
        let name = match self.scanner.read_name() {
            Some(n) => n,
            None => {
                if self.strict {
                    self.set_error("Invalid processing instruction target");
                }
                return None;
            }
        };

        // Strict mode: validate PI target name
        if self.strict && !self.validate_name_strict(name) {
            return None;
        }

        // Strict mode: after PI target name, must have whitespace or ?>
        if self.strict {
            match self.scanner.peek() {
                Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') => {}
                Some(b'?') if self.scanner.peek_at(1) == Some(b'>') => {}
                Some(_) => {
                    self.set_error("Invalid character after PI target name");
                    return None;
                }
                None => {}
            }
        }

        // Check if it's an XML declaration
        let is_xml_decl = name.eq_ignore_ascii_case(b"xml");

        // Strict mode: PI target "xml" (any case) is reserved
        // Only exact lowercase "xml" at document start is valid (XML declaration)
        if self.strict && is_xml_decl && name != b"xml" {
            self.set_error("Processing instruction target cannot be 'xml' (case-insensitive reserved name)");
            return None;
        }

        // Capture content start for validation
        let content_start = self.scanner.position();

        // Find '?>'
        loop {
            let pos = self.scanner.find_byte(b'?')?;
            self.scanner.set_position(pos);

            if self.scanner.starts_with(b"?>") {
                let content = self.scanner.slice(content_start, pos);

                // In strict mode, validate all characters
                if self.strict {
                    if let Err(msg) = validate_xml_chars(content) {
                        self.set_error(msg);
                        return None;
                    }
                    // Also validate XML declaration has version attribute
                    if is_xml_decl {
                        if let Err(msg) = validate_xml_decl(content) {
                            self.set_error(msg);
                            return None;
                        }
                    }
                }
                self.scanner.advance(2); // Skip '?>'
                self.state = ParseState::InsideText;
                let kind = if is_xml_decl { TokenKind::XmlDeclaration } else { TokenKind::ProcessingInstruction };
                return Some(Token::new(kind, (start, self.scanner.position())).with_name(name));
            }
            self.scanner.advance(1);
        }
    }

    /// Parse text content
    fn parse_text(&mut self) -> Option<Token<'a>> {
        let start = self.scanner.position();
        self.state = ParseState::InsideText;

        // Find the next '<' or end of input
        let end = self.scanner.find_tag_start().unwrap_or(self.scanner.remaining().len() + start);

        if end == start {
            return None;
        }

        let content = self.scanner.slice(start, end);

        // Strict mode: validate text content
        if self.strict {
            // Check for invalid XML characters
            if let Err(msg) = validate_xml_chars(content) {
                self.set_error(msg);
                return None;
            }
            // Check for ']]>' in content
            if let Err(msg) = validate_text_content(content) {
                self.set_error(msg);
                return None;
            }
        }

        self.scanner.set_position(end);

        // Check if content has entities that need decoding
        let decoded = if self.strict {
            match super::entities::decode_text_strict(content) {
                Ok(v) => v,
                Err(msg) => {
                    self.set_error(msg);
                    return None;
                }
            }
        } else {
            super::entities::decode_text(content)
        };

        Some(Token::new(TokenKind::Text, (start, end)).with_content(decoded))
    }

    /// Validate a name in strict mode
    fn validate_name_strict(&mut self, name: &[u8]) -> bool {
        if self.strict {
            if let Err(msg) = validate_name(name) {
                self.set_error(msg);
                return false;
            }
        }
        true
    }
}

/// Iterator adapter for tokenizer
impl<'a> Iterator for Tokenizer<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let token = self.next_token()?;
        if token.kind == TokenKind::Eof {
            None
        } else {
            Some(token)
        }
    }
}

// =============================================================================
// Strict Mode Validation Functions
// =============================================================================

/// Check if a character is a valid XML NameStartChar
/// Per XML 1.0: Letter | '_' | ':'
/// Non-ASCII bytes (>= 0x80) are allowed as they may be UTF-8 encoded Unicode letters
#[inline]
pub fn is_name_start_char(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b':' || c >= 0x80
}

/// Check if a character is a valid XML NameChar
/// Per XML 1.0: Letter | Digit | '.' | '-' | '_' | ':' | CombiningChar | Extender
/// Non-ASCII bytes (>= 0x80) are allowed as they may be UTF-8 encoded Unicode
#[inline]
pub fn is_name_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'.' || c == b'-' || c == b'_' || c == b':' || c >= 0x80
}

/// Validate an XML Name using full Unicode character class validation
pub fn validate_name(name: &[u8]) -> Result<(), &'static str> {
    super::unicode::validate_name_fast(name)
}

/// Validate comment content (no '--' allowed)
pub fn validate_comment(content: &[u8]) -> Result<(), &'static str> {
    // Check for '--' in content
    for i in 0..content.len().saturating_sub(1) {
        if content[i] == b'-' && content[i + 1] == b'-' {
            return Err("Comment cannot contain '--'");
        }
    }

    // Check if ends with '-' (which would make '-->' after closing)
    if content.last() == Some(&b'-') {
        return Err("Comment cannot end with '-'");
    }

    Ok(())
}

/// Validate a character reference value
pub fn validate_char_ref(value: u32) -> Result<(), &'static str> {
    // XML 1.0 Char production:
    // #x9 | #xA | #xD | [#x20-#xD7FF] | [#xE000-#xFFFD] | [#x10000-#x10FFFF]
    match value {
        0x9 | 0xA | 0xD => Ok(()),
        0x20..=0xD7FF => Ok(()),
        0xE000..=0xFFFD => Ok(()),
        0x10000..=0x10FFFF => Ok(()),
        _ => Err("Invalid character reference"),
    }
}

/// Validate text content (no ']]>' allowed outside CDATA)
pub fn validate_text_content(content: &[u8]) -> Result<(), &'static str> {
    for i in 0..content.len().saturating_sub(2) {
        if content[i] == b']' && content[i + 1] == b']' && content[i + 2] == b'>' {
            return Err("Text content cannot contain ']]>'");
        }
    }
    Ok(())
}

/// Validate that content contains only valid XML characters
/// Uses proper UTF-8 decoding to validate codepoints (including U+FFFE/U+FFFF)
pub fn validate_xml_chars(content: &[u8]) -> Result<(), &'static str> {
    crate::core::entities::validate_xml_content(content)
}

/// Validate all characters in a tag (between < and >)
/// Handles quoted strings properly - validates both inside and outside quotes
/// Also validates UTF-8 codepoints for invalid XML characters like U+FFFE/U+FFFF
fn validate_tag_chars(content: &[u8]) -> Result<(), &'static str> {
    let mut pos = 0;
    let mut in_quote = false;
    let mut quote_char: u8 = 0;
    let mut attr_value_start = 0;

    while pos < content.len() {
        let b = content[pos];

        // Track quote state for attribute value extraction
        if !in_quote && (b == b'"' || b == b'\'') {
            in_quote = true;
            quote_char = b;
            attr_value_start = pos + 1;
            pos += 1;
            continue;
        } else if in_quote && b == quote_char {
            // End of attribute value - validate it
            let attr_value = &content[attr_value_start..pos];
            validate_attr_value(attr_value)?;
            in_quote = false;
            pos += 1;
            continue;
        }

        // Validate character encoding
        if b < 0x80 {
            // ASCII
            if b < 0x20 && b != 0x09 && b != 0x0A && b != 0x0D {
                return Err("Invalid XML character in tag (control character not allowed)");
            }
            if b == 0x7F {
                return Err("Invalid XML character in tag (DEL not allowed)");
            }
            // '<' is not allowed in attribute values
            if in_quote && b == b'<' {
                return Err("'<' not allowed in attribute value");
            }
            pos += 1;
        } else if b < 0xC0 {
            return Err("Invalid UTF-8 encoding in tag");
        } else if b < 0xE0 {
            // 2-byte sequence
            if pos + 1 >= content.len() {
                return Err("Invalid UTF-8 encoding in tag: truncated sequence");
            }
            let c1 = content[pos + 1];
            if c1 & 0xC0 != 0x80 {
                return Err("Invalid UTF-8 encoding in tag");
            }
            let cp = ((b as u32 & 0x1F) << 6) | (c1 as u32 & 0x3F);
            if !crate::core::entities::is_valid_xml_char(cp) {
                return Err("Invalid XML character in tag");
            }
            pos += 2;
        } else if b < 0xF0 {
            // 3-byte sequence
            if pos + 2 >= content.len() {
                return Err("Invalid UTF-8 encoding in tag: truncated sequence");
            }
            let c1 = content[pos + 1];
            let c2 = content[pos + 2];
            if c1 & 0xC0 != 0x80 || c2 & 0xC0 != 0x80 {
                return Err("Invalid UTF-8 encoding in tag");
            }
            let cp = ((b as u32 & 0x0F) << 12) | ((c1 as u32 & 0x3F) << 6) | (c2 as u32 & 0x3F);
            if !crate::core::entities::is_valid_xml_char(cp) {
                return Err("Invalid XML character in tag (U+FFFE/U+FFFF not allowed)");
            }
            pos += 3;
        } else if b < 0xF8 {
            // 4-byte sequence
            if pos + 3 >= content.len() {
                return Err("Invalid UTF-8 encoding in tag: truncated sequence");
            }
            let c1 = content[pos + 1];
            let c2 = content[pos + 2];
            let c3 = content[pos + 3];
            if c1 & 0xC0 != 0x80 || c2 & 0xC0 != 0x80 || c3 & 0xC0 != 0x80 {
                return Err("Invalid UTF-8 encoding in tag");
            }
            let cp = ((b as u32 & 0x07) << 18) | ((c1 as u32 & 0x3F) << 12)
                   | ((c2 as u32 & 0x3F) << 6) | (c3 as u32 & 0x3F);
            if !crate::core::entities::is_valid_xml_char(cp) {
                return Err("Invalid XML character in tag");
            }
            pos += 4;
        } else {
            return Err("Invalid UTF-8 encoding in tag: byte too large");
        }
    }
    Ok(())
}

/// Validate attribute value content
/// Checks that & is followed by valid entity reference, and < is not present
fn validate_attr_value(value: &[u8]) -> Result<(), &'static str> {
    let mut pos = 0;
    while pos < value.len() {
        if value[pos] == b'&' {
            // Check for valid entity reference
            pos += 1;
            if pos >= value.len() {
                return Err("'&' at end of attribute value must be escaped as '&amp;'");
            }
            // Check for character reference
            if value[pos] == b'#' {
                pos += 1;
                if pos >= value.len() {
                    return Err("Invalid character reference in attribute value");
                }
                // Skip hex prefix if present
                if value[pos] == b'x' || value[pos] == b'X' {
                    pos += 1;
                }
                // Find semicolon
                let ref_start = pos;
                while pos < value.len() && value[pos] != b';' {
                    let c = value[pos];
                    if !matches!(c, b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F') {
                        return Err("Invalid character reference in attribute value");
                    }
                    pos += 1;
                }
                if pos >= value.len() || pos == ref_start {
                    return Err("Invalid character reference in attribute value");
                }
            } else {
                // Named entity reference - read name
                let name_start = pos;
                while pos < value.len() && is_name_char(value[pos]) {
                    pos += 1;
                }
                if pos == name_start {
                    return Err("'&' must be followed by entity name or '#' in attribute value");
                }
                // Must have semicolon
                if pos >= value.len() || value[pos] != b';' {
                    return Err("Entity reference in attribute value must end with ';'");
                }
            }
        }
        pos += 1;
    }
    Ok(())
}

/// Validate XML declaration content
/// XMLDecl must have version attribute: <?xml version="..." ...?>
/// Whitespace required between pseudo-attributes
fn validate_xml_decl(content: &[u8]) -> Result<(), &'static str> {
    // Must start with whitespace before version
    let orig_content = content;
    let content = skip_ws(content);

    // Must start with version
    if !content.starts_with(b"version") {
        return Err("XML declaration must have version attribute");
    }

    // Check for = after version
    let rest = &content[7..];
    let rest = skip_ws(rest);
    if rest.is_empty() || rest[0] != b'=' {
        return Err("XML declaration version attribute must have '='");
    }

    let rest = skip_ws(&rest[1..]);
    if rest.is_empty() || (rest[0] != b'"' && rest[0] != b'\'') {
        return Err("XML declaration version value must be quoted");
    }

    // Skip past version value
    let quote = rest[0];
    let rest = &rest[1..];
    let value_end = rest.iter().position(|&b| b == quote).unwrap_or(rest.len());
    let rest = if value_end + 1 <= rest.len() { &rest[value_end + 1..] } else { &[] };

    // Check for encoding or standalone - they require preceding whitespace
    if !rest.is_empty() {
        // Must have whitespace before next attribute
        if !matches!(rest[0], b' ' | b'\t' | b'\n' | b'\r') {
            return Err("Whitespace required before encoding or standalone in XML declaration");
        }
        let rest = skip_ws(rest);

        // Check for encoding attribute
        if rest.starts_with(b"encoding") {
            let after_enc = &rest[8..];
            let after_enc = skip_ws(after_enc);
            if after_enc.is_empty() || after_enc[0] != b'=' {
                return Err("encoding attribute must have '='");
            }
            let after_eq = skip_ws(&after_enc[1..]);
            if after_eq.is_empty() || (after_eq[0] != b'"' && after_eq[0] != b'\'') {
                return Err("encoding value must be quoted");
            }
            let quote = after_eq[0];
            if let Some(value_end) = after_eq[1..].iter().position(|&b| b == quote) {
                let enc_value = &after_eq[1..1 + value_end];
                validate_enc_name(enc_value)?;
            }
        } else if rest.starts_with(b"standalone") {
            // standalone attribute is valid without encoding
        }
    }

    Ok(())
}

/// Validate encoding name per XML spec: [A-Za-z] ([A-Za-z0-9._] | '-')*
fn validate_enc_name(name: &[u8]) -> Result<(), &'static str> {
    if name.is_empty() {
        return Err("Encoding name cannot be empty");
    }

    // First character must be a letter
    let first = name[0];
    if !matches!(first, b'A'..=b'Z' | b'a'..=b'z') {
        return Err("Encoding name must start with a letter");
    }

    // Remaining characters: letters, digits, '.', '_', or '-'
    for &b in &name[1..] {
        if !matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-') {
            if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
                return Err("Encoding name cannot contain whitespace");
            }
            return Err("Invalid character in encoding name");
        }
    }

    Ok(())
}

/// Find subsequence in a byte slice
fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Validate DTD declaration keyword (ELEMENT, ATTLIST, ENTITY, NOTATION)
fn validate_dtd_keyword(keyword: &[u8]) -> Result<(), &'static str> {
    match keyword {
        b"ELEMENT" | b"ATTLIST" | b"ENTITY" | b"NOTATION" => Ok(()),
        _ => {
            // Check for common case errors
            if keyword.eq_ignore_ascii_case(b"ELEMENT") {
                return Err("'ELEMENT' must be uppercase");
            }
            if keyword.eq_ignore_ascii_case(b"ATTLIST") {
                return Err("'ATTLIST' must be uppercase");
            }
            if keyword.eq_ignore_ascii_case(b"ENTITY") {
                return Err("'ENTITY' must be uppercase");
            }
            if keyword.eq_ignore_ascii_case(b"NOTATION") {
                return Err("'NOTATION' must be uppercase");
            }
            // Unknown keyword - allow it (could be extension)
            Ok(())
        }
    }
}

/// Validate ELEMENT declaration content (contentspec keywords)
fn validate_element_content(content: &[u8]) -> Result<(), &'static str> {
    // Skip leading whitespace
    let content = skip_ws(content);

    if content.is_empty() {
        return Err("Missing content specification in ELEMENT declaration");
    }

    // Check for EMPTY
    if content.starts_with(b"EMPTY") {
        return Ok(());
    }
    // Check for lowercase 'empty'
    if content.len() >= 5 && content[..5].eq_ignore_ascii_case(b"EMPTY") {
        return Err("'EMPTY' must be uppercase");
    }

    // Check for ANY
    if content.starts_with(b"ANY") {
        return Ok(());
    }
    // Check for lowercase 'any'
    if content.len() >= 3 && content[..3].eq_ignore_ascii_case(b"ANY") {
        return Err("'ANY' must be uppercase");
    }

    // Check for content model starting with (
    if content.starts_with(b"(") {
        return validate_content_model(&content[1..]);
    }

    Err("Invalid content specification in ELEMENT declaration")
}

/// Validate content model (Mixed or children)
fn validate_content_model(content: &[u8]) -> Result<(), &'static str> {
    let content = skip_ws(content);

    // Check for #PCDATA (Mixed content)
    if content.starts_with(b"#PCDATA") {
        return validate_mixed_content(&content[7..]);
    }
    // Check for lowercase #pcdata
    if content.len() >= 7 && content[..7].eq_ignore_ascii_case(b"#PCDATA") {
        return Err("'#PCDATA' must be uppercase");
    }

    // Children content model - validate syntax
    validate_children_content(content)
}

/// Validate Mixed content model after #PCDATA
fn validate_mixed_content(content: &[u8]) -> Result<(), &'static str> {
    let mut pos = 0;
    let len = content.len();
    let mut has_alternation = false;

    while pos < len {
        // Skip whitespace
        while pos < len && matches!(content[pos], b' ' | b'\t' | b'\n' | b'\r') {
            pos += 1;
        }

        if pos >= len {
            return Err("Unclosed mixed content model");
        }

        match content[pos] {
            b')' => {
                pos += 1;
                // If had alternations, must have * suffix
                if has_alternation {
                    if pos >= len || content[pos] != b'*' {
                        return Err("Mixed content with element names must end with ')*'");
                    }
                }
                return Ok(());
            }
            b'|' => {
                has_alternation = true;
                pos += 1;
                // Skip whitespace
                while pos < len && matches!(content[pos], b' ' | b'\t' | b'\n' | b'\r') {
                    pos += 1;
                }
                // Expect a name
                if pos >= len || !is_name_start(content[pos]) {
                    return Err("Expected element name after '|' in mixed content");
                }
                // Skip name
                while pos < len && is_name_char_byte(content[pos]) {
                    pos += 1;
                }
            }
            _ => {
                return Err("Invalid character in mixed content model");
            }
        }
    }
    Err("Unclosed mixed content model")
}

/// Validate children content model
fn validate_children_content(content: &[u8]) -> Result<(), &'static str> {
    let mut pos = 0;
    let len = content.len();
    let mut depth = 1; // Already inside one '('
    let mut last_was_occurrence = false;
    let mut last_was_separator = true; // Start position is like after separator
    let mut last_was_close_paren = false;

    while pos < len && depth > 0 {
        // Skip whitespace
        while pos < len && matches!(content[pos], b' ' | b'\t' | b'\n' | b'\r') {
            pos += 1;
        }

        if pos >= len {
            break;
        }

        let b = content[pos];

        match b {
            b'(' => {
                if !last_was_separator && !last_was_close_paren {
                    // ( can only come after separator or at start
                    if depth > 1 && !last_was_occurrence {
                        // It's ok after an occurrence indicator followed by separator
                    }
                }
                depth += 1;
                last_was_occurrence = false;
                last_was_separator = true; // Content after ( is like after separator
                last_was_close_paren = false;
                pos += 1;
            }
            b')' => {
                depth -= 1;
                last_was_occurrence = false;
                last_was_separator = false;
                last_was_close_paren = true;
                pos += 1;
            }
            b'?' | b'*' | b'+' => {
                // Occurrence indicator must follow name, ')', or nothing (at name end)
                if last_was_occurrence {
                    return Err("Invalid double occurrence indicator in content model");
                }
                if last_was_separator {
                    return Err("Occurrence indicator cannot follow separator");
                }
                last_was_occurrence = true;
                last_was_separator = false;
                last_was_close_paren = false;
                pos += 1;
            }
            b',' | b'|' => {
                // Separator
                last_was_occurrence = false;
                last_was_separator = true;
                last_was_close_paren = false;
                pos += 1;
            }
            _ if is_name_start(b) => {
                if !last_was_separator {
                    return Err("Expected separator before element name in content model");
                }
                // Skip name
                while pos < len && is_name_char_byte(content[pos]) {
                    pos += 1;
                }
                last_was_occurrence = false;
                last_was_separator = false;
                last_was_close_paren = false;
            }
            _ => {
                return Err("Invalid character in content model");
            }
        }
    }

    if depth != 0 {
        return Err("Unbalanced parentheses in content model");
    }

    Ok(())
}

#[inline]
fn is_name_start(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'_' | b':') || b >= 0x80
}

#[inline]
fn is_name_char_byte(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b':') || b >= 0x80
}

/// Validate ATTLIST declaration content
fn validate_attlist_content(content: &[u8]) -> Result<(), &'static str> {
    // Scan for case-sensitive keywords
    let keywords = [
        (b"CDATA" as &[u8], "CDATA"),
        (b"IDREFS", "IDREFS"),
        (b"IDREF", "IDREF"),
        (b"ID", "ID"),
        (b"ENTITIES", "ENTITIES"),
        (b"ENTITY", "ENTITY"),
        (b"NMTOKENS", "NMTOKENS"),
        (b"NMTOKEN", "NMTOKEN"),
        (b"NOTATION", "NOTATION"),
        (b"#REQUIRED", "#REQUIRED"),
        (b"#IMPLIED", "#IMPLIED"),
        (b"#FIXED", "#FIXED"),
    ];

    let mut pos = 0;
    while pos < content.len() {
        // Skip whitespace and quoted strings
        if content[pos] == b'"' || content[pos] == b'\'' {
            let quote = content[pos];
            pos += 1;
            while pos < content.len() && content[pos] != quote {
                pos += 1;
            }
            pos += 1;
            continue;
        }

        // Check for keywords at this position
        for (upper, name) in &keywords {
            if pos + upper.len() <= content.len() {
                let slice = &content[pos..pos + upper.len()];
                // Check if this matches case-insensitively but not exactly
                if slice.eq_ignore_ascii_case(upper) && slice != *upper {
                    // Make sure it's a word boundary
                    let at_end = pos + upper.len() >= content.len();
                    let next_is_boundary = at_end
                        || !matches!(
                            content[pos + upper.len()],
                            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_'
                        );
                    if next_is_boundary {
                        return Err(match *name {
                            "CDATA" => "'CDATA' must be uppercase",
                            "ID" => "'ID' must be uppercase",
                            "IDREF" => "'IDREF' must be uppercase",
                            "IDREFS" => "'IDREFS' must be uppercase",
                            "ENTITY" => "'ENTITY' must be uppercase",
                            "ENTITIES" => "'ENTITIES' must be uppercase",
                            "NMTOKEN" => "'NMTOKEN' must be uppercase",
                            "NMTOKENS" => "'NMTOKENS' must be uppercase",
                            "NOTATION" => "'NOTATION' must be uppercase",
                            "#REQUIRED" => "'#REQUIRED' must be uppercase",
                            "#IMPLIED" => "'#IMPLIED' must be uppercase",
                            "#FIXED" => "'#FIXED' must be uppercase",
                            _ => "Keyword must be uppercase",
                        });
                    }
                }
            }
        }
        pos += 1;
    }
    Ok(())
}

/// Validate ENTITY declaration content
fn validate_entity_content(content: &[u8]) -> Result<(), &'static str> {
    let content = skip_ws(content);

    // Check for SYSTEM/PUBLIC keywords - case sensitive
    let keywords = [
        (b"SYSTEM" as &[u8], "SYSTEM"),
        (b"PUBLIC", "PUBLIC"),
        (b"NDATA", "NDATA"),
    ];

    for (upper, name) in &keywords {
        if let Some(idx) = find_word(content, upper) {
            let slice = &content[idx..idx + upper.len()];
            if slice.eq_ignore_ascii_case(upper) && slice != *upper {
                return Err(match *name {
                    "SYSTEM" => "'SYSTEM' must be uppercase",
                    "PUBLIC" => "'PUBLIC' must be uppercase",
                    "NDATA" => "'NDATA' must be uppercase",
                    _ => "Keyword must be uppercase",
                });
            }
        }
    }

    // Validate quoted strings (entity value or literals)
    // In entity values:
    // - & must be followed by valid entity/char reference
    // - % must be followed by valid PE reference (or escaped as &#37;)
    // - < and > ARE allowed (will be parsed when entity is expanded)
    let mut pos = 0;
    let len = content.len();
    while pos < len {
        if content[pos] == b'"' || content[pos] == b'\'' {
            let quote = content[pos];
            pos += 1;

            // Find closing quote
            while pos < len && content[pos] != quote {
                // Check for unescaped & in entity value
                if content[pos] == b'&' {
                    // Must be followed by valid entity or char reference
                    pos += 1;
                    if pos >= len {
                        return Err("Unescaped '&' in entity value");
                    }
                    if content[pos] == b'#' {
                        // Character reference - skip to ;
                        while pos < len && content[pos] != b';' && content[pos] != quote {
                            pos += 1;
                        }
                        if pos >= len || content[pos] != b';' {
                            return Err("Invalid character reference in entity value");
                        }
                    } else if is_name_start(content[pos]) {
                        // Entity reference - skip to ;
                        while pos < len && content[pos] != b';' && content[pos] != quote {
                            pos += 1;
                        }
                        if pos >= len || content[pos] != b';' {
                            return Err("Invalid entity reference in entity value");
                        }
                    } else {
                        return Err("Unescaped '&' in entity value");
                    }
                }
                pos += 1;
            }

            if pos >= len {
                return Err("Unclosed quoted string in entity declaration");
            }
            pos += 1; // Skip closing quote
        } else {
            pos += 1;
        }
    }

    Ok(())
}

/// Find a word in content (case-insensitive search)
fn find_word(content: &[u8], word: &[u8]) -> Option<usize> {
    let mut pos = 0;
    while pos + word.len() <= content.len() {
        if content[pos..pos + word.len()].eq_ignore_ascii_case(word) {
            return Some(pos);
        }
        pos += 1;
    }
    None
}

/// Skip leading whitespace
fn skip_ws(content: &[u8]) -> &[u8] {
    let mut pos = 0;
    while pos < content.len() && matches!(content[pos], b' ' | b'\t' | b'\n' | b'\r') {
        pos += 1;
    }
    &content[pos..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_element() {
        let mut tok = Tokenizer::new(b"<root>content</root>");

        let t1 = tok.next_token().unwrap();
        assert_eq!(t1.kind, TokenKind::StartTag);
        assert_eq!(t1.name.as_ref().map(|c| c.as_ref()), Some(b"root" as &[u8]));

        let t2 = tok.next_token().unwrap();
        assert_eq!(t2.kind, TokenKind::Text);
        assert_eq!(t2.content.as_ref().map(|c| c.as_ref()), Some(b"content" as &[u8]));

        let t3 = tok.next_token().unwrap();
        assert_eq!(t3.kind, TokenKind::EndTag);
        assert_eq!(t3.name.as_ref().map(|c| c.as_ref()), Some(b"root" as &[u8]));
    }

    #[test]
    fn test_empty_element() {
        let mut tok = Tokenizer::new(b"<br/>");
        let t = tok.next_token().unwrap();
        assert_eq!(t.kind, TokenKind::EmptyTag);
        assert_eq!(t.name.as_ref().map(|c| c.as_ref()), Some(b"br" as &[u8]));
    }

    #[test]
    fn test_cdata() {
        let mut tok = Tokenizer::new(b"<![CDATA[<script>code</script>]]>");
        let t = tok.next_token().unwrap();
        assert_eq!(t.kind, TokenKind::CData);
        assert_eq!(t.content.as_ref().map(|c| c.as_ref()), Some(b"<script>code</script>" as &[u8]));
    }

    #[test]
    fn test_comment() {
        let mut tok = Tokenizer::new(b"<!-- comment -->");
        let t = tok.next_token().unwrap();
        assert_eq!(t.kind, TokenKind::Comment);
        assert_eq!(t.content.as_ref().map(|c| c.as_ref()), Some(b" comment " as &[u8]));
    }
}
