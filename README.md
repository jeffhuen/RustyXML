# RustyXML

**Ultra-fast XML parsing for Elixir.** A purpose-built Rust NIF with SIMD acceleration, arena-based DOM, and full XPath 1.0 support. Drop-in replacement for SweetXml.

[![Hex.pm](https://img.shields.io/hexpm/v/rusty_xml.svg)](https://hex.pm/packages/rusty_xml)

## Features

- **SIMD-accelerated parsing** via memchr for fast delimiter detection
- **Arena-based DOM** with NodeId indices for cache-friendly traversal
- **Full XPath 1.0** with all 13 axes and 27+ functions
- **SweetXml-compatible API** with `~x` sigil and modifiers
- **Streaming support** for large files with bounded memory
- **Parallel XPath** evaluation for batch queries
- **Zero-copy parsing** where possible using Cow types

## Installation

```elixir
def deps do
  [{:rusty_xml, "~> 0.1.0"}]
end
```

Precompiled binaries are available for common platforms. For source compilation, Rust 1.70+ is required.

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

# Count elements
RustyXML.xpath(xml, "count(//book)")
#=> 2.0

# Extract multiple values with xmap
RustyXML.xmap(xml, [
  titles: ~x"//title",
  prices: ~x"//price"
])
#=> %{titles: [...], prices: [...]}
```

## The `~x` Sigil

The `~x` sigil creates XPath expressions with optional modifiers:

```elixir
import RustyXML

~x"//item"        # Basic XPath
~x"//item"l       # Return as list
~x"//item"s       # Return as string
~x"//item"i       # Cast to integer
~x"//item"f       # Cast to float
~x"//item"o       # Optional (nil on missing)
~x"//item"e       # Decode entities
~x"//item"k       # Return as keyword list
~x"//item"slo     # Combine modifiers
```

## Parsing Strategies

RustyXML provides multiple strategies for different use cases:

| Strategy | Use Case | Memory Model |
|----------|----------|--------------|
| `parse/1` + `xpath/2` | Multiple queries | Cached DOM |
| `xpath/2` (raw XML) | Single query | Temporary DOM |
| `stream_tags/3` | Large files | Bounded memory |
| `xmap_parallel/2` | Multiple queries | Parallel evaluation |

```elixir
# Parse once, query multiple times
doc = RustyXML.parse(xml)
RustyXML.xpath(doc, ~x"//item"l)
RustyXML.xpath(doc, ~x"//price"l)

# Stream large files (returns {tag_atom, xml_string} tuples)
"large.xml"
|> RustyXML.stream_tags(:item)
|> Stream.each(fn {:item, item_xml} ->
  name = RustyXML.xpath(item_xml, ~x"./name/text()"s)
  IO.puts("Processing: #{name}")
end)
|> Stream.run()
```

## XPath 1.0 Support

### Axes (13)

- `child`, `descendant`, `descendant-or-self`
- `parent`, `ancestor`, `ancestor-or-self`
- `following`, `following-sibling`
- `preceding`, `preceding-sibling`
- `self`, `attribute`, `namespace`

### Functions (27+)

**Node Functions:**
- `position()`, `last()`, `count()`, `local-name()`, `namespace-uri()`, `name()`

**String Functions:**
- `string()`, `concat()`, `starts-with()`, `contains()`, `substring()`
- `substring-before()`, `substring-after()`, `string-length()`
- `normalize-space()`, `translate()`

**Boolean Functions:**
- `boolean()`, `not()`, `true()`, `false()`, `lang()`

**Number Functions:**
- `number()`, `sum()`, `floor()`, `ceiling()`, `round()`

## API Reference

### Core Functions

```elixir
# Parse XML (strict by default, like SweetXml)
doc = RustyXML.parse("<root>...</root>")

# Parse with lenient mode (accepts malformed XML)
doc = RustyXML.parse("<root/>", lenient: true)

# Parse with tuple return (for pattern matching errors)
{:ok, doc} = RustyXML.parse_document("<root/>")
{:error, reason} = RustyXML.parse_document("<1bad/>")

# Execute XPath query
RustyXML.xpath(xml_or_doc, ~x"//item"l)

# Extract multiple values
RustyXML.xmap(xml_or_doc, [key: ~x"//path"])

# Get root element
RustyXML.root(doc)
```

### Streaming

Stream large XML files with bounded memory. Returns `{tag_atom, xml_string}` tuples compatible with SweetXml.

```elixir
# Stream specific tags from a file
RustyXML.stream_tags("data.xml", :item)
|> Enum.each(fn {:item, item_xml} ->
  # Each item_xml is a complete XML string that can be queried
  id = RustyXML.xpath(item_xml, ~x"./@id"s)
  name = RustyXML.xpath(item_xml, ~x"./name/text()"s)
  IO.puts("Item #{id}: #{name}")
end)

# Stream from enumerable (useful for network streams)
File.stream!("data.xml", [], 64 * 1024)
|> RustyXML.stream_tags(:item)
|> Stream.map(fn {:item, item} ->
  %{
    id: RustyXML.xpath(item, ~x"./@id"s),
    name: RustyXML.xpath(item, ~x"./name/text()"s)
  }
end)
|> Enum.to_list()

# Works correctly with Stream.take (unlike SweetXml issue #97)
"large.xml"
|> RustyXML.stream_tags(:item)
|> Stream.take(5)
|> Enum.to_list()

# Stream from XML string
xml_string
|> RustyXML.stream_tags(:item, chunk_size: 32 * 1024)
|> Enum.to_list()
```

**Key features:**
- Bounded memory regardless of file size
- Handles elements spanning chunk boundaries
- No hanging with `Stream.take` (fixes SweetXml issue #97)
- Works with files, streams, and strings

### Low-Level Native Functions

```elixir
# Event-based parsing
RustyXML.Native.parse_events("<root>...</root>")
#=> [{:start_element, "root", []}, ...]

# Streaming parser
parser = RustyXML.Native.streaming_new()
RustyXML.Native.streaming_feed(parser, chunk)
RustyXML.Native.streaming_take_events(parser, 100)
```

## Architecture

```
native/rustyxml/src/
├── lib.rs              # NIF entry points
├── core/
│   ├── scanner.rs      # SIMD byte scanning (memchr)
│   ├── tokenizer.rs    # State machine tokenizer
│   ├── entities.rs     # Entity decoding with Cow
│   └── attributes.rs   # Attribute parsing
├── reader/
│   ├── slice.rs        # Zero-copy slice parser
│   ├── buffered.rs     # Buffer-based reader
│   └── events.rs       # XML event types
├── dom/
│   ├── document.rs     # Arena-based DOM
│   ├── node.rs         # Node types with NodeId
│   ├── strings.rs      # String interning pool
│   └── namespace.rs    # Namespace resolver
├── xpath/
│   ├── lexer.rs        # XPath tokenizer
│   ├── parser.rs       # Recursive descent parser
│   ├── compiler.rs     # Expression compiler
│   ├── eval.rs         # Evaluation engine
│   ├── axes.rs         # All 13 axes
│   └── functions.rs    # 27+ XPath functions
├── strategy/
│   ├── streaming.rs    # Stateful streaming parser
│   └── parallel.rs     # Parallel XPath (DirtyCpu)
├── term.rs             # BEAM term building
└── resource.rs         # ResourceArc wrappers
```

## Parsing Modes: Lenient vs Strict

### The Reality of XML in the Wild

In theory, XML is strictly defined by the W3C specification. In practice, many real-world XML documents contain minor well-formedness violations that strict parsers reject. This creates a tension between correctness and practicality.

**Common violations found in production XML:**
- Unquoted attribute values: `<div class=main>`
- Invalid element names: `<123-item>` (names can't start with digits)
- Comments with `--` inside: `<!-- TODO -- fix later -->`
- Control characters in content

### How Parsers Handle This

| Parser | Approach | Trade-off |
|--------|----------|-----------|
| libxml2 | Strict by default | Rejects common real-world XML |
| Nokogiri | Lenient (via libxml2 recovery) | Accepts malformed, may misparse |
| SweetXml/xmerl | **Strict only** | Exits process on malformed input |
| quick-xml (Rust) | Lenient | Accepts most input |
| **RustyXML** | **Strict default, lenient optional** | **SweetXml compatible + flexibility** |

#### SweetXml/xmerl Behavior

SweetXml wraps Erlang's `:xmerl` parser, which is strictly compliant with no lenient mode. Per the xmerl documentation: *"Fatal errors must be detected by a conforming parser... This version of xmerl reports both categories of errors as fatal errors, most often resulting in an exit."*

```elixir
# SweetXml crashes on malformed XML - sends an exit signal
SweetXml.parse("<1invalid/>")
#=> ** (exit) {:fatal, {...}}

# Must use try/catch (not try/rescue) to handle xmerl exits
try do
  SweetXml.parse(possibly_malformed_xml)
catch
  :exit, _ -> {:error, :malformed}
end
```

This makes SweetXml unsuitable for processing untrusted or third-party XML without wrapping every call in `try/catch`. RustyXML's lenient mode handles this gracefully.

### RustyXML's Approach

RustyXML defaults to **strict mode** to match SweetXml behavior, while offering **lenient mode** for real-world XML that may have minor issues.

```elixir
# Strict mode (default) - matches SweetXml behavior
doc = RustyXML.parse("<root/>")

# Raises on malformed XML (like SweetXml, but with a proper exception)
RustyXML.parse("<1invalid/>")
#=> ** (RustyXML.ParseError) Invalid element name: must start with letter...

# Lenient mode - accepts malformed XML
doc = RustyXML.parse("<1invalid/>", lenient: true)  # Works

# For tuple-based error handling (no exceptions)
{:ok, doc} = RustyXML.parse_document("<root/>")
{:error, reason} = RustyXML.parse_document("<1invalid/>")
```

### When to Use Each Mode

| Mode | Use When |
|------|----------|
| **Strict** (default) | Drop-in SweetXml replacement, validating input, spec compliance |
| **Lenient** | Processing third-party XML, web scraping, legacy data, fault tolerance |

### OASIS/W3C Conformance

RustyXML is tested against the official [W3C XML Conformance Test Suite](https://www.w3.org/XML/Test/) (xmlconf) — the industry standard with 2000+ test cases from Sun, IBM, OASIS/NIST, and others.

| Mode | Valid Documents | Not-Well-Formed |
|------|-----------------|-----------------|
| Lenient | 218/218 ✅ (100%) | 0/871 rejected |
| Strict | 214/218 ✅ (98.2%) | 20/871 rejected |

**Lenient mode** correctly parses all valid XML. **Strict mode** catches common well-formedness errors (invalid names, `--` in comments, `]]>` in text) while accepting valid documents.

To run conformance tests yourself:
```bash
# Download W3C test suite (50MB)
mkdir -p test/xmlconf && cd test/xmlconf
curl -LO https://www.w3.org/XML/Test/xmlts20130923.tar.gz
tar -xzf xmlts20130923.tar.gz && rm xmlts20130923.tar.gz

# Run tests
FORCE_RUSTYXML_BUILD=1 mix test test/oasis_conformance_test.exs --only valid
```

## Security

RustyXML is designed to be **secure by default** against common XML vulnerabilities.

### XXE (XML External Entity) - IMMUNE

RustyXML **does not** process external entities. Attacks like this are completely ineffective:

```xml
<!DOCTYPE foo [
  <!ENTITY xxe SYSTEM "file:///etc/passwd">
  <!ENTITY remote SYSTEM "http://evil.com/steal">
]>
<root>&xxe; &remote;</root>
```

External entity declarations are parsed but **ignored**. No file system access, no network requests.

### Billion Laughs (Entity Expansion Bomb) - IMMUNE

RustyXML **only expands** the 5 XML built-in entities:

| Entity | Expansion |
|--------|-----------|
| `&lt;` | `<` |
| `&gt;` | `>` |
| `&amp;` | `&` |
| `&quot;` | `"` |
| `&apos;` | `'` |

Custom entity definitions in DTDs are **ignored**. This XML bomb has no effect:

```xml
<!DOCTYPE lolz [
  <!ENTITY lol "lol">
  <!ENTITY lol2 "&lol;&lol;&lol;&lol;&lol;">
  <!ENTITY lol3 "&lol2;&lol2;&lol2;&lol2;&lol2;">
]>
<root>&lol3;</root>
```

Result: `<root>&lol3;</root>` (unexpanded, safe)

### DTD Processing - Disabled

- External DTDs are **never** fetched
- Internal DTD subsets are **parsed but not processed**
- No entity definitions are honored (except built-ins)

### XPath Injection - Application Responsibility

If your application interpolates user input into XPath queries, sanitize it:

```elixir
# ⚠️ DANGEROUS - user input directly in XPath
xpath(doc, "//user[@name='#{user_input}']")

# User could input: ' or '1'='1
# Resulting in: //user[@name='' or '1'='1']

# ✅ SAFE - validate/escape user input first
safe_input = String.replace(user_input, "'", "\\'")
xpath(doc, "//user[@name='#{safe_input}']")
```

### Summary

| Vulnerability | Status |
|--------------|--------|
| XXE (External Entity) | ✅ Immune |
| Billion Laughs | ✅ Immune |
| Quadratic Blowup | ✅ Immune |
| External DTD | ✅ Immune |
| XPath Injection | ⚠️ Sanitize user input |

## Development

```bash
# Install dependencies
mix deps.get

# Compile (builds Rust NIF)
FORCE_RUSTYXML_BUILD=1 mix compile

# Run tests
FORCE_RUSTYXML_BUILD=1 mix test

# Run benchmarks
mix run bench/xml_bench.exs
```

## License

MIT License - see LICENSE file for details.

---

**RustyXML** - Purpose-built Rust NIF for ultra-fast XML parsing in Elixir.
