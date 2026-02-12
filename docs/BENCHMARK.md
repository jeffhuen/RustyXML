# RustyXML Benchmarks

Performance comparisons against SweetXml/xmerl.

## Test Environment

- **Elixir**: 1.19.4
- **OTP**: 28
- **Hardware**: Apple Silicon M1 Pro (10 cores)
- **RustyXML**: 0.2.1
- **SweetXml**: 0.7.5
- **NIF memory tracking**: enabled

> **Variance:** Throughput numbers vary ±15% between runs on the same hardware
> depending on thermal state, background load, and GC timing. Speedup ratios
> (RustyXML vs baseline) are more stable than absolute ips. Streaming memory
> measurements use `:erlang.memory(:total)` deltas and can vary ±30%.

## Parsing Performance

RustyXML's structural index uses SIMD-accelerated scanning (`memchr`) and zero-copy spans. Gains increase with document size.

### Small Document (14.6 KB, 50 items)

| Parser | Throughput | vs SweetXml |
|--------|------------|-------------|
| RustyXML | 9,370 ips | **8.2x faster** |
| SweetXml | 1,140 ips | baseline |

### Medium Document (290.6 KB, 1,000 items)

| Parser | Throughput | vs SweetXml |
|--------|------------|-------------|
| RustyXML | 533 ips | **9.5x faster** |
| SweetXml | 56 ips | baseline |

### Large Document (2.93 MB, 10,000 items)

| Parser | Throughput | vs SweetXml |
|--------|------------|-------------|
| RustyXML | 54 ips | **72x faster** |
| SweetXml | 0.75 ips | baseline |

## XPath Query Performance

All queries run on pre-parsed documents.

### XPath on Pre-Parsed Document (290.6 KB, 1,000 items)

| Query Type | RustyXML | SweetXml | vs SweetXml |
|------------|----------|----------|-------------|
| `//item` (full elements) | 589 ips | 397 ips | **1.48x faster** |
| `//item/name/text()` | 676 ips | 337 ips | **2.0x faster** |
| `//item/@id` | 737 ips | 433 ips | **1.7x faster** |

### Complex XPath Queries (2.93 MB, 10,000 items)

| Query Type | RustyXML | SweetXml | vs SweetXml |
|------------|----------|----------|-------------|
| Predicate (`[price > 50]`) | 60 ips | 16 ips | **3.65x faster** |
| Count function | 63 ips | 22 ips | **2.8x faster** |

### Lazy XPath API (290 KB, 1,000 items)

The lazy API keeps results in Rust memory, building BEAM terms only when accessed:

| API | Latency (100 runs) | vs SweetXml |
|-----|-------------------|-------------|
| Regular `xpath/2` | 104 ms | baseline |
| Lazy `xpath_lazy/2` (count only) | 31 ms | **3.0x faster** |
| Lazy + batch accessor | 31 ms | **3.1x faster** |
| Parse + lazy + batch | 130 ms | **4.4x faster** |

**Recommendation:** Use batch accessors (`result_texts`, `result_attrs`, `result_extract`) when accessing multiple items to reduce NIF call overhead.

## Memory Comparison

### Measurement Methodology

- **RustyXML "Total"** = NIF peak (instantaneous high-water mark via mimalloc tracking) + BEAM allocations (Benchee GC tracing for parse; `:erlang.memory(:total)` delta for streaming)
- **SweetXml "BEAM"** = Total BEAM allocations measured by Benchee's GC tracing, which includes memory that was subsequently garbage-collected — i.e. cumulative allocations, not peak
- **Streaming** uses `:erlang.memory(:total)` snapshot deltas, which are noisier than Benchee's per-process GC tracing

These are not identical metrics. The ratios are directionally correct but should not be taken as exact.

### Parse Memory

| Document | RustyXML (NIF peak + BEAM) | SweetXml (BEAM total allocs) |
|----------|----------------------------|------------------------------|
| Small (14.6 KB) | **63.4 KB** | 5.65 MB |
| Medium (290.6 KB) | **1.23 MB** | 112 MB |
| Large (2.93 MB) | **12.81 MB** | 1,133 MB |

### XPath Memory

| Query | RustyXML (NIF peak + BEAM) | SweetXml (BEAM total allocs) |
|-------|----------------------------|------------------------------|
| `//item` (1K items) | **475 KB** | 6.12 MB |
| `text()` (1K items) | **491 KB** | 7.10 MB |
| `@id` (1K items) | **491 KB** | 6.45 MB |
| Predicate (10K items) | **5.96 MB** | 68.2 MB |
| Count (10K items) | **5.94 MB** | 60.8 MB |

### Streaming Memory

| Operation | RustyXML (NIF peak + BEAM delta) | SweetXml (BEAM delta) |
|-----------|----------------------------------|-----------------------|
| Stream 10K items | **128 KB** | 73 MB |

> **Note:** SweetXml's `stream_tags` retains every parsed element in xmerl's
> accumulator for the entire parse, so 73 MB reflects genuine SweetXml behavior
> but is not representative of a properly bounded streaming parser.
> For a fairer comparison, see the [Saxy Comparison](#saxy-comparison)
> where both parsers use bounded memory (~128 KB vs ~124 KB).

## Streaming Comparison

### Feature Comparison

| Feature | RustyXML | SweetXml |
|---------|----------|----------|
| Memory model | Bounded (~128 KB) | Unbounded |
| `Stream.take` | Works correctly | Hangs (issue #97) |
| Chunk boundary handling | Handled correctly | N/A |
| Output format | `{tag_atom, xml_string}` | `{tag_atom, xml_string}` |
| Early termination | Proper cleanup | Can hang |

### Streaming Performance (10,000 items, 2.93 MB)

| Metric | RustyXML | SweetXml | vs SweetXml |
|--------|----------|----------|-------------|
| Time | 23.3 ms | 376.7 ms | **16.2x faster** |
| Throughput | ~43/s | ~2.7/s | **16.2x faster** |
| `Stream.take(5)` | Works | Hangs | RustyXML wins |

## Saxy Comparison

RustyXML also serves as a drop-in Saxy replacement. SAX parsing benchmarks against Saxy 1.6:

### SAX Parse Performance

Measured on Apple Silicon M1 Pro. Throughput varies ±15% between runs; speedup ratios are more stable.

| Operation | XML Size | RustyXML | Saxy | Speedup |
|-----------|----------|----------|------|---------|
| `parse_string/4` | 14.6 KB | ~8.5K ips | ~6.5K ips | **~1.3x** |
| `parse_string/4` | 290.6 KB | ~435 ips | ~320 ips | **~1.4x** |
| `parse_string/4` | 2.93 MB | ~40 ips | ~25 ips | **~1.6x** |
| `SimpleForm` | 14.6 KB | ~6.1K ips | ~4.7K ips | **~1.3x** |
| `SimpleForm` | 290.6 KB | ~330 ips | ~215 ips | **~1.5x** |
| `parse_stream/4` | 2.93 MB | ~41 ips | ~23 ips | **~1.8x** |

### SAX Memory

| Operation | XML Size | RustyXML Total | Saxy (BEAM) | Ratio |
|-----------|----------|----------------|-------------|-------|
| `parse_string/4` | 14.6 KB | 127 KB | 308 KB | **0.41x** |
| `parse_string/4` | 2.93 MB | 26.4 MB | 59.6 MB | **0.44x** |
| `SimpleForm` | 290.6 KB | 1.43 MB | 10.7 MB | **0.13x** |
| `parse_stream/4` | 2.93 MB | ~130–162 KB | ~124–133 KB | **~1x** |

`parse_stream` memory is comparable: both parsers operate in bounded memory.
RustyXML uses zero-copy tokenization and direct BEAM binary encoding to keep
the NIF peak at ~67 KB. Streaming memory measurements use `:erlang.memory(:total)`
deltas which are noisier than Benchee's per-process tracing — expect ±30%
variance between runs on the same hardware.

## Summary

### Speed Rankings

| Operation | vs SweetXml |
|-----------|-------------|
| Parse large (2.93 MB) | **72x faster** |
| Parse medium (290 KB) | **9.5x faster** |
| Parse small (14.6 KB) | **8.2x faster** |
| Streaming (10K items) | **16.2x faster** |
| Parse + lazy + batch | **4.4x faster** |
| Complex XPath (predicate) | **3.65x faster** |
| Lazy XPath (count only) | **3.0x faster** |
| Complex XPath (count) | **2.8x faster** |
| XPath text extraction | **2.0x faster** |
| XPath attribute extraction | **1.7x faster** |
| XPath full elements | **1.48x faster** |

### Memory Rankings

| Operation | Comparison | Notes |
|-----------|------------|-------|
| Streaming (vs Saxy) | **~1x** | Both properly bounded; fairest comparison |
| Parse (vs SweetXml) | **~90x less** | Different metrics; see [methodology](#measurement-methodology) |
| XPath (vs SweetXml) | **10-14x less** | |

### Recommended API by Use Case

| Use Case | Recommended |
|----------|-------------|
| General XML processing | `parse/1` + `xpath/2` |
| Single query on XML string | `xpath/2` with raw XML |
| Large result sets, partial access | `xpath_lazy/2` + batch accessors |
| Count results only | `xpath_lazy/2` + `result_count/1` |
| Elements as XML strings | `Native.xpath_query_raw/2` |
| Large files (GB+) | `stream_tags/3` |
| Batch queries | `xmap/2` |
| Event-driven SAX processing | `parse_string/4` with handler |
| Streaming SAX (sockets, HTTP) | `parse_stream/4` or `Partial` |
| Simple tuple tree | `SimpleForm.parse_string/2` |
| Generating XML | `encode!/2` with `RustyXML.XML` |

### Key Findings

1. **Parsing is 8-72x faster than SweetXml** — The structural index with SIMD scanning dramatically outperforms xmerl, with gains increasing on larger documents.

2. **SAX parsing is ~1.3-1.8x faster than Saxy** — with comparable streaming memory. This is the fairest streaming comparison since both parsers are properly bounded.

3. **All XPath queries are faster** — Full elements (1.48x), text (2.0x), attributes (1.7x), predicates (3.65x), counts (2.8x) vs SweetXml.

4. **Lazy XPath is 3-4.4x faster** — Keeping node IDs in Rust and accessing on-demand eliminates BEAM term construction overhead.

5. **Significantly less memory for parsing** — The structural index uses compact spans instead of string copies. Parse memory for 2.93 MB doc: 12.8 MB (NIF peak + BEAM) vs SweetXml's 1,133 MB (BEAM total allocations). Note: these are [different metrics](#measurement-methodology) — the magnitude of difference is real but the exact ratio is approximate.

6. **Stream.take works correctly** — Fixes SweetXml issue #97. Bounded memory regardless of file size.

### Improvement from v0.1.1 to v0.1.2

The unified structural index brought substantial gains over the prior DOM-based approach:

| Metric | v0.1.1 (DOM) | v0.1.2 (Index) | Improvement |
|--------|-------------|----------------|-------------|
| Parse throughput (large) | 30.7 ips | 54.0 ips | **1.76x** |
| Parse memory (large) | 30.17 MB | 12.81 MB | **58% less** |
| XPath `//item` | 0.83x SweetXml | 1.48x SweetXml | **was slower, now faster** |
| XPath memory (medium) | 28.3 MB | 475 KB | **60x less** |
| Streaming throughput | 21.9/s | 43.0/s | **1.96x** |
| Streaming memory | 52.8 MB | 319 KB | **165x less** |

## Running the Benchmarks

```bash
# vs SweetXml
FORCE_RUSTYXML_BUILD=1 mix run bench/sweet_bench.exs

# vs Saxy
FORCE_RUSTYXML_BUILD=1 mix run bench/saxy_bench.exs
```

### Enabling Memory Tracking

```toml
# In native/rustyxml/Cargo.toml
[features]
default = ["mimalloc", "memory_tracking"]
```

```bash
FORCE_RUSTYXML_BUILD=1 mix compile --force
```

```elixir
RustyXML.Native.reset_rust_memory_stats()
doc = RustyXML.parse(xml)
peak = RustyXML.Native.get_rust_memory_peak()
current = RustyXML.Native.get_rust_memory()
```

## Correctness Verification

All benchmarks include correctness verification:

```
count(//item): RustyXML=1000, SweetXml=1000 - ok
//item[1]/name/text(): RustyXML="Product 1", SweetXml="Product 1" - ok
//item/@id count: RustyXML=1000, SweetXml=1000 - ok

Overall: ALL TESTS PASSED
```
