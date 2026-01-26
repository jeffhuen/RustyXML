# RustyXML Architecture

A purpose-built Rust NIF for ultra-fast XML parsing in Elixir. Not a wrapper around an existing library—custom-built from the ground up for optimal BEAM integration with full XPath 1.0 support.

## Key Innovations

### Purpose-Built, Not Wrapped

Unlike projects that wrap existing Rust crates (like quick-xml or roxmltree), RustyXML is **designed specifically for Elixir**:

- **Direct BEAM term construction** - Results go straight to Erlang terms, no intermediate serialization
- **ResourceArc integration** - DOM documents and streaming parser state managed by BEAM's garbage collector
- **Dirty scheduler awareness** - Parallel XPath operations run on dirty CPU schedulers
- **Zero-copy where possible** - `Cow<[u8]>` borrows data, only allocates for entity decoding
- **Arena-based DOM** - Cache-friendly node storage with `u32` NodeId indices

### Five Parsing Strategies

RustyXML offers unmatched flexibility with five parsing strategies:

| Strategy | Innovation |
|----------|------------|
| Event Parser | SIMD-accelerated tokenization via `memchr` crate |
| DOM Parser | Arena allocation with string interning for memory efficiency |
| Streaming Parser | Stateful parser with bounded memory for multi-GB files |
| XPath Query | Full XPath 1.0 with all 13 axes and 27+ functions |
| Parallel XPath | Multi-threaded query evaluation via `rayon` on dirty schedulers |

### Memory Efficiency

- **Arena-based DOM** - Nodes stored contiguously with `u32` indices instead of pointers
- **String interning** - Repeated tag/attribute names stored once
- **Streaming bounded memory** - Process 10GB+ files with configurable buffer size
- **mimalloc allocator** - High-performance allocator for reduced fragmentation
- **Optional memory tracking** - Opt-in profiling with zero overhead when disabled

### Validated Correctness

- **180+ tests** covering W3C XML 1.0, XPath 1.0, edge cases, and conformance
- **Cross-strategy validation** - All strategies produce consistent output
- **SweetXml compatibility** - Verified identical behavior for common API patterns

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

## Parsing Strategies

### Strategy A: Event Parser (`parse_events/1`)

Zero-copy event-based parsing using SIMD-accelerated tokenization.

```elixir
RustyXML.Native.parse_events("<root><item/></root>")
#=> [{:start_element, "root", []}, {:empty_element, "item", []}, {:end_element, "root"}]
```

**Best for:** Event-driven processing, when you don't need a full DOM

**Events:**
- `{:start_element, name, attributes}` - Opening tag
- `{:end_element, name}` - Closing tag
- `{:empty_element, name, attributes}` - Self-closing tag
- `{:text, content}` - Text content
- `{:cdata, content}` - CDATA section
- `{:comment, content}` - XML comment
- `{:processing_instruction, target, data}` - Processing instruction

### Strategy B: DOM Parser (`parse/1`)

Arena-based DOM construction with string interning for efficient memory usage.

```elixir
doc = RustyXML.parse("<root><item id=\"1\"/></root>")
RustyXML.xpath(doc, ~x"//item/@id"s)
#=> "1"
```

**Best for:** Multiple XPath queries on the same document

**Architecture:**
- Arena allocation stores nodes contiguously for cache-friendly access
- `NodeId` (u32) indices instead of pointers reduce memory and improve cache performance
- String pool interns repeated tag/attribute names
- Document wrapped in `ResourceArc` for automatic memory management

### Strategy C: Direct XPath (`xpath/2` with raw XML)

Parse and query in a single call when only one query is needed.

```elixir
RustyXML.xpath("<root><item/></root>", ~x"//item"l)
```

**Best for:** Single-query scenarios, avoids persistent document reference

### Strategy D: Streaming Parser (`stream_tags/3`)

SweetXml-compatible streaming interface for processing large files with bounded memory.

```elixir
# High-level API (recommended)
"large_file.xml"
|> RustyXML.stream_tags(:item)
|> Stream.each(fn {:item, item_xml} ->
  # Each item_xml is a complete XML string
  name = RustyXML.xpath(item_xml, ~x"./name/text()"s)
  IO.puts("Processing: #{name}")
end)
|> Stream.run()

# Works with Stream.take (no hanging like SweetXml issue #97)
"large_file.xml"
|> RustyXML.stream_tags(:item)
|> Stream.take(10)
|> Enum.to_list()

# Low-level API (for custom streaming)
parser = RustyXML.Native.streaming_new_with_filter("item")
RustyXML.Native.streaming_feed(parser, chunk1)
RustyXML.Native.streaming_feed(parser, chunk2)
events = RustyXML.Native.streaming_take_events(parser, 100)
remaining = RustyXML.Native.streaming_finalize(parser)
```

**Best for:** Large files (GB+), network streams, memory-constrained environments

**Features:**
- Returns `{tag_atom, xml_string}` tuples compatible with SweetXml
- Complete XML elements that can be queried with `xpath/2`
- Maintains parse state across chunks
- Handles elements split across chunk boundaries
- Tag filtering emits only matching elements and their children
- Preserves all content including whitespace
- Wrapped in `ResourceArc` for BEAM garbage collection
- Does NOT hang with `Stream.take` (fixes SweetXml issue #97)

### Strategy E: Parallel XPath (`xpath_parallel/2`)

Execute multiple XPath queries in parallel using Rayon thread pool.

```elixir
doc = RustyXML.parse(large_xml)
results = RustyXML.Native.xpath_parallel(doc, ["//item", "//price", "//title"])
#=> [items, prices, titles]
```

**Best for:** Batch queries, xmap with many keys

**Implementation:**
- Uses `DirtyCpu` scheduler to avoid blocking BEAM schedulers
- Rayon provides work-stealing parallelism
- Queries share the immutable DOM reference

---

## Project Structure

```
native/rustyxml/src/
├── lib.rs                 # NIF entry points, memory tracking, mimalloc
├── core/
│   ├── mod.rs            # Re-exports
│   ├── scanner.rs        # SIMD byte scanning (memchr)
│   ├── tokenizer.rs      # State machine tokenizer
│   ├── entities.rs       # Entity decoding with Cow
│   └── attributes.rs     # Attribute parsing
├── reader/
│   ├── mod.rs            # Reader exports
│   ├── slice.rs          # Strategy A: Zero-copy slice parser
│   ├── buffered.rs       # Buffer-based reader for streams
│   └── events.rs         # XML event types
├── dom/
│   ├── mod.rs            # DOM exports
│   ├── document.rs       # Arena-based document structure
│   ├── node.rs           # Node types with NodeId (u32)
│   ├── strings.rs        # String interning pool
│   └── namespace.rs      # Namespace resolution stack
├── xpath/
│   ├── mod.rs            # XPath exports
│   ├── lexer.rs          # XPath tokenizer
│   ├── parser.rs         # Recursive descent parser
│   ├── compiler.rs       # Expression compiler
│   ├── eval.rs           # Evaluation engine
│   ├── axes.rs           # All 13 XPath axes
│   ├── functions.rs      # 27+ XPath 1.0 functions
│   └── value.rs          # XPath value types
├── strategy/
│   ├── mod.rs            # Strategy exports
│   ├── streaming.rs      # Strategy D: Stateful streaming parser
│   └── parallel.rs       # Strategy E: Parallel XPath (DirtyCpu)
├── term.rs               # BEAM term building utilities
└── resource.rs           # ResourceArc wrappers

lib/
├── rusty_xml.ex          # Main module with xpath/2, xmap/2, stream_tags/3, sigil
├── rusty_xml/
│   ├── native.ex         # NIF bindings (RustlerPrecompiled)
│   └── streaming.ex      # High-level streaming interface (stream_tags/3)
```

---

## Implementation Details

### SIMD-Accelerated Scanning

Tag and content boundary detection uses `memchr` for hardware-accelerated scanning:

```rust
use memchr::{memchr, memchr2, memchr3};

// Find next tag start - SIMD accelerated
fn find_tag_start(input: &[u8], pos: usize) -> Option<usize> {
    memchr(b'<', &input[pos..]).map(|i| pos + i)
}

// Find tag end with quote handling - parallel search
fn find_tag_end(input: &[u8], pos: usize) -> Option<usize> {
    // memchr2 finds '>' or '"' in parallel using SIMD
    // State machine handles quote contexts
}

// Content scanning for entities and markup
fn find_content_break(input: &[u8], pos: usize) -> Option<usize> {
    memchr3(b'<', b'&', b']', &input[pos..])
}
```

**SIMD support:** SSE2 (x86_64 default), AVX2 (runtime detect), NEON (aarch64), simd128 (wasm)

### State Machine Tokenizer

The tokenizer uses an efficient state machine for XML parsing:

```rust
enum ParseState {
    Init,           // Before first content
    InsideText,     // Between tags, collecting text
    InsideMarkup,   // Inside <...>
    InsideRef,      // Inside &...;
    Done,           // EOF reached
}

enum TokenKind {
    StartTag,       // <element
    EndTag,         // </element>
    EmptyTag,       // <element/>
    Text,           // Character data
    CData,          // <![CDATA[...]]>
    Comment,        // <!--...-->
    ProcessingInstruction, // <?target data?>
    Declaration,    // <?xml version="1.0"?>
    Doctype,        // <!DOCTYPE ...>
    Eof,
}
```

### Arena-Based DOM

The DOM uses arena allocation for cache-friendly traversal:

- **NodeId** (`u32`) indices instead of 64-bit pointers for cache efficiency and 50% memory savings
- **XmlDocument** holds the node arena, attribute arena, string pool, and root element reference
- **XmlNode** contains kind, parent/child/sibling links, name/namespace IDs, attribute range, and text span
- **StringPool** interns repeated tag/attribute names
- Text content accessed via spans into original input (zero-copy when possible)

### Zero-Copy with Cow

Entity decoding uses `Cow<[u8]>` for optimal allocation:

```rust
pub fn decode_text(input: &[u8]) -> Cow<'_, [u8]> {
    // Fast path: SIMD check for '&'
    if memchr(b'&', input).is_none() {
        return Cow::Borrowed(input);  // Zero-copy!
    }
    // Slow path: decode entities
    Cow::Owned(decode_entities(input))
}
```

### XPath 1.0 Engine

Full XPath 1.0 implementation with recursive descent parsing:

- **All 13 axes**: child, parent, self, attribute, descendant, descendant-or-self, ancestor, ancestor-or-self, following, following-sibling, preceding, preceding-sibling, namespace
- **27+ functions**: position, last, count, local-name, namespace-uri, name, string, concat, starts-with, contains, substring, substring-before, substring-after, string-length, normalize-space, translate, boolean, not, true, false, lang, number, sum, floor, ceiling, round
- **Predicates**: Full predicate support with position, boolean, and comparison expressions
- **Operators**: Arithmetic (+, -, *, div, mod), comparison (=, !=, <, >, <=, >=), logical (and, or)

### Streaming Parser State

The streaming parser maintains state across chunks:

- **Buffer** for accumulated input from partial elements
- **Complete elements queue** for yielding finished XML strings
- **Element builder** tracks in-progress element capture across chunk boundaries
- **Tag filter** emits only matching elements and their children
- **Depth tracking** knows when a target element is complete
- Wrapped in `ResourceArc` for BEAM memory management

---

## Performance Optimizations

RustyXML achieves its speed through several key optimizations:

### Bypassing BEAM Term Construction

For element queries, building nested Elixir tuples (`{:element, name, attrs, children}`) is expensive—1000 elements with 5 children = 6000 recursive term constructions.

`xpath_query_raw/2` bypasses this entirely by serializing nodes to XML strings in Rust. The serialization uses an iterative approach with an explicit stack (not recursion) to safely handle arbitrarily deep XML without stack overflow.

**Result:** 1.39x faster than SweetXml for element queries.

### Complete Elements Streaming

The streaming parser returns complete XML elements directly from Rust rather than individual events. This eliminates the need for Elixir-side event reconstruction.

**Result:** 3.87x faster streaming than SweetXml.

### Compile-Time Atoms

Atom lookup is expensive at runtime. We use `rustler::atoms!` to pre-define atoms (`:element`, `:text`, etc.) at compile time, eliminating per-call overhead.

### Direct Binary Encoding

Strings are encoded using `NewBinary` for direct memory copy instead of going through Rustler's encoder machinery.

### O(1) Document Access

The `DocumentAccess` trait allows XPath evaluation on pre-parsed documents without re-parsing. `XmlDocumentView` borrows from `OwnedXmlDocument` with just pointer assignments.

### O(n) Node Set Operations

XPath union/intersection operations use `HashSet` for O(n) deduplication instead of naive O(n²) contains-checks.

### Summary

| Optimization | Impact |
|--------------|--------|
| XML string serialization | 1.39x faster element queries |
| Complete elements streaming | 3.87x faster streaming |
| Compile-time atoms | Eliminates per-call atom lookup |
| Direct binary encoding | Faster string→term conversion |
| DocumentAccess trait | O(1) pre-parsed access |
| HashSet deduplication | O(n²) → O(n) for node sets |

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
- 10-20% faster allocation for many small objects (nodes, strings)
- Reduced fragmentation in arena patterns
- No tracking overhead in default configuration

### Optional Memory Tracking

For profiling, enable the `memory_tracking` feature:

```toml
# In native/rustyxml/Cargo.toml
[features]
default = ["mimalloc", "memory_tracking"]
```

When enabled, these functions return actual values:
- `RustyXML.Native.get_rust_memory/0` - Current allocation
- `RustyXML.Native.get_rust_memory_peak/0` - Peak allocation
- `RustyXML.Native.reset_rust_memory_stats/0` - Reset and get stats

### Pre-allocated Vectors

All parsing paths pre-allocate vectors with capacity estimates:

```rust
// Nodes: estimate 1 node per 50 bytes
let nodes = Vec::with_capacity(input.len() / 50);

// Strings: estimate 20 unique names
let strings = StringPool::with_capacity(20);

// Events: estimate based on document structure
let events = Vec::with_capacity(estimated_events);
```

---

## NIF Safety

### The 1ms Rule

NIFs should complete in under 1ms to avoid blocking schedulers.

| Approach | Used By | Description |
|----------|---------|-------------|
| Dirty Schedulers | `xpath_parallel` | Runs on separate dirty CPU scheduler |
| Chunked Processing | `streaming_*` | Returns control between chunks |
| Stateful Resource | `streaming_*` | Lets Elixir control iteration |
| Fast SIMD | all strategies | Completes quickly via hardware acceleration |

### Memory Safety

- DOM documents wrapped in `ResourceArc` with automatic cleanup
- Streaming parsers use `Mutex<StreamingParser>` for thread safety
- String pool uses safe indices with bounds checking
- All allocations tracked when memory_tracking enabled

---

## The `~x` Sigil

The sigil creates XPath expressions with modifiers:

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

## SweetXml Compatibility

RustyXML provides a SweetXml-compatible API:

| Function | Description | Status |
|----------|-------------|--------|
| `xpath/2,3` | Execute XPath query | Complete |
| `xmap/2,3` | Extract multiple values | Complete |
| `~x` sigil | XPath with modifiers | Complete |
| `stream_tags/2,3` | Stream specific tags | Complete |

### Migration from SweetXml

```elixir
# Before
import SweetXml
doc |> xpath(~x"//item"l)

# After
import RustyXML
doc |> xpath(~x"//item"l)
```

---

## Benchmark Results

See [BENCHMARK.md](BENCHMARK.md) for detailed performance comparisons.

**Summary:**
- **DOM parsing**: 10-50x faster than SweetXml/xmerl
- **XPath queries**: 5-20x faster than SweetXml
- **Streaming**: Bounded memory for GB+ files
- **Parallel XPath**: Near-linear scaling with query count

---

## Compliance & Validation

RustyXML is validated against industry-standard test suites:

- **W3C XML 1.0 (Fifth Edition)** - Lenient parser, accepts all valid XML (218/218 tests)
- **XPath 1.0 Specification** - Full axis and function support (13 axes, 27+ functions)
- **W3C/OASIS Conformance Suite** - 100% of valid document tests pass

**Note**: RustyXML is a lenient parser - it accepts all valid XML correctly but does not reject malformed XML. This is a deliberate trade-off for real-world usability.

See [COMPLIANCE.md](COMPLIANCE.md) for full details including test suite instructions.

---

## References

- [W3C XML 1.0 (Fifth Edition)](https://www.w3.org/TR/xml/) - XML specification
- [XPath 1.0](https://www.w3.org/TR/xpath-10/) - XPath specification
- [OASIS XML Conformance](https://www.oasis-open.org/committees/xml-conformance/) - Test suite
- [memchr crate](https://docs.rs/memchr/latest/memchr/) - SIMD byte searching
- [rayon crate](https://docs.rs/rayon/latest/rayon/) - Parallel iteration
- [mimalloc](https://github.com/microsoft/mimalloc) - High-performance allocator
- [SweetXml](https://github.com/kbrw/sweet_xml) - Elixir XML library (compatibility target)
