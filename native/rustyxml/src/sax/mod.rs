//! SAX (Simple API for XML) Module
//!
//! Provides event-based XML parsing compatible with the Saxy Elixir library.
//!
//! ## Architecture
//!
//! The SAX module uses the unified scanner with a SaxCollector handler:
//!
//! ```text
//! UnifiedScanner ---> SaxCollector ---> CompactSaxEvent[]
//!                          |
//!                          v
//!                    Elixir Terms (via NIF)
//! ```
//!
//! ## Event Types
//!
//! - `StartElement` - Element opening tag with name and attributes
//! - `EndElement` - Element closing tag
//! - `Text` - Character data (with entity decode flag)
//! - `CData` - CDATA section content
//! - `Comment` - Comment content
//! - `ProcessingInstruction` - PI target and data
//!
//! ## Memory Efficiency
//!
//! Events use Spans (offset + length) into the original input, enabling:
//! - Zero-copy parsing: no string allocations during scan
//! - Sub-binary returns: Elixir binaries share input memory

pub mod collector;
pub mod events;

// Re-export only what's needed externally
pub use collector::SaxCollector;
pub use events::CompactSaxEvent;
