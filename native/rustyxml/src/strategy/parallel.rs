//! Parallel XPath Evaluation (Strategy E)
//!
//! Uses Rayon for parallel evaluation of multiple XPath queries.

use crate::dom::DocumentAccess;
#[cfg(test)]
use crate::dom::XmlDocument;
use crate::xpath::{evaluate, XPathValue};
use rayon::prelude::*;

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
}
