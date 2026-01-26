# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-01-25

### Added

- **`:zero_copy` strategy** - New parsing strategy using BEAM sub-binary references
  - Zero-copy for unquoted and simply-quoted fields
  - Hybrid approach: only copies when quote unescaping is needed (`""` → `"`)
  - Matches NimbleCSV's memory model while keeping SIMD scanning speed
  - Trade-off: sub-binaries keep parent binary alive until GC

- **SIMD-accelerated row boundary scanning** - `memchr3` for parallel strategy
  - Replaces byte-by-byte scanning with hardware-accelerated jumps
  - Only examines positions where quotes or newlines appear
  - Properly handles RFC 4180 escaped quotes

- **mimalloc allocator** - High-performance memory allocator (enabled by default)
  - 10-20% faster allocation for many small objects
  - Reduced memory fragmentation
  - Zero tracking overhead in default configuration

- **Optional memory tracking** - Opt-in profiling via `memory_tracking` Cargo feature
  - When disabled (default): `get_rust_memory/0` etc. return `0` with zero overhead
  - When enabled: full allocation tracking for profiling
  - Enable with `default = ["mimalloc", "memory_tracking"]` in Cargo.toml

### Changed

- Memory tracking is now opt-in instead of always-on (removes ~5-10% overhead)
- Pre-allocated vectors throughout parsing paths for reduced reallocation
- Updated ARCHITECTURE.md with comprehensive strategy documentation
- Six parsing strategies now available (was five)

### Performance

- `:parallel` strategy benefits from SIMD row boundary scanning
- `:zero_copy` strategy eliminates copy overhead for clean CSV data
- All strategies benefit from mimalloc and pre-allocation improvements

### Fixed

- **Benchmark methodology** - Corrected unfair streaming comparison (NimbleCSV now uses line-based streams)
- **Memory claims** - Honest metrics showing both BEAM and Rust allocations
- **`:parallel` threshold** - Updated from 100MB+ to 500MB+ based on actual crossover testing
- Documentation now accurately reflects 3.5x-9x speedups (up to 18x for quoted data)

## [0.1.0] - 2025-01-25

### Added

- Initial release
- Five parsing strategies: `:simd`, `:parallel`, `:streaming`, `:indexed`, `:basic`
- Full NimbleCSV API compatibility
- RFC 4180 compliance with 147 tests
- Configurable separators (CSV, TSV, PSV, etc.)
- Bounded-memory streaming for large files
- Character encoding support: UTF-8, UTF-16 (LE/BE), UTF-32 (LE/BE), Latin-1
- Pre-defined `RustyCSV.Spreadsheet` parser for Excel-compatible UTF-16 LE TSV
- Rust memory tracking for profiling (now opt-in, see Unreleased)
- Comprehensive documentation

### Parsing Strategies

- **`:simd`** - SIMD-accelerated delimiter scanning via `memchr` (default)
- **`:parallel`** - Multi-threaded parsing via `rayon` for 500MB+ files with complex quoting
- **`:streaming`** - Stateful chunked parser for unbounded files
- **`:indexed`** - Two-phase index-then-extract for row range access
- **`:basic`** - Simple byte-by-byte parsing for debugging

### Encoding Support

- `:utf8` - UTF-8 (default, zero overhead)
- `:latin1` - ISO-8859-1 / Latin-1
- `{:utf16, :little}` - UTF-16 Little Endian (Excel/Windows)
- `{:utf16, :big}` - UTF-16 Big Endian
- `{:utf32, :little}` - UTF-32 Little Endian
- `{:utf32, :big}` - UTF-32 Big Endian

### Validation

- csv-spectrum acid test suite (12 tests)
- csv-test-data RFC 4180 suite (17 tests)
- PapaParse-inspired edge cases (53 tests)
- Encoding conversion tests (20 tests)
- Cross-strategy consistency validation
- NimbleCSV output compatibility verification
