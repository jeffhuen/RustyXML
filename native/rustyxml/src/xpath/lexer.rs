//! XPath Lexer
//!
//! Tokenizes XPath expressions into tokens.

/// XPath token types
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Operators
    Slash,       // /
    DoubleSlash, // //
    Dot,         // .
    DoubleDot,   // ..
    At,          // @
    Pipe,        // |
    Plus,        // +
    Minus,       // -
    Star,        // *
    Eq,          // =
    NotEq,       // !=
    Lt,          // <
    LtEq,        // <=
    Gt,          // >
    GtEq,        // >=
    And,         // and
    Or,          // or
    Mod,         // mod
    Div,         // div

    // Brackets
    LeftParen,    // (
    RightParen,   // )
    LeftBracket,  // [
    RightBracket, // ]

    // Literals
    Number(f64),
    String(String),

    // Names
    Name(String),     // NCName
    NameTest(String), // namespace:* or NCName:NCName
    NodeType(String), // node(), text(), comment(), processing-instruction()

    // Axis
    Axis(String), // child::, descendant::, etc.

    // Special
    DoubleColon, // ::
    Comma,       // ,
    Dollar,      // $

    // End of input
    Eof,
}

/// XPath lexer
pub struct Lexer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer
    pub fn new(input: &'a str) -> Self {
        Lexer { input, pos: 0 }
    }

    /// Get the remaining input
    fn remaining(&self) -> &'a str {
        &self.input[self.pos..]
    }

    /// Peek at current character
    fn peek(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    /// Peek at character at offset
    fn peek_at(&self, offset: usize) -> Option<char> {
        self.remaining().chars().nth(offset)
    }

    /// Advance by n bytes
    fn advance(&mut self, n: usize) {
        self.pos = (self.pos + n).min(self.input.len());
    }

    /// Skip whitespace
    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.advance(c.len_utf8());
            } else {
                break;
            }
        }
    }

    /// Get the next token
    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace();

        let c = match self.peek() {
            Some(c) => c,
            None => return Token::Eof,
        };

        match c {
            '/' => {
                self.advance(1);
                if self.peek() == Some('/') {
                    self.advance(1);
                    Token::DoubleSlash
                } else {
                    Token::Slash
                }
            }
            '.' => {
                self.advance(1);
                if self.peek() == Some('.') {
                    self.advance(1);
                    Token::DoubleDot
                } else if self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                    // Number starting with .
                    self.pos -= 1; // Backtrack
                    self.read_number()
                } else {
                    Token::Dot
                }
            }
            '@' => {
                self.advance(1);
                Token::At
            }
            '|' => {
                self.advance(1);
                Token::Pipe
            }
            '+' => {
                self.advance(1);
                Token::Plus
            }
            '-' => {
                self.advance(1);
                Token::Minus
            }
            '*' => {
                self.advance(1);
                Token::Star
            }
            '=' => {
                self.advance(1);
                Token::Eq
            }
            '!' => {
                self.advance(1);
                if self.peek() == Some('=') {
                    self.advance(1);
                    Token::NotEq
                } else {
                    // Invalid, but return something
                    Token::Name("!".to_string())
                }
            }
            '<' => {
                self.advance(1);
                if self.peek() == Some('=') {
                    self.advance(1);
                    Token::LtEq
                } else {
                    Token::Lt
                }
            }
            '>' => {
                self.advance(1);
                if self.peek() == Some('=') {
                    self.advance(1);
                    Token::GtEq
                } else {
                    Token::Gt
                }
            }
            '(' => {
                self.advance(1);
                Token::LeftParen
            }
            ')' => {
                self.advance(1);
                Token::RightParen
            }
            '[' => {
                self.advance(1);
                Token::LeftBracket
            }
            ']' => {
                self.advance(1);
                Token::RightBracket
            }
            ',' => {
                self.advance(1);
                Token::Comma
            }
            '$' => {
                self.advance(1);
                Token::Dollar
            }
            ':' => {
                self.advance(1);
                if self.peek() == Some(':') {
                    self.advance(1);
                    Token::DoubleColon
                } else {
                    Token::Name(":".to_string())
                }
            }
            '"' | '\'' => self.read_string(),
            '0'..='9' => self.read_number(),
            _ if is_name_start_char(c) => self.read_name_or_keyword(),
            _ => {
                self.advance(c.len_utf8());
                Token::Name(c.to_string())
            }
        }
    }

    /// Read a number literal
    fn read_number(&mut self) -> Token {
        let start = self.pos;

        // Integer part
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.advance(1);
            } else {
                break;
            }
        }

        // Decimal part
        if self.peek() == Some('.') && self.peek_at(1).map(|c| c.is_ascii_digit()).unwrap_or(false)
        {
            self.advance(1); // Skip '.'
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    self.advance(1);
                } else {
                    break;
                }
            }
        }

        let num_str = &self.input[start..self.pos];
        let value = num_str.parse().unwrap_or(f64::NAN);
        Token::Number(value)
    }

    /// Read a string literal
    fn read_string(&mut self) -> Token {
        // Caller guarantees peek() matched a quote char.
        // Use unwrap_or with '"' as defensive fallback to avoid panicking in NIF paths.
        let quote = self.peek().unwrap_or('"');
        self.advance(1); // Skip opening quote

        let start = self.pos;

        while let Some(c) = self.peek() {
            if c == quote {
                break;
            }
            self.advance(c.len_utf8());
        }

        let value = self.input[start..self.pos].to_string();
        self.advance(1); // Skip closing quote

        Token::String(value)
    }

    /// Read a name or keyword
    fn read_name_or_keyword(&mut self) -> Token {
        let start = self.pos;

        while let Some(c) = self.peek() {
            if is_name_char(c) {
                self.advance(c.len_utf8());
            } else {
                break;
            }
        }

        let name = &self.input[start..self.pos];

        // Check for keywords
        match name {
            "and" => Token::And,
            "or" => Token::Or,
            "mod" => Token::Mod,
            "div" => Token::Div,
            _ => {
                // Check if followed by :: (axis)
                self.skip_whitespace();
                if self.remaining().starts_with("::") {
                    Token::Axis(name.to_string())
                }
                // Check if followed by ( (function or node type)
                else if self.peek() == Some('(') {
                    match name {
                        "node" | "text" | "comment" | "processing-instruction" => {
                            Token::NodeType(name.to_string())
                        }
                        _ => Token::Name(name.to_string()),
                    }
                }
                // Check for namespace prefix
                else if self.peek() == Some(':') && self.peek_at(1) != Some(':') {
                    self.advance(1); // Skip ':'
                    if self.peek() == Some('*') {
                        self.advance(1);
                        Token::NameTest(format!("{}:*", name))
                    } else {
                        // Read local name
                        let local_start = self.pos;
                        while let Some(c) = self.peek() {
                            if is_name_char(c) {
                                self.advance(c.len_utf8());
                            } else {
                                break;
                            }
                        }
                        let local = &self.input[local_start..self.pos];
                        Token::NameTest(format!("{}:{}", name, local))
                    }
                } else {
                    Token::Name(name.to_string())
                }
            }
        }
    }

    /// Tokenize entire input
    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token();
            if matches!(token, Token::Eof) {
                break;
            }
            tokens.push(token);
        }
        tokens
    }
}

fn is_name_start_char(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == '.'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_path() {
        let mut lexer = Lexer::new("/root/child");
        assert_eq!(lexer.next_token(), Token::Slash);
        assert_eq!(lexer.next_token(), Token::Name("root".to_string()));
        assert_eq!(lexer.next_token(), Token::Slash);
        assert_eq!(lexer.next_token(), Token::Name("child".to_string()));
        assert_eq!(lexer.next_token(), Token::Eof);
    }

    #[test]
    fn test_descendant() {
        let mut lexer = Lexer::new("//item");
        assert_eq!(lexer.next_token(), Token::DoubleSlash);
        assert_eq!(lexer.next_token(), Token::Name("item".to_string()));
    }

    #[test]
    fn test_predicate() {
        let mut lexer = Lexer::new("item[@id='test']");
        assert_eq!(lexer.next_token(), Token::Name("item".to_string()));
        assert_eq!(lexer.next_token(), Token::LeftBracket);
        assert_eq!(lexer.next_token(), Token::At);
        assert_eq!(lexer.next_token(), Token::Name("id".to_string()));
        assert_eq!(lexer.next_token(), Token::Eq);
        assert_eq!(lexer.next_token(), Token::String("test".to_string()));
        assert_eq!(lexer.next_token(), Token::RightBracket);
    }

    #[test]
    fn test_axis() {
        let mut lexer = Lexer::new("child::element");
        assert_eq!(lexer.next_token(), Token::Axis("child".to_string()));
        assert_eq!(lexer.next_token(), Token::DoubleColon);
        assert_eq!(lexer.next_token(), Token::Name("element".to_string()));
    }

    #[test]
    fn test_number() {
        let mut lexer = Lexer::new("position() = 1");
        let tokens = lexer.tokenize();
        assert!(matches!(tokens.last(), Some(Token::Number(n)) if *n == 1.0));
    }
}
