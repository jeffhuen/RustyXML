//! XPath Expression Compiler
//!
//! Compiles parsed XPath expressions into an optimized intermediate representation.

use super::parser::{Expr, Step, Axis, NodeTest, BinaryOp};

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
    /// Apply predicate filter
    Predicate(Box<CompiledExpr>),
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
            let pred_compiled = CompiledExpr::compile(pred);
            ops.push(Op::Predicate(Box::new(pred_compiled)));
        }
    }
}

/// Compile an XPath expression string
pub fn compile(xpath: &str) -> Result<CompiledExpr, String> {
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
