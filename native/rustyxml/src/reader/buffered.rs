//! Buffered XML Reader (Strategy B)
//!
//! Reads XML from any source implementing Read trait,
//! using an internal buffer for efficient parsing.

use std::io::Read;

/// Buffer size for reading chunks
const DEFAULT_BUFFER_SIZE: usize = 8192;

/// Buffered XML reader for streaming input
pub struct BufferedReader<R: Read> {
    reader: R,
    buffer: Vec<u8>,
    pos: usize,
    end: usize,
    eof: bool,
}

impl<R: Read> BufferedReader<R> {
    /// Create a new buffered reader
    pub fn new(reader: R) -> Self {
        Self::with_capacity(reader, DEFAULT_BUFFER_SIZE)
    }

    /// Create a new buffered reader with specified buffer capacity
    pub fn with_capacity(reader: R, capacity: usize) -> Self {
        BufferedReader {
            reader,
            buffer: vec![0u8; capacity],
            pos: 0,
            end: 0,
            eof: false,
        }
    }

    /// Fill the buffer from the reader
    pub fn fill_buffer(&mut self) -> std::io::Result<bool> {
        if self.eof {
            return Ok(false);
        }

        // Compact: move remaining data to start
        if self.pos > 0 {
            let remaining = self.end - self.pos;
            if remaining > 0 {
                self.buffer.copy_within(self.pos..self.end, 0);
            }
            self.end = remaining;
            self.pos = 0;
        }

        // Read more data
        let read = self.reader.read(&mut self.buffer[self.end..])?;
        if read == 0 {
            self.eof = true;
            Ok(false)
        } else {
            self.end += read;
            Ok(true)
        }
    }

    /// Get current buffered data as a slice
    pub fn buffered(&self) -> &[u8] {
        &self.buffer[self.pos..self.end]
    }

    /// Check if we've reached end of input
    pub fn is_eof(&self) -> bool {
        self.eof && self.pos >= self.end
    }

    /// Consume n bytes from the buffer
    pub fn consume(&mut self, n: usize) {
        self.pos += n.min(self.end - self.pos);
    }

    /// Read all remaining content into a Vec
    pub fn read_to_end(&mut self) -> std::io::Result<Vec<u8>> {
        let mut result = Vec::new();

        // First, copy buffered data
        result.extend_from_slice(self.buffered());
        self.pos = self.end;

        // Then read the rest
        self.reader.read_to_end(&mut result)?;
        self.eof = true;

        Ok(result)
    }
}

/// Read entire XML document from a Read source
pub fn read_all<R: Read>(mut reader: R) -> std::io::Result<Vec<u8>> {
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer)?;
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_buffered_reader() {
        let data = b"<root>content</root>";
        let cursor = Cursor::new(data.to_vec());
        let mut reader = BufferedReader::new(cursor);

        reader.fill_buffer().unwrap();
        assert_eq!(reader.buffered(), data);
    }

    #[test]
    fn test_read_to_end() {
        let data = b"<root>content</root>";
        let cursor = Cursor::new(data.to_vec());
        let mut reader = BufferedReader::new(cursor);

        let result = reader.read_to_end().unwrap();
        assert_eq!(result, data);
    }
}
