# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1] - 2026-01-29

### NIF Safety & BEAM Integration Hardening

Comprehensive pass to align the Rust NIF layer with BEAM best practices:
scheduler safety, explicit error propagation, input bounds checking, and
modern Rustler patterns.

- **Dirty CPU schedulers for parse NIFs** — `parse/1`, `parse_strict/1`,
  `parse_events/1`, `parse_and_xpath/2`, `xpath_with_subspecs/3`, and
  `xpath_string_value/2` now run on dirty CPU schedulers. These NIFs accept
  raw XML input whose parse time scales with input size, so they should not
  block normal BEAM schedulers.

- **Consistent mutex poison handling** — All NIFs that access a `Mutex`-protected
  resource now return `{:error, :mutex_poisoned}` when the mutex is poisoned,
  instead of silently returning `nil`, an empty list, or a zero value. Affected
  functions: `xpath_query/2`, `xpath_query_raw/2`, `xpath_lazy/2`,
  `result_text/2`, `result_attr/3`, `result_name/2`, `result_node/2`,
  `result_texts/3`, `result_attrs/4`, `result_extract/5`, `get_root/1`,
  `xpath_string_value_doc/2`, `xpath_parallel/2`, `streaming_take_events/2`,
  `streaming_take_elements/2`, `streaming_available_elements/1`,
  `streaming_finalize/1`, and `streaming_status/1`.

- **Batch accessor overflow and DoS guard** — `result_texts/3`, `result_attrs/4`,
  and `result_extract/5` now use `saturating_add` for overflow-safe arithmetic
  and clamp the iteration range to the actual result count. This prevents both
  OOM from adversarial `count` values and CPU stalls from iterating billions of
  no-op indices. The returned list may now be shorter than `count` when the
  range extends beyond the result set (previously those trailing entries were
  `nil`). A `count` greater than or equal to the remaining results returns
  all available items from `start`.

- **Modern Rustler resource registration** — Replaced the deprecated
  `rustler::resource!` macro and `load` callback with `#[rustler::resource_impl]`
  trait implementations (Rustler 0.37+ pattern). No API or behavioral changes;
  this is an internal modernisation.

- **Dead code removal** — Removed unused `with_doc`, `node_count`, and
  `root_name` methods from `DocumentResource`.

## [0.1.0] - 2026-01-26

### Added

- **Purpose-built Rust NIF** for ultra-fast XML parsing in Elixir
- **SIMD-accelerated parsing** via `memchr` for fast delimiter detection
- **Arena-based DOM** with `NodeId` indices for cache-friendly traversal
- **Full XPath 1.0 support** with all 13 axes and 27+ functions
- **SweetXml-compatible API** with `~x` sigil and modifiers (`s`, `l`, `e`, `o`, `i`, `f`, `k`)

### Parsing Strategies

- **DOM Parser** (`parse/1` + `xpath/2`) - Parse once, query multiple times
- **Direct Query** (`xpath/2` on raw XML) - Single query, temporary DOM
- **Lazy XPath** (`Native.xpath_lazy/2`) - Keep results in Rust memory, 3x faster for large result sets
- **Streaming** (`stream_tags/3`) - Bounded memory for large files
- **Parallel XPath** (`xmap_parallel/2`) - Multi-threaded query evaluation

### XPath Features

- **All 13 axes**: child, descendant, descendant-or-self, parent, ancestor, ancestor-or-self, following, following-sibling, preceding, preceding-sibling, self, attribute, namespace
- **27+ functions**: position(), last(), count(), string(), concat(), contains(), substring(), normalize-space(), boolean(), not(), number(), sum(), floor(), ceiling(), round(), and more
- **LRU cache** for compiled XPath expressions (256 entries)
- **Fast-path predicates** for common patterns (`[@attr='value']`, `[n]`)

### Security

- **XXE immune** - External entities parsed but ignored
- **Billion Laughs immune** - Only 5 built-in entities expanded
- **DTD processing disabled** - No external DTD fetching
- **Panic-safe NIF** - No `.unwrap()` in NIF code paths
- **Atom table safe** - User-provided values use binary keys

### Conformance

- **100% OASIS/W3C XML Conformance** - 1089/1089 applicable tests pass
- **218/218** valid document tests
- **871/871** not-well-formed rejection tests

### Performance

- **8.9x faster parsing** than SweetXml
- **3x faster** lazy XPath for count-only queries
- **5x faster** combined parse + query workflows
