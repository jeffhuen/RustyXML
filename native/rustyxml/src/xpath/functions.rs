//! XPath 1.0 Functions
//!
//! Implements all 27+ XPath 1.0 core functions:
//!
//! Node Set Functions:
//! - position(), last(), count(), local-name(), namespace-uri(), name()
//!
//! String Functions:
//! - string(), concat(), starts-with(), contains(), substring(),
//!   substring-before(), substring-after(), string-length(),
//!   normalize-space(), translate()
//!
//! Boolean Functions:
//! - boolean(), not(), true(), false(), lang()
//!
//! Number Functions:
//! - number(), sum(), floor(), ceiling(), round()

use super::value::XPathValue;
#[cfg(test)]
use crate::dom::XmlDocument;
use crate::dom::{self, DocumentAccess, NodeId};

/// Evaluate a function call
pub fn call<D: DocumentAccess>(
    name: &str,
    args: Vec<XPathValue>,
    doc: &D,
    context: NodeId,
    position: usize,
    size: usize,
) -> Result<XPathValue, String> {
    match name {
        // Node Set Functions
        "position" => Ok(XPathValue::Number(position as f64)),
        "last" => Ok(XPathValue::Number(size as f64)),
        "count" => fn_count(args),
        "local-name" => fn_local_name(args, doc, context),
        "namespace-uri" => fn_namespace_uri(args, doc, context),
        "name" => fn_name(args, doc, context),
        "id" => fn_id(args),

        // String Functions
        "string" => fn_string(args, doc, context),
        "concat" => fn_concat(args, doc),
        "starts-with" => fn_starts_with(args, doc),
        "contains" => fn_contains(args, doc),
        "substring" => fn_substring(args, doc),
        "substring-before" => fn_substring_before(args, doc),
        "substring-after" => fn_substring_after(args, doc),
        "string-length" => fn_string_length(args, doc, context),
        "normalize-space" => fn_normalize_space(args, doc, context),
        "translate" => fn_translate(args, doc),

        // Boolean Functions
        "boolean" => fn_boolean(args),
        "not" => fn_not(args),
        "true" => Ok(XPathValue::Boolean(true)),
        "false" => Ok(XPathValue::Boolean(false)),
        "lang" => fn_lang(args, doc, context),

        // Number Functions
        "number" => fn_number(args, doc, context),
        "sum" => fn_sum(args, doc),
        "floor" => fn_floor(args),
        "ceiling" => fn_ceiling(args),
        "round" => fn_round(args),

        _ => Err(format!("Unknown function: {}", name)),
    }
}

// Node Set Functions

fn fn_count(args: Vec<XPathValue>) -> Result<XPathValue, String> {
    if args.len() != 1 {
        return Err("count() requires exactly 1 argument".to_string());
    }
    match &args[0] {
        XPathValue::NodeSet(nodes) => Ok(XPathValue::Number(nodes.len() as f64)),
        _ => Err("count() argument must be a node-set".to_string()),
    }
}

fn fn_local_name<D: DocumentAccess>(
    args: Vec<XPathValue>,
    doc: &D,
    context: NodeId,
) -> Result<XPathValue, String> {
    let node = if args.is_empty() {
        context
    } else {
        match &args[0] {
            XPathValue::NodeSet(nodes) if !nodes.is_empty() => nodes[0],
            XPathValue::NodeSet(_) => return Ok(XPathValue::String(String::new())),
            _ => return Err("local-name() argument must be a node-set".to_string()),
        }
    };

    let name = doc.node_local_name(node).unwrap_or("");
    Ok(XPathValue::String(name.to_string()))
}

fn fn_namespace_uri<D: DocumentAccess>(
    args: Vec<XPathValue>,
    doc: &D,
    context: NodeId,
) -> Result<XPathValue, String> {
    let node = if args.is_empty() {
        context
    } else {
        match &args[0] {
            XPathValue::NodeSet(nodes) if !nodes.is_empty() => nodes[0],
            XPathValue::NodeSet(_) => return Ok(XPathValue::String(String::new())),
            _ => return Err("namespace-uri() argument must be a node-set".to_string()),
        }
    };

    let uri = doc.node_namespace_uri(node).unwrap_or("");
    Ok(XPathValue::String(uri.to_string()))
}

fn fn_name<D: DocumentAccess>(
    args: Vec<XPathValue>,
    doc: &D,
    context: NodeId,
) -> Result<XPathValue, String> {
    let node = if args.is_empty() {
        context
    } else {
        match &args[0] {
            XPathValue::NodeSet(nodes) if !nodes.is_empty() => nodes[0],
            XPathValue::NodeSet(_) => return Ok(XPathValue::String(String::new())),
            _ => return Err("name() argument must be a node-set".to_string()),
        }
    };

    let name = doc.node_name(node).unwrap_or("");
    Ok(XPathValue::String(name.to_string()))
}

fn fn_id(_args: Vec<XPathValue>) -> Result<XPathValue, String> {
    Err(
        "id() is not supported: DTD processing is disabled for security (XXE prevention)"
            .to_string(),
    )
}

// String Functions

fn fn_string<D: DocumentAccess>(
    args: Vec<XPathValue>,
    doc: &D,
    context: NodeId,
) -> Result<XPathValue, String> {
    let value = if args.is_empty() {
        dom::node_string_value(doc, context)
    } else {
        resolve_string(&args[0], doc)
    };
    Ok(XPathValue::String(value))
}

fn fn_concat<D: DocumentAccess>(args: Vec<XPathValue>, doc: &D) -> Result<XPathValue, String> {
    if args.len() < 2 {
        return Err("concat() requires at least 2 arguments".to_string());
    }
    let result: String = args.iter().map(|a| resolve_string(a, doc)).collect();
    Ok(XPathValue::String(result))
}

fn fn_starts_with<D: DocumentAccess>(args: Vec<XPathValue>, doc: &D) -> Result<XPathValue, String> {
    if args.len() != 2 {
        return Err("starts-with() requires exactly 2 arguments".to_string());
    }
    let s = resolve_string(&args[0], doc);
    let prefix = resolve_string(&args[1], doc);
    Ok(XPathValue::Boolean(s.starts_with(&prefix)))
}

fn fn_contains<D: DocumentAccess>(args: Vec<XPathValue>, doc: &D) -> Result<XPathValue, String> {
    if args.len() != 2 {
        return Err("contains() requires exactly 2 arguments".to_string());
    }
    let s = resolve_string(&args[0], doc);
    let pattern = resolve_string(&args[1], doc);
    Ok(XPathValue::Boolean(s.contains(&pattern)))
}

fn fn_substring<D: DocumentAccess>(args: Vec<XPathValue>, doc: &D) -> Result<XPathValue, String> {
    if args.len() < 2 || args.len() > 3 {
        return Err("substring() requires 2 or 3 arguments".to_string());
    }

    let s = resolve_string(&args[0], doc);
    let start = args[1].to_number().round() as i64 - 1; // XPath is 1-indexed
    let start = start.max(0) as usize;

    let chars: Vec<char> = s.chars().collect();

    let result = if args.len() == 3 {
        let len = args[2].to_number().round() as usize;
        let end = (start + len).min(chars.len());
        chars[start.min(chars.len())..end].iter().collect()
    } else {
        chars[start.min(chars.len())..].iter().collect()
    };

    Ok(XPathValue::String(result))
}

fn fn_substring_before<D: DocumentAccess>(
    args: Vec<XPathValue>,
    doc: &D,
) -> Result<XPathValue, String> {
    if args.len() != 2 {
        return Err("substring-before() requires exactly 2 arguments".to_string());
    }
    let s = resolve_string(&args[0], doc);
    let pattern = resolve_string(&args[1], doc);

    let result = if let Some(pos) = s.find(&pattern) {
        s[..pos].to_string()
    } else {
        String::new()
    };

    Ok(XPathValue::String(result))
}

fn fn_substring_after<D: DocumentAccess>(
    args: Vec<XPathValue>,
    doc: &D,
) -> Result<XPathValue, String> {
    if args.len() != 2 {
        return Err("substring-after() requires exactly 2 arguments".to_string());
    }
    let s = resolve_string(&args[0], doc);
    let pattern = resolve_string(&args[1], doc);

    let result = if let Some(pos) = s.find(&pattern) {
        s[pos + pattern.len()..].to_string()
    } else {
        String::new()
    };

    Ok(XPathValue::String(result))
}

fn fn_string_length<D: DocumentAccess>(
    args: Vec<XPathValue>,
    doc: &D,
    context: NodeId,
) -> Result<XPathValue, String> {
    if args.len() > 1 {
        return Err("string-length() requires 0 or 1 arguments".to_string());
    }
    let s = if args.is_empty() {
        dom::node_string_value(doc, context)
    } else {
        resolve_string(&args[0], doc)
    };
    Ok(XPathValue::Number(s.chars().count() as f64))
}

fn fn_normalize_space<D: DocumentAccess>(
    args: Vec<XPathValue>,
    doc: &D,
    context: NodeId,
) -> Result<XPathValue, String> {
    let s = if args.is_empty() {
        dom::node_string_value(doc, context)
    } else if args.len() == 1 {
        resolve_string(&args[0], doc)
    } else {
        return Err("normalize-space() requires 0 or 1 arguments".to_string());
    };

    let normalized: String = s.split_whitespace().collect::<Vec<_>>().join(" ");

    Ok(XPathValue::String(normalized))
}

fn fn_translate<D: DocumentAccess>(args: Vec<XPathValue>, doc: &D) -> Result<XPathValue, String> {
    if args.len() != 3 {
        return Err("translate() requires exactly 3 arguments".to_string());
    }

    let s = resolve_string(&args[0], doc);
    let from: Vec<char> = resolve_string(&args[1], doc).chars().collect();
    let to: Vec<char> = resolve_string(&args[2], doc).chars().collect();

    let result: String = s
        .chars()
        .filter_map(|c| {
            if let Some(pos) = from.iter().position(|&fc| fc == c) {
                to.get(pos).copied()
            } else {
                Some(c)
            }
        })
        .collect();

    Ok(XPathValue::String(result))
}

// Boolean Functions

fn fn_boolean(args: Vec<XPathValue>) -> Result<XPathValue, String> {
    if args.len() != 1 {
        return Err("boolean() requires exactly 1 argument".to_string());
    }
    Ok(XPathValue::Boolean(args[0].to_boolean()))
}

fn fn_not(args: Vec<XPathValue>) -> Result<XPathValue, String> {
    if args.len() != 1 {
        return Err("not() requires exactly 1 argument".to_string());
    }
    Ok(XPathValue::Boolean(!args[0].to_boolean()))
}

fn fn_lang<D: DocumentAccess>(
    args: Vec<XPathValue>,
    doc: &D,
    context: NodeId,
) -> Result<XPathValue, String> {
    if args.len() != 1 {
        return Err("lang() requires exactly 1 argument".to_string());
    }
    let target_lang = args[0].to_string_value().to_lowercase();

    // Walk up ancestor chain looking for xml:lang attribute
    let mut node = context;
    loop {
        if let Some(lang_val) = doc.get_attribute(node, "xml:lang") {
            let lang_lower = lang_val.to_lowercase();
            // Exact match or subtag prefix match (e.g., "en" matches "en-US")
            if lang_lower == target_lang
                || (lang_lower.starts_with(&target_lang)
                    && lang_lower.as_bytes().get(target_lang.len()) == Some(&b'-'))
            {
                return Ok(XPathValue::Boolean(true));
            }
            return Ok(XPathValue::Boolean(false));
        }
        match doc.parent_of(node) {
            Some(parent) => node = parent,
            None => break,
        }
    }
    Ok(XPathValue::Boolean(false))
}

// Number Functions

fn fn_number<D: DocumentAccess>(
    args: Vec<XPathValue>,
    doc: &D,
    context: NodeId,
) -> Result<XPathValue, String> {
    let value = if args.is_empty() {
        let s = dom::node_string_value(doc, context);
        s.trim().parse().unwrap_or(f64::NAN)
    } else if args.len() == 1 {
        args[0].to_number()
    } else {
        return Err("number() requires 0 or 1 arguments".to_string());
    };
    Ok(XPathValue::Number(value))
}

fn fn_sum<D: DocumentAccess>(args: Vec<XPathValue>, doc: &D) -> Result<XPathValue, String> {
    if args.len() != 1 {
        return Err("sum() requires exactly 1 argument".to_string());
    }

    match &args[0] {
        XPathValue::NodeSet(nodes) => {
            let mut total = 0.0;
            for &node in nodes {
                let s = dom::node_string_value(doc, node);
                if let Ok(n) = s.trim().parse::<f64>() {
                    total += n;
                } else {
                    return Ok(XPathValue::Number(f64::NAN));
                }
            }
            Ok(XPathValue::Number(total))
        }
        _ => Err("sum() argument must be a node-set".to_string()),
    }
}

fn fn_floor(args: Vec<XPathValue>) -> Result<XPathValue, String> {
    if args.len() != 1 {
        return Err("floor() requires exactly 1 argument".to_string());
    }
    Ok(XPathValue::Number(args[0].to_number().floor()))
}

fn fn_ceiling(args: Vec<XPathValue>) -> Result<XPathValue, String> {
    if args.len() != 1 {
        return Err("ceiling() requires exactly 1 argument".to_string());
    }
    Ok(XPathValue::Number(args[0].to_number().ceil()))
}

fn fn_round(args: Vec<XPathValue>) -> Result<XPathValue, String> {
    if args.len() != 1 {
        return Err("round() requires exactly 1 argument".to_string());
    }
    let n = args[0].to_number();
    // XPath round: rounds towards positive infinity for .5
    let rounded = if n.fract() == 0.5 || n.fract() == -0.5 {
        n.ceil()
    } else {
        n.round()
    };
    Ok(XPathValue::Number(rounded))
}

/// Convert an XPath value to a string, using document access for NodeSets.
///
/// Per XPath 1.0 spec, the string-value of a node-set is the string-value
/// of the first node in document order. This requires document access to
/// extract actual text content (unlike `XPathValue::to_string_value()`).
fn resolve_string<D: DocumentAccess>(val: &XPathValue, doc: &D) -> String {
    match val {
        XPathValue::NodeSet(nodes) => {
            if let Some(&first) = nodes.first() {
                dom::node_string_value(doc, first)
            } else {
                String::new()
            }
        }
        _ => val.to_string_value(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_concat() {
        let doc = XmlDocument::parse(b"<r/>");
        let args = vec![
            XPathValue::String("hello".to_string()),
            XPathValue::String(" ".to_string()),
            XPathValue::String("world".to_string()),
        ];
        let result = fn_concat(args, &doc).unwrap();
        assert_eq!(result.to_string_value(), "hello world");
    }

    #[test]
    fn test_contains() {
        let doc = XmlDocument::parse(b"<r/>");
        let args = vec![
            XPathValue::String("hello world".to_string()),
            XPathValue::String("world".to_string()),
        ];
        assert!(fn_contains(args, &doc).unwrap().to_boolean());
    }

    #[test]
    fn test_substring() {
        let doc = XmlDocument::parse(b"<r/>");
        let args = vec![
            XPathValue::String("hello".to_string()),
            XPathValue::Number(2.0),
            XPathValue::Number(3.0),
        ];
        let result = fn_substring(args, &doc).unwrap();
        assert_eq!(result.to_string_value(), "ell");
    }

    #[test]
    fn test_normalize_space() {
        let doc = XmlDocument::parse(b"<r/>");
        let args = vec![XPathValue::String("  hello   world  ".to_string())];
        let result = fn_normalize_space(args, &doc, 0).unwrap();
        assert_eq!(result.to_string_value(), "hello world");
    }

    #[test]
    fn id_returns_explicit_error() {
        let result = fn_id(vec![XPathValue::String("foo".to_string())]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not supported"));
    }

    #[test]
    fn lang_matches_xml_lang_attribute() {
        let doc = XmlDocument::parse(b"<root xml:lang=\"en\"><child/></root>");
        let root = doc.root_element_id().unwrap();
        let children: Vec<_> = doc.children_vec(root);
        let child = children[0];
        let result = call(
            "lang",
            vec![XPathValue::String("en".to_string())],
            &doc,
            child,
            1,
            1,
        )
        .unwrap();
        assert!(
            result.to_boolean(),
            "lang('en') should match xml:lang='en' on ancestor"
        );
    }

    #[test]
    fn lang_matches_subtag_prefix() {
        let doc = XmlDocument::parse(b"<root xml:lang=\"en-US\"><child/></root>");
        let root = doc.root_element_id().unwrap();
        let children: Vec<_> = doc.children_vec(root);
        let child = children[0];
        let result = call(
            "lang",
            vec![XPathValue::String("en".to_string())],
            &doc,
            child,
            1,
            1,
        )
        .unwrap();
        assert!(
            result.to_boolean(),
            "lang('en') should match xml:lang='en-US'"
        );
    }

    #[test]
    fn namespace_uri_returns_uri_for_prefixed_element() {
        let doc = XmlDocument::parse(b"<root xmlns:ns=\"http://example.com\"><ns:child/></root>");
        let root = doc.root_element_id().unwrap();
        let children: Vec<_> = doc.children_vec(root);
        let child = children[0];
        let result = fn_namespace_uri(vec![XPathValue::NodeSet(vec![child])], &doc, child).unwrap();
        assert_eq!(result.to_string_value(), "http://example.com");
    }
}
