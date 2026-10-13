//! Plain text: UTF-8 (with or without BOM), UTF-16 LE/BE (BOM or sniffed), else Windows-1252.
//! Export writes UTF-8 with CRLF line ends; list paragraphs get their label and a tab, table
//! cells are separated by tabs.

use wordcraft_doc::Document;

pub use wordcraft_doc::encoding::cp1252;

use crate::model::{FBlock, ListCounter, from_doc};

fn utf16(b: &[u8], le: bool) -> String {
    let units: Vec<u16> = b.as_chunks::<2>().0.iter().map(|c| if le { u16::from_le_bytes(*c) } else { u16::from_be_bytes(*c) }).collect();
    String::from_utf16_lossy(&units)
}

/// UTF-16 without a BOM, told by its zero bytes: `Some(true)` little-endian, `Some(false)`
/// big-endian.
fn unmarked_utf16(b: &[u8]) -> Option<bool> {
    if b.len() < 4 || !b.len().is_multiple_of(2) {
        return None;
    }
    let sample = b.get(..b.len().min(512))?;
    let even_zero = sample.iter().step_by(2).filter(|x| **x == 0).count();
    let odd_zero = sample.iter().skip(1).step_by(2).filter(|x| **x == 0).count();
    let half = sample.len() / 2;
    if odd_zero * 10 >= half * 4 && even_zero * 10 < half {
        return Some(true);
    }
    if even_zero * 10 >= half * 4 && odd_zero * 10 < half {
        return Some(false);
    }
    None
}

/// Does `b` look like plain text, so a file with an extension WordCraft doesn't know (a
/// playlist, a log, a config file…) can open as text the way Word's text import opens it?
///
/// Text has a BOM, or reads as UTF-16 by its zero bytes, or has no NUL byte and next to no
/// other control characters in its first 64 KB. PDFs are refused by their signature (they
/// start as text). An empty file is text.
pub fn is_text(b: &[u8]) -> bool {
    if b.starts_with(&[0xEF, 0xBB, 0xBF]) || b.starts_with(&[0xFF, 0xFE]) || b.starts_with(&[0xFE, 0xFF]) {
        return true;
    }
    if b.starts_with(b"%PDF-") {
        return false;
    }
    let sample = b.get(..b.len().min(64 << 10)).unwrap_or(b);
    // Tab, line feed, form feed, carriage return, DOS end-of-file (^Z) and escape appear in text;
    // other controls in more than 1 of 100 characters mean binary.
    let odd = |c: u32| c == 0 || (c < 0x20 && !matches!(c, 0x09 | 0x0A | 0x0C | 0x0D | 0x1A | 0x1B)) || c == 0x7F;
    if let Some(le) = unmarked_utf16(sample) {
        let units = sample.as_chunks::<2>().0;
        let n = units.iter().filter(|c| odd(u32::from(if le { u16::from_le_bytes(**c) } else { u16::from_be_bytes(**c) }))).count();
        return n.saturating_mul(100) <= units.len();
    }
    if sample.contains(&0) {
        return false;
    }
    sample.iter().filter(|c| odd(u32::from(**c))).count().saturating_mul(100) <= sample.len()
}

/// Decode text bytes: BOMs first, then UTF-16 without BOM (by its zero bytes), UTF-8, and
/// Windows-1252 as the last resort.
pub fn decode(b: &[u8]) -> String {
    if let Some(rest) = b.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if let Some(rest) = b.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(rest, true);
    }
    if let Some(rest) = b.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(rest, false);
    }
    if let Some(le) = unmarked_utf16(b) {
        return utf16(b, le);
    }
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => b.iter().map(|c| cp1252(*c)).collect(),
    }
}

/// A document with one paragraph per line.
pub fn import(bytes: &[u8]) -> Document {
    let text = decode(bytes);
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let clean: String = text.chars().filter(|c| !c.is_control() || matches!(c, '\n' | '\t' | '\u{000C}')).collect();
    Document::from_text(&clean)
}

fn blocks_text(blocks: &[FBlock], out: &mut Vec<String>, lists: &mut ListCounter, depth: usize) {
    for b in blocks {
        match b {
            FBlock::Para(p) => {
                let mut line = String::new();
                match p.list {
                    Some(li) => {
                        line.push_str(&"\t".repeat(li.level as usize));
                        line.push_str(&lists.label(li));
                        line.push('\t');
                    }
                    None => lists.reset(),
                }
                line.push_str(&p.shown_text().replace('\n', "\r\n").replace(['\u{000C}', '\u{000E}'], ""));
                out.push(line);
            }
            FBlock::Table(t) => {
                lists.reset();
                for row in &t.rows {
                    let cells: Vec<String> = row
                        .iter()
                        .map(|c| {
                            if depth > 8 {
                                return String::new();
                            }
                            let mut v = Vec::new();
                            blocks_text(&c.blocks, &mut v, &mut ListCounter::default(), depth + 1);
                            v.join(" ")
                        })
                        .collect();
                    out.push(cells.join("\t"));
                }
            }
        }
    }
}

/// UTF-8 text, CRLF line ends.
pub fn export(doc: &Document) -> Vec<u8> {
    let flow = from_doc(doc);
    let mut lines = Vec::new();
    blocks_text(&flow.blocks, &mut lines, &mut ListCounter::default(), 0);
    lines.join("\r\n").into_bytes()
}
