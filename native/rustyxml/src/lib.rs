//! RustyXML - Fast XML parsing with multiple strategies
//!
//! Strategies:
//! A: Zero-copy slice parser (parse_events)
//! B: Buffer-based reader (for streams)
//! C: DOM parser with XPath (parse, xpath)
//! D: Streaming tag parser (streaming_*)
//! E: Parallel XPath (xpath_parallel)

// Allow dead code for scaffolded modules not yet fully integrated
#![allow(dead_code)]

use rustler::{Binary, Encoder, Env, NifResult, ResourceArc, Term};

mod core;
mod dom;
mod reader;
mod resource;
mod strategy;
mod term;
mod xpath;

use dom::XmlDocument;
use resource::{StreamingParserResource, StreamingParserRef, DocumentResource, DocumentRef};
use term::{xpath_value_to_term, events_to_term, node_to_term};
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
// Strategy A: Zero-Copy Event Parser
// ============================================================================

/// Parse XML and return list of events
#[rustler::nif]
fn parse_events<'a>(env: Env<'a>, input: Binary<'a>) -> NifResult<Term<'a>> {
    let bytes = input.as_slice();
    let events: Vec<_> = reader::slice::SliceReader::new(bytes)
        .filter_map(|event| {
            // Convert to owned events for term building
            match event {
                reader::events::XmlEvent::StartElement(e) => {
                    Some(strategy::streaming::OwnedXmlEvent::StartElement {
                        name: e.name.into_owned(),
                        attributes: e.attributes.into_iter().map(|a| {
                            (a.name.into_owned(), a.value.into_owned())
                        }).collect(),
                    })
                }
                reader::events::XmlEvent::EndElement(e) => {
                    Some(strategy::streaming::OwnedXmlEvent::EndElement {
                        name: e.name.into_owned(),
                    })
                }
                reader::events::XmlEvent::EmptyElement(e) => {
                    Some(strategy::streaming::OwnedXmlEvent::EmptyElement {
                        name: e.name.into_owned(),
                        attributes: e.attributes.into_iter().map(|a| {
                            (a.name.into_owned(), a.value.into_owned())
                        }).collect(),
                    })
                }
                reader::events::XmlEvent::Text(t) => {
                    Some(strategy::streaming::OwnedXmlEvent::Text(t.into_owned()))
                }
                reader::events::XmlEvent::CData(t) => {
                    Some(strategy::streaming::OwnedXmlEvent::CData(t.into_owned()))
                }
                reader::events::XmlEvent::Comment(t) => {
                    Some(strategy::streaming::OwnedXmlEvent::Comment(t.into_owned()))
                }
                reader::events::XmlEvent::ProcessingInstruction { target, data } => {
                    Some(strategy::streaming::OwnedXmlEvent::ProcessingInstruction {
                        target: target.into_owned(),
                        data: data.map(|d| d.into_owned()).unwrap_or_default(),
                    })
                }
                _ => None,
            }
        })
        .collect();

    Ok(events_to_term(env, events))
}

// ============================================================================
// Strategy C: DOM Parser with XPath
// ============================================================================

/// Parse XML into a DOM document (returns ResourceArc)
/// Lenient mode - accepts malformed XML
#[rustler::nif]
fn parse<'a>(env: Env<'a>, input: Binary<'a>) -> NifResult<Term<'a>> {
    let bytes = input.as_slice().to_vec();
    let resource = DocumentResource::new(bytes);
    let arc = ResourceArc::new(resource);
    Ok(arc.encode(env))
}

/// Parse XML in strict mode (returns {:ok, doc} or {:error, reason})
/// Rejects malformed XML per XML 1.0 specification
#[rustler::nif]
fn parse_strict<'a>(env: Env<'a>, input: Binary<'a>) -> NifResult<Term<'a>> {
    let bytes = input.as_slice().to_vec();

    match dom::OwnedXmlDocument::parse_strict(bytes) {
        Ok(doc) => {
            let resource = DocumentResource::from_owned(doc);
            let arc = ResourceArc::new(resource);
            let ok_atom = rustler::types::atom::Atom::from_str(env, "ok").unwrap();
            Ok((ok_atom, arc).encode(env))
        }
        Err(msg) => {
            let error_atom = rustler::types::atom::Atom::from_str(env, "error").unwrap();
            Ok((error_atom, msg).encode(env))
        }
    }
}

/// Execute XPath query on a document
/// Uses with_view for O(1) access - no re-parsing!
#[rustler::nif]
fn xpath_query<'a>(env: Env<'a>, doc_ref: DocumentRef, xpath_str: &str) -> NifResult<Term<'a>> {
    let result = doc_ref.with_view(|view| {
        match evaluate(&view, xpath_str) {
            Ok(value) => xpath_value_to_term(env, value, &view),
            Err(e) => {
                let error_atom = rustler::types::atom::Atom::from_str(env, "error").unwrap();
                (error_atom, e).encode(env)
            }
        }
    });

    match result {
        Some(term) => Ok(term),
        None => {
            let nil = rustler::types::atom::nil();
            Ok(nil.encode(env))
        }
    }
}

/// Execute XPath query returning XML strings for node sets (fast path)
/// Bypasses BEAM term construction - returns list of XML binaries for elements
#[rustler::nif]
fn xpath_query_raw<'a>(env: Env<'a>, doc_ref: DocumentRef, xpath_str: &str) -> NifResult<Term<'a>> {
    use xpath::XPathValue;
    use term::nodeset_to_xml_binaries;

    let result = doc_ref.with_view(|view| {
        match evaluate(&view, xpath_str) {
            Ok(value) => match value {
                XPathValue::NodeSet(nodes) => {
                    // Fast path: serialize nodes to XML binaries
                    nodeset_to_xml_binaries(env, &nodes, &view)
                }
                // For non-node results, use regular conversion
                _ => xpath_value_to_term(env, value, &view),
            },
            Err(e) => {
                let error_atom = rustler::types::atom::Atom::from_str(env, "error").unwrap();
                (error_atom, e).encode(env)
            }
        }
    });

    match result {
        Some(term) => Ok(term),
        None => {
            let nil = rustler::types::atom::nil();
            Ok(nil.encode(env))
        }
    }
}

/// Parse and immediately query (convenience function)
#[rustler::nif]
fn parse_and_xpath<'a>(env: Env<'a>, input: Binary<'a>, xpath_str: &str) -> NifResult<Term<'a>> {
    let bytes = input.as_slice();
    let doc = XmlDocument::parse(bytes);

    match evaluate(&doc, xpath_str) {
        Ok(value) => Ok(xpath_value_to_term(env, value, &doc)),
        Err(e) => {
            let error_atom = rustler::types::atom::Atom::from_str(env, "error").unwrap();
            Ok((error_atom, e).encode(env))
        }
    }
}

/// Get root element of a document
#[rustler::nif]
fn get_root<'a>(env: Env<'a>, doc_ref: DocumentRef) -> NifResult<Term<'a>> {
    let result = doc_ref.with_view(|view| {
        if let Some(root_id) = view.root_element_id() {
            node_to_term(env, &view, root_id)
        } else {
            rustler::types::atom::nil().encode(env)
        }
    });

    match result {
        Some(term) => Ok(term),
        None => Ok(rustler::types::atom::nil().encode(env)),
    }
}

// ============================================================================
// XPath with Subspecs (for xpath/3 nesting)
// ============================================================================

/// Execute parent XPath and evaluate subspecs for each result node
/// Returns a list of maps: [%{key1: val1, key2: val2}, ...]
#[rustler::nif]
fn xpath_with_subspecs<'a>(
    env: Env<'a>,
    input: Binary<'a>,
    parent_xpath: &str,
    subspecs: Vec<(&str, &str)>,  // [(key, xpath), ...]
) -> NifResult<Term<'a>> {
    use xpath::evaluate_from_node;

    let bytes = input.as_slice();
    let doc = XmlDocument::parse(bytes);

    // Execute parent XPath
    let parent_result = match evaluate(&doc, parent_xpath) {
        Ok(v) => v,
        Err(e) => {
            let error_atom = rustler::types::atom::Atom::from_str(env, "error").unwrap();
            return Ok((error_atom, e).encode(env));
        }
    };

    // Get the node set from parent result
    let nodes = match parent_result {
        xpath::XPathValue::NodeSet(nodes) => nodes,
        _ => return Ok(Term::list_new_empty(env)),
    };

    // Build result list
    let mut result_list = Term::list_new_empty(env);

    for &node_id in nodes.iter().rev() {
        // Build a map for this node
        let mut map_pairs: Vec<(Term, Term)> = Vec::new();

        for (key, subxpath) in &subspecs {
            let key_atom = rustler::types::atom::Atom::from_str(env, key).unwrap();

            let sub_result = match evaluate_from_node(&doc, node_id, subxpath) {
                Ok(v) => xpath_value_to_term(env, v, &doc),
                Err(_) => rustler::types::atom::nil().encode(env),
            };

            map_pairs.push((key_atom.encode(env), sub_result));
        }

        let map = Term::map_from_pairs(env, &map_pairs).unwrap();
        result_list = result_list.list_prepend(map);
    }

    Ok(result_list)
}

/// Get string value of an XPath result (for `s` modifier)
/// Handles node-set by getting text content of first node
#[rustler::nif]
fn xpath_string_value<'a>(
    env: Env<'a>,
    input: Binary<'a>,
    xpath_str: &str,
) -> NifResult<Term<'a>> {
    let bytes = input.as_slice();
    let doc = XmlDocument::parse(bytes);

    match evaluate(&doc, xpath_str) {
        Ok(value) => {
            let string_val = match value {
                xpath::XPathValue::String(s) => s,
                xpath::XPathValue::Number(n) => n.to_string(),
                xpath::XPathValue::Boolean(b) => b.to_string(),
                xpath::XPathValue::NodeSet(nodes) => {
                    // Get text content of first node
                    if let Some(&node_id) = nodes.first() {
                        get_node_text_content(&doc, node_id)
                    } else {
                        String::new()
                    }
                }
                xpath::XPathValue::StringList(list) => {
                    // Return first string from list
                    list.into_iter().next().unwrap_or_default()
                }
            };
            Ok(string_val.encode(env))
        }
        Err(e) => {
            let error_atom = rustler::types::atom::Atom::from_str(env, "error").unwrap();
            Ok((error_atom, e).encode(env))
        }
    }
}

/// Get string value from document reference
/// Uses with_view for O(1) access - no re-parsing!
#[rustler::nif]
fn xpath_string_value_doc<'a>(
    env: Env<'a>,
    doc_ref: DocumentRef,
    xpath_str: &str,
) -> NifResult<Term<'a>> {
    let result = doc_ref.with_view(|view| {
        match evaluate(&view, xpath_str) {
            Ok(value) => {
                let string_val = match value {
                    xpath::XPathValue::String(s) => s,
                    xpath::XPathValue::Number(n) => n.to_string(),
                    xpath::XPathValue::Boolean(b) => b.to_string(),
                    xpath::XPathValue::NodeSet(nodes) => {
                        if let Some(&node_id) = nodes.first() {
                            get_node_text_content(&view, node_id)
                        } else {
                            String::new()
                        }
                    }
                    xpath::XPathValue::StringList(list) => {
                        list.into_iter().next().unwrap_or_default()
                    }
                };
                string_val.encode(env)
            }
            Err(e) => {
                let error_atom = rustler::types::atom::Atom::from_str(env, "error").unwrap();
                (error_atom, e).encode(env)
            }
        }
    });

    match result {
        Some(term) => Ok(term),
        None => Ok("".encode(env)),
    }
}

/// Helper to get text content of a node
fn get_node_text_content<D: dom::DocumentAccess>(doc: &D, node_id: dom::NodeId) -> String {
    use dom::NodeKind;

    let node = match doc.get_node(node_id) {
        Some(n) => n,
        None => return String::new(),
    };

    match node.kind {
        NodeKind::Text | NodeKind::CData => {
            doc.text_content(node_id).unwrap_or("").to_string()
        }
        NodeKind::Element => {
            // Concatenate all descendant text nodes
            let mut result = String::new();
            collect_text_content(doc, node_id, &mut result);
            result
        }
        NodeKind::Attribute => {
            // Attribute values are stored in name_id for virtual attribute nodes
            doc.strings().get_str(node.name_id).unwrap_or("").to_string()
        }
        _ => String::new(),
    }
}

/// Recursively collect text content from descendants
fn collect_text_content<D: dom::DocumentAccess>(doc: &D, node_id: dom::NodeId, result: &mut String) {
    use dom::NodeKind;

    for child_id in doc.children_vec(node_id) {
        let child = match doc.get_node(child_id) {
            Some(n) => n,
            None => continue,
        };

        match child.kind {
            NodeKind::Text | NodeKind::CData => {
                if let Some(text) = doc.text_content(child_id) {
                    result.push_str(text);
                }
            }
            NodeKind::Element => {
                collect_text_content(doc, child_id, result);
            }
            _ => {}
        }
    }
}

// ============================================================================
// Strategy D: Streaming Parser
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
fn streaming_feed(parser: StreamingParserRef, chunk: Binary) -> (usize, usize) {
    let mut inner = parser.inner.lock().unwrap();
    inner.feed(chunk.as_slice());
    (inner.available_events(), inner.buffer_size())
}

/// Take up to `max` events from the streaming parser
#[rustler::nif]
fn streaming_take_events<'a>(
    env: Env<'a>,
    parser: StreamingParserRef,
    max: usize,
) -> NifResult<Term<'a>> {
    let mut inner = parser.inner.lock().unwrap();
    let events = inner.take_events(max);
    Ok(events_to_term(env, events))
}

/// Take up to `max` complete elements from the streaming parser
/// Returns list of XML binaries - faster than event-based reconstruction
#[rustler::nif]
fn streaming_take_elements<'a>(
    env: Env<'a>,
    parser: StreamingParserRef,
    max: usize,
) -> NifResult<Term<'a>> {
    let mut inner = parser.inner.lock().unwrap();
    let elements = inner.take_elements(max);

    // Convert Vec<Vec<u8>> to Elixir list of binaries
    let mut list = Term::list_new_empty(env);
    for element in elements.into_iter().rev() {
        let binary = term::bytes_to_binary(env, &element);
        list = list.list_prepend(binary);
    }
    Ok(list)
}

/// Get number of available complete elements
#[rustler::nif]
fn streaming_available_elements(parser: StreamingParserRef) -> usize {
    let inner = parser.inner.lock().unwrap();
    inner.available_elements()
}

/// Finalize the streaming parser
#[rustler::nif]
fn streaming_finalize<'a>(env: Env<'a>, parser: StreamingParserRef) -> NifResult<Term<'a>> {
    let mut inner = parser.inner.lock().unwrap();
    let events = inner.finalize();
    Ok(events_to_term(env, events))
}

/// Get streaming parser status
#[rustler::nif]
fn streaming_status(parser: StreamingParserRef) -> (usize, usize, bool) {
    let inner = parser.inner.lock().unwrap();
    (
        inner.available_events(),
        inner.buffer_size(),
        inner.has_pending(),
    )
}

// ============================================================================
// Strategy E: Parallel XPath
// ============================================================================

/// Execute multiple XPath queries in parallel
/// Uses with_view for O(1) access - no re-parsing!
#[rustler::nif(schedule = "DirtyCpu")]
fn xpath_parallel<'a>(
    env: Env<'a>,
    doc_ref: DocumentRef,
    xpaths: Vec<&str>,
) -> NifResult<Term<'a>> {
    let result = doc_ref.with_view(|view| {
        let results = strategy::parallel::evaluate_parallel(&view, &xpaths);

        let mut list = Term::list_new_empty(env);
        for result in results.into_iter().rev() {
            let term = match result {
                Ok(value) => xpath_value_to_term(env, value, &view),
                Err(e) => {
                    let error_atom = rustler::types::atom::Atom::from_str(env, "error").unwrap();
                    (error_atom, e).encode(env)
                }
            };
            list = list.list_prepend(term);
        }
        list
    });

    match result {
        Some(term) => Ok(term),
        None => Ok(Term::list_new_empty(env)),
    }
}

// ============================================================================
// NIF Initialization
// ============================================================================

#[allow(non_local_definitions)]
fn load(env: Env, _info: Term) -> bool {
    let _ = rustler::resource!(StreamingParserResource, env);
    let _ = rustler::resource!(DocumentResource, env);
    true
}

rustler::init!("Elixir.RustyXML.Native", load = load);
