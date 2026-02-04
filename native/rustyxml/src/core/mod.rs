//! Core XML parsing primitives
//!
//! This module contains the fundamental building blocks for XML parsing:
//! - Scanner: SIMD-accelerated delimiter detection using memchr
//! - Tokenizer: State machine for XML token extraction
//! - Entities: XML entity decoding with Cow (zero-copy when possible)
//! - Attributes: Attribute parsing and extraction
//! - Encoding: UTF-16 detection and conversion to UTF-8
//! - Unicode: XML 1.0 Unicode character class validation
//! - DTD: DTD declaration store and post-parse validation
//! - UnifiedScanner: ScanHandler-based scanner for Index/SAX modes

pub mod attributes;
pub mod dtd;
pub mod encoding;
pub mod entities;
pub mod scanner;
pub mod tokenizer;
pub mod unicode;
pub mod unified_scanner;
