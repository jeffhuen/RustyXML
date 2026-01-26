//! Parsing Strategy Module
//!
//! Multiple strategies for different use cases:
//! - Strategy A: Zero-copy slice parser (default, fastest for small-medium docs)
//! - Strategy B: Buffer-based parser (for streams)
//! - Strategy C: DOM parser (for XPath queries)
//! - Strategy D: Streaming tag parser (for large files)
//! - Strategy E: Parallel XPath (for multiple queries)

pub mod streaming;
pub mod parallel;

pub use streaming::StreamingParser;
