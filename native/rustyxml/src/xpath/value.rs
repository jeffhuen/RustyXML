//! XPath Value Types
//!
//! XPath 1.0 has four data types: node-set, boolean, number, and string.

use crate::dom::NodeId;

/// XPath value types
#[derive(Debug, Clone)]
#[must_use]
pub enum XPathValue {
    /// A set of nodes (ordered, no duplicates)
    NodeSet(Vec<NodeId>),
    /// Boolean value
    Boolean(bool),
    /// Floating-point number
    Number(f64),
    /// String value
    String(String),
    /// List of strings (for attribute values)
    StringList(Vec<String>),
}

impl XPathValue {
    /// Create an empty node set
    pub fn empty_nodeset() -> Self {
        XPathValue::NodeSet(Vec::new())
    }

    /// Create a node set with a single node
    pub fn single_node(id: NodeId) -> Self {
        XPathValue::NodeSet(vec![id])
    }

    /// Convert to boolean (XPath boolean() function semantics)
    pub fn to_boolean(&self) -> bool {
        match self {
            XPathValue::NodeSet(nodes) => !nodes.is_empty(),
            XPathValue::Boolean(b) => *b,
            XPathValue::Number(n) => *n != 0.0 && !n.is_nan(),
            XPathValue::String(s) => !s.is_empty(),
            XPathValue::StringList(list) => !list.is_empty(),
        }
    }

    /// Convert to number (XPath number() function semantics)
    pub fn to_number(&self) -> f64 {
        match self {
            XPathValue::NodeSet(_) => {
                let s = self.to_string_value();
                s.trim().parse().unwrap_or(f64::NAN)
            }
            XPathValue::Boolean(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            XPathValue::Number(n) => *n,
            XPathValue::String(s) => s.trim().parse().unwrap_or(f64::NAN),
            XPathValue::StringList(list) => {
                if list.is_empty() {
                    f64::NAN
                } else {
                    list[0].trim().parse().unwrap_or(f64::NAN)
                }
            }
        }
    }

    /// Convert to string (XPath string() function semantics).
    ///
    /// **Warning:** For `NodeSet` values, this returns an empty string because
    /// proper XPath 1.0 string conversion requires document access to extract
    /// text content. Use `dom::node_string_value()` directly when you have a
    /// document reference and need the string-value of a node.
    pub fn to_string_value(&self) -> String {
        match self {
            XPathValue::NodeSet(_) => {
                // XPath 1.0 spec: string-value of a node-set is the string-value
                // of the first node in document order. We cannot resolve this without
                // document access, so return empty string. Callers with doc access
                // should use dom::node_string_value() instead.
                String::new()
            }
            XPathValue::Boolean(b) => if *b { "true" } else { "false" }.to_string(),
            XPathValue::Number(n) => {
                if n.is_nan() {
                    "NaN".to_string()
                } else if n.is_infinite() {
                    if *n > 0.0 { "Infinity" } else { "-Infinity" }.to_string()
                } else if *n == n.trunc() && n.abs() < 1e15 {
                    format!("{}", *n as i64)
                } else {
                    format!("{}", n)
                }
            }
            XPathValue::String(s) => s.clone(),
            XPathValue::StringList(list) => {
                if list.is_empty() {
                    String::new()
                } else {
                    list[0].clone()
                }
            }
        }
    }

    /// Check if this is a node set
    pub fn is_nodeset(&self) -> bool {
        matches!(self, XPathValue::NodeSet(_))
    }

    /// Check if this is a string list (attribute values)
    pub fn is_string_list(&self) -> bool {
        matches!(self, XPathValue::StringList(_))
    }

    /// Get as string list, or None
    pub fn as_string_list(&self) -> Option<&Vec<String>> {
        match self {
            XPathValue::StringList(list) => Some(list),
            _ => None,
        }
    }

    /// Get as node set, or None
    pub fn as_nodeset(&self) -> Option<&Vec<NodeId>> {
        match self {
            XPathValue::NodeSet(nodes) => Some(nodes),
            _ => None,
        }
    }

    /// Get as mutable node set
    pub fn as_nodeset_mut(&mut self) -> Option<&mut Vec<NodeId>> {
        match self {
            XPathValue::NodeSet(nodes) => Some(nodes),
            _ => None,
        }
    }
}

impl Default for XPathValue {
    fn default() -> Self {
        XPathValue::NodeSet(Vec::new())
    }
}

impl From<bool> for XPathValue {
    fn from(b: bool) -> Self {
        XPathValue::Boolean(b)
    }
}

impl From<f64> for XPathValue {
    fn from(n: f64) -> Self {
        XPathValue::Number(n)
    }
}

impl From<i64> for XPathValue {
    fn from(n: i64) -> Self {
        XPathValue::Number(n as f64)
    }
}

impl From<String> for XPathValue {
    fn from(s: String) -> Self {
        XPathValue::String(s)
    }
}

impl From<&str> for XPathValue {
    fn from(s: &str) -> Self {
        XPathValue::String(s.to_string())
    }
}

impl From<Vec<NodeId>> for XPathValue {
    fn from(nodes: Vec<NodeId>) -> Self {
        XPathValue::NodeSet(nodes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boolean_conversion() {
        assert!(XPathValue::NodeSet(vec![1]).to_boolean());
        assert!(!XPathValue::NodeSet(vec![]).to_boolean());
        assert!(XPathValue::Boolean(true).to_boolean());
        assert!(!XPathValue::Boolean(false).to_boolean());
        assert!(XPathValue::Number(1.0).to_boolean());
        assert!(!XPathValue::Number(0.0).to_boolean());
        assert!(XPathValue::String("hello".to_string()).to_boolean());
        assert!(!XPathValue::String(String::new()).to_boolean());
    }

    #[test]
    fn test_number_conversion() {
        assert_eq!(XPathValue::Boolean(true).to_number(), 1.0);
        assert_eq!(XPathValue::Boolean(false).to_number(), 0.0);
        assert_eq!(XPathValue::String("42".to_string()).to_number(), 42.0);
        assert!(XPathValue::String("abc".to_string()).to_number().is_nan());
    }

    #[test]
    fn test_string_conversion() {
        assert_eq!(XPathValue::Boolean(true).to_string_value(), "true");
        assert_eq!(XPathValue::Boolean(false).to_string_value(), "false");
        assert_eq!(XPathValue::Number(42.0).to_string_value(), "42");
        assert_eq!(XPathValue::Number(3.25).to_string_value(), "3.25");
    }
}
