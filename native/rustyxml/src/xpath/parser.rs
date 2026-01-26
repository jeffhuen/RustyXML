//! XPath Parser
//!
//! Recursive descent parser for XPath 1.0 expressions.

use super::lexer::{Lexer, Token};

/// XPath expression AST node
#[derive(Debug, Clone)]
pub enum Expr {
    /// Root path (/)
    Root,
    /// Current context (.)
    Context,
    /// Parent (..)
    Parent,
    /// Union of two expressions (|)
    Union(Box<Expr>, Box<Expr>),
    /// Path expression (expr/expr or expr//expr)
    Path(Box<Expr>, Box<Step>),
    /// Filter expression with predicate
    Filter(Box<Expr>, Box<Expr>),
    /// Function call
    Function(String, Vec<Expr>),
    /// Binary operation
    Binary(Box<Expr>, BinaryOp, Box<Expr>),
    /// Unary negation
    Negate(Box<Expr>),
    /// Literal number
    Number(f64),
    /// Literal string
    String(String),
    /// Variable reference
    Variable(String),
    /// Location step
    Step(Box<Step>),
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Or,
    And,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

/// Location step in a path
#[derive(Debug, Clone)]
pub struct Step {
    pub axis: Axis,
    pub node_test: NodeTest,
    pub predicates: Vec<Expr>,
}

/// XPath axes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Child,
    Descendant,
    DescendantOrSelf,
    Parent,
    Ancestor,
    AncestorOrSelf,
    FollowingSibling,
    PrecedingSibling,
    Following,
    Preceding,
    Self_,
    Attribute,
    Namespace,
}

impl Axis {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "child" => Some(Axis::Child),
            "descendant" => Some(Axis::Descendant),
            "descendant-or-self" => Some(Axis::DescendantOrSelf),
            "parent" => Some(Axis::Parent),
            "ancestor" => Some(Axis::Ancestor),
            "ancestor-or-self" => Some(Axis::AncestorOrSelf),
            "following-sibling" => Some(Axis::FollowingSibling),
            "preceding-sibling" => Some(Axis::PrecedingSibling),
            "following" => Some(Axis::Following),
            "preceding" => Some(Axis::Preceding),
            "self" => Some(Axis::Self_),
            "attribute" => Some(Axis::Attribute),
            "namespace" => Some(Axis::Namespace),
            _ => None,
        }
    }
}

/// Node test in a location step
#[derive(Debug, Clone)]
pub enum NodeTest {
    /// Matches any node (*)
    Any,
    /// Matches elements with name
    Name(String),
    /// Matches namespace:localname
    QName(String, String),
    /// Matches namespace:*
    NamespaceWildcard(String),
    /// node() - matches any node
    Node,
    /// text() - matches text nodes
    Text,
    /// comment() - matches comments
    Comment,
    /// processing-instruction() - matches PIs
    ProcessingInstruction(Option<String>),
}

/// XPath parser
pub struct Parser<'a> {
    lexer: Lexer<'a>,
    current: Token,
    peeked: Option<Token>,
}

impl<'a> Parser<'a> {
    /// Create a new parser
    pub fn new(input: &'a str) -> Self {
        let mut lexer = Lexer::new(input);
        let current = lexer.next_token();
        Parser {
            lexer,
            current,
            peeked: None,
        }
    }

    /// Parse an XPath expression
    pub fn parse(&mut self) -> Result<Expr, String> {
        self.parse_expr()
    }

    /// Advance to next token
    fn advance(&mut self) {
        self.current = if let Some(t) = self.peeked.take() {
            t
        } else {
            self.lexer.next_token()
        };
    }

    /// Peek at next token
    fn peek(&mut self) -> &Token {
        if self.peeked.is_none() {
            self.peeked = Some(self.lexer.next_token());
        }
        self.peeked.as_ref().unwrap()
    }

    /// Parse expression (handles union)
    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_or_expr()
    }

    /// Parse or expression
    fn parse_or_expr(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_and_expr()?;

        while matches!(self.current, Token::Or) {
            self.advance();
            let right = self.parse_and_expr()?;
            left = Expr::Binary(Box::new(left), BinaryOp::Or, Box::new(right));
        }

        Ok(left)
    }

    /// Parse and expression
    fn parse_and_expr(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_equality_expr()?;

        while matches!(self.current, Token::And) {
            self.advance();
            let right = self.parse_equality_expr()?;
            left = Expr::Binary(Box::new(left), BinaryOp::And, Box::new(right));
        }

        Ok(left)
    }

    /// Parse equality expression
    fn parse_equality_expr(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_relational_expr()?;

        loop {
            let op = match &self.current {
                Token::Eq => BinaryOp::Eq,
                Token::NotEq => BinaryOp::NotEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_relational_expr()?;
            left = Expr::Binary(Box::new(left), op, Box::new(right));
        }

        Ok(left)
    }

    /// Parse relational expression
    fn parse_relational_expr(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_additive_expr()?;

        loop {
            let op = match &self.current {
                Token::Lt => BinaryOp::Lt,
                Token::LtEq => BinaryOp::LtEq,
                Token::Gt => BinaryOp::Gt,
                Token::GtEq => BinaryOp::GtEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_additive_expr()?;
            left = Expr::Binary(Box::new(left), op, Box::new(right));
        }

        Ok(left)
    }

    /// Parse additive expression
    fn parse_additive_expr(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_multiplicative_expr()?;

        loop {
            let op = match &self.current {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative_expr()?;
            left = Expr::Binary(Box::new(left), op, Box::new(right));
        }

        Ok(left)
    }

    /// Parse multiplicative expression
    fn parse_multiplicative_expr(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_unary_expr()?;

        loop {
            let op = match &self.current {
                Token::Star => BinaryOp::Mul,
                Token::Div => BinaryOp::Div,
                Token::Mod => BinaryOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary_expr()?;
            left = Expr::Binary(Box::new(left), op, Box::new(right));
        }

        Ok(left)
    }

    /// Parse unary expression
    fn parse_unary_expr(&mut self) -> Result<Expr, String> {
        if matches!(self.current, Token::Minus) {
            self.advance();
            let expr = self.parse_unary_expr()?;
            Ok(Expr::Negate(Box::new(expr)))
        } else {
            self.parse_union_expr()
        }
    }

    /// Parse union expression
    fn parse_union_expr(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_path_expr()?;

        while matches!(self.current, Token::Pipe) {
            self.advance();
            let right = self.parse_path_expr()?;
            left = Expr::Union(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    /// Parse path expression
    fn parse_path_expr(&mut self) -> Result<Expr, String> {
        let mut expr = match &self.current {
            Token::Slash => {
                self.advance();
                if matches!(self.current, Token::Eof | Token::RightBracket | Token::RightParen | Token::Pipe | Token::Comma) {
                    // Just /
                    return Ok(Expr::Root);
                } else {
                    // /path
                    let step = self.parse_step()?;
                    Expr::Path(Box::new(Expr::Root), Box::new(step))
                }
            }
            Token::DoubleSlash => {
                self.advance();
                // //path is shorthand for /descendant-or-self::node()/path
                let desc_step = Step {
                    axis: Axis::DescendantOrSelf,
                    node_test: NodeTest::Node,
                    predicates: Vec::new(),
                };
                let step = self.parse_step()?;
                Expr::Path(
                    Box::new(Expr::Path(Box::new(Expr::Root), Box::new(desc_step))),
                    Box::new(step),
                )
            }
            _ => return self.parse_filter_expr(),
        };

        // Handle path continuation (e.g., /root/child/grandchild)
        loop {
            match &self.current {
                Token::Slash => {
                    self.advance();
                    let step = self.parse_step()?;
                    expr = Expr::Path(Box::new(expr), Box::new(step));
                }
                Token::DoubleSlash => {
                    self.advance();
                    let desc_step = Step {
                        axis: Axis::DescendantOrSelf,
                        node_test: NodeTest::Node,
                        predicates: Vec::new(),
                    };
                    let step = self.parse_step()?;
                    expr = Expr::Path(
                        Box::new(Expr::Path(Box::new(expr), Box::new(desc_step))),
                        Box::new(step),
                    );
                }
                Token::LeftBracket => {
                    self.advance();
                    let pred = self.parse_expr()?;
                    if !matches!(self.current, Token::RightBracket) {
                        return Err("Expected ]".to_string());
                    }
                    self.advance();
                    expr = Expr::Filter(Box::new(expr), Box::new(pred));
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    /// Parse relative path
    fn parse_relative_path(&mut self) -> Result<Step, String> {
        self.parse_step()
    }

    /// Parse filter expression
    fn parse_filter_expr(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_primary_expr()?;

        // Handle predicates and path continuation
        loop {
            match &self.current {
                Token::LeftBracket => {
                    self.advance();
                    let pred = self.parse_expr()?;
                    if !matches!(self.current, Token::RightBracket) {
                        return Err("Expected ]".to_string());
                    }
                    self.advance();
                    expr = Expr::Filter(Box::new(expr), Box::new(pred));
                }
                Token::Slash => {
                    self.advance();
                    let step = self.parse_step()?;
                    expr = Expr::Path(Box::new(expr), Box::new(step));
                }
                Token::DoubleSlash => {
                    self.advance();
                    let desc_step = Step {
                        axis: Axis::DescendantOrSelf,
                        node_test: NodeTest::Node,
                        predicates: Vec::new(),
                    };
                    let step = self.parse_step()?;
                    expr = Expr::Path(
                        Box::new(Expr::Path(Box::new(expr), Box::new(desc_step))),
                        Box::new(step),
                    );
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    /// Parse primary expression
    fn parse_primary_expr(&mut self) -> Result<Expr, String> {
        match &self.current {
            Token::Number(n) => {
                let n = *n;
                self.advance();
                Ok(Expr::Number(n))
            }
            Token::String(s) => {
                let s = s.clone();
                self.advance();
                Ok(Expr::String(s))
            }
            Token::Dollar => {
                self.advance();
                if let Token::Name(name) = &self.current {
                    let name = name.clone();
                    self.advance();
                    Ok(Expr::Variable(name))
                } else {
                    Err("Expected variable name".to_string())
                }
            }
            Token::LeftParen => {
                self.advance();
                let expr = self.parse_expr()?;
                if !matches!(self.current, Token::RightParen) {
                    return Err("Expected )".to_string());
                }
                self.advance();
                Ok(expr)
            }
            Token::Name(name) => {
                let name = name.clone();
                if matches!(self.peek(), Token::LeftParen) {
                    // Function call
                    self.advance();
                    self.advance(); // Skip (
                    let args = self.parse_function_args()?;
                    Ok(Expr::Function(name, args))
                } else {
                    // Step
                    let step = self.parse_step()?;
                    Ok(Expr::Step(Box::new(step)))
                }
            }
            Token::NodeType(name) => {
                let name = name.clone();
                self.advance();
                self.advance(); // Skip (
                let arg = if matches!(self.current, Token::String(..)) {
                    if let Token::String(s) = &self.current {
                        let s = s.clone();
                        self.advance();
                        Some(s)
                    } else {
                        None
                    }
                } else {
                    None
                };
                if !matches!(self.current, Token::RightParen) {
                    return Err("Expected )".to_string());
                }
                self.advance();

                let node_test = match name.as_str() {
                    "node" => NodeTest::Node,
                    "text" => NodeTest::Text,
                    "comment" => NodeTest::Comment,
                    "processing-instruction" => NodeTest::ProcessingInstruction(arg),
                    _ => return Err(format!("Unknown node type: {}", name)),
                };

                Ok(Expr::Step(Box::new(Step {
                    axis: Axis::Child,
                    node_test,
                    predicates: Vec::new(),
                })))
            }
            Token::Star => {
                self.advance();
                Ok(Expr::Step(Box::new(Step {
                    axis: Axis::Child,
                    node_test: NodeTest::Any,
                    predicates: Vec::new(),
                })))
            }
            Token::At => {
                self.advance();
                let step = self.parse_step_with_axis(Axis::Attribute)?;
                Ok(Expr::Step(Box::new(step)))
            }
            Token::Dot => {
                self.advance();
                Ok(Expr::Context)
            }
            Token::DoubleDot => {
                self.advance();
                Ok(Expr::Parent)
            }
            Token::Axis(axis) => {
                let axis = Axis::from_str(axis).ok_or_else(|| format!("Unknown axis: {}", axis))?;
                self.advance();
                if !matches!(self.current, Token::DoubleColon) {
                    return Err("Expected ::".to_string());
                }
                self.advance();
                let step = self.parse_step_with_axis(axis)?;
                Ok(Expr::Step(Box::new(step)))
            }
            _ => Err(format!("Unexpected token: {:?}", self.current)),
        }
    }

    /// Parse a location step
    fn parse_step(&mut self) -> Result<Step, String> {
        self.parse_step_with_axis(Axis::Child)
    }

    /// Parse step with given axis
    fn parse_step_with_axis(&mut self, mut axis: Axis) -> Result<Step, String> {
        // Check for @ abbreviation (attribute axis)
        if matches!(self.current, Token::At) {
            axis = Axis::Attribute;
            self.advance();
        }

        // Check if there's an explicit axis specification
        if let Token::Axis(axis_name) = &self.current {
            axis = Axis::from_str(axis_name).ok_or_else(|| format!("Unknown axis: {}", axis_name))?;
            self.advance();
            if !matches!(self.current, Token::DoubleColon) {
                return Err("Expected :: after axis".to_string());
            }
            self.advance();
        }

        let node_test = match &self.current {
            Token::Star => {
                self.advance();
                NodeTest::Any
            }
            Token::Name(name) => {
                let name = name.clone();
                self.advance();
                NodeTest::Name(name)
            }
            Token::NameTest(qname) => {
                let qname = qname.clone();
                self.advance();
                if qname.ends_with(":*") {
                    let ns = &qname[..qname.len() - 2];
                    NodeTest::NamespaceWildcard(ns.to_string())
                } else if let Some(pos) = qname.find(':') {
                    NodeTest::QName(qname[..pos].to_string(), qname[pos + 1..].to_string())
                } else {
                    NodeTest::Name(qname)
                }
            }
            Token::NodeType(name) => {
                let name = name.clone();
                self.advance();
                self.advance(); // Skip (
                let arg = if let Token::String(s) = &self.current {
                    let s = s.clone();
                    self.advance();
                    Some(s)
                } else {
                    None
                };
                if !matches!(self.current, Token::RightParen) {
                    return Err("Expected )".to_string());
                }
                self.advance();

                match name.as_str() {
                    "node" => NodeTest::Node,
                    "text" => NodeTest::Text,
                    "comment" => NodeTest::Comment,
                    "processing-instruction" => NodeTest::ProcessingInstruction(arg),
                    _ => return Err(format!("Unknown node type: {}", name)),
                }
            }
            _ => return Err(format!("Expected node test, got {:?}", self.current)),
        };

        // Parse predicates
        let mut predicates = Vec::new();
        while matches!(self.current, Token::LeftBracket) {
            self.advance();
            predicates.push(self.parse_expr()?);
            if !matches!(self.current, Token::RightBracket) {
                return Err("Expected ]".to_string());
            }
            self.advance();
        }

        Ok(Step {
            axis,
            node_test,
            predicates,
        })
    }

    /// Parse function arguments
    fn parse_function_args(&mut self) -> Result<Vec<Expr>, String> {
        let mut args = Vec::new();

        if !matches!(self.current, Token::RightParen) {
            args.push(self.parse_expr()?);

            while matches!(self.current, Token::Comma) {
                self.advance();
                args.push(self.parse_expr()?);
            }
        }

        if !matches!(self.current, Token::RightParen) {
            return Err("Expected )".to_string());
        }
        self.advance();

        Ok(args)
    }
}

/// Parse an XPath expression string
pub fn parse(input: &str) -> Result<Expr, String> {
    Parser::new(input).parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_path() {
        let expr = parse("/root/child").unwrap();
        assert!(matches!(expr, Expr::Path(..)));
    }

    #[test]
    fn test_predicate() {
        let expr = parse("item[@id='test']").unwrap();
        assert!(matches!(expr, Expr::Step(_) | Expr::Filter(..)));
    }

    #[test]
    fn test_descendant() {
        let expr = parse("//item").unwrap();
        assert!(matches!(expr, Expr::Path(..)));
    }

    #[test]
    fn test_function() {
        let expr = parse("count(//item)").unwrap();
        assert!(matches!(expr, Expr::Function(name, _) if name == "count"));
    }
}
