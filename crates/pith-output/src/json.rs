//! JSON renderer: one `serde_json::Value` per line (JSON Lines).

use std::io::{self, Write};

use crate::{OutputRecord, Renderer};

pub struct JsonRenderer<W: Write> {
    out: W,
}

impl<W: Write> JsonRenderer<W> {
    pub fn new(out: W) -> Self {
        Self { out }
    }
}

impl<W: Write> Renderer for JsonRenderer<W> {
    fn emit(&mut self, record: &OutputRecord) -> io::Result<()> {
        let line = serde_json::to_string(record)?;
        writeln!(self.out, "{line}")
    }

    fn write_raw(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.out.write_all(bytes)
    }

    fn finish(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CacheOutcome, OutputRecord};

    #[test]
    fn json_emits_one_object_per_line() {
        let recs = [
            OutputRecord::cache(CacheOutcome::Hit),
            OutputRecord::cache(CacheOutcome::Miss),
        ];
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut r = JsonRenderer::new(&mut buf);
            for rec in &recs {
                r.emit(rec).unwrap();
            }
            r.finish().unwrap();
        }
        let s = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines.len(), 2);
        // each line parses as its own JSON object
        for line in &lines {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(
                v.get("kind").and_then(serde_json::Value::as_str),
                Some("cache")
            );
        }
        // and the two outcomes differ
        let first: serde_json::Value = serde_json::from_str(lines.first().unwrap()).unwrap();
        let second: serde_json::Value = serde_json::from_str(lines.get(1).unwrap()).unwrap();
        assert_ne!(first.get("outcome"), second.get("outcome"));
    }
}
