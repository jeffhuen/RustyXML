//! XML Event Types
//!
//! Event types for pull-parser style XML processing.

use crate::core::attributes::Attribute;
use std::borrow::Cow;

/// XML parsing event
#[derive(Debug, Clone)]
pub enum XmlEvent<'a> {
    /// Start of an element: <name attrs...>
    StartElement(StartElement<'a>),
    /// End of an element: </name>
    EndElement(EndElement<'a>),
    /// Empty element: <name attrs.../>
    EmptyElement(StartElement<'a>),
    /// Text content between tags
    Text(Cow<'a, [u8]>),
    /// CDATA section content
    CData(Cow<'a, [u8]>),
    /// Comment content
    Comment(Cow<'a, [u8]>),
    /// Processing instruction: <?target data?>
    ProcessingInstruction {
        target: Cow<'a, [u8]>,
        data: Option<Cow<'a, [u8]>>,
    },
    /// XML declaration: <?xml version="1.0"?>
    XmlDeclaration {
        version: Cow<'a, [u8]>,
        encoding: Option<Cow<'a, [u8]>>,
        standalone: Option<bool>,
    },
    /// DOCTYPE declaration
    DocType(Cow<'a, [u8]>),
    /// End of document
    EndDocument,
}

/// Start element event data
#[derive(Debug, Clone)]
pub struct StartElement<'a> {
    /// Full element name (may include prefix)
    pub name: Cow<'a, [u8]>,
    /// Local name (after colon)
    pub local_name: Cow<'a, [u8]>,
    /// Namespace prefix (before colon), if any
    pub prefix: Option<Cow<'a, [u8]>>,
    /// Namespace URI (resolved), if any
    pub namespace: Option<Cow<'a, [u8]>>,
    /// Element attributes
    pub attributes: Vec<Attribute<'a>>,
}

impl<'a> StartElement<'a> {
    /// Create a new start element from a byte slice
    pub fn new(name: &'a [u8], attributes: Vec<Attribute<'a>>) -> Self {
        let (prefix, local_name) = split_name(name);
        StartElement {
            name: Cow::Borrowed(name),
            local_name: Cow::Borrowed(local_name),
            prefix: prefix.map(Cow::Borrowed),
            namespace: None,
            attributes,
        }
    }

    /// Create a new start element from a Cow
    pub fn from_cow(name: Cow<'a, [u8]>, attributes: Vec<Attribute<'a>>) -> Self {
        let (prefix, local_name) = match &name {
            Cow::Borrowed(n) => {
                let (p, l) = split_name(n);
                (p.map(Cow::Borrowed), Cow::Borrowed(l))
            }
            Cow::Owned(n) => {
                if let Some(pos) = memchr::memchr(b':', n) {
                    (Some(Cow::Owned(n[..pos].to_vec())), Cow::Owned(n[pos + 1..].to_vec()))
                } else {
                    (None, Cow::Owned(n.clone()))
                }
            }
        };
        StartElement {
            name,
            local_name,
            prefix,
            namespace: None,
            attributes,
        }
    }

    /// Get the name as a string
    pub fn name_str(&self) -> Option<&str> {
        std::str::from_utf8(self.name.as_ref()).ok()
    }

    /// Get the local name as a string
    pub fn local_name_str(&self) -> Option<&str> {
        std::str::from_utf8(self.local_name.as_ref()).ok()
    }

    /// Get an attribute by name
    pub fn get_attribute(&self, name: &[u8]) -> Option<&Attribute<'a>> {
        self.attributes.iter().find(|a| a.name.as_ref() == name)
    }

    /// Get an attribute value by name as string
    pub fn get_attribute_value(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|a| a.name_str() == Some(name))
            .and_then(|a| a.value_str())
    }
}

/// End element event data
#[derive(Debug, Clone)]
pub struct EndElement<'a> {
    /// Full element name
    pub name: Cow<'a, [u8]>,
    /// Local name (after colon)
    pub local_name: Cow<'a, [u8]>,
    /// Namespace prefix (before colon), if any
    pub prefix: Option<Cow<'a, [u8]>>,
}

impl<'a> EndElement<'a> {
    /// Create a new end element from a byte slice
    pub fn new(name: &'a [u8]) -> Self {
        let (prefix, local_name) = split_name(name);
        EndElement {
            name: Cow::Borrowed(name),
            local_name: Cow::Borrowed(local_name),
            prefix: prefix.map(Cow::Borrowed),
        }
    }

    /// Create a new end element from a Cow
    pub fn from_cow(name: Cow<'a, [u8]>) -> Self {
        let (prefix, local_name) = match &name {
            Cow::Borrowed(n) => {
                let (p, l) = split_name(n);
                (p.map(Cow::Borrowed), Cow::Borrowed(l))
            }
            Cow::Owned(n) => {
                if let Some(pos) = memchr::memchr(b':', n) {
                    (Some(Cow::Owned(n[..pos].to_vec())), Cow::Owned(n[pos + 1..].to_vec()))
                } else {
                    (None, Cow::Owned(n.clone()))
                }
            }
        };
        EndElement {
            name,
            local_name,
            prefix,
        }
    }

    /// Get the name as a string
    pub fn name_str(&self) -> Option<&str> {
        std::str::from_utf8(self.name.as_ref()).ok()
    }
}

/// Split a name into prefix and local name at the colon
fn split_name(name: &[u8]) -> (Option<&[u8]>, &[u8]) {
    if let Some(pos) = memchr::memchr(b':', name) {
        (Some(&name[..pos]), &name[pos + 1..])
    } else {
        (None, name)
    }
}

impl<'a> XmlEvent<'a> {
    /// Check if this is a start element event
    pub fn is_start_element(&self) -> bool {
        matches!(self, XmlEvent::StartElement(_) | XmlEvent::EmptyElement(_))
    }

    /// Check if this is an end element event
    pub fn is_end_element(&self) -> bool {
        matches!(self, XmlEvent::EndElement(_))
    }

    /// Check if this is a text event
    pub fn is_text(&self) -> bool {
        matches!(self, XmlEvent::Text(_))
    }

    /// Get as start element if applicable
    pub fn as_start_element(&self) -> Option<&StartElement<'a>> {
        match self {
            XmlEvent::StartElement(e) | XmlEvent::EmptyElement(e) => Some(e),
            _ => None,
        }
    }

    /// Get as end element if applicable
    pub fn as_end_element(&self) -> Option<&EndElement<'a>> {
        match self {
            XmlEvent::EndElement(e) => Some(e),
            _ => None,
        }
    }

    /// Get text content if applicable
    pub fn as_text(&self) -> Option<&[u8]> {
        match self {
            XmlEvent::Text(t) | XmlEvent::CData(t) => Some(t.as_ref()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_start_element() {
        let elem = StartElement::new(b"div", vec![]);
        assert_eq!(elem.name_str(), Some("div"));
        assert_eq!(elem.local_name_str(), Some("div"));
        assert!(elem.prefix.is_none());
    }

    #[test]
    fn test_namespaced_element() {
        let elem = StartElement::new(b"svg:rect", vec![]);
        assert_eq!(elem.name_str(), Some("svg:rect"));
        assert_eq!(elem.local_name_str(), Some("rect"));
        assert_eq!(elem.prefix.as_ref().and_then(|p| std::str::from_utf8(p.as_ref()).ok()), Some("svg"));
    }
}
