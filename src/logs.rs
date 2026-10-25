use crate::{Evidence, Result};
use serde::{Deserialize, Serialize};
use std::{fs::File, io::Read, path::Path};

pub const MAX_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_LINES: usize = 30_000;
pub const MAX_LINE_BYTES: usize = 4096;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Line {
    pub number: usize,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Log {
    pub source: String,
    pub lines: Vec<Line>,
    pub truncated: bool,
}

pub fn read(path: &Path) -> Result<Log> {
    crate::discovery::safe_path(path)?;
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|e| format!("Cannot open {}: {e}", path.display()))?
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let truncated = bytes.len() > MAX_BYTES;
    bytes.truncate(MAX_BYTES);
    Ok(parse(
        path.to_string_lossy().into_owned(),
        &bytes,
        truncated,
    ))
}

pub fn parse(source: String, bytes: &[u8], mut truncated: bool) -> Log {
    let text = String::from_utf8_lossy(bytes);
    let mut lines = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if index == MAX_LINES {
            truncated = true;
            break;
        }
        let end = line.floor_char_boundary(MAX_LINE_BYTES.min(line.len()));
        truncated |= end < line.len();
        lines.push(Line {
            number: index + 1,
            text: line[..end].into(),
        });
    }
    Log {
        source,
        lines,
        truncated,
    }
}

impl Log {
    pub fn evidence(&self, index: usize) -> Evidence {
        Evidence {
            source: self.source.clone(),
            line: self.lines[index].number,
            text: self.lines[index].text.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_and_original_numbers() {
        let log = parse(
            "test.log".into(),
            format!("{}\nnext\n", "a".repeat(MAX_LINE_BYTES + 5)).as_bytes(),
            false,
        );
        assert!(log.truncated);
        assert_eq!(log.lines[1].number, 2);
        assert_eq!(log.lines[0].text.len(), MAX_LINE_BYTES);
    }
}
