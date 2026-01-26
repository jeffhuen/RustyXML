//! DTD Declaration Store and Validation
//!
//! Collects DTD declarations during parsing and validates them post-parse.
//! This matches xmerl's approach: parse first, validate after.

use std::collections::{HashMap, HashSet};

/// Collected DTD declarations for post-parse validation
#[derive(Debug, Default)]
pub struct DtdDeclarations {
    /// Element declarations: name -> content spec
    pub elements: HashMap<Vec<u8>, ElementDecl>,
    /// Attribute lists: element name -> attributes
    pub attlists: HashMap<Vec<u8>, Vec<AttDef>>,
    /// General entities: name -> definition
    pub entities: HashMap<Vec<u8>, EntityDecl>,
    /// Parameter entities: name -> definition
    pub pe_entities: HashMap<Vec<u8>, EntityDecl>,
    /// Notations: name -> definition
    pub notations: HashMap<Vec<u8>, NotationDecl>,
}

#[derive(Debug, Clone)]
pub struct ElementDecl {
    pub content_spec: ContentSpec,
}

#[derive(Debug, Clone)]
pub enum ContentSpec {
    Empty,
    Any,
    Mixed(Vec<Vec<u8>>),      // List of allowed element names
    Children(Vec<u8>),         // Raw content model (simplified)
}

#[derive(Debug, Clone)]
pub struct AttDef {
    pub name: Vec<u8>,
    pub att_type: AttType,
    pub default: AttDefault,
}

#[derive(Debug, Clone)]
pub enum AttType {
    CData,
    Id,
    IdRef,
    IdRefs,
    Entity,
    Entities,
    NmToken,
    NmTokens,
    Notation(Vec<Vec<u8>>),
    Enumeration(Vec<Vec<u8>>),
}

#[derive(Debug, Clone)]
pub enum AttDefault {
    Required,
    Implied,
    Fixed(Vec<u8>),
    Default(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct EntityDecl {
    pub is_external: bool,
    pub value: Option<Vec<u8>>,          // For internal entities
    pub system_id: Option<Vec<u8>>,      // For external entities
    pub public_id: Option<Vec<u8>>,      // For external entities
    pub ndata: Option<Vec<u8>>,          // For unparsed entities
    pub references: Vec<Vec<u8>>,        // Entities referenced in value
}

#[derive(Debug, Clone)]
pub struct NotationDecl {
    pub system_id: Option<Vec<u8>>,
    pub public_id: Option<Vec<u8>>,
}

impl DtdDeclarations {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an element declaration
    pub fn add_element(&mut self, name: Vec<u8>, content_spec: ContentSpec) -> Result<(), &'static str> {
        if self.elements.contains_key(&name) {
            return Err("Element type declared more than once");
        }
        self.elements.insert(name, ElementDecl { content_spec });
        Ok(())
    }

    /// Add an entity declaration
    pub fn add_entity(&mut self, name: Vec<u8>, decl: EntityDecl, is_pe: bool) -> Result<(), &'static str> {
        let map = if is_pe { &mut self.pe_entities } else { &mut self.entities };
        // First declaration wins (per XML spec)
        if !map.contains_key(&name) {
            map.insert(name, decl);
        }
        Ok(())
    }

    /// Add a notation declaration
    pub fn add_notation(&mut self, name: Vec<u8>, decl: NotationDecl) -> Result<(), &'static str> {
        if self.notations.contains_key(&name) {
            return Err("Notation declared more than once");
        }
        self.notations.insert(name, decl);
        Ok(())
    }

    /// Validate all declarations (post-parse)
    pub fn validate(&self) -> Result<(), String> {
        // Check for entity recursion
        self.check_entity_recursion()?;

        // Check NOTATION attribute types reference declared notations
        self.check_notation_references()?;

        // Check ENTITY attribute types (entities must be unparsed)
        self.check_entity_attributes()?;

        Ok(())
    }

    /// Check for circular entity references
    fn check_entity_recursion(&self) -> Result<(), String> {
        for (name, _) in &self.entities {
            let mut visited = HashSet::new();
            let mut stack = vec![name.clone()];

            while let Some(current) = stack.pop() {
                if visited.contains(&current) {
                    if current == *name {
                        return Err(format!(
                            "Entity '{}' references itself (directly or indirectly)",
                            String::from_utf8_lossy(name)
                        ));
                    }
                    continue;
                }
                visited.insert(current.clone());

                if let Some(decl) = self.entities.get(&current) {
                    for ref_name in &decl.references {
                        stack.push(ref_name.clone());
                    }
                }
            }
        }
        Ok(())
    }

    /// Check NOTATION attributes reference declared notations
    fn check_notation_references(&self) -> Result<(), String> {
        for (_elem, attrs) in &self.attlists {
            for attr in attrs {
                if let AttType::Notation(names) = &attr.att_type {
                    for name in names {
                        if !self.notations.contains_key(name) {
                            return Err(format!(
                                "Notation '{}' used in attribute but not declared",
                                String::from_utf8_lossy(name)
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Check ENTITY attributes reference unparsed entities
    fn check_entity_attributes(&self) -> Result<(), String> {
        // This would check that ENTITY/ENTITIES attribute default values
        // reference unparsed entities, but we don't parse defaults fully yet
        Ok(())
    }
}

/// Parse entity value and extract entity references
pub fn extract_entity_references(value: &[u8]) -> Vec<Vec<u8>> {
    let mut refs = Vec::new();
    let mut pos = 0;

    while pos < value.len() {
        if value[pos] == b'&' && pos + 1 < value.len() && value[pos + 1] != b'#' {
            // Entity reference (not character reference)
            pos += 1;
            let start = pos;
            while pos < value.len() && value[pos] != b';' {
                pos += 1;
            }
            if pos < value.len() {
                refs.push(value[start..pos].to_vec());
            }
        }
        pos += 1;
    }

    refs
}

/// Parse content spec from DTD ELEMENT declaration
pub fn parse_content_spec(content: &[u8]) -> Result<ContentSpec, &'static str> {
    let content = skip_ws(content);

    if content.starts_with(b"EMPTY") {
        Ok(ContentSpec::Empty)
    } else if content.starts_with(b"ANY") {
        Ok(ContentSpec::Any)
    } else if content.starts_with(b"(") {
        let inner = &content[1..];
        let inner = skip_ws(inner);
        if inner.starts_with(b"#PCDATA") {
            // Mixed content - extract element names
            let names = parse_mixed_names(&content[1..]);
            Ok(ContentSpec::Mixed(names))
        } else {
            // Children content model - store raw for now
            Ok(ContentSpec::Children(content.to_vec()))
        }
    } else {
        Err("Invalid content specification")
    }
}

/// Extract element names from mixed content: (#PCDATA|a|b)*
fn parse_mixed_names(content: &[u8]) -> Vec<Vec<u8>> {
    let mut names = Vec::new();
    let mut pos = 0;
    let len = content.len();

    // Skip past #PCDATA
    while pos < len && content[pos] != b'|' && content[pos] != b')' {
        pos += 1;
    }

    while pos < len {
        if content[pos] == b'|' {
            pos += 1;
            // Skip whitespace
            while pos < len && matches!(content[pos], b' ' | b'\t' | b'\n' | b'\r') {
                pos += 1;
            }
            // Read name
            let start = pos;
            while pos < len && is_name_char(content[pos]) {
                pos += 1;
            }
            if pos > start {
                names.push(content[start..pos].to_vec());
            }
        } else if content[pos] == b')' {
            break;
        } else {
            pos += 1;
        }
    }

    names
}

#[inline]
fn skip_ws(content: &[u8]) -> &[u8] {
    let mut pos = 0;
    while pos < content.len() && matches!(content[pos], b' ' | b'\t' | b'\n' | b'\r') {
        pos += 1;
    }
    &content[pos..]
}

#[inline]
fn is_name_char(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b':') || b >= 0x80
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_recursion_direct() {
        let mut dtd = DtdDeclarations::new();
        dtd.add_entity(
            b"foo".to_vec(),
            EntityDecl {
                is_external: false,
                value: Some(b"&foo;".to_vec()),
                system_id: None,
                public_id: None,
                ndata: None,
                references: vec![b"foo".to_vec()],
            },
            false,
        ).unwrap();

        assert!(dtd.validate().is_err());
    }

    #[test]
    fn test_entity_recursion_indirect() {
        let mut dtd = DtdDeclarations::new();
        dtd.add_entity(
            b"a".to_vec(),
            EntityDecl {
                is_external: false,
                value: Some(b"&b;".to_vec()),
                system_id: None,
                public_id: None,
                ndata: None,
                references: vec![b"b".to_vec()],
            },
            false,
        ).unwrap();
        dtd.add_entity(
            b"b".to_vec(),
            EntityDecl {
                is_external: false,
                value: Some(b"&a;".to_vec()),
                system_id: None,
                public_id: None,
                ndata: None,
                references: vec![b"a".to_vec()],
            },
            false,
        ).unwrap();

        assert!(dtd.validate().is_err());
    }

    #[test]
    fn test_no_recursion() {
        let mut dtd = DtdDeclarations::new();
        dtd.add_entity(
            b"a".to_vec(),
            EntityDecl {
                is_external: false,
                value: Some(b"hello".to_vec()),
                system_id: None,
                public_id: None,
                ndata: None,
                references: vec![],
            },
            false,
        ).unwrap();

        assert!(dtd.validate().is_ok());
    }

    #[test]
    fn test_extract_entity_refs() {
        let value = b"Hello &world; and &foo;!";
        let refs = extract_entity_references(value);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0], b"world");
        assert_eq!(refs[1], b"foo");
    }
}
