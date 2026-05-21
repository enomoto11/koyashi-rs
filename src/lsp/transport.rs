//! JSON-RPC message framing for the Language Server Protocol.
//!
//! Each message is a JSON body preceded by a `Content-Length` header and an
//! empty line, per the base protocol.

use std::io::{BufRead, Write};

use anyhow::{Context, Result};
use serde_json::Value;

/// Write a single framed JSON message to `writer`.
pub fn write_message<W: Write>(writer: &mut W, body: &Value) -> Result<()> {
    let payload = serde_json::to_vec(body)?;
    write!(writer, "Content-Length: {}\r\n\r\n", payload.len())?;
    writer.write_all(&payload)?;
    writer.flush()?;
    Ok(())
}

/// Read a single framed JSON message from `reader`.
///
/// Returns `Ok(None)` when the stream reaches a clean end of input.
pub fn read_message<R: BufRead>(reader: &mut R) -> Result<Option<Value>> {
    let mut content_length: Option<usize> = None;

    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let header = line.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            break;
        }
        if let Some(value) = header.strip_prefix("Content-Length:") {
            content_length = Some(
                value
                    .trim()
                    .parse()
                    .context("received an invalid Content-Length header")?,
            );
        }
    }

    let length = content_length.context("message is missing a Content-Length header")?;
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    let value = serde_json::from_slice(&body).context("message body is not valid JSON")?;
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use serde_json::json;

    use super::*;

    #[test]
    fn write_then_read_round_trips() {
        let mut buffer = Vec::new();
        write_message(&mut buffer, &json!({ "method": "ping" })).unwrap();

        let mut reader = Cursor::new(buffer);
        let message = read_message(&mut reader).unwrap().unwrap();
        assert_eq!(message, json!({ "method": "ping" }));
        assert!(read_message(&mut reader).unwrap().is_none());
    }
}
