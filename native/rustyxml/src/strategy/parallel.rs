//! Parallel XPath Evaluation (Strategy E)
//!
//! Uses Rayon for parallel evaluation of multiple XPath queries.

use rayon::prelude::*;
use crate::dom::DocumentAccess;
#[cfg(test)]
use crate::dom::XmlDocument;
use crate::xpath::{evaluate, XPathValue};

/// Evaluate multiple XPath expressions in parallel
pub fn evaluate_parallel<D: DocumentAccess + Sync>(
    doc: &D,
    xpaths: &[&str],
) -> Vec<Result<XPathValue, String>> {
    xpaths
        .par_iter()
        .map(|xpath| evaluate(doc, xpath))
        .collect()
}

/// Evaluate an XPath expression and map results
pub fn xpath_map<D, F, T>(
    doc: &D,
    xpath: &str,
    mapper: F,
) -> Result<Vec<T>, String>
where
    D: DocumentAccess + Sync,
    F: Fn(u32) -> T + Sync + Send,
    T: Send,
{
    let result = evaluate(doc, xpath)?;

    match result {
        XPathValue::NodeSet(nodes) => {
            Ok(nodes.par_iter().map(|&n| mapper(n)).collect())
        }
        _ => Err("XPath did not return a node-set".to_string()),
    }
}

/// Parallel xmap - evaluate multiple XPath expressions and collect results
pub fn xmap<D: DocumentAccess + Sync>(
    doc: &D,
    queries: &[(&str, &str)], // (key, xpath)
) -> Result<Vec<(String, XPathValue)>, String> {
    queries
        .par_iter()
        .map(|(key, xpath)| {
            evaluate(doc, xpath).map(|v| (key.to_string(), v))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parallel_eval() {
        let doc = XmlDocument::parse(b"<root><a/><b/><c/></root>");
        let xpaths = ["//a", "//b", "//c"];

        let results = evaluate_parallel(&doc, &xpaths);
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.is_ok()));
    }

    #[test]
    fn test_xmap() {
        let doc = XmlDocument::parse(b"<root><a/><b/></root>");
        let queries = [
            ("first", "//a"),
            ("second", "//b"),
        ];

        let results = xmap(&doc, &queries).unwrap();
        assert_eq!(results.len(), 2);
    }
}
