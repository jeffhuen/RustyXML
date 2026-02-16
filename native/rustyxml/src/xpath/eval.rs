//! XPath Evaluation Engine
//!
//! Evaluates compiled XPath expressions against an XML document.

use super::axes::{matches_node_test, navigate};
use super::compiler::{CompiledExpr, CompiledNodeTest, Op};
use super::functions;
use super::parser::BinaryOp;
use super::value::XPathValue;
#[cfg(test)]
use crate::dom::XmlDocument;
use crate::dom::{self, DocumentAccess, NodeId};
use std::collections::HashSet;

/// Evaluation context - generic over document type
pub struct EvalContext<'a, D: DocumentAccess> {
    pub doc: &'a D,
    pub context_node: NodeId,
    pub context_position: usize,
    pub context_size: usize,
}

/// Evaluate an XPath expression against any document type
#[must_use = "XPath evaluation result should be used"]
pub fn evaluate<D: DocumentAccess>(doc: &D, xpath: &str) -> Result<XPathValue, String> {
    let compiled = super::compiler::compile(xpath)?;
    let context = EvalContext {
        doc,
        context_node: doc.root_element_id().unwrap_or(0),
        context_position: 1,
        context_size: 1,
    };
    evaluate_compiled(&compiled, &context)
}

/// Evaluate an XPath expression from a specific context node
#[must_use = "XPath evaluation result should be used"]
pub fn evaluate_from_node<D: DocumentAccess>(
    doc: &D,
    context_node: NodeId,
    xpath: &str,
) -> Result<XPathValue, String> {
    let compiled = super::compiler::compile(xpath)?;
    let context = EvalContext {
        doc,
        context_node,
        context_position: 1,
        context_size: 1,
    };
    evaluate_compiled(&compiled, &context)
}

/// Evaluate a compiled expression
pub fn evaluate_compiled<'a, D: DocumentAccess>(
    expr: &CompiledExpr,
    ctx: &EvalContext<'a, D>,
) -> Result<XPathValue, String> {
    let mut stack: Vec<XPathValue> = Vec::new();

    for op in &expr.ops {
        match op {
            Op::Root => {
                // Root is the document node (for XPath absolute paths like /root)
                stack.push(XPathValue::single_node(ctx.doc.document_node_id()));
            }

            Op::Context => {
                stack.push(XPathValue::single_node(ctx.context_node));
            }

            Op::Parent => {
                let current = stack
                    .pop()
                    .unwrap_or(XPathValue::single_node(ctx.context_node));
                if let XPathValue::NodeSet(nodes) = current {
                    // Use HashSet for O(1) deduplication instead of O(n) Vec::contains
                    let mut seen = HashSet::with_capacity(nodes.len());
                    let mut parents = Vec::with_capacity(nodes.len());
                    for node in nodes {
                        if let Some(parent) = ctx.doc.parent_of(node) {
                            if seen.insert(parent) {
                                parents.push(parent);
                            }
                        }
                    }
                    parents.sort_unstable(); // Document order
                    stack.push(XPathValue::NodeSet(parents));
                } else {
                    stack.push(XPathValue::empty_nodeset());
                }
            }

            Op::Navigate(axis, node_test) => {
                let current = stack
                    .pop()
                    .unwrap_or(XPathValue::single_node(ctx.context_node));
                if let XPathValue::NodeSet(nodes) = current {
                    // Special handling for attribute axis
                    if *axis == super::parser::Axis::Attribute {
                        let mut attr_values: Vec<String> = Vec::new();
                        for node in nodes {
                            match node_test {
                                CompiledNodeTest::Any => {
                                    // @* - all attributes
                                    for (_, value) in ctx.doc.get_attribute_values(node) {
                                        attr_values.push(value.to_string());
                                    }
                                }
                                CompiledNodeTest::Name(name) => {
                                    // @name - specific attribute
                                    if let Some(value) = ctx.doc.get_attribute(node, name) {
                                        attr_values.push(value.to_string());
                                    }
                                }
                                _ => {}
                            }
                        }
                        // Return as string values (attribute values are strings)
                        if attr_values.is_empty() {
                            stack.push(XPathValue::empty_nodeset());
                        } else if attr_values.len() == 1 {
                            // Safe: len() == 1 guarantees pop() returns Some
                            stack.push(XPathValue::String(attr_values.pop().unwrap_or_default()));
                        } else {
                            // Multiple attribute values - wrap in a special type
                            // For now, join them or return as strings
                            stack.push(XPathValue::StringList(attr_values));
                        }
                    } else {
                        // Use HashSet for O(1) deduplication instead of O(n) Vec::contains
                        // This changes O(n²) to O(n) for large node sets
                        // Capacity: nodes.len() is a reasonable estimate — axis navigation
                        // typically produces a similar-sized or smaller result set.
                        // Avoid over-allocating (the old 4x multiplier caused ~5 MB
                        // HashSet pre-allocation for 120K-node descendant traversals).
                        let mut seen = HashSet::with_capacity(nodes.len());
                        let mut result = Vec::with_capacity(nodes.len());
                        for node in nodes {
                            let axis_nodes = navigate(ctx.doc, node, *axis);
                            for candidate in axis_nodes {
                                if matches_node_test(ctx.doc, candidate, node_test)
                                    && seen.insert(candidate)
                                {
                                    result.push(candidate);
                                }
                            }
                        }
                        // Sort by document order (node IDs are assigned in document order)
                        result.sort_unstable();
                        stack.push(XPathValue::NodeSet(result));
                    }
                } else {
                    stack.push(XPathValue::empty_nodeset());
                }
            }

            Op::Predicate(pred_expr) => {
                let current = stack.pop().unwrap_or(XPathValue::empty_nodeset());
                if let XPathValue::NodeSet(nodes) = current {
                    let size = nodes.len();
                    let mut filtered = Vec::new();

                    for (i, &node) in nodes.iter().enumerate() {
                        let pred_ctx = EvalContext {
                            doc: ctx.doc,
                            context_node: node,
                            context_position: i + 1,
                            context_size: size,
                        };

                        let pred_result = evaluate_compiled(pred_expr, &pred_ctx)?;

                        let include = match pred_result {
                            XPathValue::Number(n) => (i + 1) as f64 == n,
                            _ => pred_result.to_boolean(),
                        };

                        if include {
                            filtered.push(node);
                        }
                    }

                    stack.push(XPathValue::NodeSet(filtered));
                } else {
                    stack.push(XPathValue::empty_nodeset());
                }
            }

            // Fast path: [@attr = 'value'] - direct attribute lookup
            Op::PredicateAttrEq(attr_name, value) => {
                let current = stack.pop().unwrap_or(XPathValue::empty_nodeset());
                if let XPathValue::NodeSet(nodes) = current {
                    let mut filtered = Vec::with_capacity(nodes.len() / 10); // Assume sparse matches
                    for &node in &nodes {
                        if let Some(attr_value) = ctx.doc.get_attribute(node, attr_name) {
                            if attr_value == value {
                                filtered.push(node);
                            }
                        }
                    }
                    stack.push(XPathValue::NodeSet(filtered));
                } else {
                    stack.push(XPathValue::empty_nodeset());
                }
            }

            // Fast path: [n] - position predicate
            Op::PredicatePosition(pos) => {
                let current = stack.pop().unwrap_or(XPathValue::empty_nodeset());
                if let XPathValue::NodeSet(nodes) = current {
                    if *pos > 0 && *pos <= nodes.len() {
                        stack.push(XPathValue::NodeSet(vec![nodes[*pos - 1]]));
                    } else {
                        stack.push(XPathValue::empty_nodeset());
                    }
                } else {
                    stack.push(XPathValue::empty_nodeset());
                }
            }

            Op::Union => {
                let right = stack.pop().unwrap_or(XPathValue::empty_nodeset());
                let left = stack.pop().unwrap_or(XPathValue::empty_nodeset());

                match (left, right) {
                    (XPathValue::NodeSet(l), XPathValue::NodeSet(r)) => {
                        // Use HashSet for O(1) deduplication instead of O(n) Vec::contains
                        let mut seen: HashSet<NodeId> = l.iter().copied().collect();
                        let mut result = l;
                        result.reserve(r.len());
                        for node in r {
                            if seen.insert(node) {
                                result.push(node);
                            }
                        }
                        // Sort by document order
                        result.sort_unstable();
                        stack.push(XPathValue::NodeSet(result));
                    }
                    _ => {
                        return Err("Union requires two node-sets".to_string());
                    }
                }
            }

            Op::Number(n) => {
                stack.push(XPathValue::Number(*n));
            }

            Op::String(s) => {
                stack.push(XPathValue::String(s.clone()));
            }

            Op::Variable(_name) => {
                // TODO: variable lookup
                stack.push(XPathValue::String(String::new()));
            }

            Op::Negate => {
                let val = stack.pop().unwrap_or(XPathValue::Number(0.0));
                stack.push(XPathValue::Number(-val.to_number()));
            }

            Op::Binary(op) => {
                let right = stack.pop().unwrap_or(XPathValue::Number(0.0));
                let left = stack.pop().unwrap_or(XPathValue::Number(0.0));

                let result = match op {
                    BinaryOp::Or => XPathValue::Boolean(left.to_boolean() || right.to_boolean()),
                    BinaryOp::And => XPathValue::Boolean(left.to_boolean() && right.to_boolean()),
                    BinaryOp::Eq => compare_values(ctx.doc, &left, &right, |a, b| a == b),
                    BinaryOp::NotEq => compare_values(ctx.doc, &left, &right, |a, b| a != b),
                    BinaryOp::Lt => compare_numbers(ctx.doc, &left, &right, |a, b| a < b),
                    BinaryOp::LtEq => compare_numbers(ctx.doc, &left, &right, |a, b| a <= b),
                    BinaryOp::Gt => compare_numbers(ctx.doc, &left, &right, |a, b| a > b),
                    BinaryOp::GtEq => compare_numbers(ctx.doc, &left, &right, |a, b| a >= b),
                    BinaryOp::Add => XPathValue::Number(left.to_number() + right.to_number()),
                    BinaryOp::Sub => XPathValue::Number(left.to_number() - right.to_number()),
                    BinaryOp::Mul => XPathValue::Number(left.to_number() * right.to_number()),
                    BinaryOp::Div => XPathValue::Number(left.to_number() / right.to_number()),
                    BinaryOp::Mod => XPathValue::Number(left.to_number() % right.to_number()),
                };

                stack.push(result);
            }

            Op::Call(name, arg_count) => {
                let mut args = Vec::new();
                for _ in 0..*arg_count {
                    args.push(stack.pop().unwrap_or(XPathValue::String(String::new())));
                }
                args.reverse();

                let result = functions::call(
                    name,
                    args,
                    ctx.doc,
                    ctx.context_node,
                    ctx.context_position,
                    ctx.context_size,
                )?;

                stack.push(result);
            }
        }
    }

    Ok(stack.pop().unwrap_or(XPathValue::empty_nodeset()))
}

/// Compare two XPath values for equality per XPath 1.0 spec.
///
/// Only used for `Eq` and `NotEq` operations — relational operators
/// (`Lt`, `Gt`, etc.) use `compare_numbers` with `f64` comparisons.
///
/// Requires document access to get the string-value of nodes in node-sets.
fn compare_values<D: DocumentAccess, F>(
    doc: &D,
    left: &XPathValue,
    right: &XPathValue,
    cmp: F,
) -> XPathValue
where
    F: Fn(&str, &str) -> bool,
{
    use crate::dom::node_string_value;

    match (left, right) {
        (XPathValue::NodeSet(ln), XPathValue::NodeSet(rn)) => {
            // Two node-sets: true if any pair of string values match.
            // Pre-compute right-side values to avoid O(n*m) recomputation.
            let right_strings: Vec<String> =
                rn.iter().map(|&r| node_string_value(doc, r)).collect();
            for &l in ln {
                let ls = node_string_value(doc, l);
                for rs in &right_strings {
                    if cmp(&ls, rs) {
                        return XPathValue::Boolean(true);
                    }
                }
            }
            XPathValue::Boolean(false)
        }
        (XPathValue::NodeSet(nodes), other) | (other, XPathValue::NodeSet(nodes)) => {
            // Node-set vs other: compare each node's string-value against the other value
            let other_str = other.to_string_value();
            for &n in nodes {
                let ns = node_string_value(doc, n);
                if cmp(&ns, &other_str) {
                    return XPathValue::Boolean(true);
                }
            }
            XPathValue::Boolean(false)
        }
        (XPathValue::Boolean(_), _) | (_, XPathValue::Boolean(_)) => XPathValue::Boolean(cmp(
            &left.to_boolean().to_string(),
            &right.to_boolean().to_string(),
        )),
        (XPathValue::Number(_), _) | (_, XPathValue::Number(_)) => {
            let ln = left.to_number();
            let rn = right.to_number();
            XPathValue::Boolean(cmp(&ln.to_string(), &rn.to_string()))
        }
        (XPathValue::String(ls), XPathValue::String(rs)) => XPathValue::Boolean(cmp(ls, rs)),
        // Handle StringList and any other combinations
        _ => XPathValue::Boolean(cmp(&left.to_string_value(), &right.to_string_value())),
    }
}

/// Compare two values as numbers (for relational operators: <, <=, >, >=).
///
/// Requires document access to resolve NodeSet values to their text content
/// before numeric conversion, per XPath 1.0 spec.
fn compare_numbers<D: DocumentAccess, F>(
    doc: &D,
    left: &XPathValue,
    right: &XPathValue,
    cmp: F,
) -> XPathValue
where
    F: Fn(f64, f64) -> bool,
{
    let ln = resolve_number(doc, left);
    let rn = resolve_number(doc, right);
    XPathValue::Boolean(cmp(ln, rn))
}

/// Convert an XPath value to a number, using document access for NodeSets.
///
/// Per XPath 1.0 spec, the number-value of a node-set is the number-value
/// of the string-value of the first node in document order.
fn resolve_number<D: DocumentAccess>(doc: &D, val: &XPathValue) -> f64 {
    match val {
        XPathValue::NodeSet(nodes) => {
            if let Some(&first) = nodes.first() {
                let s = dom::node_string_value(doc, first);
                s.trim().parse().unwrap_or(f64::NAN)
            } else {
                f64::NAN
            }
        }
        _ => val.to_number(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_path() {
        let doc = XmlDocument::parse(b"<root><child/></root>");
        let result = evaluate(&doc, "/root/child").unwrap();
        assert!(result.is_nodeset());
        assert_eq!(result.as_nodeset().unwrap().len(), 1);
    }

    #[test]
    fn test_descendant() {
        let doc = XmlDocument::parse(b"<root><a><b/></a></root>");
        let result = evaluate(&doc, "//b").unwrap();
        assert!(result.is_nodeset());
        assert_eq!(result.as_nodeset().unwrap().len(), 1);
    }

    #[test]
    fn test_predicate() {
        let doc = XmlDocument::parse(b"<root><a/><b/><c/></root>");
        let result = evaluate(&doc, "/root/*[2]").unwrap();
        assert!(result.is_nodeset());
        assert_eq!(result.as_nodeset().unwrap().len(), 1);
    }

    #[test]
    fn test_count() {
        let doc = XmlDocument::parse(b"<root><a/><b/><c/></root>");
        let result = evaluate(&doc, "count(/root/*)").unwrap();
        assert_eq!(result.to_number(), 3.0);
    }

    #[test]
    fn test_string_function() {
        let doc = XmlDocument::parse(b"<root>hello</root>");
        let result = evaluate(&doc, "string-length('hello')").unwrap();
        assert_eq!(result.to_number(), 5.0);
    }

    #[test]
    fn node_equality_compares_text_content_not_ids() {
        let doc = XmlDocument::parse(b"<r><a>hello</a><b>hello</b><c>world</c></r>");
        // Nodes with same text content should be equal
        let result = evaluate(&doc, "/r/a = /r/b").unwrap();
        assert!(
            result.to_boolean(),
            "Nodes with same text 'hello' should be equal"
        );
        // Nodes with different text content should not be equal
        let result = evaluate(&doc, "/r/a = /r/c").unwrap();
        assert!(
            !result.to_boolean(),
            "Nodes with different text 'hello' vs 'world' should not be equal"
        );
    }

    #[test]
    fn node_inequality_compares_text_content() {
        let doc = XmlDocument::parse(b"<r><a>hello</a><b>world</b></r>");
        let result = evaluate(&doc, "/r/a != /r/b").unwrap();
        assert!(
            result.to_boolean(),
            "Nodes with different text should be not-equal"
        );
    }

    #[test]
    fn string_function_on_nodeset_returns_text_content() {
        let doc = XmlDocument::parse(b"<r><item>content</item></r>");
        let result = evaluate(&doc, "string(/r/item)").unwrap();
        assert_eq!(
            result.to_string_value(),
            "content",
            "string() on a node-set should return text content, not node ID"
        );
    }

    #[test]
    fn contains_function_with_nodeset_arg() {
        let doc = XmlDocument::parse(b"<r><a>hello world</a></r>");
        let result = evaluate(&doc, "contains(/r/a, 'world')").unwrap();
        assert!(
            result.to_boolean(),
            "contains() should work with NodeSet first arg"
        );
    }

    #[test]
    fn relational_operator_on_nodeset_resolves_text_content() {
        let doc = XmlDocument::parse(b"<r><price>42.5</price></r>");
        let result = evaluate(&doc, "/r/price > 10").unwrap();
        assert!(
            result.to_boolean(),
            "NodeSet with numeric text '42.5' should be > 10"
        );
        let result = evaluate(&doc, "/r/price < 100").unwrap();
        assert!(
            result.to_boolean(),
            "NodeSet with numeric text '42.5' should be < 100"
        );
    }
}
