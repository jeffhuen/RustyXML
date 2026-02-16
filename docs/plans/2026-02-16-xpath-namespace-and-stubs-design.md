# XPath 1.0: Namespace Support & Stub Resolution

**Date:** 2026-02-16
**Issue:** https://github.com/jeffhuen/RustyXML/issues/2
**Status:** Approved

## Problem

Several XPath 1.0 functions and features return stub values instead of correct results. The README claims "Full XPath 1.0" but `namespace-uri()`, `id()`, `lang()`, `$variable`, the namespace axis, and QName node test matching are all unimplemented.

## Decisions

| Item | Decision | Rationale |
|------|----------|-----------|
| `namespace-uri()` | Implement | DOM already stores namespace URIs |
| `lang()` | Implement | Ancestor walk for `xml:lang` attribute |
| `id()` | Explicit error | DTD processing disabled for XXE security |
| `$variable` | Explicit error | XSLT feature, not relevant for standalone XPath |
| Namespace axis | Implement via ancestor walk | Rarely used but correct |
| QName node tests | Implement | Required for namespace-aware queries |
| Index support | DOM-only for now | Keep index compact, defer to future work |

## Design

### 1. DocumentAccess Trait

Add 2 new required methods (no defaults — compiler catches missing impls):

```rust
fn node_namespace_uri(&self, id: NodeId) -> Option<&str>;
fn node_prefix(&self, id: NodeId) -> Option<&str>;
```

- `XmlDocument`: looks up `namespace_id`/`prefix_id` via string pool
- `IndexedDocumentView`: returns `None` (deferred)

### 2. Functions

**`namespace-uri(node?)`** — calls `doc.node_namespace_uri()`, returns URI or `""`.

**`lang(string)`** — walks ancestors checking `xml:lang` attribute. Case-insensitive subtag prefix matching per XPath 1.0 spec: `lang("en")` matches `xml:lang="en-US"`.

**`id()`** — returns `Err("id() is not supported: DTD processing is disabled for security (XXE prevention)")`.

### 3. Variable References

**`$variable`** — returns `Err("Variable references ($name) are not supported")`.

### 4. Namespace Axis

Walk ancestors collecting `xmlns:*` attributes. Closest ancestor wins for each prefix (shadowing). Always include the implicit `xml` -> `http://www.w3.org/XML/1998/xml` binding. Returns synthetic namespace nodes per XPath 1.0 data model.

### 5. QName Node Test Matching

- `prefix:local` — resolve prefix to namespace URI via `xmlns:prefix` ancestor walk, match both URI and local name
- `prefix:*` — resolve prefix to namespace URI, match all elements with that namespace URI

### 6. README

Document `id()` and `$variable` as known limitations with rationale. Keep "Full XPath 1.0" with footnoted exceptions.

## Files

| File | Change |
|------|--------|
| `dom/mod.rs` | Add 2 trait methods |
| `dom/document.rs` | Implement for `XmlDocument` |
| `index/view.rs` | Implement (returns `None`) |
| `xpath/functions.rs` | `namespace-uri`, `lang`, `id` |
| `xpath/eval.rs` | `$variable` error |
| `xpath/axes.rs` | Namespace axis, QName matching, namespace wildcard matching |
| `README.md` | Document limitations |
| Tests | One test per feature |
