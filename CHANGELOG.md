# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-02-04

### Added

- **Saxy drop-in replacement** — full 1:1 API parity with [Saxy](https://hex.pm/packages/saxy)
  - `parse_string/4` — SAX parsing with handler callbacks
  - `parse_stream/4` — streaming SAX with binary-encoded events
  - `stream_events/2` — lazy stream of SAX events
  - `encode!/2`, `encode_to_iodata/2` — XML encoding
  - `RustyXML.Handler` — behaviour (= `Saxy.Handler`)
  - `RustyXML.Partial` — incremental parsing (= `Saxy.Partial`)
  - `RustyXML.SimpleForm` — tuple tree output (= `Saxy.SimpleForm`)
  - `RustyXML.XML` — builder DSL (= `Saxy.XML`)
  - `RustyXML.Builder` — struct→XML protocol (= `Saxy.Builder`)
  - `RustyXML.Encoder` — XML string encoding
- Binary-encoded SAX events — NIF packs all events from a chunk into a single binary
  instead of ~1,700 BEAM tuples per 64 KB chunk; Elixir decodes one event at a time
  via pattern matching

### Performance vs Saxy

| Operation | Speedup | Memory |
|-----------|---------|--------|
| `parse_string/4` (14.6 KB) | **1.31x faster** | **2.4x less** |
| `parse_string/4` (2.93 MB) | **1.72x faster** | **2.3x less** |
| `SimpleForm` (290 KB) | **1.54x faster** | **7.7x less** |
| `parse_stream/4` (2.93 MB) | **1.7x faster** | 3.5x more* |

\* `parse_stream` uses more memory due to NIF buffer retention; under investigation.

### Migration

```bash
# From SweetXml — just change the import
sed -i 's/SweetXml/RustyXML/g' lib/**/*.ex

# From Saxy — just change the module name
sed -i 's/Saxy/RustyXML/g' lib/**/*.ex
```

## [0.1.2] - 2026-02-04

### Added

- Unified structural index — single zero-copy parse path replaces the old DOM
- `UnifiedScanner` with `ScanHandler` trait for extensible tokenization
- `sax_parse/1` NIF for SAX event parsing
- `xpath_text_list` NIF for fast text extraction without building element tuples
- Lightweight `validate_strict` — checks well-formedness without allocating a DOM

### Performance vs v0.1.1

| Metric | v0.1.1 | v0.1.2 | Improvement |
|--------|--------|--------|-------------|
| Parse throughput (2.93 MB) | 30.7 ips | 54.0 ips | **1.76x faster** |
| Parse vs SweetXml (2.93 MB) | 41x | **72x** | |
| XPath `//item` vs SweetXml | **0.83x (slower)** | **1.48x faster** | fixed |
| XPath `//item` throughput | 336 ips | 589 ips | **1.75x faster** |
| Streaming vs SweetXml | 8.7x | **16.2x** | **1.87x faster** |
| Parse memory (2.93 MB) | 30.2 MB | 12.8 MB | **58% less** |
| XPath memory (290 KB doc) | 28.3 MB | 475 KB | **60x less** |
| Streaming memory | 52.8 MB | 319 KB | **165x less** |

### Changed

- All parse NIFs now use the structural index (compact spans into original input)
- `parse_strict/1` no longer builds then discards a full DOM

### Fixed

- `xpath/3` subspecs on document refs — was silently ignoring subspecs
- `xmap/3` third argument — now accepts `true` for keyword list output
- `parse/2` charlist support — accepts charlists in addition to binaries
- Lenient mode infinite loop on malformed markup like `<1invalid/>`

### Removed

- `parse_events/1` — redundant with structural index (was 7x slower, 4x more memory)

## [0.1.1] - 2026-01-29

### Changed

- Parse NIFs (`parse/1`, `parse_strict/1`, `parse_and_xpath/2`, `xpath_with_subspecs/3`, `xpath_string_value/2`, `sax_parse/1`) moved to dirty CPU schedulers
- Internal failures now return `{:error, :mutex_poisoned}` instead of silent nil/empty values
- Batch accessors (`result_texts`, `result_attrs`, `result_extract`) clamp ranges to actual result count

## [0.1.0] - 2026-01-26

### Added

- Purpose-built Rust NIF for XML parsing in Elixir
- SIMD-accelerated scanning via `memchr`
- Full XPath 1.0 — all 13 axes, 27+ functions, LRU expression cache
- SweetXml-compatible API — `xpath/2,3`, `xmap/2,3`, `~x` sigil, `stream_tags/3`
- Lazy XPath (`xpath_lazy/2`) — results stay in Rust memory, 3x faster for partial access
- Parallel XPath (`xpath_parallel/2`) — multi-threaded query evaluation via Rayon
- Streaming parser with bounded memory for large files
- 100% W3C/OASIS XML Conformance — 1089/1089 applicable tests pass
- XXE immune, Billion Laughs immune, panic-safe NIFs, atom table safe
