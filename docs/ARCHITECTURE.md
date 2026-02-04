# RustyXML Architecture

A purpose-built Rust NIF for ultra-fast XML parsing in Elixir. Not a wrapper around an existing library—custom-built from the ground up for optimal BEAM integration with full XPath 1.0 support. Drop-in replacement for both SweetXml and Saxy.

## Key Innovations

### Purpose-Built, Not Wrapped

Unlike projects that wrap existing Rust crates (like quick-xml or roxmltree), RustyXML is **designed specifically for Elixir**:

- **Direct BEAM term construction** — Results go straight to Erlang terms, no intermediate serialization
- **ResourceArc integration** — Documents and streaming parser state managed by BEAM's garbage collector
- **Dirty scheduler awareness** — All raw-XML parse NIFs run on dirty CPU schedulers
- **Zero-copy where possible** — Span-based references into original input, only allocates for entity decoding
- **Structural index** — Cache-friendly storage with compact span structs and flat arrays

### Unified Architecture

RustyXML v0.2.0 consolidated multiple parsing strategies into a single optimized path: the **structural index**. A single `UnifiedScanner` tokenizes input once, dispatching to a `ScanHandler` trait that builds the appropriate representation:

| Path | Description | Best For |
|------|-------------|----------|
| `parse/1` + `xpath/2` | Structural index with XPath | General XML processing |
| `stream_tags/3` | Bounded-memory streaming | Large files (GB+) |
| `sax_parse/1` | SAX event collection | Event-driven processing |

All three paths share the same SIMD-accelerated scanner and well-formedness validation.

### Memory Efficiency

- **Structural index** — Elements stored as compact span structs (32 bytes each) referencing the original input
- **Zero-copy strings** — Tag names, attribute values, and text stored as `(offset, length)` spans
- **Sub-binary returns** — BEAM sub-binaries share memory with the original input
- **Streaming bounded memory** — Process 10GB+ files with configurable buffer size
- **mimalloc allocator** — High-performance allocator for reduced fragmentation
- **Optional memory tracking** — Opt-in profiling with zero overhead when disabled

### Validated Correctness

- **100% W3C/OASIS XML Conformance** — All 1089 applicable tests pass (218 valid + 871 not-well-formed rejections), verified individually against the official [xmlconf](https://www.w3.org/XML/Test/) suite
- **1296+ tests** including the full conformance suite, batch accessor clamping, and lazy XPath coverage
- **Cross-path validation** — All paths produce consistent output
- **SweetXml compatibility** — Verified identical behavior for common API patterns

---

## Quick Start

```elixir
import RustyXML

xml = """
<catalog>
  <book id="1"><title>Elixir in Action</title><price>45.00</price></book>
  <book id="2"><title>Programming Phoenix</title><price>50.00</price></book>
</catalog>
"""

# Get all books
RustyXML.xpath(xml, ~x"//book"l)

# Get text content
RustyXML.xpath(xml, ~x"//title/text()"s)

# Extract multiple values
RustyXML.xmap(xml, [
  titles: ~x"//title/text()"sl,
  prices: ~x"//price/text()"sl
])
```

---

## Core Architecture

### UnifiedScanner and ScanHandler

The `UnifiedScanner` is the single entry point for all XML tokenization. It uses `memchr`-based SIMD scanning to find delimiters, then dispatches events through the `ScanHandler` trait:

```
XML Input
   |
   v
UnifiedScanner (memchr SIMD tokenization)
   |
   +---> IndexBuilder (ScanHandler) ---> StructuralIndex ---> XPath
   |
   +---> SaxCollector (ScanHandler) ---> SAX Events
   |
   +---> StreamingParser ---> Complete Elements
```

The `ScanHandler` trait:

```rust
trait ScanHandler {
    fn start_element(&mut self, name: Span, attrs: &[(Span, Span)], is_empty: bool);
    fn end_element(&mut self, name: Span);
    fn text(&mut self, span: Span, needs_entity_decode: bool);
    fn cdata(&mut self, span: Span);
    fn comment(&mut self, span: Span);
    fn processing_instruction(&mut self, target: Span, data: Option<Span>);
}
```

Adding a new processing mode requires only implementing the trait—no changes to the scanner.

### Structural Index

The structural index is the core document representation. Instead of building a DOM tree with string copies, it stores compact structs that reference byte offsets into the original input:

```rust
struct Span {
    offset: u32,
    len: u16,     // 6 bytes total
}

struct IndexElement {      // 32 bytes
    name: Span,
    ns_prefix: Option<Span>,
    parent: u32,
    children: Range<u32>,  // into flat children_data array
    attrs: Range<u32>,     // into flat attrs array
}

struct IndexText {         // 16 bytes
    span: Span,
    parent: u32,
    needs_entity_decode: bool,
}

struct IndexAttribute {    // 12 bytes
    name: Span,
    value: Span,
}
```

**Memory profile for 2.93 MB document:**
- Structural index: **12.8 MB** (4.4x input size)
- Old DOM approach: **30.2 MB** (10.3x input size)
- SweetXml/xmerl: allocated entirely on BEAM heap

The `IndexedDocumentView` implements the `DocumentAccess` trait, allowing the XPath engine to evaluate queries on the structural index without any conversion step.

### SIMD-Accelerated Scanning

Tag and content boundary detection uses `memchr` for hardware-accelerated scanning:

```rust
use memchr::{memchr, memchr2, memchr3};

// Find next tag start — SIMD accelerated
fn find_tag_start(input: &[u8], pos: usize) -> Option<usize> {
    memchr(b'<', &input[pos..]).map(|i| pos + i)
}

// Content scanning for entities and markup
fn find_content_break(input: &[u8], pos: usize) -> Option<usize> {
    memchr3(b'<', b'&', b']', &input[pos..])
}
```

**SIMD support:** SSE2 (x86_64 default), AVX2 (runtime detect), NEON (aarch64), simd128 (wasm)

---

## Parsing

### Standard Parse (`parse/1`)

All parsing flows through the structural index:

```elixir
doc = RustyXML.parse("<root><item id=\"1\"/></root>")
RustyXML.xpath(doc, ~x"//item/@id"s)
#=> "1"
```

**Best for:** Multiple XPath queries on the same document.

**Architecture:**
- `UnifiedScanner` tokenizes input with SIMD-accelerated scanning
- `IndexBuilder` collects spans into a `StructuralIndex`
- Document wrapped in `ResourceArc` for BEAM garbage collection
- XPath queries operate on the structural index via `DocumentAccess` trait

### Direct XPath (`xpath/2` with raw XML)

Parse and query in a single call:

```elixir
RustyXML.xpath("<root><item/></root>", ~x"//item"l)
```

**Best for:** Single-query scenarios, avoids persistent document reference.

### Streaming Parser (`stream_tags/3`)

Bounded-memory streaming for large files:

```elixir
# High-level API
"large_file.xml"
|> RustyXML.stream_tags(:item)
|> Stream.each(fn {:item, item_xml} ->
  name = RustyXML.xpath(item_xml, ~x"./name/text()"s)
  IO.puts("Processing: #{name}")
end)
|> Stream.run()

# Works with Stream.take (no hanging like SweetXml issue #97)
"large_file.xml"
|> RustyXML.stream_tags(:item)
|> Stream.take(10)
|> Enum.to_list()
```

**Best for:** Large files (GB+), network streams, memory-constrained environments.

**Features:**
- Returns `{tag_atom, xml_string}` tuples compatible with SweetXml
- Complete XML elements that can be queried with `xpath/2`
- Handles elements split across chunk boundaries
- Tag filtering emits only matching elements and their children
- Does NOT hang with `Stream.take` (fixes SweetXml issue #97)

### SAX Parser (`sax_parse/1`)

Event-based parsing for custom processing:

```elixir
events = RustyXML.Native.sax_parse(xml)
# Returns list of SAX events: start_element, end_element, text, etc.
```

**Best for:** Event-driven processing, custom document handling.

### Lazy XPath (`xpath_lazy/2`)

Keep XPath results in Rust memory, access on-demand:

```elixir
doc = RustyXML.parse(large_xml)

# Execute query — returns reference, not data
result = RustyXML.Native.xpath_lazy(doc, "//item")

# Access count without building terms (3x faster than regular XPath)
count = RustyXML.Native.result_count(result)

# Batch accessors for multiple items
texts = RustyXML.Native.result_texts(result, 0, 10)
ids = RustyXML.Native.result_attrs(result, "id", 0, 10)

# Extract multiple fields at once
data = RustyXML.Native.result_extract(result, 0, 10, ["id", "category"], true)
#=> [%{:name => "item", :text => "...", "id" => "1", "category" => "cat1"}, ...]
```

**Best for:** Large result sets, partial access, count-only queries.

### Parallel XPath (`xpath_parallel/2`)

Execute multiple XPath queries concurrently using Rayon:

```elixir
doc = RustyXML.parse(large_xml)
results = RustyXML.Native.xpath_parallel(doc, ["//item", "//price", "//title"])
```

**Best for:** Batch queries, `xmap` with many keys.

---

## XPath 1.0 Engine

Full XPath 1.0 implementation with recursive descent parsing:

- **All 13 axes**: child, parent, self, attribute, descendant, descendant-or-self, ancestor, ancestor-or-self, following, following-sibling, preceding, preceding-sibling, namespace
- **27+ functions**: position, last, count, local-name, namespace-uri, name, string, concat, starts-with, contains, substring, substring-before, substring-after, string-length, normalize-space, translate, boolean, not, true, false, lang, number, sum, floor, ceiling, round
- **Predicates**: Full predicate support with position, boolean, and comparison expressions
- **Operators**: Arithmetic (+, -, *, div, mod), comparison (=, !=, <, >, <=, >=), logical (and, or)

### Expression Caching

Compiled XPath expressions are cached in an LRU cache (256 entries). Repeated queries skip parsing and compilation entirely.

### Fast-Path Predicates

Common predicate patterns are optimized:

- `[@attr='value']` → `PredicateAttrEq` (direct attribute lookup)
- `[n]` → `PredicatePosition` (index access, no iteration)

### Text Extraction Fast Path

For text extraction queries, `xpath_text_list` extracts text directly from NodeSets without building recursive BEAM element tuples—eliminating the double-walk where tuples were built then discarded.

---

## Project Structure

```
native/rustyxml/src/
├── lib.rs                 # NIF entry points, memory tracking, mimalloc
├── core/
│   ├── mod.rs             # Re-exports
│   ├── scanner.rs         # SIMD byte scanning (memchr)
│   ├── unified_scanner.rs # UnifiedScanner + ScanHandler trait
│   ├── tokenizer.rs       # State machine tokenizer
│   ├── entities.rs        # Entity decoding with Cow
│   └── attributes.rs      # Attribute parsing
├── index/
│   ├── mod.rs             # Module docs, re-exports
│   ├── structural.rs      # StructuralIndex (main data structure)
│   ├── span.rs            # Span struct (offset, length)
│   ├── element.rs         # IndexElement, IndexText, IndexAttribute
│   ├── builder.rs         # IndexBuilder (ScanHandler impl)
│   └── view.rs            # IndexedDocumentView (DocumentAccess impl)
├── dom/
│   ├── mod.rs             # DocumentAccess trait, validation
│   ├── document.rs        # Document types
│   ├── node.rs            # Node types
│   └── strings.rs         # String utilities
├── xpath/
│   ├── mod.rs             # XPath exports
│   ├── lexer.rs           # XPath tokenizer
│   ├── parser.rs          # Recursive descent parser
│   ├── compiler.rs        # Expression compiler
│   ├── eval.rs            # Evaluation engine
│   ├── axes.rs            # All 13 XPath axes
│   ├── functions.rs       # 27+ XPath 1.0 functions
│   └── value.rs           # XPath value types
├── sax/
│   ├── mod.rs             # SAX module docs
│   ├── events.rs          # CompactSaxEvent types
│   └── collector.rs       # SaxCollector (ScanHandler impl)
├── strategy/
│   ├── mod.rs             # Strategy exports
│   ├── streaming.rs       # Stateful streaming parser
│   └── parallel.rs        # Parallel XPath (DirtyCpu)
├── term.rs                # BEAM term building utilities
└── resource.rs            # ResourceArc wrappers

lib/
├── rusty_xml.ex           # Main module: xpath/2, xmap/2, stream_tags/3, parse_string/4,
│                          #   parse_stream/4, stream_events/2, encode!/2, ~x sigil
├── rusty_xml/
│   ├── native.ex          # NIF bindings (RustlerPrecompiled)
│   ├── streaming.ex       # High-level streaming interface
│   ├── handler.ex         # SAX handler behaviour (= Saxy.Handler)
│   ├── event_transformer.ex # Native event → Saxy event mapping
│   ├── partial.ex         # Incremental SAX parsing (= Saxy.Partial)
│   ├── simple_form.ex     # Tuple tree output (= Saxy.SimpleForm)
│   ├── xml.ex             # Builder DSL (= Saxy.XML)
│   ├── encoder.ex         # XML string encoding
│   └── builder.ex         # Struct→XML protocol (= Saxy.Builder)
```

---

## Performance Optimizations

| Optimization | Impact |
|--------------|--------|
| Structural index (zero-copy spans) | 65-70% memory reduction vs old DOM |
| XPath text fast path | 0.74x → 1.44x faster text extraction |
| XML string serialization | 1.39x faster element queries |
| Complete elements streaming | 3.87x faster streaming |
| Lazy XPath API | 3x faster for partial access |
| XPath expression caching | Skip re-parsing repeated queries |
| Fast-path predicates | 23% faster for `[@attr='value']` |
| Compile-time atoms | Eliminates per-call atom lookup |
| Direct binary encoding | Faster string-to-term conversion |
| DocumentAccess trait | O(1) pre-parsed access |
| HashSet deduplication | O(n^2) → O(n) for node sets |

### Bypassing BEAM Term Construction

For element queries, building nested Elixir tuples (`{:element, name, attrs, children}`) is expensive. `xpath_query_raw/2` bypasses this by serializing nodes to XML strings in Rust using an iterative approach with an explicit stack.

### Lazy XPath

The regular XPath API builds BEAM terms for all results upfront. The lazy API keeps results in Rust memory as `Vec<NodeId>`:

```elixir
# Regular API: builds 1000 BEAM tuples immediately
items = RustyXML.xpath(doc, "//item")  # 104ms

# Lazy API: keeps node IDs in Rust, builds terms on-demand
result = RustyXML.Native.xpath_lazy(doc, "//item")  # 31ms
count = RustyXML.Native.result_count(result)  # instant
```

### Zero-Copy with Cow

Entity decoding uses `Cow<[u8]>` for optimal allocation:

```rust
pub fn decode_text(input: &[u8]) -> Cow<'_, [u8]> {
    if memchr(b'&', input).is_none() {
        return Cow::Borrowed(input);  // Zero-copy!
    }
    Cow::Owned(decode_entities(input))
}
```

---

## Memory Management

### mimalloc Allocator

RustyXML uses [mimalloc](https://github.com/microsoft/mimalloc) as the default allocator:

```rust
#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
```

**Benefits:**
- 10-20% faster allocation for many small objects
- Reduced fragmentation
- No tracking overhead in default configuration

### Optional Memory Tracking

For profiling, enable the `memory_tracking` feature:

```toml
# In native/rustyxml/Cargo.toml
[features]
default = ["mimalloc", "memory_tracking"]
```

When enabled:
- `RustyXML.Native.get_rust_memory/0` — Current allocation
- `RustyXML.Native.get_rust_memory_peak/0` — Peak allocation
- `RustyXML.Native.reset_rust_memory_stats/0` — Reset and get stats

### Pre-allocated Vectors

All parsing paths pre-allocate vectors with capacity estimates based on input size, reducing reallocation overhead during parsing.

---

## NIF Safety

### The 1ms Rule

NIFs should complete in under 1ms to avoid blocking schedulers.

| Approach | Used By | Description |
|----------|---------|-------------|
| Dirty Schedulers | `parse`, `parse_strict`, `parse_and_xpath`, `xpath_with_subspecs`, `xpath_string_value`, `sax_parse` | Runs on dirty CPU scheduler |
| Chunked Processing | `streaming_*` | Returns control between chunks |
| Stateful Resource | `streaming_*` | Lets Elixir control iteration |
| Fast SIMD | all paths | Completes quickly via hardware acceleration |

### Memory Safety

- Documents wrapped in `ResourceArc` with automatic cleanup
- Streaming parsers use `Mutex<StreamingParser>` for thread safety
- All allocations tracked when memory_tracking enabled

### Panic Safety

RustyXML is designed to never crash the BEAM VM:

- **No `.unwrap()` in NIF code paths** — All fallible operations use proper error handling
- **Pre-defined atoms** — Common atoms (`ok`, `error`, `nil`, `text`, `name`) created at compile time
- **Graceful mutex handling** — Poisoned mutexes return `{:error, :mutex_poisoned}` tuples

### Atom Table Safety

BEAM's atom table has a fixed limit (~1M atoms) and atoms are never garbage collected. RustyXML uses **binary keys** for user-provided values:

```elixir
# Safe: predefined atom keys + binary attribute keys
%{:name => "item", :text => "...", "id" => "1", "category" => "cat1"}
```

| Key Type | Implementation | Safe? |
|----------|----------------|-------|
| `:name`, `:text`, `:error` | Pre-defined atoms | Fixed set |
| User attribute names | Binary strings | No atom table impact |

---

## The `~x` Sigil

| Modifier | Effect | Example |
|----------|--------|---------|
| `s` | Return as string | `~x"//title/text()"s` |
| `l` | Return as list | `~x"//item"l` |
| `e` | Decode entities | `~x"//content"e` |
| `o` | Optional (nil on missing) | `~x"//optional"o` |
| `i` | Cast to integer | `~x"//count"i` |
| `f` | Cast to float | `~x"//price"f` |
| `k` | Return as keyword list | `~x"//item"k` |

Modifiers can be combined: `~x"//items"slo` (string, list, optional)

---

## API Compatibility

RustyXML is a drop-in replacement for both SweetXml and Saxy. Both APIs coexist with no conflicts (different arities and function names).

### SweetXml-Compatible

| Function | Description | Status |
|----------|-------------|--------|
| `xpath/2,3` | Execute XPath query | Complete |
| `xmap/2,3` | Extract multiple values | Complete |
| `~x` sigil | XPath with modifiers | Complete |
| `stream_tags/2,3` | Stream specific tags | Complete |

### Saxy-Compatible

| Function / Module | Description | Status |
|-------------------|-------------|--------|
| `parse_string/4` | SAX parsing with handler | Complete |
| `parse_stream/4` | Streaming SAX with handler | Complete |
| `stream_events/2` | Lazy stream of SAX events | Complete |
| `encode!/2` | XML encoding | Complete |
| `RustyXML.Handler` | Handler behaviour (= `Saxy.Handler`) | Complete |
| `RustyXML.Partial` | Incremental parsing (= `Saxy.Partial`) | Complete |
| `RustyXML.SimpleForm` | Tuple tree (= `Saxy.SimpleForm`) | Complete |
| `RustyXML.XML` | Builder DSL (= `Saxy.XML`) | Complete |
| `RustyXML.Builder` | Struct→XML protocol (= `Saxy.Builder`) | Complete |

### Migration

```elixir
# From SweetXml — just change the import
import RustyXML  # was: import SweetXml

# From Saxy — just change the module name
RustyXML.parse_string(xml, MyHandler, [])  # was: Saxy.parse_string(...)
RustyXML.SimpleForm.parse_string(xml)      # was: Saxy.SimpleForm.parse_string(...)
```

---

## Benchmark Results

See [BENCHMARK.md](BENCHMARK.md) for detailed performance comparisons.

**vs SweetXml:**
- **Parsing**: 8-72x faster
- **XPath queries**: 1.5-3.7x faster
- **Streaming**: 16x faster with 228x less memory
- **Memory**: 89-100x less for parsing

**vs Saxy:**
- **SAX parsing**: 1.3-1.7x faster
- **SimpleForm**: 1.4-1.5x faster
- **SAX memory**: 2-8x less

---

## Compliance & Validation

See [COMPLIANCE.md](COMPLIANCE.md) for full details.

- **W3C/OASIS Conformance Suite** — 100% compliance (1089/1089 tests pass)
- **W3C XML 1.0 (Fifth Edition)** — Full strict mode validation
- **XPath 1.0 Specification** — Full axis and function support (13 axes, 27+ functions)

---

## References

- [W3C XML 1.0 (Fifth Edition)](https://www.w3.org/TR/xml/) — XML specification
- [XPath 1.0](https://www.w3.org/TR/xpath-10/) — XPath specification
- [OASIS XML Conformance](https://www.oasis-open.org/committees/xml-conformance/) — Test suite
- [memchr crate](https://docs.rs/memchr/latest/memchr/) — SIMD byte searching
- [rayon crate](https://docs.rs/rayon/latest/rayon/) — Parallel iteration
- [mimalloc](https://github.com/microsoft/mimalloc) — High-performance allocator
- [SweetXml](https://github.com/kbrw/sweet_xml) — Elixir XML library (compatibility target)
