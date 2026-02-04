//! Structural Index Module
//!
//! This module provides a memory-efficient representation of XML documents
//! using only byte offsets into the original input. This enables:
//!
//! - **Zero-copy strings**: Element names, attribute values, and text content
//!   are represented as (offset, length) spans into the original input.
//! - **Sub-binary returns**: When returning strings to the BEAM, we use
//!   `make_subbinary()` to share memory with the original input.
//! - **Cache-friendly**: Compact structs (32 bytes per element) enable
//!   better CPU cache utilization.
//!
//! ## Architecture
//!
//! ```text
//! StructuralIndex
//! ├── elements: Vec<IndexElement>   # 32 bytes each
//! ├── texts: Vec<IndexText>         # 16 bytes each
//! ├── attributes: Vec<IndexAttribute> # 12 bytes each
//! └── children: flat storage of ChildRef
//! ```
//!
//! ## Memory Comparison
//!
//! | Metric | Old DOM | Structural Index |
//! |--------|---------|------------------|
//! | Per element | ~48 bytes + strings | 32 bytes |
//! | Per text | String copy | 16 bytes |
//! | String storage | ~2x input | 0 (offsets only) |

pub mod builder;
pub mod element;
pub mod span;
pub mod structural;
pub mod view;

// Re-export what's needed externally
pub use span::Span;
pub use structural::StructuralIndex;
pub use view::IndexedDocumentView;
