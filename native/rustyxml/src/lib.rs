//! RustyXML - Fast XML parsing
//!
//! Strategies:
//! - parse/1 + xpath_query/2: Structural index with XPath (main path)
//! - streaming_*: Streaming parser for large files
//! - sax_parse/1: SAX events

use rustler::{Binary, Encoder, Env, NifResult, ResourceArc, Term};

// ============================================================================
// Pre-defined Atoms (panic-safe)
// ============================================================================

mod atoms {
    rustler::atoms! {
        ok,
        error,
        nil,
        text,
        name,
        mutex_poisoned,
    }
}

#[allow(dead_code)]
mod core;
mod dom;
mod index;
#[allow(dead_code)]
mod reader;
mod resource;
#[allow(dead_code)]
mod sax;
#[allow(dead_code)]
mod strategy;
mod term;
#[allow(dead_code)]
mod xpath;

use dom::DocumentAccess;
use resource::{
    DocumentAccumulatorRef, IndexedDocumentRef, IndexedDocumentResource, StreamingParserRef,
    StreamingParserResource, StreamingSaxParserRef, StreamingSaxParserResource,
};
use term::{events_to_term, node_to_term, xpath_value_to_term};
use xpath::evaluate;

// ============================================================================
// Allocator Configuration
// ============================================================================

#[cfg(feature = "memory_tracking")]
mod tracking {
    use std::alloc::{GlobalAlloc, Layout};
    use std::sync::atomic::{AtomicUsize, Ordering};

    pub static ALLOCATED: AtomicUsize = AtomicUsize::new(0);
    pub static PEAK_ALLOCATED: AtomicUsize = AtomicUsize::new(0);

    pub struct TrackingAllocator;

    #[cfg(feature = "mimalloc")]
    static UNDERLYING: mimalloc::MiMalloc = mimalloc::MiMalloc;

    #[cfg(not(feature = "mimalloc"))]
    static UNDERLYING: std::alloc::System = std::alloc::System;

    unsafe impl GlobalAlloc for TrackingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let ptr = UNDERLYING.alloc(layout);
            if !ptr.is_null() {
                let current = ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
                let mut peak = PEAK_ALLOCATED.load(Ordering::Relaxed);
                while current > peak {
                    match PEAK_ALLOCATED.compare_exchange_weak(
                        peak,
                        current,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    ) {
                        Ok(_) => break,
                        Err(p) => peak = p,
                    }
                }
            }
            ptr
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            ALLOCATED.fetch_sub(layout.size(), Ordering::Relaxed);
            UNDERLYING.dealloc(ptr, layout)
        }
    }
}

#[cfg(feature = "memory_tracking")]
#[global_allocator]
static GLOBAL: tracking::TrackingAllocator = tracking::TrackingAllocator;

#[cfg(all(feature = "mimalloc", not(feature = "memory_tracking")))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

// ============================================================================
// Memory Tracking NIFs
// ============================================================================

#[cfg(feature = "memory_tracking")]
use std::sync::atomic::Ordering;

#[cfg(feature = "memory_tracking")]
#[rustler::nif]
fn get_rust_memory() -> usize {
    tracking::ALLOCATED.load(Ordering::SeqCst)
}

#[cfg(feature = "memory_tracking")]
#[rustler::nif]
fn get_rust_memory_peak() -> usize {
    tracking::PEAK_ALLOCATED.load(Ordering::SeqCst)
}

#[cfg(feature = "memory_tracking")]
#[rustler::nif]
fn reset_rust_memory_stats() -> (usize, usize) {
    let current = tracking::ALLOCATED.load(Ordering::SeqCst);
    let peak = tracking::PEAK_ALLOCATED.swap(current, Ordering::SeqCst);
    (current, peak)
}

#[cfg(not(feature = "memory_tracking"))]
#[rustler::nif]
fn get_rust_memory() -> usize {
    0
}

#[cfg(not(feature = "memory_tracking"))]
#[rustler::nif]
fn get_rust_memory_peak() -> usize {
    0
}

#[cfg(not(feature = "memory_tracking"))]
#[rustler::nif]
fn reset_rust_memory_stats() -> (usize, usize) {
    (0, 0)
}

// ============================================================================
// Main Parse Path: Structural Index + XPath
// ============================================================================

/// Parse XML into structural index (returns ResourceArc)
/// Lenient mode - accepts malformed XML
/// Full XPath support via xpath_query
#[rustler::nif(schedule = "DirtyCpu")]
fn parse<'a>(env: Env<'a>, input: Binary<'a>) -> NifResult<Term<'a>> {
    let bytes = input.as_slice().to_vec();
    let resource = IndexedDocumentResource::new(bytes);
    let arc = ResourceArc::new(resource);
    Ok(arc.encode(env))
}

/// Parse XML in strict mode (returns {:ok, doc} or {:error, reason})
/// Rejects malformed XML per XML 1.0 specification
#[rustler::nif(schedule = "DirtyCpu")]
fn parse_strict<'a>(env: Env<'a>, input: Binary<'a>) -> NifResult<Term<'a>> {
    let bytes = input.as_slice().to_vec();

    // Handle encoding conversion (UTF-16 → UTF-8)
    let bytes = match crate::core::encoding::convert_to_utf8(bytes) {
        Ok(b) => b,
        Err(msg) => return Ok((atoms::error(), msg).encode(env)),
    };

    // Lightweight validation — no DOM construction
    match dom::validate_strict(&bytes) {
        Ok(()) => {
            let resource = IndexedDocumentResource::new(bytes);
            let arc = ResourceArc::new(resource);
            Ok((atoms::ok(), arc).encode(env))
        }
        Err(msg) => Ok((atoms::error(), msg).encode(env)),
    }
}

/// Execute XPath query on a document
#[rustler::nif]
fn xpath_query<'a>(
    env: Env<'a>,
    doc_ref: IndexedDocumentRef,
    xpath_str: &str,
) -> NifResult<Term<'a>> {
    let view = doc_ref.as_view();
    match evaluate(&view, xpath_str) {
        Ok(value) => Ok(xpath_value_to_term(env, value, &view)),
        Err(e) => Ok((atoms::error(), e).encode(env)),
    }
}

/// Execute XPath query returning XML strings for node sets (fast path)
#[rustler::nif]
fn xpath_query_raw<'a>(
    env: Env<'a>,
    doc_ref: IndexedDocumentRef,
    xpath_str: &str,
) -> NifResult<Term<'a>> {
    use term::nodeset_to_xml_binaries;
    use xpath::XPathValue;

    let view = doc_ref.as_view();
    match evaluate(&view, xpath_str) {
        Ok(value) => match value {
            XPathValue::NodeSet(nodes) => Ok(nodeset_to_xml_binaries(env, &nodes, &view)),
            _ => Ok(xpath_value_to_term(env, value, &view)),
        },
        Err(e) => Ok((atoms::error(), e).encode(env)),
    }
}

/// Execute XPath query returning text values for node sets (optimized fast path)
/// Instead of building recursive element tuples ({:element, name, attrs, children}),
/// extracts text content directly as strings. Used when is_value: true.
#[rustler::nif]
fn xpath_text_list<'a>(
    env: Env<'a>,
    doc_ref: IndexedDocumentRef,
    xpath_str: &str,
) -> NifResult<Term<'a>> {
    use xpath::XPathValue;

    let view = doc_ref.as_view();
    match evaluate(&view, xpath_str) {
        Ok(XPathValue::NodeSet(nodes)) => {
            let mut list = Term::list_new_empty(env);
            for &id in nodes.iter().rev() {
                let text = dom::node_string_value(&view, id);
                let binary = term::bytes_to_binary(env, text.as_bytes());
                list = list.list_prepend(binary);
            }
            Ok(list)
        }
        Ok(value) => Ok(xpath_value_to_term(env, value, &view)),
        Err(e) => Ok((atoms::error(), e).encode(env)),
    }
}

/// Parse and immediately query (convenience function)
#[rustler::nif(schedule = "DirtyCpu")]
fn parse_and_xpath<'a>(env: Env<'a>, input: Binary<'a>, xpath_str: &str) -> NifResult<Term<'a>> {
    let bytes = input.as_slice();
    let idx = index::builder::build_index(bytes);
    let view = index::IndexedDocumentView::new(&idx, bytes);

    match evaluate(&view, xpath_str) {
        Ok(value) => Ok(xpath_value_to_term(env, value, &view)),
        Err(e) => Ok((atoms::error(), e).encode(env)),
    }
}

/// Parse and immediately query, returning text values for node sets
/// Optimized path for is_value: true — avoids building element tuples
#[rustler::nif(schedule = "DirtyCpu")]
fn parse_and_xpath_text<'a>(
    env: Env<'a>,
    input: Binary<'a>,
    xpath_str: &str,
) -> NifResult<Term<'a>> {
    use xpath::XPathValue;

    let bytes = input.as_slice();
    let idx = index::builder::build_index(bytes);
    let view = index::IndexedDocumentView::new(&idx, bytes);

    match evaluate(&view, xpath_str) {
        Ok(XPathValue::NodeSet(nodes)) => {
            let mut list = Term::list_new_empty(env);
            for &id in nodes.iter().rev() {
                let text = dom::node_string_value(&view, id);
                let binary = term::bytes_to_binary(env, text.as_bytes());
                list = list.list_prepend(binary);
            }
            Ok(list)
        }
        Ok(value) => Ok(xpath_value_to_term(env, value, &view)),
        Err(e) => Ok((atoms::error(), e).encode(env)),
    }
}

/// Get root element of a document
#[rustler::nif]
fn get_root<'a>(env: Env<'a>, doc_ref: IndexedDocumentRef) -> NifResult<Term<'a>> {
    let view = doc_ref.as_view();
    if let Some(root_id) = view.root_element_id() {
        Ok(node_to_term(env, &view, root_id))
    } else {
        Ok(atoms::nil().encode(env))
    }
}

// ============================================================================
// XPath Helpers
// ============================================================================

/// Execute parent XPath and evaluate subspecs for each result node
#[rustler::nif(schedule = "DirtyCpu")]
fn xpath_with_subspecs<'a>(
    env: Env<'a>,
    input: Binary<'a>,
    parent_xpath: &str,
    subspecs: Vec<(&str, &str)>,
) -> NifResult<Term<'a>> {
    use xpath::evaluate_from_node;

    let bytes = input.as_slice();
    let idx = index::builder::build_index(bytes);
    let view = index::IndexedDocumentView::new(&idx, bytes);

    let parent_result = match evaluate(&view, parent_xpath) {
        Ok(v) => v,
        Err(e) => {
            return Ok((atoms::error(), e).encode(env));
        }
    };

    let nodes = match parent_result {
        xpath::XPathValue::NodeSet(nodes) => nodes,
        _ => return Ok(Term::list_new_empty(env)),
    };

    let mut result_list = Term::list_new_empty(env);

    for &node_id in nodes.iter().rev() {
        let mut map_pairs: Vec<(Term, Term)> = Vec::new();

        for (key, subxpath) in &subspecs {
            let key_term = (*key).encode(env);
            let sub_result = match evaluate_from_node(&view, node_id, subxpath) {
                Ok(v) => xpath_value_to_term(env, v, &view),
                Err(_) => atoms::nil().encode(env),
            };
            map_pairs.push((key_term, sub_result));
        }

        if let Ok(map) = Term::map_from_pairs(env, &map_pairs) {
            result_list = result_list.list_prepend(map);
        }
    }

    Ok(result_list)
}

/// Get string value of an XPath result
#[rustler::nif(schedule = "DirtyCpu")]
fn xpath_string_value<'a>(env: Env<'a>, input: Binary<'a>, xpath_str: &str) -> NifResult<Term<'a>> {
    let bytes = input.as_slice();
    let idx = index::builder::build_index(bytes);
    let view = index::IndexedDocumentView::new(&idx, bytes);

    match evaluate(&view, xpath_str) {
        Ok(value) => {
            let string_val = match value {
                xpath::XPathValue::String(s) => s,
                xpath::XPathValue::Number(n) => n.to_string(),
                xpath::XPathValue::Boolean(b) => b.to_string(),
                xpath::XPathValue::NodeSet(nodes) => {
                    if let Some(&node_id) = nodes.first() {
                        dom::node_string_value(&view, node_id)
                    } else {
                        String::new()
                    }
                }
                xpath::XPathValue::StringList(list) => list.into_iter().next().unwrap_or_default(),
            };
            Ok(string_val.encode(env))
        }
        Err(e) => Ok((atoms::error(), e).encode(env)),
    }
}

/// Get string value from document reference
#[rustler::nif]
fn xpath_string_value_doc<'a>(
    env: Env<'a>,
    doc_ref: IndexedDocumentRef,
    xpath_str: &str,
) -> NifResult<Term<'a>> {
    let view = doc_ref.as_view();
    match evaluate(&view, xpath_str) {
        Ok(value) => {
            let string_val = match value {
                xpath::XPathValue::String(s) => s,
                xpath::XPathValue::Number(n) => n.to_string(),
                xpath::XPathValue::Boolean(b) => b.to_string(),
                xpath::XPathValue::NodeSet(nodes) => {
                    if let Some(&node_id) = nodes.first() {
                        dom::node_string_value(&view, node_id)
                    } else {
                        String::new()
                    }
                }
                xpath::XPathValue::StringList(list) => list.into_iter().next().unwrap_or_default(),
            };
            Ok(string_val.encode(env))
        }
        Err(e) => Ok((atoms::error(), e).encode(env)),
    }
}


// ============================================================================
// Streaming Parser
// ============================================================================

/// Create a new streaming parser
#[rustler::nif]
fn streaming_new() -> StreamingParserRef {
    ResourceArc::new(StreamingParserResource::new())
}

/// Create a streaming parser with tag filter
#[rustler::nif]
fn streaming_new_with_filter(tag: Binary) -> StreamingParserRef {
    ResourceArc::new(StreamingParserResource::with_filter(tag.as_slice()))
}

/// Feed a chunk of data to the streaming parser
#[rustler::nif]
fn streaming_feed<'a>(
    env: Env<'a>,
    parser: StreamingParserRef,
    chunk: Binary,
) -> NifResult<Term<'a>> {
    match parser.inner.lock() {
        Ok(mut inner) => {
            inner.feed(chunk.as_slice());
            Ok((inner.available_events(), inner.buffer_size()).encode(env))
        }
        Err(_) => Ok((atoms::error(), atoms::mutex_poisoned()).encode(env)),
    }
}

/// Take up to `max` events from the streaming parser
#[rustler::nif]
fn streaming_take_events<'a>(
    env: Env<'a>,
    parser: StreamingParserRef,
    max: usize,
) -> NifResult<Term<'a>> {
    match parser.inner.lock() {
        Ok(mut inner) => {
            let events = inner.take_events(max);
            Ok(events_to_term(env, events))
        }
        Err(_) => Ok((atoms::error(), atoms::mutex_poisoned()).encode(env)),
    }
}

/// Take up to `max` complete elements from the streaming parser
#[rustler::nif]
fn streaming_take_elements<'a>(
    env: Env<'a>,
    parser: StreamingParserRef,
    max: usize,
) -> NifResult<Term<'a>> {
    match parser.inner.lock() {
        Ok(mut inner) => {
            let elements = inner.take_elements(max);

            let mut list = Term::list_new_empty(env);
            for element in elements.into_iter().rev() {
                let binary = term::bytes_to_binary(env, &element);
                list = list.list_prepend(binary);
            }
            Ok(list)
        }
        Err(_) => Ok((atoms::error(), atoms::mutex_poisoned()).encode(env)),
    }
}

/// Get number of available complete elements
#[rustler::nif]
fn streaming_available_elements<'a>(
    env: Env<'a>,
    parser: StreamingParserRef,
) -> NifResult<Term<'a>> {
    match parser.inner.lock() {
        Ok(inner) => Ok(inner.available_elements().encode(env)),
        Err(_) => Ok((atoms::error(), atoms::mutex_poisoned()).encode(env)),
    }
}

/// Finalize the streaming parser
#[rustler::nif]
fn streaming_finalize<'a>(env: Env<'a>, parser: StreamingParserRef) -> NifResult<Term<'a>> {
    match parser.inner.lock() {
        Ok(mut inner) => {
            let events = inner.finalize();
            Ok(events_to_term(env, events))
        }
        Err(_) => Ok((atoms::error(), atoms::mutex_poisoned()).encode(env)),
    }
}

/// Get streaming parser status
#[rustler::nif]
fn streaming_status<'a>(env: Env<'a>, parser: StreamingParserRef) -> NifResult<Term<'a>> {
    match parser.inner.lock() {
        Ok(inner) => Ok((
            inner.available_events(),
            inner.buffer_size(),
            inner.has_pending(),
        )
            .encode(env)),
        Err(_) => Ok((atoms::error(), atoms::mutex_poisoned()).encode(env)),
    }
}

// ============================================================================
// SimpleForm Parsing
// ============================================================================

/// Parse XML directly into SimpleForm {name, attrs, children} tree
///
/// Bypasses SAX event pipeline entirely — builds the tree in Rust from the
/// structural index, decoding entities as needed. Returns {:ok, tree} or
/// {:error, reason}.
#[rustler::nif(schedule = "DirtyCpu")]
fn parse_to_simple_form<'a>(env: Env<'a>, input: Binary<'a>) -> NifResult<Term<'a>> {
    let bytes = input.as_slice().to_vec();

    // Handle encoding conversion (UTF-16 → UTF-8)
    let bytes = match crate::core::encoding::convert_to_utf8(bytes) {
        Ok(b) => b,
        Err(msg) => return Ok((atoms::error(), msg).encode(env)),
    };

    // Validate strict
    match dom::validate_strict(&bytes) {
        Ok(()) => {}
        Err(msg) => return Ok((atoms::error(), msg).encode(env)),
    }

    // Build index
    let idx = index::builder::build_index(&bytes);

    // Build SimpleForm from root element
    match idx.root {
        Some(root_idx) => {
            let tree = term::node_to_simple_form_term(env, &idx, &bytes, root_idx);
            Ok((atoms::ok(), tree).encode(env))
        }
        None => Ok((atoms::error(), "empty document").encode(env)),
    }
}

// ============================================================================
// Document Accumulator (Streaming SimpleForm)
// ============================================================================

/// Create a new document accumulator
#[rustler::nif]
fn accumulator_new() -> DocumentAccumulatorRef {
    ResourceArc::new(resource::DocumentAccumulator::new())
}

/// Feed a chunk to the accumulator
#[rustler::nif]
fn accumulator_feed(acc: DocumentAccumulatorRef, chunk: Binary) -> rustler::Atom {
    acc.feed(chunk.as_slice());
    atoms::ok()
}

/// Validate, index, and convert accumulated data to SimpleForm
#[rustler::nif(schedule = "DirtyCpu")]
fn accumulator_to_simple_form<'a>(
    env: Env<'a>,
    acc: DocumentAccumulatorRef,
) -> NifResult<Term<'a>> {
    let bytes = acc.take_buffer();

    // Handle encoding conversion (UTF-16 → UTF-8)
    let bytes = match crate::core::encoding::convert_to_utf8(bytes) {
        Ok(b) => b,
        Err(msg) => return Ok((atoms::error(), msg).encode(env)),
    };

    // Validate strict
    match dom::validate_strict(&bytes) {
        Ok(()) => {}
        Err(msg) => return Ok((atoms::error(), msg).encode(env)),
    }

    // Build index
    let idx = index::builder::build_index(&bytes);

    // Build SimpleForm from root element
    match idx.root {
        Some(root_idx) => {
            let tree = term::node_to_simple_form_term(env, &idx, &bytes, root_idx);
            Ok((atoms::ok(), tree).encode(env))
        }
        None => Ok((atoms::error(), "empty document").encode(env)),
    }
}

// ============================================================================
// SAX Parsing
// ============================================================================

/// Parse XML and return SAX events
#[rustler::nif(schedule = "DirtyCpu")]
fn sax_parse<'a>(env: Env<'a>, input: Binary<'a>) -> NifResult<Term<'a>> {
    use core::unified_scanner::UnifiedScanner;
    use sax::SaxCollector;

    let bytes = input.as_slice();

    let mut collector = SaxCollector::new();
    let mut scanner = UnifiedScanner::new(bytes);
    scanner.scan(&mut collector);

    let events = collector.events();
    let attrs = collector.attributes();

    let mut list = Term::list_new_empty(env);

    for event in events.iter().rev() {
        let term = sax_event_to_term(env, event, attrs, bytes);
        list = list.list_prepend(term);
    }

    Ok(list)
}

/// Convert a SAX event to an Elixir term
fn sax_event_to_term<'a>(
    env: Env<'a>,
    event: &sax::CompactSaxEvent,
    attrs: &[(u32, u32, u32, u32)],
    input: &[u8],
) -> Term<'a> {
    use sax::CompactSaxEvent;

    match event.tag {
        CompactSaxEvent::TAG_START_ELEMENT => {
            let name = span_to_binary(env, event.offset as usize, event.len as usize, input);

            let attr_start = event.tertiary as usize;
            let attr_count = event.secondary as usize;
            let mut attr_list = Term::list_new_empty(env);

            if let Some(attr_slice) = attrs.get(attr_start..attr_start + attr_count) {
                for &(no, nl, vo, vl) in attr_slice.iter().rev() {
                    let attr_name = span_to_binary(env, no as usize, nl as usize, input);
                    let attr_value = span_to_binary(env, vo as usize, vl as usize, input);
                    attr_list = attr_list.list_prepend((attr_name, attr_value).encode(env));
                }
            }

            (term::start_element(), name, attr_list).encode(env)
        }
        CompactSaxEvent::TAG_END_ELEMENT => {
            let name = span_to_binary(env, event.offset as usize, event.len as usize, input);
            (term::end_element(), name).encode(env)
        }
        CompactSaxEvent::TAG_TEXT => {
            let content = if event.needs_decode() {
                let raw = &input[event.offset as usize..(event.offset + event.len) as usize];
                let decoded = crate::core::entities::decode_text(raw);
                match decoded {
                    std::borrow::Cow::Borrowed(_) => {
                        span_to_binary(env, event.offset as usize, event.len as usize, input)
                    }
                    std::borrow::Cow::Owned(bytes) => term::bytes_to_binary(env, &bytes),
                }
            } else {
                span_to_binary(env, event.offset as usize, event.len as usize, input)
            };
            (term::text(), content).encode(env)
        }
        CompactSaxEvent::TAG_CDATA => {
            let content = span_to_binary(env, event.offset as usize, event.len as usize, input);
            (term::cdata(), content).encode(env)
        }
        CompactSaxEvent::TAG_COMMENT => {
            let content = span_to_binary(env, event.offset as usize, event.len as usize, input);
            (term::comment(), content).encode(env)
        }
        CompactSaxEvent::TAG_PI => {
            let target = span_to_binary(env, event.offset as usize, event.len as usize, input);
            let data = if event.tertiary > 0 {
                span_to_binary(
                    env,
                    event.secondary as usize,
                    event.tertiary as usize,
                    input,
                )
            } else {
                term::bytes_to_binary(env, b"")
            };
            (term::processing_instruction(), target, data).encode(env)
        }
        _ => atoms::nil().encode(env),
    }
}

/// Create a binary from a span in the input
fn span_to_binary<'a>(env: Env<'a>, offset: usize, len: usize, input: &[u8]) -> Term<'a> {
    if offset + len <= input.len() {
        term::bytes_to_binary(env, &input[offset..offset + len])
    } else {
        term::bytes_to_binary(env, b"")
    }
}

// ============================================================================
// SAX Parsing — Saxy Format (Tier 3)
// ============================================================================

/// Parse XML and return SAX events in Saxy-compatible format
///
/// Events are emitted in Saxy format:
///   {:start_element, {name, attrs}}
///   {:end_element, name}
///   {:characters, content}
///   {:cdata, content}  (or {:characters, content} if cdata_as_chars)
///
/// Comments and PIs are skipped. Empty elements emit start+end.
#[rustler::nif(schedule = "DirtyCpu")]
fn sax_parse_saxy<'a>(
    env: Env<'a>,
    input: Binary<'a>,
    cdata_as_chars: bool,
) -> NifResult<Term<'a>> {
    use core::unified_scanner::UnifiedScanner;
    use sax::SaxCollector;

    let bytes = input.as_slice();

    let mut collector = SaxCollector::new();
    let mut scanner = UnifiedScanner::new(bytes);
    scanner.scan(&mut collector);

    let events = collector.events();
    let attrs = collector.attributes();

    // Forward pass to track element depth — skip document-level text
    // (whitespace before/after root element, matching Saxy behavior)
    let mut terms: Vec<Term<'a>> = Vec::with_capacity(events.len());
    let mut depth: u32 = 0;

    for event in events.iter() {
        use sax::CompactSaxEvent;
        match event.tag {
            CompactSaxEvent::TAG_START_ELEMENT => {
                depth += 1;
                sax_event_to_saxy_terms(env, event, attrs, bytes, cdata_as_chars, &mut terms);
            }
            CompactSaxEvent::TAG_END_ELEMENT => {
                sax_event_to_saxy_terms(env, event, attrs, bytes, cdata_as_chars, &mut terms);
                depth = depth.saturating_sub(1);
            }
            CompactSaxEvent::TAG_TEXT | CompactSaxEvent::TAG_CDATA => {
                if depth > 0 {
                    sax_event_to_saxy_terms(env, event, attrs, bytes, cdata_as_chars, &mut terms);
                }
                // Skip text/cdata at document level (depth == 0)
            }
            _ => {
                sax_event_to_saxy_terms(env, event, attrs, bytes, cdata_as_chars, &mut terms);
            }
        }
    }

    // Build list in reverse for O(1) prepend
    let mut list = Term::list_new_empty(env);
    for t in terms.into_iter().rev() {
        list = list.list_prepend(t);
    }

    Ok(list)
}

/// Take events from streaming parser in Saxy-compatible format
#[rustler::nif]
fn streaming_take_saxy_events<'a>(
    env: Env<'a>,
    parser: StreamingParserRef,
    max: usize,
    cdata_as_chars: bool,
) -> NifResult<Term<'a>> {
    match parser.inner.lock() {
        Ok(mut inner) => {
            let events = inner.take_events(max);

            // Convert OwnedXmlEvent to Saxy format
            let mut list = Term::list_new_empty(env);
            for event in events.into_iter().rev() {
                owned_event_to_saxy_terms(env, event, cdata_as_chars, &mut list);
            }
            Ok(list)
        }
        Err(_) => Ok((atoms::error(), atoms::mutex_poisoned()).encode(env)),
    }
}

/// Convert a compact SAX event to Saxy-format term(s), pushing to output vec.
/// May push 0, 1, or 2 terms (empty elements emit start+end via the collector).
fn sax_event_to_saxy_terms<'a>(
    env: Env<'a>,
    event: &sax::CompactSaxEvent,
    attrs: &[(u32, u32, u32, u32)],
    input: &[u8],
    cdata_as_chars: bool,
    out: &mut Vec<Term<'a>>,
) {
    use sax::CompactSaxEvent;

    match event.tag {
        CompactSaxEvent::TAG_START_ELEMENT => {
            let name = span_to_binary(env, event.offset as usize, event.len as usize, input);

            let attr_start = event.tertiary as usize;
            let attr_count = event.secondary as usize;
            let mut attr_list = Term::list_new_empty(env);

            if let Some(attr_slice) = attrs.get(attr_start..attr_start + attr_count) {
                for &(no, nl, vo, vl) in attr_slice.iter().rev() {
                    let attr_name = span_to_binary(env, no as usize, nl as usize, input);
                    let attr_value = decode_and_binary(env, input, vo as usize, vl as usize);
                    attr_list = attr_list.list_prepend((attr_name, attr_value).encode(env));
                }
            }

            // Saxy format: {:start_element, {name, attrs}}
            out.push((term::start_element(), (name, attr_list)).encode(env));
        }
        CompactSaxEvent::TAG_END_ELEMENT => {
            let name = span_to_binary(env, event.offset as usize, event.len as usize, input);
            out.push((term::end_element(), name).encode(env));
        }
        CompactSaxEvent::TAG_TEXT => {
            let content = if event.needs_decode() {
                let raw = &input[event.offset as usize..(event.offset + event.len) as usize];
                let decoded = crate::core::entities::decode_text(raw);
                match decoded {
                    std::borrow::Cow::Borrowed(_) => {
                        span_to_binary(env, event.offset as usize, event.len as usize, input)
                    }
                    std::borrow::Cow::Owned(bytes) => term::bytes_to_binary(env, &bytes),
                }
            } else {
                span_to_binary(env, event.offset as usize, event.len as usize, input)
            };
            // Saxy format: {:characters, content}
            out.push((term::characters(), content).encode(env));
        }
        CompactSaxEvent::TAG_CDATA => {
            let content = span_to_binary(env, event.offset as usize, event.len as usize, input);
            if cdata_as_chars {
                out.push((term::characters(), content).encode(env));
            } else {
                out.push((term::cdata(), content).encode(env));
            }
        }
        // Skip comments and PIs — Saxy doesn't emit them
        CompactSaxEvent::TAG_COMMENT | CompactSaxEvent::TAG_PI => {}
        _ => {}
    }
}

/// Convert an OwnedXmlEvent to Saxy format terms, prepended to list
fn owned_event_to_saxy_terms<'a>(
    env: Env<'a>,
    event: crate::strategy::streaming::OwnedXmlEvent,
    cdata_as_chars: bool,
    list: &mut Term<'a>,
) {
    use crate::strategy::streaming::OwnedXmlEvent;

    match event {
        OwnedXmlEvent::StartElement { name, attributes } => {
            let name_term = term::bytes_to_binary(env, &name);
            let mut attrs = Term::list_new_empty(env);
            for (k, v) in attributes.into_iter().rev() {
                let tuple = (
                    term::bytes_to_binary(env, &k),
                    term::bytes_to_binary(env, &v),
                );
                attrs = attrs.list_prepend(tuple.encode(env));
            }
            *list = list.list_prepend((term::start_element(), (name_term, attrs)).encode(env));
        }
        OwnedXmlEvent::EndElement { name } => {
            let name_term = term::bytes_to_binary(env, &name);
            *list = list.list_prepend((term::end_element(), name_term).encode(env));
        }
        OwnedXmlEvent::EmptyElement { name, attributes } => {
            let name_term = term::bytes_to_binary(env, &name);
            let mut attrs = Term::list_new_empty(env);
            for (k, v) in attributes.into_iter().rev() {
                let tuple = (
                    term::bytes_to_binary(env, &k),
                    term::bytes_to_binary(env, &v),
                );
                attrs = attrs.list_prepend(tuple.encode(env));
            }
            // Empty element → start + end
            // Note: prepending in reverse order since we're building from the back
            *list = list.list_prepend((term::end_element(), name_term).encode(env));
            *list = list.list_prepend((term::start_element(), (name_term, attrs)).encode(env));
        }
        OwnedXmlEvent::Text(content) => {
            *list = list.list_prepend(
                (term::characters(), term::bytes_to_binary(env, &content)).encode(env),
            );
        }
        OwnedXmlEvent::CData(content) => {
            if cdata_as_chars {
                *list = list.list_prepend(
                    (term::characters(), term::bytes_to_binary(env, &content)).encode(env),
                );
            } else {
                *list = list.list_prepend(
                    (term::cdata(), term::bytes_to_binary(env, &content)).encode(env),
                );
            }
        }
        // Skip comments and PIs
        OwnedXmlEvent::Comment(_) | OwnedXmlEvent::ProcessingInstruction { .. } => {}
    }
}

/// Decode entities in a span and return as binary
fn decode_and_binary<'a>(env: Env<'a>, input: &[u8], offset: usize, len: usize) -> Term<'a> {
    if offset + len <= input.len() {
        let raw = &input[offset..offset + len];
        let decoded = crate::core::entities::decode_text(raw);
        match decoded {
            std::borrow::Cow::Borrowed(b) => term::bytes_to_binary(env, b),
            std::borrow::Cow::Owned(bytes) => term::bytes_to_binary(env, &bytes),
        }
    } else {
        term::bytes_to_binary(env, b"")
    }
}

// ============================================================================
// Streaming SAX Parsing (chunk-by-chunk, bounded memory)
// ============================================================================

/// Create a new streaming SAX parser
#[rustler::nif]
fn streaming_sax_new() -> StreamingSaxParserRef {
    ResourceArc::new(StreamingSaxParserResource::new())
}

/// Feed a chunk and return SAX events as a compact binary.
///
/// Two key optimizations for minimal NIF peak memory:
///
/// 1. **Zero-copy tokenization**: when the tail buffer is empty (common case),
///    the BEAM binary is tokenized in-place — no 64 KB copy into Rust.
/// 2. **Direct BEAM binary encoding**: events are written directly into an
///    `OwnedBinary` via `BinaryWriter` — no intermediate Rust `Vec<u8>`.
///
/// Only the unprocessed tail (~100 bytes at a chunk boundary) is saved in
/// the persistent buffer, which is shrunk to at most 1 KB after each call.
///
/// Combined NIF + BEAM peak is ~67 KB for a 2.93 MB document (64 KB chunks).
///
/// Binary format:
///   start_element: <<1, name_len::16, name, attr_count::16, [nlen::16, name, vlen::16, value]*>>
///   end_element:   <<2, name_len::16, name>>
///   characters:    <<3, text_len::32, text>>
///   cdata:         <<4, text_len::32, text>>
#[rustler::nif]
fn streaming_feed_sax<'a>(
    env: Env<'a>,
    parser: StreamingSaxParserRef,
    chunk: Binary,
    cdata_as_chars: bool,
) -> NifResult<Term<'a>> {
    use core::entities::decode_text;
    use core::tokenizer::{TokenKind, Tokenizer};
    use strategy::streaming::find_safe_boundary;

    let mut inner = parser
        .inner
        .lock()
        .map_err(|_| rustler::Error::Term(Box::new(atoms::mutex_poisoned())))?;

    // Zero-copy fast path: if the persistent buffer is empty (common case),
    // tokenize directly from the BEAM binary without copying 64 KB into Rust.
    // Only when there IS leftover tail bytes do we concatenate.
    let had_tail = !inner.buffer.is_empty();
    if had_tail {
        inner.buffer.extend_from_slice(chunk.as_slice());
    }

    // Scope all reads from `input` (which may alias inner.buffer) so the
    // immutable borrow ends before we mutate inner.depth / inner.buffer.
    let (depth, tail, buf) = {
        let input: &[u8] = if had_tail {
            &inner.buffer
        } else {
            chunk.as_slice()
        };

        let boundary = find_safe_boundary(input);
        if boundary == 0 {
            // Nothing processable yet — save everything as the tail.
            if !had_tail {
                inner.buffer.extend_from_slice(chunk.as_slice());
            }
            return Ok(empty_binary(env));
        }

        let mut depth = inner.depth;
        let mut buf = BinaryWriter::new(chunk.len().max(256))?;
        let processable = &input[..boundary];
        let mut tokenizer = Tokenizer::new(processable);

        while let Some(token) = tokenizer.next_token() {
            match token.kind {
                TokenKind::Eof => break,

                TokenKind::StartTag => {
                    depth += 1;
                    if let Some(name) = token.name {
                        buf.push(1);
                        encode_bytes(&mut buf, name.as_ref());
                        encode_attrs(&mut buf, processable, token.span);
                    }
                }

                TokenKind::EndTag => {
                    if let Some(name) = token.name {
                        buf.push(2);
                        encode_bytes(&mut buf, name.as_ref());
                    }
                    depth = depth.saturating_sub(1);
                }

                TokenKind::EmptyTag => {
                    if let Some(name) = token.name {
                        buf.push(1);
                        encode_bytes(&mut buf, name.as_ref());
                        encode_attrs(&mut buf, processable, token.span);
                        buf.push(2);
                        encode_bytes(&mut buf, name.as_ref());
                    }
                }

                TokenKind::Text => {
                    if depth > 0 {
                        if let Some(content) = token.content {
                            if !content.is_empty() {
                                let decoded = decode_text(content.as_ref());
                                buf.push(3);
                                encode_content(&mut buf, decoded.as_ref());
                            }
                        }
                    }
                }

                TokenKind::CData => {
                    if depth > 0 {
                        if let Some(content) = token.content {
                            if cdata_as_chars {
                                buf.push(3);
                            } else {
                                buf.push(4);
                            }
                            encode_content(&mut buf, content.as_ref());
                        }
                    }
                }

                _ => {}
            }
        }

        let tail = input[boundary..].to_vec();
        (depth, tail, buf)
    };
    // immutable borrow of inner.buffer is now released.

    inner.depth = depth;

    // Save only the unprocessed tail (typically ~100 bytes).
    inner.buffer.clear();
    if !tail.is_empty() {
        inner.buffer.extend_from_slice(&tail);
    }
    inner.buffer.shrink_to(1024);

    buf.into_term(env)
}

/// Process remaining bytes in the buffer after all chunks have been fed.
#[rustler::nif]
fn streaming_finalize_sax<'a>(
    env: Env<'a>,
    parser: StreamingSaxParserRef,
    cdata_as_chars: bool,
) -> NifResult<Term<'a>> {
    use core::entities::decode_text;
    use core::tokenizer::{TokenKind, Tokenizer};

    let mut inner = parser
        .inner
        .lock()
        .map_err(|_| rustler::Error::Term(Box::new(atoms::mutex_poisoned())))?;

    if inner.buffer.is_empty() {
        return Ok(empty_binary(env));
    }

    let remaining = std::mem::take(&mut inner.buffer);
    let mut depth = inner.depth;
    let mut buf = BinaryWriter::new(remaining.len().max(256))?;
    let mut tokenizer = Tokenizer::new(&remaining);

    while let Some(token) = tokenizer.next_token() {
        match token.kind {
            TokenKind::Eof => break,

            TokenKind::StartTag => {
                depth += 1;
                if let Some(name) = token.name {
                    buf.push(1);
                    encode_bytes(&mut buf, name.as_ref());
                    encode_attrs(&mut buf, &remaining, token.span);
                }
            }

            TokenKind::EndTag => {
                if let Some(name) = token.name {
                    buf.push(2);
                    encode_bytes(&mut buf, name.as_ref());
                }
                depth = depth.saturating_sub(1);
            }

            TokenKind::EmptyTag => {
                if let Some(name) = token.name {
                    buf.push(1);
                    encode_bytes(&mut buf, name.as_ref());
                    encode_attrs(&mut buf, &remaining, token.span);
                    buf.push(2);
                    encode_bytes(&mut buf, name.as_ref());
                }
            }

            TokenKind::Text => {
                if depth > 0 {
                    if let Some(content) = token.content {
                        if !content.is_empty() {
                            let decoded = decode_text(content.as_ref());
                            buf.push(3);
                            encode_content(&mut buf, decoded.as_ref());
                        }
                    }
                }
            }

            TokenKind::CData => {
                if depth > 0 {
                    if let Some(content) = token.content {
                        if cdata_as_chars {
                            buf.push(3);
                        } else {
                            buf.push(4);
                        }
                        encode_content(&mut buf, content.as_ref());
                    }
                }
            }

            _ => {}
        }
    }

    inner.depth = depth;

    buf.into_term(env)
}

// --- BinaryWriter: write directly into OwnedBinary ---

/// Growable writer backed by an OwnedBinary (BEAM heap).
///
/// Events are encoded directly into the BEAM binary — no intermediate
/// Rust Vec allocation. The binary is over-allocated with an estimate
/// and trimmed to exact size before returning to the BEAM.
struct BinaryWriter {
    bin: rustler::OwnedBinary,
    pos: usize,
}

impl BinaryWriter {
    /// Allocate an OwnedBinary with `capacity` bytes.
    fn new(capacity: usize) -> Result<Self, rustler::Error> {
        let bin = rustler::OwnedBinary::new(capacity)
            .ok_or_else(|| rustler::Error::Term(Box::new("alloc_failed")))?;
        Ok(BinaryWriter { bin, pos: 0 })
    }

    /// Ensure at least `additional` bytes are available, reallocating if needed.
    #[inline]
    fn reserve(&mut self, additional: usize) {
        let needed = self.pos + additional;
        if needed > self.bin.len() {
            // Double or jump to needed, whichever is larger.
            let new_cap = std::cmp::max(self.bin.len() * 2, needed);
            self.bin.realloc_or_copy(new_cap);
        }
    }

    /// Append a single byte.
    #[inline]
    fn push(&mut self, byte: u8) {
        self.reserve(1);
        self.bin.as_mut_slice()[self.pos] = byte;
        self.pos += 1;
    }

    /// Append a byte slice.
    #[inline]
    fn extend(&mut self, data: &[u8]) {
        self.reserve(data.len());
        self.bin.as_mut_slice()[self.pos..self.pos + data.len()].copy_from_slice(data);
        self.pos += data.len();
    }

    /// Trim to written size and release as a BEAM binary term.
    fn into_term<'a>(mut self, env: Env<'a>) -> NifResult<Term<'a>> {
        if self.pos < self.bin.len() {
            self.bin.realloc_or_copy(self.pos);
        }
        Ok(self.bin.release(env).encode(env))
    }
}

// --- Binary encoding helpers ---

/// Encode a name/short string: <<len::16, bytes>>
#[inline]
fn encode_bytes(buf: &mut BinaryWriter, data: &[u8]) {
    buf.extend(&(data.len() as u16).to_be_bytes());
    buf.extend(data);
}

/// Encode text content: <<len::32, bytes>>
#[inline]
fn encode_content(buf: &mut BinaryWriter, data: &[u8]) {
    buf.extend(&(data.len() as u32).to_be_bytes());
    buf.extend(data);
}

/// Encode attributes from a tag span into the buffer.
fn encode_attrs(buf: &mut BinaryWriter, input: &[u8], span: (usize, usize)) {
    use core::attributes::parse_attributes;

    let (start, end) = span;
    if end <= start || end > input.len() {
        buf.extend(&0u16.to_be_bytes());
        return;
    }

    let tag_content = &input[start..end];

    // Skip '<' and optional '/' or '?'
    let mut pos = 1;
    if tag_content.get(1) == Some(&b'/') || tag_content.get(1) == Some(&b'?') {
        pos = 2;
    }

    // Skip tag name
    while pos < tag_content.len() {
        let b = tag_content[pos];
        if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' || b == b'>' || b == b'/' {
            break;
        }
        pos += 1;
    }

    // Find end of attributes
    let mut attr_end = tag_content.len();
    if tag_content.ends_with(b"/>") || tag_content.ends_with(b"?>") {
        attr_end -= 2;
    } else if tag_content.ends_with(b">") {
        attr_end -= 1;
    }

    if pos >= attr_end {
        buf.extend(&0u16.to_be_bytes());
        return;
    }

    let attrs = parse_attributes(&tag_content[pos..attr_end]);
    buf.extend(&(attrs.len() as u16).to_be_bytes());
    for attr in &attrs {
        encode_bytes(buf, attr.name.as_ref());
        encode_bytes(buf, attr.value.as_ref());
    }
}

/// Return an empty BEAM binary.
#[inline]
fn empty_binary<'a>(env: Env<'a>) -> Term<'a> {
    // OwnedBinary::new(0) should never fail, but avoid panicking in a NIF.
    match rustler::OwnedBinary::new(0) {
        Some(owned) => owned.release(env).encode(env),
        None => "".encode(env),
    }
}

// ============================================================================
// NIF Initialization
// ============================================================================

rustler::init!("Elixir.RustyXML.Native");
