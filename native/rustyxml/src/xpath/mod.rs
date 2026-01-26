//! XPath 1.0 Engine
//!
//! Full XPath 1.0 implementation with:
//! - All 13 axes
//! - 27+ functions
//! - Compiled expression caching

pub mod lexer;
pub mod parser;
pub mod compiler;
pub mod eval;
pub mod axes;
pub mod functions;
pub mod value;

pub use eval::{evaluate, evaluate_from_node};
pub use value::XPathValue;
