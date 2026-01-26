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
use std::collections::{HashMap, HashSet};

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

/// Information about a declared entity
#[derive(Debug, Clone)]
struct EntityInfo {
    /// True if entity is external (has SYSTEM or PUBLIC)
    is_external: bool,
    /// True if entity is unparsed (has NDATA)
    is_unparsed: bool,
    /// Replacement text for internal entities
    value: Option<Vec<u8>>,
    /// Notation name for unparsed entities (NDATA)
    ndata_notation: Option<Vec<u8>>,
    /// True if entity references an external entity (directly or indirectly)
    references_external: bool,
}

/// XML tokenizer implementing a pull-parser pattern
pub struct Tokenizer<'a> {
    scanner: Scanner<'a>,
    state: ParseState,
    strict: bool,
    error: Option<ParseError>,
    /// General entities declared in DTD (only allocated in strict mode)
    entities: Option<HashMap<Vec<u8>, EntityInfo>>,
    /// Parameter entities declared in DTD (only allocated in strict mode)
    parameter_entities: Option<HashMap<Vec<u8>, EntityInfo>>,
    /// Notations declared in DTD (only allocated in strict mode)
    notations: Option<HashSet<Vec<u8>>>,
}

impl<'a> Tokenizer<'a> {
    /// Create a new tokenizer for the given input (lenient mode)
    pub fn new(input: &'a [u8]) -> Self {
        Tokenizer {
            scanner: Scanner::new(input),
            state: ParseState::Init,
            strict: false,
            error: None,
            entities: None,
            parameter_entities: None,
            notations: None,
        }
    }

    /// Create a new tokenizer in strict mode
    pub fn new_strict(input: &'a [u8]) -> Self {
        Tokenizer {
            scanner: Scanner::new(input),
            state: ParseState::Init,
            strict: true,
            error: None,
            entities: Some(HashMap::new()),
            parameter_entities: Some(HashMap::new()),
            notations: Some(HashSet::new()),
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

        // Handle Init state
        if self.state == ParseState::Init {
            // In strict mode, check for whitespace before XML declaration
            if self.strict {
                // Check if there's whitespace at position 0
                let has_leading_ws = self.scanner.position() == 0 &&
                    matches!(self.scanner.peek(), Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r'));

                if has_leading_ws {
                    // Skip whitespace to see what follows
                    let start_pos = self.scanner.position();
                    self.scanner.skip_whitespace();

                    // Check if what follows is <?xml
                    if self.scanner.starts_with(b"<?xml") {
                        // Whitespace before XML declaration is not allowed
                        self.set_error("XML declaration must be at the very start of the document (no leading whitespace)");
                        return None;
                    }

                    // Restore position - the whitespace will be parsed as text
                    self.scanner.set_position(start_pos);
                }
            } else {
                // Lenient mode: skip leading whitespace
                self.scanner.skip_whitespace();
            }
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
            if let Err(msg) = validate_tag_chars(tag_content, self.entities.as_ref()) {
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

        // Strict mode: end tag can only have whitespace after name, not attributes
        if self.strict {
            let name_end = self.scanner.position();
            self.scanner.skip_whitespace();

            // Check if there's anything other than '>' after name and whitespace
            if let Some(b) = self.scanner.peek() {
                if b != b'>' {
                    self.set_error("End tag cannot have attributes or other content");
                    return None;
                }
            }
            // Restore position for find_tag_end
            self.scanner.set_position(name_end);
        }

        // Find '>'
        let end = self.scanner.find_tag_end()?;

        // Strict mode: validate all characters in tag content
        if self.strict {
            let tag_content = self.scanner.slice(start + 2, end); // +2 to skip "</"
            if let Err(msg) = validate_tag_chars(tag_content, self.entities.as_ref()) {
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

        // Strict mode: check for <!xml which should be <?xml
        if self.strict {
            // Check for <!xml (case-insensitive)
            let remaining = self.scanner.remaining();
            if remaining.len() >= 3 {
                let maybe_xml = &remaining[..3];
                if maybe_xml.eq_ignore_ascii_case(b"xml") {
                    self.set_error("Invalid '<!xml' - XML declaration must use '<?xml'");
                    return None;
                }
            }
        }

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
            // Strict mode: unknown declaration is an error
            if self.strict {
                self.set_error("Invalid declaration - expected comment, CDATA, or DOCTYPE");
                return None;
            }
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

        // Skip "DOCTYPE" keyword
        self.scanner.advance(7);

        // In strict mode, validate DOCTYPE structure
        if self.strict {
            // Must have whitespace after DOCTYPE
            if !matches!(self.scanner.peek(), Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')) {
                self.set_error("Whitespace required after DOCTYPE");
                return None;
            }
            self.scanner.skip_whitespace();

            // Read and validate the DOCTYPE name
            if let Some(name) = self.scanner.read_name() {
                // Check if the "name" is actually a keyword (SYSTEM/PUBLIC) - means real name is missing
                if name == b"SYSTEM" || name == b"PUBLIC" {
                    self.set_error("DOCTYPE declaration requires a name (found keyword without name)");
                    return None;
                }
                if let Err(msg) = validate_name(name) {
                    self.set_error(msg);
                    return None;
                }
            } else {
                // No valid name - check if we got SYSTEM/PUBLIC directly (missing name)
                let remaining = self.scanner.remaining();
                if remaining.starts_with(b"SYSTEM") || remaining.starts_with(b"PUBLIC") {
                    self.set_error("DOCTYPE declaration requires a name");
                    return None;
                }
                // Check if we're at [ or > without a name
                if remaining.starts_with(b"[") || remaining.starts_with(b">") {
                    self.set_error("DOCTYPE declaration requires a name");
                    return None;
                }
                self.set_error("Invalid DOCTYPE name");
                return None;
            }

            // Check for SGML-style comments in DOCTYPE (before internal subset)
            // Look for -- outside of quotes up to [ or >
            let remaining = self.scanner.remaining();
            let mut in_quote = false;
            let mut quote_char = 0u8;
            for i in 0..remaining.len() {
                if !in_quote && (remaining[i] == b'"' || remaining[i] == b'\'') {
                    in_quote = true;
                    quote_char = remaining[i];
                } else if in_quote && remaining[i] == quote_char {
                    in_quote = false;
                } else if !in_quote {
                    if remaining[i] == b'[' || remaining[i] == b'>' {
                        break;
                    }
                    if remaining[i] == b'-' && i + 1 < remaining.len() && remaining[i + 1] == b'-' {
                        self.set_error("SGML-style comments (-- ... --) not allowed in DOCTYPE");
                        return None;
                    }
                }
            }

            // P75 validation: Check DOCTYPE external ID structure
            // Scan remaining content up to [ or > for external ID issues
            let remaining = self.scanner.remaining();
            let mut ext_id_pos = 0;

            // Skip any initial whitespace
            while ext_id_pos < remaining.len() && matches!(remaining[ext_id_pos], b' ' | b'\t' | b'\n' | b'\r') {
                ext_id_pos += 1;
            }

            // Check what comes next
            if ext_id_pos < remaining.len() {
                let first_char = remaining[ext_id_pos];

                // P75n08: If starts with a quote without PUBLIC/SYSTEM
                if first_char == b'"' || first_char == b'\'' {
                    self.set_error("DOCTYPE external ID requires SYSTEM or PUBLIC keyword before quoted literal");
                    return None;
                }

                // Check for PUBLIC or SYSTEM
                let has_public = remaining[ext_id_pos..].starts_with(b"PUBLIC");
                let has_system = remaining[ext_id_pos..].starts_with(b"SYSTEM");

                if has_public {
                    // P75n09: Check for required whitespace after PUBLIC
                    let after_public_pos = ext_id_pos + 6;
                    if after_public_pos < remaining.len() {
                        let after_public = remaining[after_public_pos];
                        if after_public == b'"' || after_public == b'\'' {
                            self.set_error("Whitespace required after PUBLIC keyword");
                            return None;
                        }

                        // P75n07: Check for whitespace between public ID and system literal
                        // Find the public ID literal, then check what follows
                        let mut pub_scan = after_public_pos;
                        // Skip whitespace
                        while pub_scan < remaining.len() && matches!(remaining[pub_scan], b' ' | b'\t' | b'\n' | b'\r') {
                            pub_scan += 1;
                        }
                        // Find and skip the public ID literal
                        if pub_scan < remaining.len() && (remaining[pub_scan] == b'"' || remaining[pub_scan] == b'\'') {
                            let pub_quote = remaining[pub_scan];
                            pub_scan += 1;
                            while pub_scan < remaining.len() && remaining[pub_scan] != pub_quote {
                                pub_scan += 1;
                            }
                            if pub_scan < remaining.len() {
                                pub_scan += 1; // Skip closing quote
                                // Now check what immediately follows - if it's a quote without whitespace, error
                                if pub_scan < remaining.len() && (remaining[pub_scan] == b'"' || remaining[pub_scan] == b'\'') {
                                    self.set_error("Whitespace required between public ID and system literal");
                                    return None;
                                }
                            }
                        }
                    }
                }

                if has_system {
                    // Check for required whitespace after SYSTEM
                    let after_system_pos = ext_id_pos + 6;
                    if after_system_pos < remaining.len() {
                        let after_system = remaining[after_system_pos];
                        if after_system == b'"' || after_system == b'\'' {
                            self.set_error("Whitespace required after SYSTEM keyword");
                            return None;
                        }
                    }
                }
            }
        }

        let mut in_internal_subset = false;
        let mut in_string = false;
        let mut string_char: u8 = 0;
        let mut string_start: usize = 0;
        let mut validated_external_id = false;
        let mut expecting_pubid = false;  // Next quoted string is PUBLIC ID

        while !self.scanner.is_eof() {
            let b = self.scanner.peek()?;

            // Handle strings (quoted sections in DTD)
            if in_string {
                if b == string_char {
                    // End of quoted string - validate public ID if that's what we were expecting
                    if self.strict && expecting_pubid {
                        let pubid = self.scanner.slice(string_start, self.scanner.position());
                        if let Err(msg) = validate_pubid_literal(pubid) {
                            self.set_error(msg);
                            return None;
                        }
                        expecting_pubid = false;
                    }
                    in_string = false;
                }
                self.scanner.advance(1);
                continue;
            }

            // In strict mode, validate PUBLIC/SYSTEM keywords (before internal subset)
            if self.strict && !in_internal_subset && !validated_external_id {
                let remaining = self.scanner.remaining();
                // Check for PUBLIC keyword (case-insensitive then exact match)
                if remaining.len() >= 6 {
                    let maybe_public = &remaining[..6];
                    if maybe_public.eq_ignore_ascii_case(b"PUBLIC") {
                        if maybe_public != b"PUBLIC" {
                            self.set_error("'PUBLIC' keyword must be uppercase");
                            return None;
                        }
                        validated_external_id = true;
                        expecting_pubid = true;  // Next quoted string will be public ID
                    }
                }
                // Check for SYSTEM keyword
                if remaining.len() >= 6 {
                    let maybe_system = &remaining[..6];
                    if maybe_system.eq_ignore_ascii_case(b"SYSTEM") {
                        if maybe_system != b"SYSTEM" {
                            self.set_error("'SYSTEM' keyword must be uppercase");
                            return None;
                        }
                        validated_external_id = true;
                    }
                }
            }

            match b {
                b'"' | b'\'' => {
                    in_string = true;
                    string_char = b;
                    self.scanner.advance(1);
                    string_start = self.scanner.position();  // Mark start of string content
                }
                b'[' => {
                    in_internal_subset = true;
                    self.scanner.advance(1);
                }
                b']' => {
                    in_internal_subset = false;
                    self.scanner.advance(1);

                    // In strict mode, check for PE reference between ] and > (not allowed)
                    // sa-164: `] %e; >` is not valid
                    if self.strict {
                        // Skip whitespace to check what follows ]
                        let save_pos = self.scanner.position();
                        self.scanner.skip_whitespace();
                        if self.scanner.peek() == Some(b'%') {
                            // Check if this is a PE reference
                            self.scanner.advance(1);
                            if let Some(c) = self.scanner.peek() {
                                if is_name_start(c) {
                                    self.set_error("Parameter entity reference not allowed between ']' and '>' in DOCTYPE declaration");
                                    return None;
                                }
                            }
                        }
                        self.scanner.set_position(save_pos);
                    }
                }
                b'>' if !in_internal_subset => {
                    // Validate NDATA notation references before completing DOCTYPE
                    if self.strict {
                        if let (Some(ref entity_map), Some(ref notation_set)) = (&self.entities, &self.notations) {
                            for (_, info) in entity_map.iter() {
                                if let Some(ref notation_name) = info.ndata_notation {
                                    if !notation_set.contains(notation_name) {
                                        self.set_error("NDATA references undeclared notation");
                                        return None;
                                    }
                                }
                            }
                        }
                    }
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

                                    // Check PI content for PE references (not allowed in PI)
                                    // P29n05: PE reference in PI content is invalid
                                    let pi_content_start = self.scanner.position();

                                    // Skip to end of PI
                                    loop {
                                        if let Some(pos) = self.scanner.find_byte(b'?') {
                                            self.scanner.set_position(pos);
                                            if self.scanner.starts_with(b"?>") {
                                                // Check PI content for PE references before closing
                                                let pi_content = self.scanner.slice(pi_content_start, pos);
                                                for i in 0..pi_content.len() {
                                                    if pi_content[i] == b'%' {
                                                        // Check if this looks like a PE reference
                                                        if i + 1 < pi_content.len() && is_name_start(pi_content[i + 1]) {
                                                            self.set_error("Parameter entity reference not allowed in processing instruction");
                                                            return None;
                                                        }
                                                    }
                                                }
                                                self.scanner.advance(2);
                                                break;
                                            }
                                            self.scanner.advance(1);
                                        } else {
                                            // P29n05: Check for PE reference in PI content even if PI is unclosed
                                            // Scan up to end of data or until we see ]> which closes the internal subset
                                            let remaining = self.scanner.remaining();
                                            let end_offset = remaining.iter().position(|&b| b == b']')
                                                .unwrap_or(remaining.len());
                                            let end_pos = self.scanner.position() + end_offset;
                                            let pi_content = self.scanner.slice(pi_content_start, end_pos);
                                            for i in 0..pi_content.len() {
                                                if pi_content[i] == b'%' {
                                                    if i + 1 < pi_content.len() && is_name_start(pi_content[i + 1]) {
                                                        self.set_error("Parameter entity reference not allowed in processing instruction");
                                                        return None;
                                                    }
                                                }
                                            }
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
                                    let is_notation_decl = keyword == b"NOTATION";

                                    self.scanner.skip_whitespace();

                                    // For ENTITY, check for % (parameter entity)
                                    let mut is_parameter_entity = false;
                                    if is_entity_decl && self.scanner.peek() == Some(b'%') {
                                        is_parameter_entity = true;
                                        self.scanner.advance(1);
                                        // Must have whitespace after %
                                        if !matches!(self.scanner.peek(), Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')) {
                                            self.set_error("Missing whitespace after '%' in entity declaration");
                                            return None;
                                        }
                                        self.scanner.skip_whitespace();
                                    }

                                    // Read and validate the declared name
                                    let decl_name: Option<Vec<u8>> = if let Some(name) = self.scanner.read_name() {
                                        if let Err(msg) = validate_name(name) {
                                            self.set_error(msg);
                                            return None;
                                        }

                                        // For ELEMENT declarations, check for required whitespace after name
                                        if is_element_decl {
                                            // Peek at next character - should be whitespace
                                            if let Some(next) = self.scanner.peek() {
                                                if !matches!(next, b' ' | b'\t' | b'\n' | b'\r') {
                                                    self.set_error("Whitespace required between element name and content specification");
                                                    return None;
                                                }
                                            }
                                        }

                                        // For ENTITY declarations, check for required whitespace after name
                                        // sa-062: <!ENTITY foo"some text"> is invalid (missing space before quote)
                                        if is_entity_decl {
                                            if let Some(next) = self.scanner.peek() {
                                                if next == b'"' || next == b'\'' {
                                                    self.set_error("Whitespace required between entity name and value");
                                                    return None;
                                                }
                                            }
                                        }

                                        // Capture name for entity/notation registration
                                        if is_entity_decl || is_notation_decl {
                                            Some(name.to_vec())
                                        } else {
                                            None
                                        }
                                    } else {
                                        self.set_error("Missing name in DTD declaration");
                                        return None;
                                    };

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

                                    // Check for invalid '!>' closing (SGML style)
                                    if !content.is_empty() && content.last() == Some(&b'!') {
                                        self.set_error("DTD declaration cannot end with '!>' - must end with '>'");
                                        return None;
                                    }

                                    // Validate ELEMENT declaration content
                                    if is_element_decl {
                                        if let Err(msg) = validate_element_content(content) {
                                            self.set_error(msg);
                                            return None;
                                        }
                                    }

                                    // Validate ATTLIST declaration content
                                    if is_attlist_decl {
                                        if let Err(msg) = validate_attlist_content(content, self.entities.as_ref()) {
                                            self.set_error(msg);
                                            return None;
                                        }
                                    }

                                    // Validate ENTITY declaration content
                                    if is_entity_decl {
                                        if let Err(msg) = validate_entity_content(content, is_parameter_entity) {
                                            self.set_error(msg);
                                            return None;
                                        }
                                    }

                                    // Validate NOTATION declaration content
                                    if is_notation_decl {
                                        if let Err(msg) = validate_notation_content(content) {
                                            self.set_error(msg);
                                            return None;
                                        }
                                    }

                                    // Register entities and notations for validation (strict mode only)
                                    if let Some(name) = decl_name {
                                        if is_entity_decl {
                                            // Parse entity metadata from content
                                            let entity_info = parse_entity_info(content);
                                            if is_parameter_entity {
                                                if let Some(ref mut pe_map) = self.parameter_entities {
                                                    pe_map.insert(name, entity_info);
                                                }
                                            } else {
                                                if let Some(ref mut e_map) = self.entities {
                                                    e_map.insert(name, entity_info);
                                                }
                                            }
                                        } else if is_notation_decl {
                                            if let Some(ref mut n_set) = self.notations {
                                                n_set.insert(name);
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {
                                // Check for DTD declaration keywords without <! prefix
                                // e.g., <ELEMENT instead of <!ELEMENT
                                let remaining = self.scanner.remaining();
                                if remaining.starts_with(b"ELEMENT") ||
                                   remaining.starts_with(b"ATTLIST") ||
                                   remaining.starts_with(b"ENTITY") ||
                                   remaining.starts_with(b"NOTATION") {
                                    self.set_error("DTD declaration must start with '<!' not '<'");
                                    return None;
                                }
                                // Check for any element tag inside DTD (elements not allowed in DTD)
                                // If we see <name where name is a valid element name (starts with letter, underscore, or colon)
                                if !remaining.is_empty() && is_name_start(remaining[0]) {
                                    self.set_error("Element tags not allowed inside DTD internal subset");
                                    return None;
                                }
                            }
                        }
                    }
                }
                // Check for PE references in internal subset
                b'%' if in_internal_subset && self.strict => {
                    self.scanner.advance(1);
                    // PE reference must have a name after %
                    // P69n01: `%;` is invalid (empty PE reference)
                    match self.scanner.peek() {
                        Some(b';') => {
                            self.set_error("Invalid PE reference: name required after '%'");
                            return None;
                        }
                        Some(c) if is_name_start(c) => {
                            // Valid PE reference start - read name and verify semicolon
                            let ref_start = self.scanner.position();
                            while !self.scanner.is_eof() {
                                match self.scanner.peek() {
                                    Some(b';') => {
                                        self.scanner.advance(1);
                                        break;
                                    }
                                    Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') => {
                                        // P69n03: `%paaa ;` - whitespace before semicolon
                                        self.set_error("Invalid PE reference: no whitespace allowed before ';'");
                                        return None;
                                    }
                                    Some(c) if is_name_char_byte(c) => {
                                        self.scanner.advance(1);
                                    }
                                    _ => {
                                        // P69n02: `%paaa` without semicolon
                                        self.set_error("Invalid PE reference: missing ';' terminator");
                                        return None;
                                    }
                                }
                            }
                        }
                        Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') => {
                            // P69n04: `% ;` - whitespace after %
                            self.set_error("Invalid PE reference: no whitespace allowed after '%'");
                            return None;
                        }
                        _ => {
                            // Just a standalone % character - allow it for now
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

            // Validate entity references in content
            if let Some(ref entity_map) = self.entities {
                if let Err(msg) = validate_content_entity_refs(content, entity_map) {
                    self.set_error(msg);
                    return None;
                }
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

/// Validate entity references in element content
/// Checks that all entity references are declared (except predefined entities)
fn validate_content_entity_refs(content: &[u8], entities: &HashMap<Vec<u8>, EntityInfo>) -> Result<(), &'static str> {
    let mut pos = 0;
    while pos < content.len() {
        if content[pos] == b'&' {
            pos += 1;
            if pos >= content.len() {
                return Ok(()); // Will be caught by decode_text_strict
            }
            // Skip character references
            if content[pos] == b'#' {
                while pos < content.len() && content[pos] != b';' {
                    pos += 1;
                }
                pos += 1;
                continue;
            }
            // Named entity reference - read name
            let name_start = pos;
            while pos < content.len() && is_name_char(content[pos]) {
                pos += 1;
            }
            if pos > name_start && pos < content.len() && content[pos] == b';' {
                let entity_name = &content[name_start..pos];
                // Check for predefined entities (always valid)
                let is_predefined = matches!(entity_name, b"lt" | b"gt" | b"amp" | b"quot" | b"apos");
                if !is_predefined {
                    // Look up entity (case-sensitive)
                    if let Some(info) = entities.get(entity_name) {
                        // Unparsed entities (NDATA) cannot be used in content
                        // They can only be used in ENTITY/ENTITIES type attributes
                        if info.is_unparsed {
                            return Err("Unparsed entity reference is not allowed in content");
                        }
                        // Check if entity replacement text contains forbidden content
                        if let Some(ref value) = info.value {
                            // XML declaration (<?xml...) is not allowed in entity replacement text
                            // The target name "xml" is reserved
                            if value.len() >= 5 {
                                let starts_with_pi = value[0] == b'<' && value[1] == b'?';
                                if starts_with_pi {
                                    let target = &value[2..];
                                    if target.len() >= 3 {
                                        let t0 = target[0].to_ascii_lowercase();
                                        let t1 = target[1].to_ascii_lowercase();
                                        let t2 = target[2].to_ascii_lowercase();
                                        if t0 == b'x' && t1 == b'm' && t2 == b'l' {
                                            // Check that it's followed by whitespace or ? (not xmlfoo)
                                            if target.len() == 3 || !is_name_char(target[3]) {
                                                return Err("Entity replacement text contains XML declaration or reserved PI target");
                                            }
                                        }
                                    }
                                }
                            }
                            // Check for invalid character references in nested attribute values
                            validate_entity_nested_attrs(value)?;

                            // Check if entity starts with &#60; which produces '<' and creates markup
                            // This is invalid if it creates unbalanced tags (start tag without end tag)
                            if value.starts_with(b"&#60;") || value.starts_with(b"&#x3c;") || value.starts_with(b"&#x3C;") {
                                // Check if followed by name-start char (would create start tag)
                                let prefix_len = if value.starts_with(b"&#60;") { 5 } else { 6 };
                                if value.len() > prefix_len && is_name_start_char(value[prefix_len]) {
                                    // Extract the tag name
                                    let name_start = prefix_len;
                                    let mut name_end = name_start;
                                    while name_end < value.len() && (is_name_char(value[name_end]) || value[name_end] == b'-' || value[name_end] == b'.' || value[name_end] == b':') {
                                        name_end += 1;
                                    }
                                    let tag_name = &value[name_start..name_end];
                                    // Check if there's a matching end tag in the entity
                                    // Look for </tagname>
                                    let mut end_tag = Vec::with_capacity(tag_name.len() + 3);
                                    end_tag.extend_from_slice(b"</");
                                    end_tag.extend_from_slice(tag_name);
                                    end_tag.push(b'>');
                                    if !find_subsequence(value, &end_tag).is_some() {
                                        return Err("Entity replacement text contains character reference that produces unbalanced markup");
                                    }
                                }
                            }

                            // Check if entity starts with end tag (</...) - creates mismatched tags
                            if value.starts_with(b"</") {
                                return Err("Entity replacement text starts with end tag which creates mismatched elements");
                            }

                            // Check for &#38; that produces bare & followed by # which would create split char ref
                            // Also check for character references in element names that produce invalid characters
                            validate_entity_content_structure(value)?;
                        }
                    } else {
                        // Check for case-insensitive match
                        let has_case_mismatch = entities.keys().any(|k| {
                            k.len() == entity_name.len() &&
                            k.iter().zip(entity_name.iter()).all(|(a, b)| a.eq_ignore_ascii_case(b)) &&
                            k != entity_name
                        });
                        if has_case_mismatch {
                            return Err("Entity reference uses wrong case (entity names are case-sensitive)");
                        }
                        return Err("Reference to undeclared entity");
                    }
                }
            }
        }
        pos += 1;
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
fn validate_tag_chars(content: &[u8], entities: Option<&HashMap<Vec<u8>, EntityInfo>>) -> Result<(), &'static str> {
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
            validate_attr_value(attr_value, entities)?;
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

/// Check if entity replacement text is valid for use in attribute values.
/// Returns error message if the entity or any referenced entity:
/// - Is external
/// - Contains literal '<'
/// - Contains character reference that produces '<' (&#60; or &#x3c;)
/// - References an undeclared entity
/// Uses a visited set to prevent infinite loops in circular references.
fn validate_entity_for_attr(
    entity_name: &[u8],
    entities: &HashMap<Vec<u8>, EntityInfo>,
    visited: &mut HashSet<Vec<u8>>,
) -> Result<(), &'static str> {
    // Prevent infinite recursion
    if visited.contains(entity_name) {
        return Ok(()); // Already validated this entity
    }
    visited.insert(entity_name.to_vec());

    let info = match entities.get(entity_name) {
        Some(info) => info,
        None => return Err("Reference to undeclared entity in attribute value"),
    };

    // WFC: No External Entity References
    if info.is_external || info.references_external {
        return Err("Attribute values cannot contain references to external entities");
    }

    // Unparsed entities are not allowed in attribute values (except ENTITY/ENTITIES attrs)
    if info.is_unparsed {
        return Err("Reference to unparsed entity in attribute value");
    }

    // Check replacement text if available
    if let Some(ref value) = info.value {
        // Check for literal '<' in replacement text
        if value.contains(&b'<') {
            return Err("Entity replacement text contains '<' which is not allowed in attribute values");
        }

        // Check for character references that produce '<' or bare '&'
        let mut pos = 0;
        while pos < value.len() {
            if value[pos] == b'&' {
                pos += 1;
                if pos >= value.len() {
                    continue;
                }
                if value[pos] == b'#' {
                    // Character reference - check if it produces '<' or bare '&'
                    pos += 1;
                    if pos >= value.len() {
                        continue;
                    }
                    let is_hex = value[pos] == b'x' || value[pos] == b'X';
                    if is_hex {
                        pos += 1;
                    }
                    // Find semicolon and extract number
                    let num_start = pos;
                    while pos < value.len() && value[pos] != b';' {
                        pos += 1;
                    }
                    if pos > num_start && pos < value.len() && value[pos] == b';' {
                        let num_str = &value[num_start..pos];
                        let codepoint = if is_hex {
                            parse_hex_codepoint(num_str)
                        } else {
                            parse_dec_codepoint(num_str)
                        };
                        // &#60; or &#x3c; produce '<'
                        if codepoint == Some(0x3C) {
                            return Err("Entity replacement text contains character reference for '<' which is not allowed in attribute values");
                        }
                        // &#38; produces '&' - check if it forms a bare ampersand
                        // A bare ampersand is when & is not followed by # (char ref) or name (entity ref)
                        if codepoint == Some(0x26) {
                            // Check what follows the semicolon
                            let after_semi = pos + 1;
                            if after_semi >= value.len() {
                                // Nothing after - bare &
                                return Err("Entity replacement text produces bare '&' which is not allowed in attribute values");
                            }
                            let next_char = value[after_semi];
                            // Valid continuations: # (char ref) or name-start (entity ref)
                            if next_char != b'#' && !is_name_start_char(next_char) {
                                return Err("Entity replacement text produces bare '&' which is not allowed in attribute values");
                            }
                        }
                    }
                } else {
                    // Named entity reference - find end and recursively validate
                    let name_start = pos;
                    while pos < value.len() && is_name_char(value[pos]) {
                        pos += 1;
                    }
                    if pos > name_start && pos < value.len() && value[pos] == b';' {
                        let ref_name = &value[name_start..pos];
                        // Skip predefined entities
                        if !matches!(ref_name, b"lt" | b"gt" | b"amp" | b"quot" | b"apos") {
                            validate_entity_for_attr(ref_name, entities, visited)?;
                        }
                    }
                }
            }
            pos += 1;
        }
    }

    Ok(())
}

/// Parse hex character reference digits to codepoint
fn parse_hex_codepoint(digits: &[u8]) -> Option<u32> {
    let mut result: u32 = 0;
    for &d in digits {
        let digit = match d {
            b'0'..=b'9' => d - b'0',
            b'a'..=b'f' => d - b'a' + 10,
            b'A'..=b'F' => d - b'A' + 10,
            _ => return None,
        };
        result = result.checked_mul(16)?.checked_add(digit as u32)?;
    }
    Some(result)
}

/// Parse decimal character reference digits to codepoint
fn parse_dec_codepoint(digits: &[u8]) -> Option<u32> {
    let mut result: u32 = 0;
    for &d in digits {
        if !d.is_ascii_digit() {
            return None;
        }
        result = result.checked_mul(10)?.checked_add((d - b'0') as u32)?;
    }
    Some(result)
}

/// Validate character references in attribute values within entity replacement text.
/// This checks for &#60; (produces '<') and &#38; (produces bare '&') in nested attribute values.
/// Used when validating entities used in content.
fn validate_entity_nested_attrs(value: &[u8]) -> Result<(), &'static str> {
    let mut pos = 0;
    while pos < value.len() {
        // Look for attribute value start: = followed by quote
        if value[pos] == b'=' {
            pos += 1;
            // Skip whitespace
            while pos < value.len() && (value[pos] == b' ' || value[pos] == b'\t' || value[pos] == b'\n' || value[pos] == b'\r') {
                pos += 1;
            }
            if pos >= value.len() {
                break;
            }
            let quote = value[pos];
            if quote == b'"' || quote == b'\'' {
                pos += 1;
                // Scan attribute value for character references
                while pos < value.len() && value[pos] != quote {
                    if value[pos] == b'&' && pos + 1 < value.len() && value[pos + 1] == b'#' {
                        // Character reference - check if it produces '<' or '&'
                        let ref_start = pos;
                        pos += 2; // Skip &#
                        let is_hex = pos < value.len() && (value[pos] == b'x' || value[pos] == b'X');
                        if is_hex {
                            pos += 1;
                        }
                        let num_start = pos;
                        while pos < value.len() && value[pos] != b';' && value[pos] != quote {
                            pos += 1;
                        }
                        if pos < value.len() && value[pos] == b';' && pos > num_start {
                            let num_str = &value[num_start..pos];
                            let codepoint = if is_hex {
                                parse_hex_codepoint(num_str)
                            } else {
                                parse_dec_codepoint(num_str)
                            };
                            // &#60; produces '<' which is invalid in attribute values
                            if codepoint == Some(0x3C) {
                                return Err("Entity replacement text contains '<' (via character reference) in nested attribute value");
                            }
                            // &#38; produces '&' - check if it's followed by valid reference
                            if codepoint == Some(0x26) {
                                let after_semi = pos + 1;
                                // If at end of attr value or not followed by # or name-start, it's bare &
                                if after_semi >= value.len() || value[after_semi] == quote {
                                    return Err("Entity replacement text contains bare '&' (via character reference) in nested attribute value");
                                }
                                let next_char = value[after_semi];
                                if next_char != b'#' && !is_name_start_char(next_char) {
                                    return Err("Entity replacement text contains bare '&' (via character reference) in nested attribute value");
                                }
                            }
                        }
                        continue;
                    }
                    pos += 1;
                }
            }
        }
        pos += 1;
    }
    Ok(())
}

/// Validate entity replacement text structure for content use.
/// Checks for:
/// - &#38; producing bare & followed by # (would create split char ref)
/// - &#38; at end of entity (produces bare & that could form split ref with following content)
/// - Character references in element names that produce invalid name characters
fn validate_entity_content_structure(value: &[u8]) -> Result<(), &'static str> {
    let mut pos = 0;
    while pos < value.len() {
        // Check for &#38; which produces &
        if value.len() >= 5 && pos + 5 <= value.len() && &value[pos..pos+5] == b"&#38;" {
            let after_pos = pos + 5;
            // If #38; is at the END of the entity, it produces bare & which can form split ref
            if after_pos == value.len() {
                return Err("Entity replacement text ends with '&#38;' which produces bare '&' that may form split reference");
            }
            // Check what follows the semicolon - if #, it creates split char ref
            if value[after_pos] == b'#' {
                return Err("Entity replacement text contains '&#38;' followed by '#' which creates a split character reference");
            }
            pos = after_pos;
            continue;
        }
        // Also check hex form &#x26;
        if value.len() >= 6 && pos + 6 <= value.len() && (&value[pos..pos+6] == b"&#x26;" || &value[pos..pos+6] == b"&#X26;") {
            let after_pos = pos + 6;
            // If at end of entity, produces bare &
            if after_pos == value.len() {
                return Err("Entity replacement text ends with '&#x26;' which produces bare '&' that may form split reference");
            }
            if value[after_pos] == b'#' {
                return Err("Entity replacement text contains '&#x26;' followed by '#' which creates a split character reference");
            }
            pos = after_pos;
            continue;
        }

        // Check for element names with character references that produce invalid characters
        // Pattern: <[name chars and char refs]> or </[name chars and char refs]>
        if value[pos] == b'<' {
            pos += 1;
            if pos >= value.len() {
                break;
            }
            // Skip / for end tags
            if value[pos] == b'/' {
                pos += 1;
            }
            if pos >= value.len() {
                break;
            }
            // Now we're at the start of element name - check for char refs
            let mut name_start = true;
            while pos < value.len() && value[pos] != b'>' && value[pos] != b' ' && value[pos] != b'\t' && value[pos] != b'\n' && value[pos] != b'\r' && value[pos] != b'/' {
                if value[pos] == b'&' && pos + 1 < value.len() && value[pos + 1] == b'#' {
                    // Character reference in element name - decode and validate
                    let ref_start = pos;
                    pos += 2; // Skip &#
                    let is_hex = pos < value.len() && (value[pos] == b'x' || value[pos] == b'X');
                    if is_hex {
                        pos += 1;
                    }
                    let num_start = pos;
                    while pos < value.len() && value[pos] != b';' {
                        pos += 1;
                    }
                    if pos > num_start && pos < value.len() && value[pos] == b';' {
                        let num_str = &value[num_start..pos];
                        let codepoint = if is_hex {
                            parse_hex_codepoint(num_str)
                        } else {
                            parse_dec_codepoint(num_str)
                        };
                        if let Some(cp) = codepoint {
                            // Check if this character is valid in element name
                            // At name start, must be NameStartChar; otherwise NameChar
                            if name_start {
                                if !is_name_start_char_codepoint(cp) {
                                    return Err("Character reference in element name produces invalid name start character");
                                }
                            } else {
                                if !is_name_char_codepoint(cp) {
                                    return Err("Character reference in element name produces invalid name character");
                                }
                            }
                        }
                        name_start = false;
                        pos += 1; // Skip ;
                        continue;
                    }
                }
                name_start = false;
                pos += 1;
            }
        }
        pos += 1;
    }
    Ok(())
}

/// Check if a Unicode codepoint is a valid XML 1.0 Letter (BaseChar | Ideographic)
/// This uses the stricter XML 1.0 edition 4 rules, not Fifth Edition
fn is_letter_codepoint(cp: u32) -> bool {
    is_base_char(cp) || is_ideographic(cp)
}

/// Check if codepoint is an XML 1.0 BaseChar
fn is_base_char(cp: u32) -> bool {
    matches!(cp,
        0x0041..=0x005A | 0x0061..=0x007A | 0x00C0..=0x00D6 | 0x00D8..=0x00F6 |
        0x00F8..=0x00FF | 0x0100..=0x0131 | 0x0134..=0x013E | 0x0141..=0x0148 |
        0x014A..=0x017E | 0x0180..=0x01C3 | 0x01CD..=0x01F0 | 0x01F4..=0x01F5 |
        0x01FA..=0x0217 | 0x0250..=0x02A8 | 0x02BB..=0x02C1 | 0x0386 |
        0x0388..=0x038A | 0x038C | 0x038E..=0x03A1 | 0x03A3..=0x03CE |
        0x03D0..=0x03D6 | 0x03DA | 0x03DC | 0x03DE | 0x03E0 | 0x03E2..=0x03F3 |
        0x0401..=0x040C | 0x040E..=0x044F | 0x0451..=0x045C | 0x045E..=0x0481 |
        0x0490..=0x04C4 | 0x04C7..=0x04C8 | 0x04CB..=0x04CC | 0x04D0..=0x04EB |
        0x04EE..=0x04F5 | 0x04F8..=0x04F9 | 0x0531..=0x0556 | 0x0559 |
        0x0561..=0x0586 | 0x05D0..=0x05EA | 0x05F0..=0x05F2 | 0x0621..=0x063A |
        0x0641..=0x064A | 0x0671..=0x06B7 | 0x06BA..=0x06BE | 0x06C0..=0x06CE |
        0x06D0..=0x06D3 | 0x06D5 | 0x06E5..=0x06E6 | 0x0905..=0x0939 | 0x093D |
        0x0958..=0x0961 | 0x0985..=0x098C | 0x098F..=0x0990 | 0x0993..=0x09A8 |
        0x09AA..=0x09B0 | 0x09B2 | 0x09B6..=0x09B9 | 0x09DC..=0x09DD |
        0x09DF..=0x09E1 | 0x09F0..=0x09F1 | 0x0A05..=0x0A0A | 0x0A0F..=0x0A10 |
        0x0A13..=0x0A28 | 0x0A2A..=0x0A30 | 0x0A32..=0x0A33 | 0x0A35..=0x0A36 |
        0x0A38..=0x0A39 | 0x0A59..=0x0A5C | 0x0A5E | 0x0A72..=0x0A74 |
        0x0A85..=0x0A8B | 0x0A8D | 0x0A8F..=0x0A91 | 0x0A93..=0x0AA8 |
        0x0AAA..=0x0AB0 | 0x0AB2..=0x0AB3 | 0x0AB5..=0x0AB9 | 0x0ABD | 0x0AE0 |
        0x0B05..=0x0B0C | 0x0B0F..=0x0B10 | 0x0B13..=0x0B28 | 0x0B2A..=0x0B30 |
        0x0B32..=0x0B33 | 0x0B36..=0x0B39 | 0x0B3D | 0x0B5C..=0x0B5D |
        0x0B5F..=0x0B61 | 0x0B85..=0x0B8A | 0x0B8E..=0x0B90 | 0x0B92..=0x0B95 |
        0x0B99..=0x0B9A | 0x0B9C | 0x0B9E..=0x0B9F | 0x0BA3..=0x0BA4 |
        0x0BA8..=0x0BAA | 0x0BAE..=0x0BB5 | 0x0BB7..=0x0BB9 | 0x0C05..=0x0C0C |
        0x0C0E..=0x0C10 | 0x0C12..=0x0C28 | 0x0C2A..=0x0C33 | 0x0C35..=0x0C39 |
        0x0C60..=0x0C61 | 0x0C85..=0x0C8C | 0x0C8E..=0x0C90 | 0x0C92..=0x0CA8 |
        0x0CAA..=0x0CB3 | 0x0CB5..=0x0CB9 | 0x0CDE | 0x0CE0..=0x0CE1 |
        0x0D05..=0x0D0C | 0x0D0E..=0x0D10 | 0x0D12..=0x0D28 | 0x0D2A..=0x0D39 |
        0x0D60..=0x0D61 | 0x0E01..=0x0E2E | 0x0E30 | 0x0E32..=0x0E33 |
        0x0E40..=0x0E45 | 0x0E81..=0x0E82 | 0x0E84 | 0x0E87..=0x0E88 | 0x0E8A |
        0x0E8D | 0x0E94..=0x0E97 | 0x0E99..=0x0E9F | 0x0EA1..=0x0EA3 | 0x0EA5 |
        0x0EA7 | 0x0EAA..=0x0EAB | 0x0EAD..=0x0EAE | 0x0EB0 | 0x0EB2..=0x0EB3 |
        0x0EBD | 0x0EC0..=0x0EC4 | 0x0F40..=0x0F47 | 0x0F49..=0x0F69 |
        0x10A0..=0x10C5 | 0x10D0..=0x10F6 | 0x1100 | 0x1102..=0x1103 |
        0x1105..=0x1107 | 0x1109 | 0x110B..=0x110C | 0x110E..=0x1112 | 0x113C |
        0x113E | 0x1140 | 0x114C | 0x114E | 0x1150 | 0x1154..=0x1155 | 0x1159 |
        0x115F..=0x1161 | 0x1163 | 0x1165 | 0x1167 | 0x1169 | 0x116D..=0x116E |
        0x1172..=0x1173 | 0x1175 | 0x119E | 0x11A8 | 0x11AB | 0x11AE..=0x11AF |
        0x11B7..=0x11B8 | 0x11BA | 0x11BC..=0x11C2 | 0x11EB | 0x11F0 | 0x11F9 |
        0x1E00..=0x1E9B | 0x1EA0..=0x1EF9 | 0x1F00..=0x1F15 | 0x1F18..=0x1F1D |
        0x1F20..=0x1F45 | 0x1F48..=0x1F4D | 0x1F50..=0x1F57 | 0x1F59 | 0x1F5B |
        0x1F5D | 0x1F5F..=0x1F7D | 0x1F80..=0x1FB4 | 0x1FB6..=0x1FBC | 0x1FBE |
        0x1FC2..=0x1FC4 | 0x1FC6..=0x1FCC | 0x1FD0..=0x1FD3 | 0x1FD6..=0x1FDB |
        0x1FE0..=0x1FEC | 0x1FF2..=0x1FF4 | 0x1FF6..=0x1FFC | 0x2126 |
        0x212A..=0x212B | 0x212E | 0x2180..=0x2182 | 0x3041..=0x3094 |
        0x30A1..=0x30FA | 0x3105..=0x312C | 0xAC00..=0xD7A3
    )
}

/// Check if codepoint is an XML 1.0 Ideographic character
fn is_ideographic(cp: u32) -> bool {
    matches!(cp, 0x4E00..=0x9FA5 | 0x3007 | 0x3021..=0x3029)
}

/// Check if codepoint is an XML 1.0 CombiningChar
fn is_combining_char(cp: u32) -> bool {
    matches!(cp,
        0x0300..=0x0345 | 0x0360..=0x0361 | 0x0483..=0x0486 | 0x0591..=0x05A1 |
        0x05A3..=0x05B9 | 0x05BB..=0x05BD | 0x05BF | 0x05C1..=0x05C2 | 0x05C4 |
        0x064B..=0x0652 | 0x0670 | 0x06D6..=0x06DC | 0x06DD..=0x06DF |
        0x06E0..=0x06E4 | 0x06E7..=0x06E8 | 0x06EA..=0x06ED | 0x0901..=0x0903 |
        0x093C | 0x093E..=0x094C | 0x094D | 0x0951..=0x0954 | 0x0962..=0x0963 |
        0x0981..=0x0983 | 0x09BC | 0x09BE | 0x09BF | 0x09C0..=0x09C4 |
        0x09C7..=0x09C8 | 0x09CB..=0x09CD | 0x09D7 | 0x09E2..=0x09E3 | 0x0A02 |
        0x0A3C | 0x0A3E | 0x0A3F | 0x0A40..=0x0A42 | 0x0A47..=0x0A48 |
        0x0A4B..=0x0A4D | 0x0A70..=0x0A71 | 0x0A81..=0x0A83 | 0x0ABC |
        0x0ABE..=0x0AC5 | 0x0AC7..=0x0AC9 | 0x0ACB..=0x0ACD | 0x0B01..=0x0B03 |
        0x0B3C | 0x0B3E..=0x0B43 | 0x0B47..=0x0B48 | 0x0B4B..=0x0B4D |
        0x0B56..=0x0B57 | 0x0B82..=0x0B83 | 0x0BBE..=0x0BC2 | 0x0BC6..=0x0BC8 |
        0x0BCA..=0x0BCD | 0x0BD7 | 0x0C01..=0x0C03 | 0x0C3E..=0x0C44 |
        0x0C46..=0x0C48 | 0x0C4A..=0x0C4D | 0x0C55..=0x0C56 | 0x0C82..=0x0C83 |
        0x0CBE..=0x0CC4 | 0x0CC6..=0x0CC8 | 0x0CCA..=0x0CCD | 0x0CD5..=0x0CD6 |
        0x0D02..=0x0D03 | 0x0D3E..=0x0D43 | 0x0D46..=0x0D48 | 0x0D4A..=0x0D4D |
        0x0D57 | 0x0E31 | 0x0E34..=0x0E3A | 0x0E47..=0x0E4E | 0x0EB1 |
        0x0EB4..=0x0EB9 | 0x0EBB..=0x0EBC | 0x0EC8..=0x0ECD | 0x0F18..=0x0F19 |
        0x0F35 | 0x0F37 | 0x0F39 | 0x0F3E | 0x0F3F | 0x0F71..=0x0F84 |
        0x0F86..=0x0F8B | 0x0F90..=0x0F95 | 0x0F97 | 0x0F99..=0x0FAD |
        0x0FB1..=0x0FB7 | 0x0FB9 | 0x20D0..=0x20DC | 0x20E1 | 0x302A..=0x302F |
        0x3099 | 0x309A
    )
}

/// Check if codepoint is an XML 1.0 Digit
fn is_digit_codepoint(cp: u32) -> bool {
    matches!(cp,
        0x0030..=0x0039 | 0x0660..=0x0669 | 0x06F0..=0x06F9 | 0x0966..=0x096F |
        0x09E6..=0x09EF | 0x0A66..=0x0A6F | 0x0AE6..=0x0AEF | 0x0B66..=0x0B6F |
        0x0BE7..=0x0BEF | 0x0C66..=0x0C6F | 0x0CE6..=0x0CEF | 0x0D66..=0x0D6F |
        0x0E50..=0x0E59 | 0x0ED0..=0x0ED9 | 0x0F20..=0x0F29
    )
}

/// Check if codepoint is an XML 1.0 Extender
fn is_extender(cp: u32) -> bool {
    matches!(cp,
        0x00B7 | 0x02D0 | 0x02D1 | 0x0387 | 0x0640 | 0x0E46 | 0x0EC6 | 0x3005 |
        0x3031..=0x3035 | 0x309D..=0x309E | 0x30FC..=0x30FE
    )
}

/// Check if a Unicode codepoint is a valid XML 1.0 NameStartChar (edition 4 rules)
/// NameStartChar = Letter | '_' | ':'
fn is_name_start_char_codepoint(cp: u32) -> bool {
    cp == 0x5F || // _
    cp == 0x3A || // :
    is_letter_codepoint(cp)
}

/// Check if a Unicode codepoint is a valid XML 1.0 NameChar (edition 4 rules)
/// NameChar = Letter | Digit | '.' | '-' | '_' | ':' | CombiningChar | Extender
fn is_name_char_codepoint(cp: u32) -> bool {
    is_letter_codepoint(cp) ||
    is_digit_codepoint(cp) ||
    cp == 0x2E || // .
    cp == 0x2D || // -
    cp == 0x5F || // _
    cp == 0x3A || // :
    is_combining_char(cp) ||
    is_extender(cp)
}

/// Validate attribute value content
/// Checks that & is followed by valid entity reference, and < is not present
/// If entity registry is provided, also validates that entities are declared and not external
fn validate_attr_value(value: &[u8], entities: Option<&HashMap<Vec<u8>, EntityInfo>>) -> Result<(), &'static str> {
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
                // Skip hex prefix if present (lowercase only per XML 1.0)
                let is_hex = if value[pos] == b'x' {
                    pos += 1;
                    true
                } else if value[pos] == b'X' {
                    return Err("Hex character references must use lowercase 'x' (&#x..;)");
                } else {
                    false
                };
                // Find semicolon
                let ref_start = pos;
                while pos < value.len() && value[pos] != b';' {
                    let c = value[pos];
                    if is_hex {
                        if !matches!(c, b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F') {
                            return Err("Invalid hex digit in character reference");
                        }
                    } else {
                        if !matches!(c, b'0'..=b'9') {
                            return Err("Invalid digit in decimal character reference");
                        }
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

                // Validate entity reference if registry provided
                if let Some(entity_map) = entities {
                    let entity_name = &value[name_start..pos];
                    // Check for predefined entities (always valid)
                    let is_predefined = matches!(entity_name, b"lt" | b"gt" | b"amp" | b"quot" | b"apos");
                    if !is_predefined {
                        // Check if entity exists first for case mismatch detection
                        if !entity_map.contains_key(entity_name) {
                            // Entity not declared - check for case-insensitive match
                            let has_case_mismatch = entity_map.keys().any(|k| {
                                k.len() == entity_name.len() &&
                                k.iter().zip(entity_name.iter()).all(|(a, b)| a.eq_ignore_ascii_case(b)) &&
                                k != entity_name
                            });
                            if has_case_mismatch {
                                return Err("Entity reference uses wrong case (entity names are case-sensitive)");
                            }
                        }
                        // Recursively validate entity and its references
                        let mut visited = HashSet::new();
                        validate_entity_for_attr(entity_name, entity_map, &mut visited)?;
                    }
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

    // Extract and validate version value
    let quote = rest[0];
    let rest = &rest[1..];
    let value_end = rest.iter().position(|&b| b == quote).unwrap_or(rest.len());
    let version_value = &rest[..value_end];

    // Validate version number: VersionNum ::= '1.' [0-9]+
    if version_value.is_empty() {
        return Err("XML declaration version cannot be empty");
    }
    if !version_value.starts_with(b"1.") {
        return Err("XML declaration version must start with '1.'");
    }
    if version_value.len() < 3 {
        return Err("XML declaration version must have digits after '1.'");
    }
    for &b in &version_value[2..] {
        if !b.is_ascii_digit() {
            return Err("XML declaration version must be '1.' followed by digits only");
        }
    }

    let rest = if value_end + 1 <= rest.len() { &rest[value_end + 1..] } else { &[] };

    // Check for encoding or standalone - they require preceding whitespace
    if !rest.is_empty() {
        // Must have whitespace before next attribute
        if !matches!(rest[0], b' ' | b'\t' | b'\n' | b'\r') {
            return Err("Whitespace required before encoding or standalone in XML declaration");
        }
        let rest = skip_ws(rest);

        // Check for encoding attribute
        let rest = if rest.starts_with(b"encoding") {
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
                // Continue past encoding to check for standalone
                let rest = &after_eq[1 + value_end + 1..];
                skip_ws(rest)
            } else {
                return Err("encoding value not properly terminated");
            }
        } else {
            rest
        };

        // Check for standalone attribute
        let rest = if rest.starts_with(b"standalone") {
            let after_sa = &rest[10..];
            let after_sa = skip_ws(after_sa);
            if after_sa.is_empty() || after_sa[0] != b'=' {
                return Err("standalone attribute must have '='");
            }
            let after_eq = skip_ws(&after_sa[1..]);
            if after_eq.is_empty() || (after_eq[0] != b'"' && after_eq[0] != b'\'') {
                return Err("standalone value must be quoted");
            }
            let quote = after_eq[0];
            if let Some(value_end) = after_eq[1..].iter().position(|&b| b == quote) {
                let sa_value = &after_eq[1..1 + value_end];
                // Standalone value must be exactly "yes" or "no" (case-sensitive)
                if sa_value != b"yes" && sa_value != b"no" {
                    return Err("standalone value must be 'yes' or 'no'");
                }
                skip_ws(&after_eq[1 + value_end + 1..])
            } else {
                return Err("standalone value not properly terminated");
            }
        } else {
            rest
        };

        // Check for unknown pseudo-attributes (only version, encoding, standalone allowed)
        if !rest.is_empty() {
            // Check if there's a name-like thing that could be an unknown attribute
            if is_name_start(rest[0]) {
                return Err("Unknown attribute in XML declaration (only version, encoding, standalone allowed)");
            }
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

/// Validate Public ID literal per XML spec
/// PubidChar ::= #x20 | #xD | #xA | [a-zA-Z0-9] | [-'()+,./:=?;!*#@$_%]
fn validate_pubid_literal(pubid: &[u8]) -> Result<(), &'static str> {
    for &b in pubid {
        let valid = matches!(b,
            b' ' | b'\r' | b'\n' |
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' |
            b'-' | b'\'' | b'(' | b')' | b'+' | b',' | b'.' |
            b'/' | b':' | b'=' | b'?' | b';' | b'!' | b'*' |
            b'#' | b'@' | b'$' | b'_' | b'%'
        );
        if !valid {
            return Err("Invalid character in public ID literal");
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

            // Check for missing whitespace after keyword (e.g., ELEMENTfoo)
            let known_prefixes: &[&[u8]] = &[b"ELEMENT", b"ATTLIST", b"ENTITY", b"NOTATION"];
            for prefix in known_prefixes {
                if keyword.len() > prefix.len() && keyword.starts_with(prefix) {
                    return Err("Missing whitespace after DTD keyword");
                }
            }

            // Unknown keyword - allow it (could be extension)
            Ok(())
        }
    }
}

/// Validate ELEMENT declaration content (contentspec keywords)
fn validate_element_content(content: &[u8]) -> Result<(), &'static str> {
    // Check for PE references inside markup declaration (not allowed in internal subset)
    // WFC: PEs in Internal Subset
    for i in 0..content.len() {
        if content[i] == b'%' {
            // Check if followed by name char (PE reference)
            if i + 1 < content.len() && is_name_start(content[i + 1]) {
                // Check if there's a ; later (completes the PE reference)
                for j in (i + 2)..content.len() {
                    if content[j] == b';' {
                        return Err("Parameter entity reference not allowed inside markup declarations in internal subset");
                    }
                    if !is_name_char_byte(content[j]) {
                        break;
                    }
                }
            }
        }
    }

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
                } else {
                    // Pure (#PCDATA) - only * is allowed as modifier (means same thing as nothing)
                    // + and ? are not valid for #PCDATA
                    if pos < len && matches!(content[pos], b'+' | b'?') {
                        return Err("Invalid modifier on (#PCDATA) - only (#PCDATA) or (#PCDATA)* is valid");
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
    let mut depth: usize = 1; // Already inside one '('
    let mut last_was_occurrence = false;
    let mut last_was_separator = true; // Start position is like after separator
    let mut last_was_close_paren = false;
    // Track which separator is used at each depth level (0 = none, b',' = seq, b'|' = choice)
    let mut separator_at_depth: Vec<u8> = vec![0];

    while pos < len && depth > 0 {
        // Skip whitespace but track if we did
        let pos_before_ws = pos;
        while pos < len && matches!(content[pos], b' ' | b'\t' | b'\n' | b'\r') {
            pos += 1;
        }
        let had_whitespace = pos > pos_before_ws;

        if pos >= len {
            break;
        }

        let b = content[pos];

        match b {
            b'(' => {
                // '(' can only appear:
                // 1. After a separator (,|)
                // 2. At the start (depth=1 and it's the first position)
                // NOT after ')' without a separator, NOT after a name without a separator
                if !last_was_separator {
                    // Allow at very start of content model (when depth=1 and we just started)
                    // but not after ) or after a name without separator
                    if last_was_close_paren || last_was_occurrence {
                        return Err("Separator required between content particles");
                    }
                    // Check if we're not at the start of a group
                    if depth > 1 {
                        return Err("Separator required between content particles");
                    }
                }
                depth += 1;
                separator_at_depth.push(0); // New level has no separator yet
                last_was_occurrence = false;
                last_was_separator = true; // Content after ( is like after separator
                last_was_close_paren = false;
                pos += 1;
            }
            b')' => {
                if depth == 0 {
                    return Err("Extra closing parenthesis in content model");
                }
                // Closing paren cannot follow separator (e.g., (a,) or (a|) is invalid)
                if last_was_separator {
                    return Err("Content model cannot have trailing separator before ')'");
                }
                depth -= 1;
                separator_at_depth.pop();
                last_was_occurrence = false;
                last_was_separator = false;
                last_was_close_paren = true;
                pos += 1;
            }
            b'?' | b'*' | b'+' => {
                // Occurrence indicator must follow name, ')', or nothing (at name end)
                // Whitespace is NOT allowed before occurrence indicator
                if had_whitespace {
                    return Err("Whitespace not allowed before occurrence indicator");
                }
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
                // Separator cannot follow another separator or open paren
                if last_was_separator {
                    return Err("Invalid consecutive or misplaced separators in content model");
                }
                // Check that separators are consistent within the current group
                let current_depth_idx = depth.saturating_sub(1) as usize;
                if current_depth_idx < separator_at_depth.len() {
                    let prev_sep = separator_at_depth[current_depth_idx];
                    if prev_sep == 0 {
                        // First separator at this level
                        separator_at_depth[current_depth_idx] = b;
                    } else if prev_sep != b {
                        // Mixed separators (seq and choice cannot be mixed at same level)
                        return Err("Content model cannot mix ',' (sequence) and '|' (choice) at the same level");
                    }
                }
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

    // Check what follows the final ')' - should be optional occurrence indicator (* + ?)
    // and then whitespace or end
    if pos < len {
        let b = content[pos];
        if matches!(b, b'*' | b'+' | b'?') {
            pos += 1;
        }
        // Skip trailing whitespace
        while pos < len && matches!(content[pos], b' ' | b'\t' | b'\n' | b'\r') {
            pos += 1;
        }
        // Should be at end now
        if pos < len {
            return Err("Invalid character after content model");
        }
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

/// Validate entity references within entity values
/// Checks that all referenced entities are already declared (no forward references)
/// Also checks for indirect external entity references
/// Skips validation inside CDATA sections
fn validate_entity_value_refs(value: &[u8], entities: &HashMap<Vec<u8>, EntityInfo>) -> Result<(), &'static str> {
    let mut pos = 0;
    while pos < value.len() {
        // Skip CDATA sections - entity references inside CDATA are literal text
        if pos + 9 <= value.len() && &value[pos..pos+9] == b"<![CDATA[" {
            pos += 9;
            // Find end of CDATA
            while pos + 3 <= value.len() {
                if &value[pos..pos+3] == b"]]>" {
                    pos += 3;
                    break;
                }
                pos += 1;
            }
            continue;
        }

        if value[pos] == b'&' {
            pos += 1;
            if pos >= value.len() {
                break;
            }
            // Skip character references
            if value[pos] == b'#' {
                while pos < value.len() && value[pos] != b';' {
                    pos += 1;
                }
                pos += 1;
                continue;
            }
            // Named entity reference
            let name_start = pos;
            while pos < value.len() && is_name_char_byte(value[pos]) {
                pos += 1;
            }
            if pos > name_start && pos < value.len() && value[pos] == b';' {
                let entity_name = &value[name_start..pos];
                // Check for predefined entities (always valid)
                let is_predefined = matches!(entity_name, b"lt" | b"gt" | b"amp" | b"quot" | b"apos");
                if !is_predefined {
                    // Check if entity is declared
                    if !entities.contains_key(entity_name) {
                        return Err("Entity value references undeclared entity");
                    }
                }
            }
        }
        pos += 1;
    }
    Ok(())
}

/// Validate entity references in attribute default values
/// Checks that all referenced entities are already declared (no forward references)
fn validate_default_value_entities(value: &[u8], entities: &HashMap<Vec<u8>, EntityInfo>) -> Result<(), &'static str> {
    let mut pos = 0;
    while pos < value.len() {
        if value[pos] == b'&' {
            pos += 1;
            if pos >= value.len() {
                break;
            }
            // Skip character references
            if value[pos] == b'#' {
                while pos < value.len() && value[pos] != b';' {
                    pos += 1;
                }
                pos += 1;
                continue;
            }
            // Named entity reference
            let name_start = pos;
            while pos < value.len() && is_name_char_byte(value[pos]) {
                pos += 1;
            }
            if pos > name_start && pos < value.len() && value[pos] == b';' {
                let entity_name = &value[name_start..pos];
                // Check for predefined entities (always valid)
                let is_predefined = matches!(entity_name, b"lt" | b"gt" | b"amp" | b"quot" | b"apos");
                if !is_predefined {
                    // Check if entity is declared
                    if !entities.contains_key(entity_name) {
                        return Err("Reference to undeclared entity in attribute default value");
                    }
                }
            }
        }
        pos += 1;
    }
    Ok(())
}

/// Validate ATTLIST declaration content
/// AttlistDecl ::= '<!ATTLIST' S Name AttDef* S? '>'
/// AttDef ::= S Name S AttType S DefaultDecl
fn validate_attlist_content(content: &[u8], entities: Option<&HashMap<Vec<u8>, EntityInfo>>) -> Result<(), &'static str> {
    // Check for PE references inside markup declaration (not allowed in internal subset)
    // WFC: PEs in Internal Subset
    for i in 0..content.len() {
        if content[i] == b'%' {
            // Check if followed by name char (PE reference)
            if i + 1 < content.len() && is_name_start(content[i + 1]) {
                // Check if there's a ; later (completes the PE reference)
                for j in (i + 2)..content.len() {
                    if content[j] == b';' {
                        return Err("Parameter entity reference not allowed inside markup declarations in internal subset");
                    }
                    if !is_name_char_byte(content[j]) {
                        break;
                    }
                }
            }
        }
    }

    // Valid attribute types per XML 1.0
    let valid_att_types: &[&[u8]] = &[
        b"CDATA", b"ID", b"IDREF", b"IDREFS", b"ENTITY", b"ENTITIES",
        b"NMTOKEN", b"NMTOKENS", b"NOTATION",
    ];

    // Default declarations
    let default_keywords: &[&[u8]] = &[b"#REQUIRED", b"#IMPLIED", b"#FIXED"];

    // Helper: check if slice starts with keyword at word boundary
    fn is_keyword_at(content: &[u8], pos: usize, keyword: &[u8]) -> bool {
        if pos + keyword.len() > content.len() {
            return false;
        }
        if !content[pos..].starts_with(keyword) {
            return false;
        }
        // Check word boundary after
        let after = pos + keyword.len();
        after >= content.len() || !is_name_char_byte(content[after])
    }

    // Helper: check if slice starts with keyword (case insensitive) at word boundary
    fn is_keyword_at_ci(content: &[u8], pos: usize, keyword: &[u8]) -> bool {
        if pos + keyword.len() > content.len() {
            return false;
        }
        let slice = &content[pos..pos + keyword.len()];
        if !slice.eq_ignore_ascii_case(keyword) {
            return false;
        }
        // Check word boundary after
        let after = pos + keyword.len();
        after >= content.len() || !is_name_char_byte(content[after])
    }

    // State machine for parsing ATTLIST
    // States: ExpectAttrName, ExpectAttType, ExpectDefaultDecl
    #[derive(Clone, Copy, PartialEq)]
    enum State {
        ExpectAttrName,
        ExpectAttType,
        ExpectDefaultDecl,
    }

    let mut pos = 0;
    let mut state = State::ExpectAttrName;
    let len = content.len();
    let mut had_whitespace: bool;

    // NOTE: The content passed here starts AFTER the element name has been read
    // So content contains only the attribute definitions, e.g., " a1 CDATA #IMPLIED"

    // Parse the AttDef entries
    while pos < len {
        // Skip whitespace and track if any was found
        let ws_start = pos;
        while pos < len && matches!(content[pos], b' ' | b'\t' | b'\n' | b'\r') {
            pos += 1;
        }
        had_whitespace = pos > ws_start;

        if pos >= len {
            // End of content - check if we were expecting something
            if state == State::ExpectAttType {
                return Err("Incomplete attribute definition: missing attribute type");
            }
            if state == State::ExpectDefaultDecl {
                return Err("Incomplete attribute definition: missing default declaration");
            }
            break;
        }

        // Check for required whitespace in attribute definitions
        // AttDef ::= S Name S AttType S DefaultDecl
        if state == State::ExpectAttType && !had_whitespace {
            // Missing whitespace between attribute name and type
            // sa-065: a1(foo|bar) - missing space
            return Err("Whitespace required between attribute name and type");
        }
        if state == State::ExpectDefaultDecl && !had_whitespace {
            // Missing whitespace between type and default
            // sa-064: CDATA"foo", sa-066: (foo|bar)#IMPLIED, sa-067: (foo)"foo"
            return Err("Whitespace required between attribute type and default declaration");
        }

        match state {
            State::ExpectAttrName => {
                // Check if this is a type keyword instead of a name
                // This would indicate missing attribute name (e.g., element name as attr type)
                for kw in valid_att_types.iter() {
                    if is_keyword_at(content, pos, kw) {
                        // Check if it's wrong case
                        let slice = &content[pos..pos + kw.len()];
                        if slice.eq_ignore_ascii_case(kw) && slice != *kw {
                            return Err(match *kw {
                                b"CDATA" => "'CDATA' must be uppercase",
                                b"ID" => "'ID' must be uppercase",
                                b"IDREF" => "'IDREF' must be uppercase",
                                b"IDREFS" => "'IDREFS' must be uppercase",
                                b"ENTITY" => "'ENTITY' must be uppercase",
                                b"ENTITIES" => "'ENTITIES' must be uppercase",
                                b"NMTOKEN" => "'NMTOKEN' must be uppercase",
                                b"NMTOKENS" => "'NMTOKENS' must be uppercase",
                                b"NOTATION" => "'NOTATION' must be uppercase",
                                _ => "Attribute type must be uppercase",
                            });
                        }
                    }
                }

                // Check for default keyword where attr name expected
                for kw in default_keywords.iter() {
                    if is_keyword_at_ci(content, pos, kw) {
                        return Err("Missing attribute type before default declaration");
                    }
                }

                // Check for enumeration where attr name expected
                if content[pos] == b'(' {
                    return Err("Missing attribute name before enumeration type");
                }

                // Check for quoted string where attr name expected
                if content[pos] == b'"' || content[pos] == b'\'' {
                    return Err("Missing attribute name and type before default value");
                }

                // Read attribute name
                if !is_name_start(content[pos]) {
                    // End of attribute definitions
                    break;
                }

                let name_start = pos;
                while pos < len && is_name_char_byte(content[pos]) {
                    pos += 1;
                }

                state = State::ExpectAttType;
            }

            State::ExpectAttType => {
                // Must be an AttType: tokenized type, enumeration, or NOTATION
                let mut found_type = false;

                // Check for valid tokenized types
                for kw in valid_att_types.iter() {
                    if is_keyword_at_ci(content, pos, kw) {
                        // Check case
                        let slice = &content[pos..pos + kw.len()];
                        if slice != *kw {
                            return Err(match *kw {
                                b"CDATA" => "'CDATA' must be uppercase",
                                b"ID" => "'ID' must be uppercase",
                                b"IDREF" => "'IDREF' must be uppercase",
                                b"IDREFS" => "'IDREFS' must be uppercase",
                                b"ENTITY" => "'ENTITY' must be uppercase",
                                b"ENTITIES" => "'ENTITIES' must be uppercase",
                                b"NMTOKEN" => "'NMTOKEN' must be uppercase",
                                b"NMTOKENS" => "'NMTOKENS' must be uppercase",
                                b"NOTATION" => "'NOTATION' must be uppercase",
                                _ => "Attribute type must be uppercase",
                            });
                        }
                        pos += kw.len();
                        found_type = true;

                        // NOTATION must be followed by whitespace and enumeration
                        if *kw == b"NOTATION" {
                            // Check for required whitespace between NOTATION and enumeration
                            // sa-068: NOTATION(foo) - missing space
                            let notation_ws_start = pos;
                            while pos < len && matches!(content[pos], b' ' | b'\t' | b'\n' | b'\r') {
                                pos += 1;
                            }
                            let notation_had_ws = pos > notation_ws_start;

                            if pos >= len || content[pos] != b'(' {
                                return Err("NOTATION type must be followed by enumeration");
                            }
                            if !notation_had_ws {
                                return Err("Whitespace required between NOTATION and enumeration");
                            }
                            // Parse NOTATION enumeration
                            let mut depth = 1;
                            pos += 1;

                            // Skip initial whitespace inside enum
                            while pos < len && matches!(content[pos], b' ' | b'\t' | b'\n' | b'\r') {
                                pos += 1;
                            }

                            // Check for empty NOTATION enumeration - P58n03
                            if pos < len && content[pos] == b')' {
                                return Err("NOTATION enumeration cannot be empty");
                            }

                            while pos < len && depth > 0 {
                                match content[pos] {
                                    b'(' => depth += 1,
                                    b')' => depth -= 1,
                                    b',' => {
                                        return Err("Invalid comma in enumeration - use '|' separator");
                                    }
                                    b'"' | b'\'' => {
                                        // Quoted name in NOTATION enumeration - P58n08
                                        return Err("NOTATION names must not be quoted");
                                    }
                                    _ => {}
                                }
                                pos += 1;
                            }
                        }
                        break;
                    }
                }

                // Check for enumeration type (a|b|c)
                if !found_type && content[pos] == b'(' {
                    let enum_start = pos;
                    let mut depth = 1;
                    let mut has_content = false;
                    pos += 1;

                    // Skip initial whitespace inside enum
                    while pos < len && matches!(content[pos], b' ' | b'\t' | b'\n' | b'\r') {
                        pos += 1;
                    }

                    // Check for empty enumeration - P58n03, P59n01
                    if pos < len && content[pos] == b')' {
                        return Err("Enumeration cannot be empty");
                    }

                    while pos < len && depth > 0 {
                        match content[pos] {
                            b'(' => depth += 1,
                            b')' => depth -= 1,
                            b',' => {
                                return Err("Invalid comma in enumeration - use '|' separator");
                            }
                            b'"' | b'\'' => {
                                // Quoted value in enumeration - P58n08, P59n04
                                return Err("Enumeration values must not be quoted");
                            }
                            b'|' | b' ' | b'\t' | b'\n' | b'\r' => {}
                            _ => has_content = true,
                        }
                        pos += 1;
                    }
                    found_type = true;
                }

                // Check for default keyword where type expected
                if !found_type {
                    for kw in default_keywords.iter() {
                        if is_keyword_at_ci(content, pos, kw) {
                            return Err("Missing attribute type before default declaration");
                        }
                    }
                }

                // Check for quoted string where type expected (P53n08)
                if !found_type && (content[pos] == b'"' || content[pos] == b'\'') {
                    return Err("Missing attribute type (found default value where type expected)");
                }

                // Check for invalid type names
                if !found_type && is_name_start(content[pos]) {
                    // Read the token
                    let tok_start = pos;
                    while pos < len && is_name_char_byte(content[pos]) {
                        pos += 1;
                    }
                    let token = &content[tok_start..pos];

                    // Check common invalid types
                    if token.eq_ignore_ascii_case(b"PCDATA") {
                        return Err("Invalid attribute type 'PCDATA' - use 'CDATA' instead");
                    }
                    if token.eq_ignore_ascii_case(b"NAME") {
                        return Err("Invalid attribute type 'NAME' - use 'NMTOKEN' instead");
                    }
                    // Generic invalid type
                    return Err("Invalid attribute type (must be CDATA, ID, IDREF, IDREFS, ENTITY, ENTITIES, NMTOKEN, NMTOKENS, NOTATION, or enumeration)");
                }

                if found_type {
                    state = State::ExpectDefaultDecl;
                } else {
                    // Syntax error - can't determine type
                    break;
                }
            }

            State::ExpectDefaultDecl => {
                let mut found_default = false;

                // Check for #REQUIRED, #IMPLIED, #FIXED
                for kw in default_keywords.iter() {
                    if is_keyword_at_ci(content, pos, kw) {
                        // Check case
                        let slice = &content[pos..pos + kw.len()];
                        if slice != *kw {
                            return Err(match *kw {
                                b"#REQUIRED" => "'#REQUIRED' must be uppercase",
                                b"#IMPLIED" => "'#IMPLIED' must be uppercase",
                                b"#FIXED" => "'#FIXED' must be uppercase",
                                _ => "Default declaration must be uppercase",
                            });
                        }
                        pos += kw.len();
                        found_default = true;

                        // #FIXED must be followed by whitespace and then default value
                        if *kw == b"#FIXED" {
                            // Check for required whitespace before value
                            // P60n05: #FIXED"Introduction" is invalid (missing space)
                            let fixed_ws_start = pos;
                            while pos < len && matches!(content[pos], b' ' | b'\t' | b'\n' | b'\r') {
                                pos += 1;
                            }
                            let fixed_had_ws = pos > fixed_ws_start;

                            if pos >= len {
                                return Err("#FIXED requires a default value");
                            }
                            if !fixed_had_ws && (content[pos] == b'"' || content[pos] == b'\'') {
                                return Err("Whitespace required between #FIXED and default value");
                            }
                            if content[pos] != b'"' && content[pos] != b'\'' {
                                return Err("#FIXED default value must be quoted");
                            }
                            // Skip quoted value and validate entity references
                            let quote = content[pos];
                            let val_start = pos + 1;
                            pos += 1;
                            while pos < len && content[pos] != quote {
                                pos += 1;
                            }
                            // Validate entity references in default value
                            if let Some(entity_map) = entities {
                                let default_val = &content[val_start..pos];
                                validate_default_value_entities(default_val, entity_map)?;
                            }
                            if pos < len {
                                pos += 1;
                            }
                        }
                        break;
                    }
                }

                // Check for quoted default value
                if !found_default && (content[pos] == b'"' || content[pos] == b'\'') {
                    let quote = content[pos];
                    let val_start = pos + 1;
                    pos += 1;
                    while pos < len && content[pos] != quote {
                        pos += 1;
                    }
                    // Validate entity references in default value
                    if let Some(entity_map) = entities {
                        let default_val = &content[val_start..pos];
                        validate_default_value_entities(default_val, entity_map)?;
                    }
                    if pos < len {
                        pos += 1;
                    }
                    found_default = true;
                }

                // Check for unquoted default value - P53n05, sa-059
                // <!ATTLIST doc a1 NMTOKEN v1> - "v1" is unquoted
                if !found_default && is_name_start(content[pos]) {
                    // This could be an unquoted value or the next attribute name
                    // Read the token
                    let tok_start = pos;
                    while pos < len && is_name_char_byte(content[pos]) {
                        pos += 1;
                    }
                    let token = &content[tok_start..pos];

                    // Skip whitespace to see what follows
                    let mut check_pos = pos;
                    while check_pos < len && matches!(content[check_pos], b' ' | b'\t' | b'\n' | b'\r') {
                        check_pos += 1;
                    }

                    // If followed by AttType keyword or enumeration, this was an unquoted value
                    let mut next_is_type = false;
                    if check_pos < len {
                        for kw in valid_att_types.iter() {
                            if is_keyword_at_ci(content, check_pos, kw) {
                                next_is_type = true;
                                break;
                            }
                        }
                        if content[check_pos] == b'(' {
                            next_is_type = true;
                        }
                    }

                    if next_is_type || check_pos >= len {
                        // This token was an unquoted default value
                        return Err("Attribute default value must be quoted");
                    }

                    // Otherwise it's the next attribute name
                    pos = tok_start;  // Reset position
                    found_default = true;  // Treat as implied default and continue
                }

                if found_default {
                    state = State::ExpectAttrName;
                } else {
                    // Missing default declaration
                    return Err("Missing default declaration in attribute definition");
                }
            }
        }
    }

    // Check final state
    if state == State::ExpectAttType {
        return Err("Incomplete attribute definition: missing attribute type");
    }
    if state == State::ExpectDefaultDecl {
        return Err("Incomplete attribute definition: missing default declaration");
    }

    Ok(())
}

/// Parse entity info from declaration content for tracking
fn parse_entity_info(content: &[u8]) -> EntityInfo {
    let content = skip_ws(content);
    let has_system = find_word(content, b"SYSTEM").is_some();
    let has_public = find_word(content, b"PUBLIC").is_some();
    let has_ndata = find_word(content, b"NDATA").is_some();
    let is_external = has_system || has_public;
    let is_unparsed = has_ndata;

    // Extract value for internal entities (quoted string)
    let value = if !is_external && !content.is_empty() {
        // Find the quoted string value
        let quote = content[0];
        if quote == b'"' || quote == b'\'' {
            if let Some(end) = content[1..].iter().position(|&b| b == quote) {
                Some(content[1..1 + end].to_vec())
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // Extract NDATA notation name
    let ndata_notation = if has_ndata {
        if let Some(ndata_idx) = find_word(content, b"NDATA") {
            let after_ndata = skip_ws(&content[ndata_idx + 5..]);
            if !after_ndata.is_empty() && is_name_start(after_ndata[0]) {
                let mut end = 1;
                while end < after_ndata.len() && is_name_char_byte(after_ndata[end]) {
                    end += 1;
                }
                Some(after_ndata[..end].to_vec())
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    EntityInfo {
        is_external,
        is_unparsed,
        value,
        ndata_notation,
        references_external: false, // Will be updated after parsing when we can check referenced entities
    }
}

/// Validate ENTITY declaration content
fn validate_entity_content(content: &[u8], is_parameter_entity: bool) -> Result<(), &'static str> {
    let content = skip_ws(content);

    // P71n03, P72n04: Entity declaration requires a value or external ID
    if content.is_empty() {
        return Err("Entity declaration requires a value or external ID");
    }

    // Check for SGML-style comments (-- ... --) which are not allowed in XML
    if find_subsequence(content, b"--").is_some() {
        return Err("SGML-style comments (-- ... --) are not allowed in XML declarations");
    }

    // Parameter entities cannot have NDATA (only general unparsed entities can)
    if is_parameter_entity && find_word(content, b"NDATA").is_some() {
        return Err("Parameter entities cannot have NDATA declaration");
    }

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

            // Check for missing whitespace after keyword
            let after_idx = idx + upper.len();
            if after_idx < content.len() {
                let next = content[after_idx];
                if next == b'"' || next == b'\'' {
                    return Err(match *name {
                        "SYSTEM" => "Whitespace required between SYSTEM and URI",
                        "PUBLIC" => "Whitespace required between PUBLIC and public ID",
                        "NDATA" => "Whitespace required between NDATA and notation name",
                        _ => "Whitespace required after keyword",
                    });
                }
            }

            // Check for missing whitespace before NDATA
            if *name == "NDATA" && idx > 0 {
                let before = content[idx - 1];
                if before == b'"' || before == b'\'' {
                    return Err("Whitespace required before NDATA");
                }
            }
        }
    }

    // Check for PUBLIC keyword and validate the public ID literal
    let has_public = find_word(content, b"PUBLIC").is_some();
    let has_system = find_word(content, b"SYSTEM").is_some();
    let has_ndata = find_word(content, b"NDATA").is_some();

    // P75n12: Check for wrong ordering (quoted string before SYSTEM/PUBLIC keyword)
    let system_idx = find_word(content, b"SYSTEM");
    let public_idx = find_word(content, b"PUBLIC");

    // Check if there's a quoted string before SYSTEM/PUBLIC
    if has_system || has_public {
        let keyword_idx = match (system_idx, public_idx) {
            (Some(s), Some(p)) => Some(s.min(p)),
            (Some(s), None) => Some(s),
            (None, Some(p)) => Some(p),
            (None, None) => None,
        };
        if let Some(kw_idx) = keyword_idx {
            // Look for quoted string before the keyword
            for i in 0..kw_idx {
                if content[i] == b'"' || content[i] == b'\'' {
                    return Err("External ID has wrong field ordering (quoted string before SYSTEM/PUBLIC keyword)");
                }
            }
        }
    }

    // Validate NDATA structure if present
    if has_ndata {
        if let Some(ndata_idx) = find_word(content, b"NDATA") {
            // P73n01, P73n03: NDATA requires an ExternalID (SYSTEM/PUBLIC) before it
            if !has_system && !has_public {
                return Err("NDATA requires an ExternalID (SYSTEM or PUBLIC) before it");
            }

            // P73n01: Check NDATA comes AFTER SYSTEM/PUBLIC, not before
            let system_idx = find_word(content, b"SYSTEM");
            let public_idx = find_word(content, b"PUBLIC");
            let external_id_idx = match (system_idx, public_idx) {
                (Some(s), Some(p)) => Some(s.min(p)),
                (Some(s), None) => Some(s),
                (None, Some(p)) => Some(p),
                (None, None) => None,
            };
            if let Some(ext_idx) = external_id_idx {
                if ndata_idx < ext_idx {
                    return Err("NDATA must appear after ExternalID (SYSTEM/PUBLIC), not before");
                }
            }

            // P76n06: Check that NDATA is followed by whitespace (NDATAJPGformat is invalid)
            let after_ndata_raw = &content[ndata_idx + 5..];
            if !after_ndata_raw.is_empty() && !matches!(after_ndata_raw[0], b' ' | b'\t' | b'\n' | b'\r') {
                // Could be NDATAfoo (no space) or NDATA at end
                if is_name_char_byte(after_ndata_raw[0]) {
                    return Err("Whitespace required between NDATA and notation name");
                }
            }

            // P76n05: NDATA must be followed by a notation name
            let after_ndata = skip_ws(after_ndata_raw);
            if after_ndata.is_empty() {
                return Err("NDATA requires a notation name");
            }
            // The next thing after NDATA (and whitespace) should be a valid name
            if !is_name_start(after_ndata[0]) {
                return Err("NDATA requires a notation name");
            }
        }
    }

    // Check for wrong ordering or missing NDATA keyword after system literal
    // P76n04: Name after system literal without NDATA keyword
    // P76n07: Name appears between system literal and NDATA
    if has_system && !is_parameter_entity {
        if let Some(sys_idx) = find_word(content, b"SYSTEM") {
            // Find the system literal after SYSTEM
            let after_system = &content[sys_idx + 6..];
            let after_system = skip_ws(after_system);
            if !after_system.is_empty() && (after_system[0] == b'"' || after_system[0] == b'\'') {
                let quote = after_system[0];
                if let Some(end_pos) = after_system[1..].iter().position(|&b| b == quote) {
                    // Skip past the system literal
                    let after_literal = &after_system[2 + end_pos..];
                    let after_literal = skip_ws(after_literal);
                    if !after_literal.is_empty() {
                        // Something follows the system literal
                        // It should be NDATA or nothing
                        if !after_literal.starts_with(b"NDATA") {
                            // Check if it starts with a name-like token
                            if is_name_start(after_literal[0]) {
                                // Read the token
                                let mut tok_end = 1;
                                while tok_end < after_literal.len() && is_name_char_byte(after_literal[tok_end]) {
                                    tok_end += 1;
                                }
                                let token = &after_literal[..tok_end];
                                // Check if NDATA follows this token
                                let after_token = skip_ws(&after_literal[tok_end..]);
                                if after_token.starts_with(b"NDATA") {
                                    // P76n07: Wrong order - name before NDATA
                                    return Err("NDATA declaration has wrong field ordering (notation name must follow NDATA keyword)");
                                } else {
                                    // P76n04: Name without NDATA keyword
                                    return Err("Unexpected token after system literal (did you mean to use NDATA?)");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(pub_idx) = find_word(content, b"PUBLIC") {
        // Skip past PUBLIC keyword
        let after_public = &content[pub_idx + 6..];
        let after_public = skip_ws(after_public);

        // Extract and validate the public ID literal (first quoted string)
        if !after_public.is_empty() && (after_public[0] == b'"' || after_public[0] == b'\'') {
            let quote = after_public[0];
            if let Some(end_pos) = after_public[1..].iter().position(|&b| b == quote) {
                let pubid = &after_public[1..1 + end_pos];
                validate_pubid_literal(pubid)?;

                // Check for whitespace between public ID and system literal
                let after_pubid = &after_public[2 + end_pos..];
                if !after_pubid.is_empty() {
                    let first_non_ws = after_pubid.iter().position(|&b| !matches!(b, b' ' | b'\t' | b'\n' | b'\r'));
                    if let Some(first_idx) = first_non_ws {
                        // If next char is a quote, check we had whitespace
                        if after_pubid[first_idx] == b'"' || after_pubid[first_idx] == b'\'' {
                            if first_idx == 0 {
                                return Err("Whitespace required between public ID and system literal");
                            }
                        }
                    }
                }
            }
        }
    }

    // Count quoted strings
    let mut quote_count = 0;
    let mut pos = 0;
    let len = content.len();

    while pos < len {
        if content[pos] == b'"' || content[pos] == b'\'' {
            let quote = content[pos];
            pos += 1;

            // Find closing quote and validate content
            while pos < len && content[pos] != quote {
                // Check for invalid XML characters (sa-175: U+FFFF is not allowed)
                // UTF-8 encoding: U+FFFE = EF BF BE, U+FFFF = EF BF BF
                if content[pos] == 0xEF && pos + 2 < len {
                    if content[pos + 1] == 0xBF {
                        if content[pos + 2] == 0xBE || content[pos + 2] == 0xBF {
                            return Err("Invalid XML character (U+FFFE or U+FFFF not allowed)");
                        }
                    }
                }

                // Check for PE reference in entity value (not allowed in internal subset)
                // WFC: PEs in Internal Subset
                // Also check for incomplete PE references (P09n01)
                if content[pos] == b'%' && !has_public && !has_system {
                    // Check if followed by name char (PE reference)
                    if pos + 1 < len && is_name_start(content[pos + 1]) {
                        // Check if there's a ; later (completes the PE reference)
                        let mut j = pos + 2;
                        while j < len && content[j] != quote && is_name_char_byte(content[j]) {
                            j += 1;
                        }
                        if j < len && content[j] == b';' {
                            return Err("Parameter entity reference not allowed inside markup declarations in internal subset");
                        } else {
                            // P09n01: Incomplete PE reference - % followed by name but no ; terminator
                            return Err("Incomplete parameter entity reference (missing ';' terminator)");
                        }
                    }
                }

                // Check for unescaped & in entity value (not in external ID literals)
                if content[pos] == b'&' && !has_public && !has_system {
                    // Must be followed by valid entity or char reference
                    pos += 1;
                    if pos >= len {
                        return Err("Unescaped '&' in entity value");
                    }
                    if content[pos] == b'#' {
                        // Character reference - validate digits
                        pos += 1;
                        if pos >= len || content[pos] == b';' || content[pos] == quote {
                            return Err("Empty character reference in entity value");
                        }
                        let is_hex = if content[pos] == b'x' {
                            pos += 1;
                            if pos >= len || content[pos] == b';' || content[pos] == quote {
                                return Err("Empty hex character reference in entity value");
                            }
                            true
                        } else if content[pos] == b'X' {
                            return Err("Hex character references must use lowercase 'x' (&#x..;)");
                        } else {
                            false
                        };
                        let ref_start = pos;
                        while pos < len && content[pos] != b';' && content[pos] != quote {
                            let c = content[pos];
                            if is_hex {
                                if !matches!(c, b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F') {
                                    return Err("Invalid hex digit in character reference");
                                }
                            } else {
                                if !matches!(c, b'0'..=b'9') {
                                    return Err("Invalid digit in decimal character reference");
                                }
                            }
                            pos += 1;
                        }
                        if pos >= len || content[pos] != b';' || pos == ref_start {
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
            quote_count += 1;
        } else {
            pos += 1;
        }
    }

    // PUBLIC external parsed entities (no NDATA) require both public ID AND system literal
    // PUBLIC external unparsed entities (with NDATA) also require both
    if has_public && quote_count < 2 {
        return Err("PUBLIC entity requires both public ID and system literal");
    }

    // SYSTEM requires exactly 1 quoted string
    if has_system && !has_public && quote_count < 1 {
        return Err("SYSTEM entity requires a system literal");
    }

    Ok(())
}

/// Validate NOTATION declaration content
/// Grammar: NotationDecl ::= '<!NOTATION' S Name S (ExternalID | PublicID) S? '>'
fn validate_notation_content(content: &[u8]) -> Result<(), &'static str> {
    let content = skip_ws(content);

    if content.is_empty() {
        return Err("NOTATION declaration requires external ID or public ID");
    }

    // NOTATION must have SYSTEM or PUBLIC
    let has_system = find_word(content, b"SYSTEM").is_some();
    let has_public = find_word(content, b"PUBLIC").is_some();

    if !has_system && !has_public {
        return Err("NOTATION declaration requires SYSTEM or PUBLIC");
    }

    // Check for wrong ordering: quoted string before SYSTEM/PUBLIC
    // P83n06: <!NOTATION JPGformat "-//JPG//DTD//JPGFormat" PUBLIC>
    let system_idx = find_word(content, b"SYSTEM");
    let public_idx = find_word(content, b"PUBLIC");
    let keyword_idx = match (system_idx, public_idx) {
        (Some(s), Some(p)) => Some(s.min(p)),
        (Some(s), None) => Some(s),
        (None, Some(p)) => Some(p),
        (None, None) => None,
    };

    if let Some(kw_idx) = keyword_idx {
        // Check if there's a quoted string before the keyword
        let mut pos = 0;
        while pos < kw_idx {
            if content[pos] == b'"' || content[pos] == b'\'' {
                return Err("NOTATION external ID has wrong field ordering (quoted string before SYSTEM/PUBLIC keyword)");
            }
            pos += 1;
        }
    }

    // Check case sensitivity of keywords
    if has_system {
        if let Some(idx) = find_word(content, b"SYSTEM") {
            let slice = &content[idx..idx + 6];
            if slice != b"SYSTEM" {
                return Err("'SYSTEM' must be uppercase");
            }
            // Check for whitespace after SYSTEM
            let after_idx = idx + 6;
            if after_idx < content.len() {
                let next = content[after_idx];
                if next == b'"' || next == b'\'' {
                    return Err("Whitespace required between SYSTEM and URI");
                }
            }
        }
    }

    if has_public {
        if let Some(idx) = find_word(content, b"PUBLIC") {
            let slice = &content[idx..idx + 6];
            if slice != b"PUBLIC" {
                return Err("'PUBLIC' must be uppercase");
            }
            // Check for whitespace after PUBLIC
            let after_idx = idx + 6;
            if after_idx < content.len() {
                let next = content[after_idx];
                if next == b'"' || next == b'\'' {
                    return Err("Whitespace required between PUBLIC and public ID");
                }
            }

            // Validate the public ID literal
            let after_public = skip_ws(&content[idx + 6..]);
            if !after_public.is_empty() && (after_public[0] == b'"' || after_public[0] == b'\'') {
                let quote = after_public[0];
                if let Some(end_pos) = after_public[1..].iter().position(|&b| b == quote) {
                    let pubid = &after_public[1..1 + end_pos];
                    validate_pubid_literal(pubid)?;
                }
            }
        }
    }

    // Count quoted strings to check for proper structure
    let mut quote_count = 0;
    let mut pos = 0;
    while pos < content.len() {
        if content[pos] == b'"' || content[pos] == b'\'' {
            let quote = content[pos];
            pos += 1;
            while pos < content.len() && content[pos] != quote {
                pos += 1;
            }
            if pos < content.len() {
                quote_count += 1;
                pos += 1;
            }
        } else {
            pos += 1;
        }
    }

    // SYSTEM requires exactly 1 quoted string (system literal)
    // PUBLIC requires 1 (public ID only for NOTATION) or 2 (public + system) quoted strings
    // Note: NOTATION with PUBLIC can have just the public ID (PublicID production)
    if has_system && !has_public && quote_count < 1 {
        return Err("NOTATION SYSTEM requires a system literal");
    }
    if has_public && quote_count < 1 {
        return Err("NOTATION PUBLIC requires at least a public ID literal");
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
