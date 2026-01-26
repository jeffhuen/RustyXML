# RustyXML Compliance & Validation

RustyXML takes correctness seriously. With **1275+ tests** across multiple test suites, including the complete W3C/OASIS XML Conformance Test Suite, RustyXML achieves **100% compliance** with the industry-standard XML validation tests.

This document describes W3C XML 1.0 compliance, XPath 1.0 support, and the validation methodology.

---

## W3C XML 1.0 Compliance

RustyXML is designed to comply with [W3C XML 1.0 (Fifth Edition)](https://www.w3.org/TR/xml/).

### Core XML Requirements

| Section | Requirement | Status |
|---------|-------------|--------|
| 2.1 | Well-formed documents | ✅ |
| 2.2 | Characters (Unicode) | ✅ |
| 2.3 | Common syntactic constructs | ✅ |
| 2.4 | Character data | ✅ |
| 2.5 | Comments | ✅ |
| 2.6 | Processing instructions | ✅ |
| 2.7 | CDATA sections | ✅ |
| 2.8 | Prolog and document type declaration | ✅ Parsed |
| 2.9 | Standalone document declaration | ✅ |
| 2.10 | White space handling | ✅ |
| 2.11 | End-of-line handling | ✅ |
| 2.12 | Language identification | ✅ |

### Element and Attribute Support

| Feature | Status | Notes |
|---------|--------|-------|
| Start tags | ✅ | Full attribute support |
| End tags | ✅ | Name matching validation |
| Empty element tags | ✅ | `<element/>` syntax |
| Attributes | ✅ | Single and double quotes |
| Namespaces | ✅ | Prefix resolution |
| Default namespaces | ✅ | `xmlns="..."` |

### Character and Entity Support

| Feature | Status | Notes |
|---------|--------|-------|
| Character references | ✅ | `&#N;` and `&#xN;` |
| Predefined entities | ✅ | `&lt;`, `&gt;`, `&amp;`, `&apos;`, `&quot;` |
| Character encoding | ✅ | UTF-8 (primary), UTF-16 detection |
| Unicode characters | ✅ | Full Unicode support |
| BOM handling | ✅ | UTF-8/UTF-16 BOM detection |

### Document Structure

| Feature | Status | Notes |
|---------|--------|-------|
| XML declaration | ✅ | Version, encoding, standalone |
| DOCTYPE declaration | ✅ | Parsed but not validated |
| Single root element | ✅ | Enforced |
| Comments | ✅ | `<!-- ... -->` |
| Processing instructions | ✅ | `<?target data?>` |
| CDATA sections | ✅ | `<![CDATA[...]]>` |

### Differences from Strict XML 1.0

RustyXML makes practical concessions shared by most XML implementations:

1. **Non-validating parser** - DTD declarations are parsed but entity definitions are not expanded (except predefined entities)
2. **Lenient character handling** - Control characters in content generate warnings rather than errors
3. **Flexible encoding** - Automatically detects UTF-8, UTF-16 LE/BE

---

## XPath 1.0 Support

RustyXML implements the complete [XPath 1.0 specification](https://www.w3.org/TR/xpath-10/).

### Axes (13 of 13)

All XPath axes are fully supported:

| Axis | Status | Description |
|------|--------|-------------|
| `child` | ✅ | Direct children |
| `parent` | ✅ | Parent node |
| `self` | ✅ | Context node |
| `attribute` | ✅ | Attributes of context node |
| `descendant` | ✅ | All descendants |
| `descendant-or-self` | ✅ | Context node and descendants |
| `ancestor` | ✅ | All ancestors |
| `ancestor-or-self` | ✅ | Context node and ancestors |
| `following` | ✅ | All following nodes |
| `following-sibling` | ✅ | Following siblings |
| `preceding` | ✅ | All preceding nodes |
| `preceding-sibling` | ✅ | Preceding siblings |
| `namespace` | ✅ | Namespace nodes |

### Node Tests

| Test | Status | Example |
|------|--------|---------|
| Node name | ✅ | `child::book` |
| Wildcard | ✅ | `child::*` |
| Node type | ✅ | `text()`, `comment()`, `processing-instruction()`, `node()` |
| Processing instruction target | ✅ | `processing-instruction('xml-stylesheet')` |

### Predicates

| Feature | Status | Example |
|---------|--------|---------|
| Position predicates | ✅ | `[1]`, `[last()]` |
| Attribute predicates | ✅ | `[@id='1']` |
| Element predicates | ✅ | `[title]` |
| Comparison operators | ✅ | `=`, `!=`, `<`, `>`, `<=`, `>=` |
| Boolean operators | ✅ | `and`, `or` |
| Arithmetic operators | ✅ | `+`, `-`, `*`, `div`, `mod` |
| Nested predicates | ✅ | `[item[@type='book']]` |

### Functions (27+)

#### Node Set Functions

| Function | Status | Description |
|----------|--------|-------------|
| `position()` | ✅ | Current position in node set |
| `last()` | ✅ | Size of node set |
| `count(node-set)` | ✅ | Number of nodes |
| `local-name()` | ✅ | Local part of name |
| `namespace-uri()` | ✅ | Namespace URI |
| `name()` | ✅ | Qualified name |
| `id(string)` | ✅ | Select by ID |

#### String Functions

| Function | Status | Description |
|----------|--------|-------------|
| `string()` | ✅ | Convert to string |
| `concat(str, str, ...)` | ✅ | Concatenate strings |
| `starts-with(str, prefix)` | ✅ | Test string prefix |
| `contains(str, substr)` | ✅ | Test substring presence |
| `substring(str, start, len?)` | ✅ | Extract substring |
| `substring-before(str, delim)` | ✅ | String before delimiter |
| `substring-after(str, delim)` | ✅ | String after delimiter |
| `string-length(str?)` | ✅ | String length |
| `normalize-space(str?)` | ✅ | Normalize whitespace |
| `translate(str, from, to)` | ✅ | Character translation |

#### Boolean Functions

| Function | Status | Description |
|----------|--------|-------------|
| `boolean()` | ✅ | Convert to boolean |
| `not(bool)` | ✅ | Logical negation |
| `true()` | ✅ | Boolean true |
| `false()` | ✅ | Boolean false |
| `lang(lang)` | ✅ | Test language |

#### Number Functions

| Function | Status | Description |
|----------|--------|-------------|
| `number()` | ✅ | Convert to number |
| `sum(node-set)` | ✅ | Sum of node values |
| `floor(num)` | ✅ | Floor function |
| `ceiling(num)` | ✅ | Ceiling function |
| `round(num)` | ✅ | Round to nearest |

### Abbreviated Syntax

| Syntax | Expansion | Status |
|--------|-----------|--------|
| `//` | `/descendant-or-self::node()/` | ✅ |
| `.` | `self::node()` | ✅ |
| `..` | `parent::node()` | ✅ |
| `@attr` | `attribute::attr` | ✅ |
| `[n]` | `[position() = n]` | ✅ |

---

## Conformance Test Suite

RustyXML includes a comprehensive conformance test suite based on W3C and OASIS standards.

### Test Categories

| Category | Tests | Description |
|----------|-------|-------------|
| Well-Formedness | 18 | Basic XML structure |
| Characters | 12 | Unicode and special characters |
| Whitespace | 8 | Whitespace preservation and normalization |
| Entities | 10 | Entity references and escaping |
| CDATA | 8 | CDATA section handling |
| Comments | 6 | Comment parsing |
| Processing Instructions | 6 | PI parsing and data extraction |
| Namespaces | 12 | Namespace declaration and resolution |
| Attributes | 10 | Attribute parsing and quoting |
| Elements | 8 | Element naming and nesting |
| XML Declaration | 6 | Version, encoding, standalone |
| DOCTYPE | 4 | DOCTYPE declaration parsing |
| Edge Cases | 8 | Complex real-world scenarios |
| XPath Axes | 15 | All 13 axes plus edge cases |
| **Total** | **121** | Conformance tests |

### Test File

```
test/xml_conformance_test.exs
```

### Running Conformance Tests

```bash
# Run all conformance tests
mix test test/xml_conformance_test.exs

# Run specific category
mix test test/xml_conformance_test.exs --only wellformedness
mix test test/xml_conformance_test.exs --only xpath
```

---

## W3C/OASIS XML Conformance Test Suite

RustyXML is tested against the official W3C XML Conformance Test Suite (xmlconf), the industry standard with 2000+ test cases from Sun, IBM, OASIS/NIST, and others.

### Test Results

#### Strict Mode (Default)

| Category | Tests | Passed | Status |
|----------|-------|--------|--------|
| Valid documents (must accept) | 218 | 218 | ✅ **100%** |
| Not-well-formed (must reject) | 871 | 871 | ✅ **100%** |
| Invalid (DTD validation) | - | - | N/A (non-validating) |

**RustyXML achieves 100% compliance** with all 1089 applicable OASIS/W3C XML Conformance tests.

#### Lenient Mode (`lenient: true`)

| Category | Tests | Passed | Status |
|----------|-------|--------|--------|
| Valid documents (must accept) | 218 | 218 | ✅ **100%** |
| Not-well-formed (must reject) | 871 | 0 | ⚠️ **Lenient** |
| Invalid (DTD validation) | - | - | N/A (non-validating) |

Lenient mode accepts malformed XML for processing third-party or legacy documents.

### Parser Behavior

RustyXML supports two modes:

**Strict Mode (Default)** - Matches SweetXml/xmerl behavior:
- Validates element and attribute names
- Checks comment content (no `--` sequences)
- Validates text content (no unescaped `]]>`)
- Raises `ParseError` for malformed documents

**Lenient Mode** (`lenient: true`) - Accepts malformed XML:
- Best for processing real-world documents that may have minor issues
- 100% acceptance of valid documents
- Does not reject malformed documents

```elixir
# Strict mode (default) - matches SweetXml
doc = RustyXML.parse("<root/>")
RustyXML.parse("<1invalid/>")  # Raises ParseError

# Lenient mode - accepts malformed XML
doc = RustyXML.parse("<1invalid/>", lenient: true)

# Tuple-based error handling (no exceptions)
{:ok, doc} = RustyXML.parse_document("<root/>")
{:error, reason} = RustyXML.parse_document("<1invalid/>")
```

| Malformed Input | Strict Mode (Default) | Lenient Mode |
|-----------------|----------------------|--------------|
| `<!-- comment -- inside -->` | ❌ Error | ✅ Accepts |
| `<1invalid-name>` | ❌ Error | ✅ Accepts |
| `<valid>text ]]> more</valid>` | ❌ Error | ✅ Accepts |
| `<?XML version="1.0"?>` (wrong case) | ❌ Error | ✅ Accepts |
| `standalone="YES"` (wrong case) | ❌ Error | ✅ Accepts |
| `&undefined;` in attributes | ❌ Error | ✅ Accepts |
| External entity in attribute | ❌ Error | ✅ Accepts |

**Rationale**: Strict mode by default ensures SweetXml compatibility and full XML 1.0 compliance. Lenient mode is available for processing third-party or legacy XML that may have minor issues.

### Obtaining the Test Suite

The W3C/OASIS XML Conformance Test Suite is **not included** in the RustyXML package to keep the download size small (~50MB of test data). To run the conformance tests locally:

**Option 1: Download directly from W3C**

```bash
mkdir -p test/xmlconf && cd test/xmlconf
curl -LO https://www.w3.org/XML/Test/xmlts20130923.tar.gz
tar -xzf xmlts20130923.tar.gz && rm xmlts20130923.tar.gz
```

**Option 2: Use the convenience script**

```bash
./scripts/download-xmlconf.sh
```

The test suite version `xmlts20130923` (September 2013) is the latest official release from the W3C. Since XML 1.0 Fifth Edition (2008) has been stable for over 15 years, no updates to the conformance tests have been necessary.

### Running the Test Suite

```bash
# Run all conformance tests (requires test suite download)
mix test test/oasis_conformance_test.exs

# Run only valid document tests
mix test test/oasis_conformance_test.exs --only valid

# Run only not-well-formed tests
mix test test/oasis_conformance_test.exs --only not_wf

# Include skipped tests (shows full results)
mix test test/oasis_conformance_test.exs --include skip
```

### References

- **W3C Test Suite**: https://www.w3.org/XML/Test/
- **OASIS Committee**: https://www.oasis-open.org/committees/xml-conformance/
- **Test Suite Archive**: https://www.w3.org/XML/Test/xmlts20130923.tar.gz

### XPath Conformance

XPath compliance is tested against:

- W3C XPath 1.0 specification examples
- XSLT/XPath conformance test suite
- Real-world query patterns from SweetXml users

---

## SweetXml Compatibility

RustyXML is designed as a drop-in replacement for SweetXml.

### API Compatibility

| Function | SweetXml | RustyXML | Status |
|----------|----------|----------|--------|
| `xpath/2` | ✅ | ✅ | Compatible |
| `xpath/3` | ✅ | ✅ | Compatible |
| `xmap/2` | ✅ | ✅ | Compatible |
| `xmap/3` | ✅ | ✅ | Compatible |
| `~x` sigil | ✅ | ✅ | Compatible |
| `stream_tags/2` | ✅ | ✅ | Compatible |
| `stream_tags/3` | ✅ | ✅ | Compatible |

### Sigil Modifiers

| Modifier | SweetXml | RustyXML | Status |
|----------|----------|----------|--------|
| `s` (string) | ✅ | ✅ | Compatible |
| `l` (list) | ✅ | ✅ | Compatible |
| `e` (entities) | ✅ | ✅ | Compatible |
| `o` (optional) | ✅ | ✅ | Compatible |
| `i` (integer) | ✅ | ✅ | Compatible |
| `f` (float) | ✅ | ✅ | Compatible |
| `k` (keyword) | ✅ | ✅ | Compatible |

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

## Cross-Strategy Validation

All parsing strategies must produce consistent output for the same input.

| Strategy | Description | Validates Against |
|----------|-------------|-------------------|
| Event Parser | SIMD-accelerated events | All test suites |
| DOM Parser | Arena-based document | All test suites |
| Streaming | Bounded-memory chunks | All test suites |

```elixir
# Strategies are validated in test/rusty_xml_test.exs
test "all strategies produce consistent output" do
  xml = "<root><item>test</item></root>"

  events = RustyXML.Native.parse_events(xml)
  doc = RustyXML.parse(xml)

  # Verify consistency
  assert ...
end
```

## Streaming Compliance

The streaming parser (`stream_tags/3`) is validated for:

| Feature | Status | Notes |
|---------|--------|-------|
| Complete element reconstruction | ✅ | Builds valid XML strings |
| Nested element handling | ✅ | Captures full subtrees |
| Whitespace preservation | ✅ | All whitespace preserved |
| Attribute handling | ✅ | All attributes captured |
| CDATA sections | ✅ | Preserved in output |
| Entity preservation | ✅ | Entities maintained |
| Chunk boundary handling | ✅ | Elements spanning chunks work correctly |
| Early termination | ✅ | `Stream.take` works without hanging |

### SweetXml Issue Compatibility

RustyXML's streaming implementation addresses known SweetXml issues:

| Issue | SweetXml | RustyXML | Status |
|-------|----------|----------|--------|
| #97 - Stream.take hangs | ❌ Hangs | ✅ Works | Fixed |
| #50 - Nested text order | ❌ Wrong order | ✅ Correct | Fixed |
| Element boundary chunks | ⚠️ Can fail | ✅ Handles correctly | Fixed |

---

## Validation Methodology

### Test Data Sources

1. **Synthetic tests** - Generated XML covering edge cases
2. **Real-world XML** - RSS feeds, configuration files, SOAP messages
3. **Conformance suites** - W3C and OASIS standard tests
4. **Fuzz testing** - Random input to find parsing errors

### Test Execution

- All tests run on every CI build
- Cross-platform testing (Linux, macOS, Windows)
- Multiple Elixir/OTP version matrix
- Memory leak detection with Valgrind (Rust side)

### Reporting Issues

If you find XML that RustyXML doesn't handle correctly:

1. Create a minimal reproduction case
2. Open an issue with:
   - Input XML (or link to conformance test)
   - Expected output
   - Actual output
   - RustyXML version

---

## Test Summary

| Suite | Tests | Purpose |
|-------|-------|---------|
| OASIS/W3C Conformance | 1089 | Industry-standard XML validation |
| RustyXML Unit Tests | 186 | API, XPath, streaming, sigils |
| **Total** | **1275** | |

---

## Strict Mode Validation

RustyXML's strict mode (default) implements comprehensive XML 1.0 validation:

### Well-Formedness Checks

- ✅ Element and attribute names (XML 1.0 Edition 4 NameStartChar/NameChar)
- ✅ Comment content (no `--` sequences)
- ✅ Text content (no unescaped `]]>`)
- ✅ Standalone declaration values (`yes` or `no` only)
- ✅ Document structure ordering (XMLDecl → DOCTYPE → root)
- ✅ Processing instruction target validation (`xml` reserved)

### Entity Validation

- ✅ Entity registry tracking (declared entities, types, values)
- ✅ Undefined entity detection in attribute values
- ✅ Case-sensitive entity matching
- ✅ External entity detection (SYSTEM/PUBLIC)
- ✅ WFC: No External Entity References in attributes
- ✅ Unparsed entity (NDATA) restrictions
- ✅ Entity replacement text validation:
  - Split character reference detection (`&#38;` + `#`)
  - Balanced markup validation
  - Invalid name character detection (CombiningChar as first char)
  - XML declaration in entity prohibition

### Not Planned

- **XML 1.1 support** - Minimal adoption, incompatible changes
- **External entity resolution** - Security concerns (XXE attacks)
- **Full DTD processing** - Complexity vs. benefit
- **XPath 2.0** - Different specification, significant effort
- **XSD validation** - Out of scope for a parsing library

---

## References

- [W3C XML 1.0 (Fifth Edition)](https://www.w3.org/TR/xml/) - XML specification
- [W3C Namespaces in XML 1.0](https://www.w3.org/TR/xml-names/) - Namespace specification
- [W3C XPath 1.0](https://www.w3.org/TR/xpath-10/) - XPath specification
- [OASIS XML Conformance](https://www.oasis-open.org/committees/xml-conformance/) - Test suite
- [W3C XML Test Suite](https://www.w3.org/XML/Test/) - Additional tests
- [SweetXml](https://github.com/kbrw/sweet_xml) - Elixir XML library (compatibility target)
