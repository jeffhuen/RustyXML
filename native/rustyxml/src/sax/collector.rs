//! SAX Collector
//!
//! Implements ScanHandler to collect SAX events for batch return to Elixir.

use super::events::{CompactSaxEvent, SaxEvent};
use crate::core::unified_scanner::ScanHandler;
use crate::index::Span;

/// Compact SAX result: events + flat attribute quads `(name_off, name_len, val_off, val_len)`.
pub type CompactSaxResult = (Vec<CompactSaxEvent>, Vec<(u32, u32, u32, u32)>);

/// Collector that gathers SAX events during scanning
///
/// Uses compact event storage for memory efficiency.
pub struct SaxCollector {
    /// Collected events
    events: Vec<CompactSaxEvent>,
    /// Attribute storage: (name_offset, name_len, value_offset, value_len)
    attributes: Vec<(u32, u32, u32, u32)>,
}

impl SaxCollector {
    /// Create a new collector
    pub fn new() -> Self {
        Self {
            events: Vec::with_capacity(256),
            attributes: Vec::with_capacity(128),
        }
    }

    /// Create with estimated capacity
    pub fn with_capacity(events: usize, attrs: usize) -> Self {
        Self {
            events: Vec::with_capacity(events),
            attributes: Vec::with_capacity(attrs),
        }
    }

    /// Take the collected events
    pub fn take_events(&mut self) -> Vec<CompactSaxEvent> {
        std::mem::take(&mut self.events)
    }

    /// Get the collected events as a slice
    pub fn events(&self) -> &[CompactSaxEvent] {
        &self.events
    }

    /// Get the attribute storage
    pub fn attributes(&self) -> &[(u32, u32, u32, u32)] {
        &self.attributes
    }

    /// Get number of collected events
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Convert compact events to rich events (for API compatibility)
    pub fn to_sax_events(&self) -> Vec<SaxEvent> {
        self.events.iter().map(|e| self.compact_to_sax(e)).collect()
    }

    /// Convert a compact event to a SaxEvent
    fn compact_to_sax(&self, event: &CompactSaxEvent) -> SaxEvent {
        match event.tag {
            CompactSaxEvent::TAG_START_ELEMENT => {
                let name = event.span();
                let attr_start = event.tertiary as usize;
                let attr_count = event.secondary as usize;

                let attributes: Vec<(Span, Span)> = self
                    .attributes
                    .get(attr_start..attr_start + attr_count)
                    .unwrap_or(&[])
                    .iter()
                    .map(|&(no, nl, vo, vl)| {
                        (
                            Span::new(no, nl.min(u16::MAX as u32) as u16),
                            Span::new(vo, vl.min(u16::MAX as u32) as u16),
                        )
                    })
                    .collect();

                SaxEvent::StartElement { name, attributes }
            }
            CompactSaxEvent::TAG_END_ELEMENT => SaxEvent::EndElement { name: event.span() },
            CompactSaxEvent::TAG_TEXT => SaxEvent::Text {
                span: event.span(),
                needs_decode: event.needs_decode(),
            },
            CompactSaxEvent::TAG_CDATA => SaxEvent::CData { span: event.span() },
            CompactSaxEvent::TAG_COMMENT => SaxEvent::Comment { span: event.span() },
            CompactSaxEvent::TAG_PI => {
                let target = event.span();
                let data = if event.tertiary > 0 {
                    Some(Span::new(
                        event.secondary,
                        event.tertiary.min(u16::MAX as u32) as u16,
                    ))
                } else {
                    None
                };
                SaxEvent::ProcessingInstruction { target, data }
            }
            _ => SaxEvent::Text {
                span: Span::empty(),
                needs_decode: false,
            },
        }
    }
}

impl Default for SaxCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl ScanHandler for SaxCollector {
    fn start_element(&mut self, name: Span, attrs: &[(Span, Span)], is_empty: bool) {
        let attr_start = self.attributes.len() as u32;
        let attr_count = attrs.len() as u32;

        // Store attributes
        for (n, v) in attrs {
            self.attributes
                .push((n.offset, n.len as u32, v.offset, v.len as u32));
        }

        // Add start element event
        self.events.push(CompactSaxEvent::start_element(
            name.offset,
            name.len as u32,
            attr_start,
            attr_count,
        ));

        // If empty element, also add end element
        if is_empty {
            self.events
                .push(CompactSaxEvent::end_element(name.offset, name.len as u32));
        }
    }

    fn end_element(&mut self, name: Span) {
        self.events
            .push(CompactSaxEvent::end_element(name.offset, name.len as u32));
    }

    fn text(&mut self, span: Span, needs_entity_decode: bool) {
        // Skip whitespace-only text at document level (outside elements)
        // This matches typical SAX behavior
        self.events.push(CompactSaxEvent::text(
            span.offset,
            span.len as u32,
            needs_entity_decode,
        ));
    }

    fn cdata(&mut self, span: Span) {
        self.events
            .push(CompactSaxEvent::cdata(span.offset, span.len as u32));
    }

    fn comment(&mut self, span: Span) {
        self.events
            .push(CompactSaxEvent::comment(span.offset, span.len as u32));
    }

    fn processing_instruction(&mut self, target: Span, data: Option<Span>) {
        let (data_offset, data_len) = data.map(|d| (d.offset, d.len as u32)).unwrap_or((0, 0));

        self.events.push(CompactSaxEvent::pi(
            target.offset,
            target.len as u32,
            data_offset,
            data_len,
        ));
    }
}

/// Parse input and return SAX events
pub fn parse_sax(input: &[u8]) -> Vec<SaxEvent> {
    use crate::core::unified_scanner::UnifiedScanner;

    let mut collector = SaxCollector::new();
    let mut scanner = UnifiedScanner::new(input);
    scanner.scan(&mut collector);
    collector.to_sax_events()
}

/// Parse input and return compact SAX events (more efficient)
pub fn parse_sax_compact(input: &[u8]) -> CompactSaxResult {
    use crate::core::unified_scanner::UnifiedScanner;

    let mut collector = SaxCollector::new();
    let mut scanner = UnifiedScanner::new(input);
    scanner.scan(&mut collector);

    let events = collector.take_events();
    let attrs = collector.attributes().to_vec();
    (events, attrs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_parse() {
        let input = b"<root><child/></root>";
        let events = parse_sax(input);

        assert_eq!(events.len(), 4); // start root, start+end child, end root
        assert!(events[0].is_start_element());
    }

    #[test]
    fn test_with_text() {
        let input = b"<a>hello</a>";
        let events = parse_sax(input);

        assert_eq!(events.len(), 3); // start a, text, end a
        assert!(events[1].is_text());
    }

    #[test]
    fn test_with_attributes() {
        let input = b"<root id=\"1\" name=\"test\"/>";
        let events = parse_sax(input);

        assert_eq!(events.len(), 2); // start+end (empty element)

        if let SaxEvent::StartElement { attributes, .. } = &events[0] {
            assert_eq!(attributes.len(), 2);
        } else {
            panic!("Expected StartElement");
        }
    }

    #[test]
    fn test_compact_events() {
        let input = b"<root>text</root>";
        let (events, _attrs) = parse_sax_compact(input);

        assert_eq!(events.len(), 3);
        assert_eq!(events[0].tag, CompactSaxEvent::TAG_START_ELEMENT);
        assert_eq!(events[1].tag, CompactSaxEvent::TAG_TEXT);
        assert_eq!(events[2].tag, CompactSaxEvent::TAG_END_ELEMENT);
    }
}
