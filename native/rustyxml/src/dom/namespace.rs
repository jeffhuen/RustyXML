//! Namespace Resolution
//!
//! Stack-based namespace resolver for XML namespace handling.

use super::strings::StringPool;

/// Well-known namespace URIs
pub mod ns {
    pub const XML: &[u8] = b"http://www.w3.org/XML/1998/namespace";
    pub const XMLNS: &[u8] = b"http://www.w3.org/2000/xmlns/";
}

/// Namespace binding (prefix -> URI)
#[derive(Debug, Clone)]
struct NsBinding {
    prefix_id: u32,
    uri_id: u32,
    depth: u16,
}

/// Stack-based namespace resolver
#[derive(Debug)]
pub struct NamespaceResolver {
    /// Stack of namespace bindings
    bindings: Vec<NsBinding>,
    /// Current element depth
    depth: u16,
    /// Pre-interned prefix IDs
    xml_prefix_id: u32,
    xmlns_prefix_id: u32,
    /// Pre-interned URI IDs
    xml_uri_id: u32,
    xmlns_uri_id: u32,
}

impl NamespaceResolver {
    /// Create a new namespace resolver with pre-declared xml and xmlns namespaces
    pub fn new(strings: &mut StringPool) -> Self {
        let xml_prefix_id = strings.intern(b"xml");
        let xmlns_prefix_id = strings.intern(b"xmlns");
        let xml_uri_id = strings.intern(ns::XML);
        let xmlns_uri_id = strings.intern(ns::XMLNS);

        let mut resolver = NamespaceResolver {
            bindings: Vec::with_capacity(16),
            depth: 0,
            xml_prefix_id,
            xmlns_prefix_id,
            xml_uri_id,
            xmlns_uri_id,
        };

        // Pre-bind xml and xmlns prefixes
        resolver.bindings.push(NsBinding {
            prefix_id: xml_prefix_id,
            uri_id: xml_uri_id,
            depth: 0,
        });
        resolver.bindings.push(NsBinding {
            prefix_id: xmlns_prefix_id,
            uri_id: xmlns_uri_id,
            depth: 0,
        });

        resolver
    }

    /// Enter a new element scope
    pub fn push_scope(&mut self) {
        self.depth += 1;
    }

    /// Leave an element scope, removing any bindings declared in it
    pub fn pop_scope(&mut self) {
        while let Some(binding) = self.bindings.last() {
            if binding.depth < self.depth {
                break;
            }
            self.bindings.pop();
        }
        self.depth = self.depth.saturating_sub(1);
    }

    /// Declare a namespace binding for the current scope
    pub fn declare(&mut self, prefix_id: u32, uri_id: u32) {
        // Don't allow redeclaring xml or xmlns
        if prefix_id == self.xml_prefix_id || prefix_id == self.xmlns_prefix_id {
            return;
        }

        self.bindings.push(NsBinding {
            prefix_id,
            uri_id,
            depth: self.depth,
        });
    }

    /// Declare the default namespace for current scope
    pub fn declare_default(&mut self, uri_id: u32) {
        self.declare(0, uri_id);
    }

    /// Resolve a prefix to a namespace URI ID
    pub fn resolve(&self, prefix_id: u32) -> Option<u32> {
        // Search from most recent to oldest
        for binding in self.bindings.iter().rev() {
            if binding.prefix_id == prefix_id {
                return Some(binding.uri_id);
            }
        }
        None
    }

    /// Resolve the default namespace
    pub fn resolve_default(&self) -> Option<u32> {
        self.resolve(0)
    }

    /// Get current depth
    pub fn depth(&self) -> u16 {
        self.depth
    }

    /// Get all active namespace bindings at current scope
    pub fn active_bindings(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        // Return unique bindings (most recent for each prefix)
        let mut seen_prefixes = std::collections::HashSet::new();
        self.bindings.iter().rev().filter_map(move |b| {
            if seen_prefixes.insert(b.prefix_id) {
                Some((b.prefix_id, b.uri_id))
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_namespaces() {
        let mut strings = StringPool::new();
        let resolver = NamespaceResolver::new(&mut strings);

        let xml_id = strings.intern(b"xml");
        assert!(resolver.resolve(xml_id).is_some());
    }

    #[test]
    fn test_declare_and_resolve() {
        let mut strings = StringPool::new();
        let mut resolver = NamespaceResolver::new(&mut strings);

        let svg_prefix = strings.intern(b"svg");
        let svg_uri = strings.intern(b"http://www.w3.org/2000/svg");

        resolver.push_scope();
        resolver.declare(svg_prefix, svg_uri);

        assert_eq!(resolver.resolve(svg_prefix), Some(svg_uri));
    }

    #[test]
    fn test_scope_pop() {
        let mut strings = StringPool::new();
        let mut resolver = NamespaceResolver::new(&mut strings);

        let prefix = strings.intern(b"foo");
        let uri = strings.intern(b"http://example.com/foo");

        resolver.push_scope();
        resolver.declare(prefix, uri);
        assert_eq!(resolver.resolve(prefix), Some(uri));

        resolver.pop_scope();
        assert_eq!(resolver.resolve(prefix), None);
    }

    #[test]
    fn test_shadow_binding() {
        let mut strings = StringPool::new();
        let mut resolver = NamespaceResolver::new(&mut strings);

        let prefix = strings.intern(b"ns");
        let uri1 = strings.intern(b"http://example.com/ns1");
        let uri2 = strings.intern(b"http://example.com/ns2");

        resolver.push_scope();
        resolver.declare(prefix, uri1);

        resolver.push_scope();
        resolver.declare(prefix, uri2);
        assert_eq!(resolver.resolve(prefix), Some(uri2));

        resolver.pop_scope();
        assert_eq!(resolver.resolve(prefix), Some(uri1));
    }
}
