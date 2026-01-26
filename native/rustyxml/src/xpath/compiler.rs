//! XPath Expression Compiler
//!
//! Compiles parsed XPath expressions into an optimized intermediate representation.
//! Includes an LRU cache for compiled expressions to avoid re-parsing repeated queries.

use super::parser::{Expr, Step, Axis, NodeTest, BinaryOp};
use std::sync::Mutex;
use lru::LruCache;
use std::num::NonZeroUsize;

/// Global LRU cache for compiled XPath expressions
/// Using a Mutex for thread-safety across BEAM schedulers
static XPATH_CACHE: Mutex<Option<LruCache<String, CompiledExpr>>> = Mutex::new(None);

/// Cache capacity - tuned for typical XPath usage patterns
const CACHE_CAPACITY: usize = 256;

/// Compiled XPath expression
#[derive(Debug, Clone)]
pub struct CompiledExpr {
    pub ops: Vec<Op>,
}

/// Compiled operation
#[derive(Debug, Clone)]
pub enum Op {
    /// Push root node onto stack
    Root,
    /// Push context node onto stack
    Context,
    /// Navigate to parent
    Parent,
    /// Navigate along axis with node test
    Navigate(Axis, CompiledNodeTest),
    /// Apply predicate filter (general case)
    Predicate(Box<CompiledExpr>),
    /// Fast path: predicate [@attr = 'value']
    PredicateAttrEq(String, String),
    /// Fast path: predicate [position]
    PredicatePosition(usize),
    /// Union two node sets
    Union,
    /// Push literal number
    Number(f64),
    /// Push literal string
    String(String),
    /// Call function
    Call(String, usize), // name, arg count
    /// Binary operation
    Binary(BinaryOp),
    /// Negate
    Negate,
    /// Variable reference
    Variable(String),
}

/// Compiled node test
#[derive(Debug, Clone)]
pub enum CompiledNodeTest {
    Any,
    Name(String),
    QName(String, String),
    NamespaceWildcard(String),
    Node,
    Text,
    Comment,
    ProcessingInstruction(Option<String>),
}

impl CompiledExpr {
    /// Compile an XPath expression
    pub fn compile(expr: &Expr) -> Self {
        let mut ops = Vec::new();
        Self::compile_expr(expr, &mut ops);
        CompiledExpr { ops }
    }

    fn compile_expr(expr: &Expr, ops: &mut Vec<Op>) {
        match expr {
            Expr::Root => {
                ops.push(Op::Root);
            }
            Expr::Context => {
                ops.push(Op::Context);
            }
            Expr::Parent => {
                ops.push(Op::Parent);
            }
            Expr::Number(n) => {
                ops.push(Op::Number(*n));
            }
            Expr::String(s) => {
                ops.push(Op::String(s.clone()));
            }
            Expr::Variable(name) => {
                ops.push(Op::Variable(name.clone()));
            }
            Expr::Negate(inner) => {
                Self::compile_expr(inner, ops);
                ops.push(Op::Negate);
            }
            Expr::Binary(left, op, right) => {
                Self::compile_expr(left, ops);
                Self::compile_expr(right, ops);
                ops.push(Op::Binary(*op));
            }
            Expr::Union(left, right) => {
                Self::compile_expr(left, ops);
                Self::compile_expr(right, ops);
                ops.push(Op::Union);
            }
            Expr::Path(base, step) => {
                Self::compile_expr(base, ops);
                Self::compile_step(step, ops);
            }
            Expr::Filter(base, pred) => {
                Self::compile_expr(base, ops);
                let pred_compiled = CompiledExpr::compile(pred);
                ops.push(Op::Predicate(Box::new(pred_compiled)));
            }
            Expr::Step(step) => {
                ops.push(Op::Context);
                Self::compile_step(step, ops);
            }
            Expr::Function(name, args) => {
                for arg in args {
                    Self::compile_expr(arg, ops);
                }
                ops.push(Op::Call(name.clone(), args.len()));
            }
        }
    }

    fn compile_step(step: &Step, ops: &mut Vec<Op>) {
        let node_test = match &step.node_test {
            NodeTest::Any => CompiledNodeTest::Any,
            NodeTest::Name(n) => CompiledNodeTest::Name(n.clone()),
            NodeTest::QName(ns, local) => CompiledNodeTest::QName(ns.clone(), local.clone()),
            NodeTest::NamespaceWildcard(ns) => CompiledNodeTest::NamespaceWildcard(ns.clone()),
            NodeTest::Node => CompiledNodeTest::Node,
            NodeTest::Text => CompiledNodeTest::Text,
            NodeTest::Comment => CompiledNodeTest::Comment,
            NodeTest::ProcessingInstruction(arg) => {
                CompiledNodeTest::ProcessingInstruction(arg.clone())
            }
        };

        ops.push(Op::Navigate(step.axis, node_test));

        for pred in &step.predicates {
            // Try to use fast-path predicates for common patterns
            if let Some(op) = Self::try_optimize_predicate(pred) {
                ops.push(op);
            } else {
                let pred_compiled = CompiledExpr::compile(pred);
                ops.push(Op::Predicate(Box::new(pred_compiled)));
            }
        }
    }

    /// Try to optimize a predicate into a fast-path operation
    fn try_optimize_predicate(pred: &Expr) -> Option<Op> {
        match pred {
            // Pattern: [n] where n is a number - position predicate
            Expr::Number(n) if *n > 0.0 && n.fract() == 0.0 => {
                Some(Op::PredicatePosition(*n as usize))
            }

            // Pattern: [@attr = 'value'] - attribute equality
            Expr::Binary(left, BinaryOp::Eq, right) => {
                // Check if left is @attr and right is string literal
                if let (Some(attr_name), Some(value)) = (
                    Self::extract_attribute_name(left),
                    Self::extract_string_literal(right),
                ) {
                    return Some(Op::PredicateAttrEq(attr_name, value));
                }
                // Check reverse: 'value' = @attr
                if let (Some(value), Some(attr_name)) = (
                    Self::extract_string_literal(left),
                    Self::extract_attribute_name(right),
                ) {
                    return Some(Op::PredicateAttrEq(attr_name, value));
                }
                None
            }

            _ => None,
        }
    }

    /// Extract attribute name from @attr pattern
    fn extract_attribute_name(expr: &Expr) -> Option<String> {
        if let Expr::Step(step) = expr {
            if step.axis == Axis::Attribute && step.predicates.is_empty() {
                if let NodeTest::Name(name) = &step.node_test {
                    return Some(name.clone());
                }
            }
        }
        None
    }

    /// Extract string literal from expression
    fn extract_string_literal(expr: &Expr) -> Option<String> {
        if let Expr::String(s) = expr {
            Some(s.clone())
        } else {
            None
        }
    }
}

/// Compile an XPath expression string (with caching)
pub fn compile(xpath: &str) -> Result<CompiledExpr, String> {
    // Try to get from cache first
    {
        let mut guard = XPATH_CACHE.lock().unwrap();
        let cache = guard.get_or_insert_with(|| {
            LruCache::new(NonZeroUsize::new(CACHE_CAPACITY).unwrap())
        });

        if let Some(compiled) = cache.get(xpath) {
            return Ok(compiled.clone());
        }
    }

    // Not in cache - parse and compile
    let expr = super::parser::parse(xpath)?;
    let compiled = CompiledExpr::compile(&expr);

    // Store in cache
    {
        let mut guard = XPATH_CACHE.lock().unwrap();
        let cache = guard.get_or_insert_with(|| {
            LruCache::new(NonZeroUsize::new(CACHE_CAPACITY).unwrap())
        });
        cache.put(xpath.to_string(), compiled.clone());
    }

    Ok(compiled)
}

/// Compile an XPath expression string without caching (for testing)
#[allow(dead_code)]
pub fn compile_uncached(xpath: &str) -> Result<CompiledExpr, String> {
    let expr = super::parser::parse(xpath)?;
    Ok(CompiledExpr::compile(&expr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_simple() {
        let compiled = compile("/root").unwrap();
        assert!(!compiled.ops.is_empty());
        assert!(matches!(compiled.ops[0], Op::Root));
    }

    #[test]
    fn test_compile_descendant() {
        let compiled = compile("//item").unwrap();
        assert!(!compiled.ops.is_empty());
    }
}
