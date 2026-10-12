//! Mailings tab: mail merge (recipients from CSV/JSON or an Excel/OpenDocument spreadsheet,
//! merge fields, preview, finish), envelopes and labels.

use serde_json::{Value, json};
use wordcraft_doc::para::InlineObject;
use wordcraft_doc::props::{Align, CharProps};
use wordcraft_doc::section::{SectionProps, SectionStart};
use wordcraft_doc::{Block, Blocks, Document, Paragraph, Part, PartKind, Pos, StoryRef, Table, para_block};

use super::{delete_selection, sel_result};
use crate::{CmdError, CmdResult, CommandSpec, Selection, Session, p};

/// The mail-merge data source and preview state (lives in the session, not the document).
#[derive(Clone, Debug, Default)]
pub struct MergeState {
    pub kind: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub preview: bool,
    pub record: usize,
    pub highlight: bool,
}

fn has_recipients(s: &Session) -> Option<&'static str> {
    if s.merge.rows.is_empty() { Some("select recipients first") } else { None }
}

/// Edit Recipient List needs a list to edit (fields, even with no entries yet).
fn has_list(s: &Session) -> Option<&'static str> {
    if s.merge.headers.is_empty() && s.merge.rows.is_empty() { Some("select recipients first") } else { None }
}

/// The kinds of document Start Mail Merge makes. `normal` is an ordinary document again; only
/// `directory` changes how Finish & Merge joins the records (no page break between them).
pub const MERGE_KINDS: [&str; 6] = ["letters", "emails", "envelopes", "labels", "directory", "normal"];

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("mailings.start", "Start Mail Merge", "Mailings › Start Mail Merge", |s, v| {
            let kind = p::str(v, "kind").unwrap_or("letters").trim().to_ascii_lowercase();
            if !MERGE_KINDS.contains(&kind.as_str()) {
                return Err(CmdError::Params(format!("`kind` must be one of {}", MERGE_KINDS.join(", "))));
            }
            s.merge.kind = kind;
            Ok(json!({"kind": s.merge.kind}))
        })
        .params(r#"{"kind": "letters|emails|envelopes|labels|directory|normal"}"#)
        .pure(),
        CommandSpec::new("mailings.recipients", "Select Recipients", "Mailings › Start Mail Merge", recipients)
            .params(r#"{"csv"?: string, "path"?: string (.csv/.tsv/.txt, or a .xlsx/.xlsm/.ods workbook), "data"?: base64 (a file's bytes, as for path), "sheet"?: name | number (1 = first; a workbook with several sheets and no sheet answers {"sheets": [...], "chooseSheet": true} and loads nothing), "headers"?: bool (default true: the first non-blank row names the fields; false: fields are Column 1…N), "rows"?: [{field: value}] | [[value]], "fields"?: [string] (column order; required with array rows)}"#)
            .pure(),
        CommandSpec::new("mailings.editRecipients", "Edit Recipient List", "Mailings › Start Mail Merge", |s, v| {
            if v.get("rows").and_then(Value::as_array).is_some() {
                return recipients(s, &json!({"rows": v.get("rows"), "fields": v.get("fields")}));
            }
            Ok(json!({"headers": s.merge.headers, "rows": s.merge.rows}))
        })
        .params(r#"{"rows"?: [{field: value}] | [[value]], "fields"?: [string]}"#)
        .key(super::non_mac("Alt+Shift+E"))
        .when(has_list)
        .pure(),
        CommandSpec::new("mailings.insertField", "Insert Merge Field", "Mailings › Write & Insert Fields", |s, v| {
            let f = p::req_str(v, "field")?.to_string();
            insert_field(s, &format!("MERGEFIELD {}", quote(&f)), &format!("«{f}»"))
        })
        .params(r#"{"field": string}"#)
        .key(super::non_mac("Alt+Shift+F")),
        CommandSpec::new("mailings.addressBlock", "Address Block", "Mailings › Write & Insert Fields", |s, _| {
            insert_field(s, "ADDRESSBLOCK", "«AddressBlock»")
        }),
        CommandSpec::new("mailings.greetingLine", "Greeting Line", "Mailings › Write & Insert Fields", |s, _| {
            insert_field(s, "GREETINGLINE", "«GreetingLine»")
        }),
        CommandSpec::new("mailings.rules", "Rules", "Mailings › Write & Insert Fields", |s, v| {
            let rule = p::str(v, "rule").unwrap_or("NEXT").to_ascii_uppercase();
            let instr = match rule.as_str() {
                "IF" => format!(
                    "IF {{ MERGEFIELD {} }} = \"{}\" \"{}\" \"{}\"",
                    quote(p::str(v, "field").unwrap_or("Field")),
                    p::str(v, "value").unwrap_or(""),
                    p::str(v, "then").unwrap_or(""),
                    p::str(v, "else").unwrap_or("")
                ),
                "SKIPIF" | "NEXTIF" => {
                    format!("{rule} {{ MERGEFIELD {} }} = \"{}\"", quote(p::str(v, "field").unwrap_or("Field")), p::str(v, "value").unwrap_or(""))
                }
                "MERGEREC" => "MERGEREC".into(),
                _ => "NEXT".into(),
            };
            insert_field(s, &instr, "")
        })
        .params(r#"{"rule": "IF|SKIPIF|NEXTIF|NEXT|MERGEREC", "field"?, "value"?, "then"?, "else"?}"#),
        CommandSpec::new("mailings.matchFields", "Match Fields", "Mailings › Write & Insert Fields", |s, _| {
            Ok(json!({"fields": s.merge.headers, "address": address_map(&s.merge.headers)}))
        })
        .pure(),
        CommandSpec::new("mailings.highlightFields", "Highlight Merge Fields", "Mailings › Write & Insert Fields", |s, v| {
            s.merge.highlight = p::bool(v, "value").unwrap_or(!s.merge.highlight);
            Ok(json!({"value": s.merge.highlight}))
        })
        .pure(),
        CommandSpec::new("mailings.preview", "Preview Results", "Mailings › Preview Results", |s, v| {
            s.merge.preview = p::bool(v, "value").unwrap_or(!s.merge.preview);
            refresh(s);
            Ok(json!({"preview": s.merge.preview, "record": s.merge.record + 1}))
        })
        .key(super::non_mac("Alt+Shift+K"))
        .when(has_recipients),
        CommandSpec::new("mailings.next", "Next Record", "Mailings › Preview Results", |s, _| step(s, 1)).when(has_recipients),
        CommandSpec::new("mailings.previous", "Previous Record", "Mailings › Preview Results", |s, _| step(s, -1)).when(has_recipients),
        CommandSpec::new("mailings.findRecipient", "Find Recipient", "Mailings › Preview Results", |s, v| {
            let q = p::req_str(v, "text")?.to_lowercase();
            let idx = s
                .merge
                .rows
                .iter()
                .position(|r| r.iter().any(|c| c.to_lowercase().contains(&q)))
                .ok_or_else(|| CmdError::Failed("no matching recipient".into()))?;
            s.merge.record = idx;
            s.merge.preview = true;
            refresh(s);
            Ok(json!({"record": idx + 1}))
        })
        .when(has_recipients),
        CommandSpec::new("mailings.checkErrors", "Check for Errors", "Mailings › Finish", |s, _| {
            let mut missing = Vec::new();
            for f in merge_fields(&s.doc) {
                if !s.merge.headers.iter().any(|h| h.eq_ignore_ascii_case(&f)) {
                    missing.push(f);
                }
            }
            Ok(json!({"ok": missing.is_empty(), "unknownFields": missing, "records": s.merge.rows.len()}))
        })
        .pure(),
        CommandSpec::new("mailings.finish", "Finish & Merge", "Mailings › Finish", finish)
            .params(r#"{"path"?: string (save the merged document), "from"?: n, "to"?: n}"#)
            .key(super::non_mac("Alt+Shift+N"))
            .when(has_recipients),
        CommandSpec::new("mailings.envelopes", "Envelopes", "Mailings › Create", envelopes).params(
            r#"{"delivery": string, "return"?: string, "size"?: "Envelope #10|#9|Monarch|DL|C4|C5|C6|B5", "width"?: pt, "height"?: pt, "add"?: bool (add to this document instead of a new one)}"#,
        ),
        CommandSpec::new("mailings.labels", "Labels", "Mailings › Create", labels).params(
            r#"{"text"?: string, "product"?: "letter-3x10|letter-2x10|letter-2x7|letter-2x5|letter-4x20|a4-3x7|a4-2x7|a4-3x8|a4-2x4", "rows"?: n, "cols"?: n, "fromRecipients"?: bool, "single"?: {"row": n, "col": n}}"#,
        ),
    ]
}

fn quote(f: &str) -> String {
    if f.contains(' ') { format!("\"{f}\"") } else { f.to_string() }
}

fn insert_field(s: &mut Session, instr: &str, placeholder: &str) -> CmdResult {
    let props = s.typing_props();
    let at = delete_selection(s)?;
    let end = s.doc.insert_object(
        &at,
        InlineObject::Field { instr: instr.to_string(), result: placeholder.to_string(), locked: false, code: false },
        &props,
    )?;
    s.sel = Selection::caret(end);
    refresh(s);
    sel_result(s)
}

/// Parse CSV (RFC 4180-ish: quotes, doubled quotes, commas/semicolons/tabs).
pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let first = text.lines().next().unwrap_or("");
    let sep = [',', ';', '\t'].into_iter().max_by_key(|c| first.matches(*c).count()).unwrap_or(',');
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut q = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if q {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cell.push('"');
                    chars.next();
                } else {
                    q = false;
                }
            } else {
                cell.push(c);
            }
        } else if c == '"' {
            q = true;
        } else if c == sep {
            row.push(std::mem::take(&mut cell));
        } else if c == '\n' || c == '\r' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            row.push(std::mem::take(&mut cell));
            if row.iter().any(|x| !x.is_empty()) {
                rows.push(std::mem::take(&mut row));
            } else {
                row.clear();
            }
        } else {
            cell.push(c);
        }
        if rows.len() > 100_000 {
            break;
        }
    }
    row.push(cell);
    if row.iter().any(|x| !x.is_empty()) {
        rows.push(row);
    }
    rows
}

/// Most records and fields a recipient list keeps.
const MAX_RECORDS: usize = 100_000;
const MAX_FIELDS: usize = 1_000;

fn cell(x: &Value) -> String {
    match x {
        Value::Null => String::new(),
        Value::String(t) => t.clone(),
        other => other.to_string(),
    }
}

/// A recipient table from `rows` (objects keyed by field, or arrays in `fields` order). Without
/// `fields`, the columns are the objects' keys.
fn table_from_rows(fields: Option<&Vec<Value>>, arr: &[Value]) -> Result<(Vec<String>, Vec<Vec<String>>), CmdError> {
    let headers: Vec<String> = match fields {
        Some(f) => f.iter().take(MAX_FIELDS).map(|h| cell(h).trim().to_string()).collect(),
        None => {
            if arr.iter().any(Value::is_array) {
                return Err(CmdError::Params("`fields` is required when `rows` are arrays".into()));
            }
            let mut headers: Vec<String> = Vec::new();
            for o in arr.iter().filter_map(Value::as_object) {
                for k in o.keys() {
                    if !headers.contains(k) && headers.len() < MAX_FIELDS {
                        headers.push(k.clone());
                    }
                }
            }
            headers
        }
    };
    let rows = arr
        .iter()
        .take(MAX_RECORDS)
        .filter_map(|r| match r {
            Value::Array(cells) => Some((0..headers.len()).map(|i| cells.get(i).map(cell).unwrap_or_default()).collect()),
            Value::Object(o) => Some(headers.iter().map(|h| o.get(h).map(cell).unwrap_or_default()).collect()),
            _ => None,
        })
        .collect();
    Ok((headers, rows))
}

fn recipients(s: &mut Session, v: &Value) -> CmdResult {
    let mut sheet = Value::Null;
    let (headers, rows) = if let Some(arr) = v.get("rows").and_then(Value::as_array) {
        table_from_rows(v.get("fields").and_then(Value::as_array), arr)?
    } else {
        let all = if let Some(c) = p::str(v, "csv") {
            parse_csv(c)
        } else {
            let (bytes, ext) = if let Some(d) = p::str(v, "data") {
                let b = super::insert::base64_decode(d.trim()).ok_or_else(|| CmdError::Params("`data` isn't valid base64".into()))?;
                (b, p::str(v, "path").and_then(extension))
            } else if let Some(path) = p::str(v, "path") {
                (read_file(path)?, extension(path))
            } else {
                return Err(CmdError::Params("`csv`, `path`, `data` or `rows` required".into()));
            };
            let spreadsheet =
                ext.as_deref().is_some_and(wordcraft_formats::sheet::is_extension) || wordcraft_formats::sheet::detect(&bytes).is_some();
            if spreadsheet {
                use wordcraft_formats::sheet::{Pick, read_rows, sheet_names};
                let pick = match v.get("sheet") {
                    Some(Value::String(n)) => Pick::Name(n.as_str()),
                    Some(x) if x.as_u64().is_some_and(|n| n >= 1) => {
                        Pick::Index(usize::try_from(x.as_u64().unwrap_or(1).saturating_sub(1)).unwrap_or(usize::MAX))
                    }
                    Some(Value::Null) | None => {
                        let names = sheet_names(&bytes).map_err(CmdError::Failed)?;
                        if names.len() > 1 {
                            return Ok(json!({"sheets": names, "chooseSheet": true}));
                        }
                        Pick::Index(0)
                    }
                    Some(_) => return Err(CmdError::Params("`sheet` must be a sheet name or number".into())),
                };
                let (names, rows) = read_rows(&bytes, pick).map_err(CmdError::Failed)?;
                let chosen = match pick {
                    Pick::Index(i) => names.get(i).cloned(),
                    Pick::Name(n) => {
                        names.iter().find(|s| s.as_str() == n).or_else(|| names.iter().find(|s| s.eq_ignore_ascii_case(n.trim()))).cloned()
                    }
                };
                sheet = json!(chosen);
                rows
            } else {
                parse_csv(&crate::io::decode_text(&bytes))
            }
        };
        header_split(all, p::bool(v, "headers").unwrap_or(true))?
    };
    s.merge.headers = headers;
    s.merge.rows = rows;
    s.merge.record = 0;
    refresh(s);
    let mut r = json!({"fields": s.merge.headers, "records": s.merge.rows.len()});
    if !sheet.is_null() {
        r["sheet"] = sheet;
    }
    Ok(r)
}

/// A file's extension, lower case.
fn extension(path: &str) -> Option<String> {
    std::path::Path::new(path).extension().map(|e| e.to_string_lossy().to_ascii_lowercase())
}

fn read_file(path: &str) -> Result<Vec<u8>, CmdError> {
    crate::io::read_file(path).map_err(CmdError::Failed)
}

/// Field names and records from a table's rows: with `headers`, the first row names the fields
/// (blank names become `Column N`, repeats are numbered); otherwise every row is a record and
/// the fields are `Column 1`…`Column N`. Records are padded or cut to the fields, and capped.
fn header_split(mut all: Vec<Vec<String>>, headers: bool) -> Result<(Vec<String>, Vec<Vec<String>>), CmdError> {
    if all.is_empty() {
        return Err(CmdError::Failed("the recipient list is empty".into()));
    }
    let fields = if headers {
        let first = all.remove(0);
        wordcraft_formats::sheet::field_names(&first[..first.len().min(MAX_FIELDS)])
    } else {
        let width = all.iter().map(Vec::len).max().unwrap_or(0).min(MAX_FIELDS);
        (1..=width).map(|i| format!("Column {i}")).collect()
    };
    all.truncate(MAX_RECORDS);
    for r in &mut all {
        r.resize(fields.len(), String::new());
    }
    Ok((fields, all))
}

fn step(s: &mut Session, d: i64) -> CmdResult {
    let n = s.merge.rows.len().max(1) as i64;
    s.merge.record = (s.merge.record as i64 + d).clamp(0, n - 1) as usize;
    s.merge.preview = true;
    refresh(s);
    Ok(json!({"record": s.merge.record + 1, "of": n}))
}

/// Field names used by MERGEFIELD fields in the document (body, headers and footers).
fn merge_fields(doc: &Document) -> Vec<String> {
    let mut v = Vec::new();
    for (story, path) in merge_stories(doc).into_iter().flat_map(|st| doc.para_paths(st).into_iter().map(move |p| (st, p))) {
        if let Some(p) = doc.para(story, &path) {
            for o in &p.objects {
                if let InlineObject::Field { instr, .. } = o
                    && let Some(name) = mergefield_name(instr)
                    && !v.contains(&name)
                {
                    v.push(name);
                }
            }
        }
    }
    v
}

fn mergefield_name(instr: &str) -> Option<String> {
    let rest = instr.trim().strip_prefix("MERGEFIELD")?.trim();
    let name = if let Some(r) = rest.strip_prefix('"') { r.split('"').next().unwrap_or("") } else { rest.split_whitespace().next().unwrap_or("") };
    Some(name.to_string())
}

/// Which columns hold address parts.
fn address_map(headers: &[String]) -> Value {
    let find = |names: &[&str]| headers.iter().find(|h| names.iter().any(|n| h.replace([' ', '_'], "").eq_ignore_ascii_case(n))).cloned();
    json!({
        "firstName": find(&["FirstName", "First", "GivenName"]),
        "lastName": find(&["LastName", "Last", "Surname", "FamilyName"]),
        "company": find(&["Company", "Organization", "CompanyName"]),
        "address": find(&["Address", "AddressLine1", "Address1", "Street"]),
        "city": find(&["City", "Town"]),
        "state": find(&["State", "Province", "Region"]),
        "postal": find(&["PostalCode", "ZIP", "ZipCode", "Postcode"]),
        "country": find(&["Country"]),
        "title": find(&["Title", "CourtesyTitle"]),
    })
}

fn value(s: &Session, row: usize, field: &str) -> String {
    let Some(i) = s.merge.headers.iter().position(|h| h.eq_ignore_ascii_case(field)) else { return String::new() };
    s.merge.rows.get(row).and_then(|r| r.get(i)).cloned().unwrap_or_default()
}

fn address_block(s: &Session, row: usize) -> String {
    let m = address_map(&s.merge.headers);
    let g = |k: &str| m.get(k).and_then(Value::as_str).map(|f| value(s, row, f)).unwrap_or_default();
    let name = [g("title"), g("firstName"), g("lastName")].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" ");
    let city_line = [g("city"), [g("state"), g("postal")].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" ")]
        .into_iter()
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    [name, g("company"), g("address"), city_line, g("country")].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join("\n")
}

fn greeting(s: &Session, row: usize) -> String {
    let m = address_map(&s.merge.headers);
    let g = |k: &str| m.get(k).and_then(Value::as_str).map(|f| value(s, row, f)).unwrap_or_default();
    let name = [g("firstName"), g("lastName")].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" ");
    if name.is_empty() { "Dear Sir or Madam,".into() } else { format!("Dear {name},") }
}

/// What a merge-related field does for the current record.
enum Act {
    /// Show this text.
    Text(String),
    /// NEXT / a true NEXTIF: later fields in this merged document use the next record.
    Next,
    /// A true SKIPIF: drop this merged document and go on with the next record.
    Skip,
}

/// Comparison of a rule condition `{ MERGEFIELD X } op "value" …` (IF, SKIPIF, NEXTIF) for
/// `row`: whether it holds, and the quoted texts after the value (IF's true / false text).
fn condition<'a>(s: &Session, t: &'a str, row: usize) -> (bool, Vec<&'a str>) {
    let inner = t.split_once('{').and_then(|(_, r)| r.split('}').next()).unwrap_or("");
    let field = mergefield_name(inner).unwrap_or_default();
    let rest = t.split_once('}').map(|(_, r)| r).unwrap_or("");
    let op: String = rest.trim_start().chars().take_while(|c| matches!(c, '=' | '<' | '>')).collect();
    let mut quoted = rest.split('"').skip(1).step_by(2);
    let cmp = quoted.next().unwrap_or("");
    let v = value(s, row, &field);
    // Numbers compare as numbers (Word: "7" = "7.0"), everything else as text.
    let ord = match (v.trim().parse::<f64>(), cmp.trim().parse::<f64>()) {
        (Ok(a), Ok(b)) => a.partial_cmp(&b),
        _ => Some(v.as_str().cmp(cmp)),
    };
    use std::cmp::Ordering::{Equal, Greater, Less};
    let holds = match op.as_str() {
        "<>" => ord != Some(Equal),
        "<" => ord == Some(Less),
        ">" => ord == Some(Greater),
        "<=" => matches!(ord, Some(Less | Equal)),
        ">=" => matches!(ord, Some(Greater | Equal)),
        _ => ord == Some(Equal),
    };
    (holds, quoted.collect())
}

/// The first word of a field instruction, upper-cased.
fn keyword(instr: &str) -> String {
    instr.split_whitespace().next().unwrap_or("").to_ascii_uppercase()
}

/// Evaluate a merge-related field for a record (None = not a merge field).
fn act(s: &Session, instr: &str, row: usize) -> Option<Act> {
    let t = instr.trim();
    if let Some(n) = mergefield_name(t) {
        return Some(Act::Text(value(s, row, &n)));
    }
    Some(match keyword(t).as_str() {
        "ADDRESSBLOCK" => Act::Text(address_block(s, row)),
        "GREETINGLINE" => Act::Text(greeting(s, row)),
        "MERGEREC" => Act::Text(row.saturating_add(1).to_string()),
        "NEXT" => Act::Next,
        "NEXTIF" if condition(s, t, row).0 => Act::Next,
        "SKIPIF" if condition(s, t, row).0 => Act::Skip,
        "NEXTIF" | "SKIPIF" => Act::Text(String::new()),
        // IF { MERGEFIELD X } = "v" "then" "else" (an IF on other fields isn't ours to touch).
        "IF" if t.contains("MERGEFIELD") => {
            let (holds, texts) = condition(s, t, row);
            Act::Text(texts.get(if holds { 0 } else { 1 }).copied().unwrap_or("").to_string())
        }
        _ => return None,
    })
}

/// What a merge field shows outside Preview Results (None = not a merge field).
fn placeholder(instr: &str) -> Option<String> {
    if let Some(n) = mergefield_name(instr) {
        return Some(format!("«{n}»"));
    }
    match keyword(instr).as_str() {
        "ADDRESSBLOCK" => Some("«AddressBlock»".into()),
        "GREETINGLINE" => Some("«GreetingLine»".into()),
        "NEXT" | "NEXTIF" | "SKIPIF" | "MERGEREC" => Some(String::new()),
        "IF" if instr.contains("MERGEFIELD") => Some(String::new()),
        _ => None,
    }
}

/// The stories a merge resolves: the body, then every header and footer.
fn merge_stories(doc: &Document) -> Vec<StoryRef> {
    std::iter::once(StoryRef::Body).chain(header_footer_ids(doc).into_iter().map(StoryRef::Part)).collect()
}

fn header_footer_ids(doc: &Document) -> Vec<u32> {
    doc.parts.iter().filter(|(_, p)| matches!(p.kind, PartKind::Header | PartKind::Footer)).map(|(id, _)| *id).collect()
}

/// Update merge field results: record values when previewing, «Field» placeholders otherwise.
/// NEXT moves later fields of the previewed document to the next record, as Finish & Merge does.
pub fn refresh(s: &mut Session) {
    let preview = s.merge.preview && !s.merge.rows.is_empty();
    let last = s.merge.rows.len();
    for story in merge_stories(&s.doc) {
        let mut cur = s.merge.record;
        for path in s.doc.para_paths(story) {
            let updates: Vec<(usize, String)> = match s.doc.para(story, &path) {
                Some(p) => p
                    .objects
                    .iter()
                    .enumerate()
                    .filter_map(|(k, o)| {
                        let InlineObject::Field { instr, .. } = o else { return None };
                        let shown = placeholder(instr)?;
                        if !preview {
                            return Some((k, shown));
                        }
                        Some((
                            k,
                            match act(s, instr, cur) {
                                Some(Act::Text(t)) => t,
                                Some(Act::Next) => {
                                    cur = cur.saturating_add(1).min(last);
                                    String::new()
                                }
                                Some(Act::Skip) | None => String::new(),
                            },
                        ))
                    })
                    .collect(),
                None => continue,
            };
            if updates.is_empty() {
                continue;
            }
            if let Ok(p) = s.doc.para_mut(story, &path) {
                for (k, r) in updates {
                    if let Some(InlineObject::Field { result, .. }) = p.objects.get_mut(k) {
                        *result = r;
                    }
                }
                p.touch();
            }
        }
    }
    s.touch();
}

/// Most merged documents' worth of body blocks Finish & Merge builds.
const MAX_MERGED_BLOCKS: usize = 200_000;

/// Build one merged copy per record: fields become plain text; records separated by page breaks
/// (letters) or new paragraphs (directory). A true SKIPIF drops the record's copy; NEXT moves the
/// rest of a copy to the next record, and the following copy starts after it. Headers and footers
/// are merged too: with merge fields in them, each letter becomes its own section with its own
/// header/footer copies.
fn finish(s: &mut Session, v: &Value) -> CmdResult {
    let n = s.merge.rows.len();
    let from = (p::u64(v, "from").unwrap_or(1).max(1) as usize).saturating_sub(1);
    let to = (p::u64(v, "to").unwrap_or(n as u64).min(n as u64)) as usize;
    let directory = s.merge.kind == "directory";
    let hf: Vec<u32> =
        header_footer_ids(&s.doc).into_iter().filter(|id| s.doc.story(StoryRef::Part(*id)).is_some_and(|b| has_merge_fields(b, 0))).collect();
    let per_record = !directory && !hf.is_empty();
    let mut out = s.doc.clone();
    out.body.clear();
    let mut next_id = s.doc.parts.keys().next_back().map_or(1, |k| k.saturating_add(1));
    let mut start = from;
    let mut count = 0usize;
    let mut first_record = None;
    // The section that ends the previous letter (per-record sections only).
    let mut prev_section: Option<SectionProps> = None;
    while start < to {
        let mut body = s.doc.body.clone();
        let record = start;
        let mut cur = start;
        let keep = merge_blocks(s, &mut body, &mut cur, to, 0);
        start = cur.saturating_add(1);
        if !keep {
            continue;
        }
        first_record.get_or_insert(record);
        if per_record {
            // This letter's own header/footer copies, merged with its record.
            let mut map = Vec::new();
            for id in &hf {
                let Some(part) = s.doc.parts.get(id) else { continue };
                if next_id == u32::MAX {
                    return Err(CmdError::Failed("too many headers and footers to merge".into()));
                }
                let mut blocks = part.blocks.clone();
                let mut c = record;
                merge_blocks(s, &mut blocks, &mut c, to, 0);
                out.parts.insert(next_id, Part { kind: part.kind, blocks, body: part.body });
                map.push((*id, next_id));
                next_id += 1;
            }
            let mut first = true;
            for b in &mut body {
                if let Block::Para(p) = std::sync::Arc::make_mut(b)
                    && let Some(sp) = p.section.as_deref_mut()
                {
                    remap_headers(sp, &map, first);
                    first = false;
                }
            }
            let mut last = s.doc.last_section.clone();
            remap_headers(&mut last, &map, first);
            if let Some(prev) = prev_section.replace(last) {
                if !matches!(out.body.last().map(|b| &**b), Some(Block::Para(_))) {
                    out.body.push(para_block(Paragraph::new()));
                }
                if let Some(Block::Para(p)) = out.body.last_mut().map(std::sync::Arc::make_mut) {
                    p.section = Some(Box::new(prev));
                }
            }
        } else if count > 0
            && !directory
            && let Some(Block::Para(p)) = out.body.last_mut().map(std::sync::Arc::make_mut)
        {
            let end = p.len();
            let props = p.props_at(end).clone();
            let _ = p.insert_text(end, "\u{000C}", &props);
        }
        out.body.extend(body);
        count += 1;
        if out.body.len() > MAX_MERGED_BLOCKS {
            break;
        }
    }
    if let Some(last) = prev_section {
        // The template's own header/footer stories are replaced by the letters' copies.
        for id in &hf {
            out.parts.remove(id);
        }
        out.last_section = last;
    } else {
        // One shared header/footer (directory, or no letters): merged with the first record.
        let row = first_record.unwrap_or(from);
        for id in &hf {
            if let Some(part) = out.parts.get_mut(id) {
                let mut c = row;
                merge_blocks(s, &mut part.blocks, &mut c, to, 0);
            }
        }
    }
    out.ensure_nonempty();
    if let Some(path) = p::str(v, "path") {
        crate::io::save_path(std::path::Path::new(path), &out).map_err(CmdError::Failed)?;
        return Ok(json!({"records": count, "path": path}));
    }
    s.set_document(out);
    s.path = None;
    s.dirty = true;
    s.merge.preview = false;
    Ok(json!({"records": count}))
}

/// Point a letter's section at its own header/footer copies; the letter's first section starts
/// on a new page.
fn remap_headers(sp: &mut SectionProps, map: &[(u32, u32)], first: bool) {
    for slot in
        [&mut sp.headers.default, &mut sp.headers.first, &mut sp.headers.even, &mut sp.footers.default, &mut sp.footers.first, &mut sp.footers.even]
    {
        if let Some(id) = slot
            && let Some((_, new)) = map.iter().find(|(old, _)| old == id)
        {
            *id = *new;
        }
    }
    if first {
        sp.start = SectionStart::NextPage;
    }
}

/// Nesting depth of tables the merge descends into.
const MAX_DEPTH: usize = 32;

fn has_merge_fields(blocks: &Blocks, depth: usize) -> bool {
    depth <= MAX_DEPTH
        && blocks.iter().any(|b| match &**b {
            Block::Para(p) => p.objects.iter().any(|o| matches!(o, InlineObject::Field { instr, .. } if placeholder(instr).is_some())),
            Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).any(|c| has_merge_fields(&c.blocks, depth + 1)),
        })
}

/// Replace the merge fields in `blocks` by their text, in document order, starting with record
/// `*cur`. NEXT moves `*cur` on (records from `end` on are blank). Returns false when a SKIPIF
/// drops this merged document.
fn merge_blocks(s: &Session, blocks: &mut Blocks, cur: &mut usize, end: usize, depth: usize) -> bool {
    if depth > MAX_DEPTH {
        return true;
    }
    for b in blocks.iter_mut() {
        match std::sync::Arc::make_mut(b) {
            Block::Para(p) => {
                let mut repl = Vec::new();
                for off in p.object_offsets() {
                    let Some(InlineObject::Field { instr, .. }) = p.object_at(off) else { continue };
                    let row = if *cur < end { *cur } else { s.merge.rows.len() };
                    match act(s, instr, row) {
                        Some(Act::Text(t)) => repl.push((off, t)),
                        Some(Act::Next) => {
                            *cur = cur.saturating_add(1).min(end);
                            repl.push((off, String::new()));
                        }
                        Some(Act::Skip) => return false,
                        None => {}
                    }
                }
                for (off, val) in repl.into_iter().rev() {
                    let props = p.props_of_char(off).clone();
                    let _ = p.delete(off, off + wordcraft_doc::para::OBJ.len_utf8());
                    let _ = p.insert_text(off, &val, &props);
                }
            }
            Block::Table(t) => {
                for c in t.rows.iter_mut().flat_map(|r| &mut r.cells) {
                    if !merge_blocks(s, &mut c.blocks, cur, end, depth + 1) {
                        return false;
                    }
                }
            }
        }
    }
    true
}

/// Common envelope sizes: name, width and height in points (long side first).
pub const ENVELOPE_SIZES: [(&str, f32, f32); 8] = [
    ("Envelope #10", 684.0, 297.0),
    ("Envelope #9", 639.0, 279.0),
    ("Envelope Monarch", 540.0, 279.0),
    ("Envelope DL", 623.6, 311.8),
    ("Envelope C4", 918.4, 649.1),
    ("Envelope C5", 649.1, 459.2),
    ("Envelope C6", 459.2, 323.1),
    ("Envelope B5", 708.7, 498.9),
];

/// Envelope size by name (`Envelope DL`, or just `DL`, any case), or `width`/`height` in points.
fn envelope_size(v: &Value) -> Result<(f32, f32), CmdError> {
    if let (Some(w), Some(h)) = (p::f32(v, "width"), p::f32(v, "height")) {
        let (w, h) = (w.clamp(144.0, 1584.0), h.clamp(144.0, 1584.0));
        return Ok((w.max(h), w.min(h)));
    }
    let name = p::str(v, "size").unwrap_or("Envelope #10").trim();
    ENVELOPE_SIZES
        .iter()
        .find(|(n, ..)| n.eq_ignore_ascii_case(name) || n.strip_prefix("Envelope ").is_some_and(|short| short.eq_ignore_ascii_case(name)))
        .map(|(_, w, h)| (*w, *h))
        .ok_or_else(|| CmdError::Params(format!("unknown envelope size `{name}`")))
}

/// Envelopes: a landscape page with the return address at the top left and the delivery address
/// in the middle. As a new document, or (`add`) as a section of its own at the start of this one.
fn envelopes(s: &mut Session, v: &Value) -> CmdResult {
    let delivery = p::str(v, "delivery").map(str::to_string).unwrap_or_else(|| {
        if s.merge.rows.is_empty() { "Recipient Name\nStreet Address\nCity, ST 00000".into() } else { address_block(s, s.merge.record) }
    });
    let ret = p::str(v, "return").unwrap_or("").to_string();
    let (w, h) = envelope_size(v)?;
    let sect = SectionProps {
        page_w: w,
        page_h: h,
        landscape: true,
        margin_top: 22.0,
        margin_left: 22.0,
        margin_right: 22.0,
        margin_bottom: 22.0,
        ..Default::default()
    };
    // Addresses are capped: a pasted novel is not an address.
    let lines = |t: &str| t.split('\n').take(20).map(str::to_string).collect::<Vec<_>>();
    let mut blocks = Vec::new();
    let ret_lines = if ret.is_empty() { Vec::new() } else { lines(&ret) };
    for l in &ret_lines {
        blocks.push(para_block(Paragraph::with_text(l, CharProps { size: Some(10.0), ..Default::default() }).styled("NoSpacing")));
    }
    let spacer = (6 - ret_lines.len().min(6)).max(1);
    for _ in 0..spacer + 2 {
        blocks.push(para_block(Paragraph::new().styled("NoSpacing")));
    }
    let mut last = None;
    for l in lines(&delivery) {
        let mut p = Paragraph::with_text(&l, CharProps::default()).styled("NoSpacing");
        p.props.indent_left = Some(w * 0.42);
        if let Some(prev) = last.replace(p) {
            blocks.push(para_block(prev));
        }
    }
    if p::bool(v, "add") == Some(true) {
        // The envelope's own section ends with its last address line.
        let mut end = last.unwrap_or_else(|| Paragraph::new().styled("NoSpacing"));
        end.section = Some(Box::new(sect));
        blocks.push(para_block(end));
        let n = blocks.len();
        s.doc.body.splice(0..0, blocks);
        s.sel = Selection::caret(s.doc.start_of(StoryRef::Body));
        return Ok(json!({"width": w, "height": h, "added": n}));
    }
    blocks.extend(last.map(para_block));
    let mut d = Document::new();
    d.last_section = sect;
    d.body = blocks;
    s.set_document(d);
    s.path = None;
    Ok(json!({"width": w, "height": h}))
}

/// A sheet of labels: the label size, how many across and down, and where the first one sits.
/// Lengths in points; the pitch is from one label's edge to the next one's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LabelProduct {
    pub id: &'static str,
    /// What the labels are for: `address`, `shipping` or `return`.
    pub kind: &'static str,
    /// Letter or A4.
    pub paper: &'static str,
    pub cols: usize,
    pub rows: usize,
    pub width: f32,
    pub height: f32,
    pub top: f32,
    pub side: f32,
    pub h_pitch: f32,
    pub v_pitch: f32,
}

const MM: f32 = 72.0 / 25.4;

/// Common label sheets (the sizes are the industry's; the names are ours).
pub const LABEL_PRODUCTS: [LabelProduct; 9] = [
    LabelProduct {
        id: "letter-3x10",
        kind: "address",
        paper: "Letter",
        cols: 3,
        rows: 10,
        width: 189.0,
        height: 72.0,
        top: 36.0,
        side: 13.5,
        h_pitch: 198.0,
        v_pitch: 72.0,
    },
    LabelProduct {
        id: "letter-2x10",
        kind: "address",
        paper: "Letter",
        cols: 2,
        rows: 10,
        width: 288.0,
        height: 72.0,
        top: 36.0,
        side: 11.25,
        h_pitch: 301.5,
        v_pitch: 72.0,
    },
    LabelProduct {
        id: "letter-2x7",
        kind: "address",
        paper: "Letter",
        cols: 2,
        rows: 7,
        width: 288.0,
        height: 96.0,
        top: 60.0,
        side: 11.25,
        h_pitch: 301.5,
        v_pitch: 96.0,
    },
    LabelProduct {
        id: "letter-2x5",
        kind: "shipping",
        paper: "Letter",
        cols: 2,
        rows: 5,
        width: 288.0,
        height: 144.0,
        top: 36.0,
        side: 11.25,
        h_pitch: 301.5,
        v_pitch: 144.0,
    },
    LabelProduct {
        id: "letter-4x20",
        kind: "return",
        paper: "Letter",
        cols: 4,
        rows: 20,
        width: 126.0,
        height: 36.0,
        top: 36.0,
        side: 21.6,
        h_pitch: 147.6,
        v_pitch: 36.0,
    },
    LabelProduct {
        id: "a4-3x7",
        kind: "address",
        paper: "A4",
        cols: 3,
        rows: 7,
        width: 63.5 * MM,
        height: 38.1 * MM,
        top: 15.15 * MM,
        side: 7.21 * MM,
        h_pitch: 66.04 * MM,
        v_pitch: 38.1 * MM,
    },
    LabelProduct {
        id: "a4-2x7",
        kind: "address",
        paper: "A4",
        cols: 2,
        rows: 7,
        width: 99.1 * MM,
        height: 38.1 * MM,
        top: 15.15 * MM,
        side: 4.65 * MM,
        h_pitch: 101.6 * MM,
        v_pitch: 38.1 * MM,
    },
    LabelProduct {
        id: "a4-3x8",
        kind: "address",
        paper: "A4",
        cols: 3,
        rows: 8,
        width: 63.5 * MM,
        height: 33.9 * MM,
        top: 12.9 * MM,
        side: 7.21 * MM,
        h_pitch: 66.04 * MM,
        v_pitch: 33.9 * MM,
    },
    LabelProduct {
        id: "a4-2x4",
        kind: "shipping",
        paper: "A4",
        cols: 2,
        rows: 4,
        width: 99.1 * MM,
        height: 67.7 * MM,
        top: 13.1 * MM,
        side: 4.65 * MM,
        h_pitch: 101.6 * MM,
        v_pitch: 67.7 * MM,
    },
];

impl LabelProduct {
    pub fn by_id(id: &str) -> Option<&'static LabelProduct> {
        LABEL_PRODUCTS.iter().find(|p| p.id == id)
    }
    /// Label size as people say it: inches on Letter, millimetres on A4.
    pub fn size_text(&self) -> String {
        if self.paper == "A4" {
            format!("{:.1} × {:.1} mm", self.width / MM, self.height / MM)
        } else {
            let i = |pt: f32| format!("{}", ((pt / 72.0) * 1000.0).round() / 1000.0);
            format!("{}\" × {}\"", i(self.width), i(self.height))
        }
    }
}

/// Labels: a new document with a table of labels laid out like the sheet (`product`; spacer
/// columns and rows stand for the gaps). A full page of the same `text` (or one recipient per
/// label), or (`single`: `{"row", "col"}`, from 1) one label at that place on the sheet.
fn labels(s: &mut Session, v: &Value) -> CmdResult {
    let prod = match p::str(v, "product") {
        Some(id) => *LabelProduct::by_id(id).ok_or_else(|| CmdError::Params(format!("unknown label product `{id}`")))?,
        None => LABEL_PRODUCTS[0],
    };
    let rows = p::u64(v, "rows").map_or(prod.rows, |n| n.clamp(1, 40) as usize);
    let cols = p::u64(v, "cols").map_or(prod.cols, |n| n.clamp(1, 10) as usize);
    let single = v.get("single").filter(|x| !x.is_null()).map(|x| {
        let at = |k: &str| x.get(k).and_then(Value::as_u64).unwrap_or(1).max(1) as usize;
        (at("row").min(rows), at("col").min(cols))
    });
    let from_rec = single.is_none() && p::bool(v, "fromRecipients").unwrap_or(!s.merge.rows.is_empty() && p::str(v, "text").is_none());
    let text = p::str(v, "text").unwrap_or("Name\nAddress\nCity, ST 00000").to_string();
    let (page_w, page_h) = if prod.paper == "A4" { (595.3, 841.9) } else { (612.0, 792.0) };
    let h_gap = (prod.h_pitch - prod.width).max(0.0);
    let v_gap = (prod.v_pitch - prod.height).max(0.0);
    let mut d = Document::new();
    d.last_section.page_w = page_w;
    d.last_section.page_h = page_h;
    d.last_section.margin_top = prod.top;
    d.last_section.margin_bottom = 0.0;
    d.last_section.margin_left = prod.side;
    d.last_section.margin_right = (page_w - prod.side - prod.h_pitch * (cols - 1) as f32 - prod.width).max(0.0);
    d.last_section.header = 0.0;
    d.last_section.footer = 0.0;
    // Grid columns: label, gap, label, …; rows likewise.
    let grid_cols: Vec<Option<usize>> = (0..cols).flat_map(|c| [Some(c)].into_iter().chain((c + 1 < cols && h_gap > 0.5).then_some(None))).collect();
    let grid_rows: Vec<Option<usize>> = (0..rows).flat_map(|r| [Some(r)].into_iter().chain((r + 1 < rows && v_gap > 0.5).then_some(None))).collect();
    let width: f32 = grid_cols.iter().map(|c| if c.is_some() { prod.width } else { h_gap }).sum();
    let mut t = Table::new(grid_rows.len(), grid_cols.len(), width);
    t.props.style = None;
    t.props.borders = Some(wordcraft_doc::props::Borders::default());
    t.props.fixed = true;
    t.grid = grid_cols.iter().map(|c| if c.is_some() { prod.width } else { h_gap }).collect();
    let mut k = 0usize;
    let mut made = 0usize;
    for (row, r) in t.rows.iter_mut().zip(&grid_rows) {
        row.props.height = Some(if r.is_some() { prod.height } else { v_gap });
        row.props.height_rule = wordcraft_doc::props::HeightRule::Exact;
        for (cell, c) in row.cells.iter_mut().zip(&grid_cols) {
            cell.props.width = Some(if c.is_some() { prod.width } else { h_gap });
            let (Some(r), Some(c)) = (r, c) else { continue };
            let content = match single {
                Some((sr, sc)) if (sr, sc) != (r + 1, c + 1) => String::new(),
                _ if from_rec => {
                    let rec = if k < s.merge.rows.len() { address_block(s, k) } else { String::new() };
                    k += 1;
                    rec
                }
                _ => text.clone(),
            };
            if !content.is_empty() {
                made += 1;
            }
            cell.props.valign = wordcraft_doc::props::VAlign::Center;
            cell.blocks = content
                .split('\n')
                .take(20)
                .map(|l| {
                    let mut p = Paragraph::with_text(l, CharProps::default()).styled("NoSpacing");
                    p.props.align = Some(Align::Left);
                    p.props.indent_left = Some(9.0);
                    para_block(p)
                })
                .collect();
        }
    }
    d.body = vec![std::sync::Arc::new(Block::Table(t)), para_block(Paragraph::new())];
    s.set_document(d);
    s.path = None;
    let _ = Pos::body(0, 0);
    Ok(json!({"labels": if single.is_some() { 1 } else { rows * cols }, "filled": made, "product": prod.id}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_parsing() {
        let r = parse_csv("Name,City\n\"Smith, Ann\",Paris\r\nBo,\"Oslo \"\"N\"\"\"\n\n");
        assert_eq!(r, vec![vec!["Name", "City"], vec!["Smith, Ann", "Paris"], vec!["Bo", "Oslo \"N\""]]);
        assert_eq!(parse_csv("a;b\n1;2")[1], vec!["1", "2"]);
    }

    #[test]
    fn merge_end_to_end() {
        let mut s = Session::new(Document::new());
        s.run("mailings.recipients", &json!({"csv": "First Name,Last Name,City\nAda,Lovelace,London\nAlan,Turing,Wilmslow"})).unwrap();
        s.run("mailings.greetingLine", &json!({})).unwrap();
        s.run("text.newParagraph", &json!({})).unwrap();
        s.run("text.insert", &json!({"text": "See you in "})).unwrap();
        s.run("mailings.insertField", &json!({"field": "City"})).unwrap();
        assert!(s.doc.plain_text(StoryRef::Body).contains("«City»"));
        s.run("mailings.preview", &json!({"value": true})).unwrap();
        assert!(s.doc.plain_text(StoryRef::Body).contains("Dear Ada Lovelace,"));
        s.run("mailings.next", &json!({})).unwrap();
        assert!(s.doc.plain_text(StoryRef::Body).contains("Wilmslow"));
        let chk = s.run("mailings.checkErrors", &json!({})).unwrap();
        assert_eq!(chk["ok"], true);
        let r = s.run("mailings.finish", &json!({})).unwrap();
        assert_eq!(r["records"], 2);
        let t = s.doc.plain_text(StoryRef::Body);
        assert!(t.contains("See you in London") && t.contains("Dear Alan Turing,"), "{t}");
        assert_eq!(s.layout().pages.len(), 2);
    }

    /// A typed list (#240) keeps its column order, and rows may be arrays in that order.
    #[test]
    fn recipients_from_a_workbook_ask_for_the_sheet_when_there_are_several() {
        use std::io::Write;
        let cell = |t: &str| format!(r#"<table:table-cell office:value-type="string"><text:p>{t}</text:p></table:table-cell>"#);
        let row = |cells: &[&str]| format!("<table:table-row>{}</table:table-row>", cells.iter().map(|c| cell(c)).collect::<String>());
        let content = format!(
            r#"<office:document-content xmlns:office="o" xmlns:table="t" xmlns:text="x"><office:body><office:spreadsheet><table:table table:name="Notes">{}</table:table><table:table table:name="Guests">{}{}{}</table:table></office:spreadsheet></office:body></office:document-content>"#,
            row(&["n"]),
            row(&["Name", "", "Name"]),
            row(&["Ada", "London", "A"]),
            row(&["Alan", "Wilmslow", "B"]),
        );
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, body) in [("mimetype", "application/vnd.oasis.opendocument.spreadsheet"), ("content.xml", content.as_str())] {
            z.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(body.as_bytes()).unwrap();
        }
        let data = super::super::insert::base64_encode(&z.finish().unwrap().into_inner());
        let mut s = Session::new(Document::new());
        // Two sheets and none chosen: the names come back and nothing loads.
        let r = s.run("mailings.recipients", &json!({"data": data})).unwrap();
        assert_eq!(r, json!({"sheets": ["Notes", "Guests"], "chooseSheet": true}));
        assert!(s.merge.headers.is_empty());
        let r = s.run("mailings.recipients", &json!({"data": data, "sheet": "guests"})).unwrap();
        assert_eq!(r, json!({"fields": ["Name", "Column 2", "Name 2"], "records": 2, "sheet": "Guests"}));
        assert_eq!(s.merge.rows[1], ["Alan", "Wilmslow", "B"]);
        // Without a header row every row is a record.
        let r = s.run("mailings.recipients", &json!({"data": data, "sheet": 2, "headers": false})).unwrap();
        assert_eq!(r["fields"], json!(["Column 1", "Column 2", "Column 3"]));
        assert_eq!(r["records"], 3);
        assert!(s.run("mailings.recipients", &json!({"data": data, "sheet": 3})).is_err());
        assert!(s.run("mailings.recipients", &json!({"data": data, "sheet": 0})).is_err());
        assert!(s.run("mailings.recipients", &json!({"data": "%%%"})).is_err());
    }

    #[test]
    fn recipients_from_fields_and_array_rows() {
        let mut s = Session::new(Document::new());
        let r = s
            .run("mailings.recipients", &json!({"fields": ["Last Name", " City ", "Age"], "rows": [["Lovelace", "London", 36], ["Turing"], 7]}))
            .unwrap();
        assert_eq!(r, json!({"fields": ["Last Name", "City", "Age"], "records": 2}));
        assert_eq!(
            s.merge.rows,
            vec![vec!["Lovelace".to_string(), "London".into(), "36".into()], vec!["Turing".into(), String::new(), String::new()]]
        );
        // Objects follow `fields` too; missing and null cells are blank.
        s.run("mailings.editRecipients", &json!({"fields": ["Name", "City"], "rows": [{"City": "Paris", "Name": null}]})).unwrap();
        assert_eq!(s.merge.headers, vec!["Name", "City"]);
        assert_eq!(s.merge.rows, vec![vec![String::new(), "Paris".to_string()]]);
        // Array rows need the field names.
        let err = s.run("mailings.recipients", &json!({"rows": [["Ada"]]})).unwrap_err();
        assert!(matches!(err, CmdError::Params(_)), "{err}");
        assert!(s.ui_requests.is_empty(), "programmatic calls never open dialogs");
    }

    /// Start Mail Merge takes only the kinds it knows; Edit Recipient List needs a list.
    #[test]
    fn merge_kinds_and_edit_list_needs_recipients() {
        let mut s = Session::new(Document::new());
        for k in MERGE_KINDS {
            assert_eq!(s.run("mailings.start", &json!({"kind": k})).unwrap(), json!({"kind": k}));
        }
        assert_eq!(s.run("mailings.start", &json!({"kind": " Directory "})).unwrap(), json!({"kind": "directory"}));
        assert!(matches!(s.run("mailings.start", &json!({"kind": "fax"})), Err(CmdError::Params(_))));
        assert_eq!(s.merge.kind, "directory", "a rejected kind leaves the current one");
        assert!(matches!(s.run("mailings.editRecipients", &json!({})), Err(CmdError::Disabled(_))));
        s.run("mailings.recipients", &json!({"csv": "Name\nAda"})).unwrap();
        assert_eq!(s.run("mailings.editRecipients", &json!({})).unwrap(), json!({"headers": ["Name"], "rows": [["Ada"]]}));
    }

    fn merged_text(rule: Option<Value>, rows: Value) -> String {
        let mut s = Session::new(Document::new());
        s.run("mailings.recipients", &json!({ "rows": rows })).unwrap();
        s.run("mailings.start", &json!({"kind": "directory"})).unwrap();
        if let Some(r) = rule {
            s.run("mailings.rules", &r).unwrap();
        }
        s.run("mailings.insertField", &json!({"field": "Name"})).unwrap();
        s.run("mailings.finish", &json!({})).unwrap();
        s.doc.plain_text(StoryRef::Body)
    }

    /// #427: SKIPIF leaves out the recipients that match its condition.
    #[test]
    fn skipif_drops_matching_records() {
        let rows = json!([{"Name": "Alpha", "Skip": "yes"}, {"Name": "Beta", "Skip": "no"}]);
        let skip = |v: &str| Some(json!({"rule": "SKIPIF", "field": "Skip", "value": v}));
        assert_eq!(merged_text(skip("yes"), rows.clone()), "Beta");
        assert_eq!(merged_text(skip("no"), rows.clone()), "Alpha");
        assert_eq!(merged_text(skip("absent"), rows.clone()), "Alpha\nBeta");
        assert_eq!(merged_text(None, rows), "Alpha\nBeta");
        let all = json!([{"Name": "Alpha", "Skip": "yes"}, {"Name": "Beta", "Skip": "yes"}]);
        assert_eq!(merged_text(skip("yes"), all), "");
    }

    /// #428: NEXT moves the following merge fields to the next recipient, and the next merged
    /// copy starts after it.
    #[test]
    fn next_record_advances_following_fields() {
        let mut s = Session::new(Document::new());
        s.run("mailings.recipients", &json!({"rows": [{"Name": "Alpha"}, {"Name": "Beta"}, {"Name": "Gamma"}]})).unwrap();
        s.run("mailings.start", &json!({"kind": "directory"})).unwrap();
        s.run("mailings.insertField", &json!({"field": "Name"})).unwrap();
        s.run("text.insert", &json!({"text": " / "})).unwrap();
        s.run("mailings.rules", &json!({"rule": "NEXT"})).unwrap();
        s.run("mailings.insertField", &json!({"field": "Name"})).unwrap();
        s.run("mailings.preview", &json!({"value": true})).unwrap();
        assert_eq!(s.doc.plain_text(StoryRef::Body), "Alpha / Beta");
        s.run("mailings.finish", &json!({})).unwrap();
        // Gamma has no record after it: NEXT past the end leaves the field blank, never panics.
        assert_eq!(s.doc.plain_text(StoryRef::Body), "Alpha / Beta\nGamma / ");
    }

    /// #429: an IF rule shows its true / false text in Preview Results, as Finish & Merge does.
    #[test]
    fn if_rule_previews_its_result() {
        let mut s = Session::new(Document::new());
        s.run("mailings.recipients", &json!({"rows": [{"Name": "Alpha", "Flag": "yes"}, {"Name": "Beta", "Flag": "no"}]})).unwrap();
        s.run("mailings.start", &json!({"kind": "directory"})).unwrap();
        s.run("mailings.rules", &json!({"rule": "IF", "field": "Flag", "value": "yes", "then": "Selected", "else": "Other"})).unwrap();
        s.run("mailings.preview", &json!({"value": true})).unwrap();
        assert_eq!(s.doc.plain_text(StoryRef::Body), "Selected");
        s.run("mailings.next", &json!({})).unwrap();
        assert_eq!(s.doc.plain_text(StoryRef::Body), "Other");
        s.run("mailings.previous", &json!({})).unwrap();
        assert_eq!(s.doc.plain_text(StoryRef::Body), "Selected");
        s.run("mailings.finish", &json!({})).unwrap();
        assert_eq!(s.doc.plain_text(StoryRef::Body), "Selected\nOther");
    }

    /// #430: merge fields in headers and footers are previewed and merged; each letter gets its
    /// own header.
    #[test]
    fn header_and_footer_fields_merge() {
        let mut s = Session::new(Document::new());
        s.run("mailings.recipients", &json!({"rows": [{"Name": "Alpha"}, {"Name": "Beta"}]})).unwrap();
        let hdr = s.run("insert.header", &json!({"text": "Recipient: "})).unwrap()["story"].as_u64().unwrap() as u32;
        s.run("caret.docEnd", &json!({})).unwrap();
        s.run("mailings.insertField", &json!({"field": "Name"})).unwrap();
        s.run("insert.closeHeader", &json!({})).unwrap();
        s.run("text.insert", &json!({"text": "Body: "})).unwrap();
        s.run("mailings.insertField", &json!({"field": "Name"})).unwrap();
        let header = |s: &Session, id: u32| s.doc.plain_text(StoryRef::Part(id));
        s.run("mailings.preview", &json!({"value": true})).unwrap();
        assert_eq!(header(&s, hdr), "Recipient: Alpha");
        s.run("mailings.preview", &json!({"value": false})).unwrap();
        assert_eq!(header(&s, hdr), "Recipient: «Name»");
        s.run("mailings.finish", &json!({})).unwrap();
        assert_eq!(s.doc.plain_text(StoryRef::Body), "Body: Alpha\nBody: Beta");
        let secs = s.doc.sections();
        assert_eq!(secs.len(), 2, "one section per letter");
        let texts: Vec<String> = secs.iter().map(|(_, sp)| header(&s, sp.headers.default.unwrap())).collect();
        assert_eq!(texts, ["Recipient: Alpha", "Recipient: Beta"]);
        assert!(!s.doc.parts.contains_key(&hdr), "the template header is replaced by the letters' copies");
    }

    #[test]
    fn envelopes_and_labels() {
        let mut s = Session::new(Document::new());
        s.run("mailings.envelopes", &json!({"delivery": "Jo Doe\n1 Main St"})).unwrap();
        assert!(s.doc.last_section.page_w > s.doc.last_section.page_h);
        s.run("mailings.labels", &json!({"text": "Hello"})).unwrap();
        assert!(s.doc.plain_text(StoryRef::Body).matches("Hello").count() == 30);
    }

    /// #407: Envelopes adds an envelope section of the chosen size before the letter; Labels
    /// lays out a sheet of the chosen product, or a single label at a row and column.
    #[test]
    fn envelope_sizes_add_to_document_and_label_products() {
        let mut s = Session::new(Document::from_text("Dear Jo,"));
        let r = s.run("mailings.envelopes", &json!({"delivery": "Jo Doe\n1 Main St", "return": "Me", "size": "C5", "add": true})).unwrap();
        assert_eq!((r["width"].as_f64().map(|w| w.round()), r["height"].as_f64().map(|h| h.round())), (Some(649.0), Some(459.0)));
        let secs = s.doc.sections();
        assert_eq!(secs.len(), 2, "an envelope section of its own");
        assert!(secs[0].1.landscape && (secs[0].1.page_w - 649.1).abs() < 0.1);
        let text = s.doc.plain_text(StoryRef::Body);
        assert!(text.starts_with("Me") && text.contains("1 Main St") && text.trim_end().ends_with("Dear Jo,"), "{text}");
        assert!(s.run("mailings.envelopes", &json!({"size": "Envelope X"})).is_err());

        let r = s.run("mailings.labels", &json!({"text": "Ada", "product": "a4-2x7"})).unwrap();
        assert_eq!(r["labels"], 14);
        assert!((s.doc.last_section.page_w - 595.3).abs() < 0.1, "A4 sheet");
        let Some(Block::Table(t)) = s.doc.body.first().map(|b| &**b) else { panic!("no table") };
        assert_eq!((t.rows.len(), t.grid.len()), (7, 3), "two label columns and the gap between");
        assert_eq!(s.doc.plain_text(StoryRef::Body).matches("Ada").count(), 14);
        let r = s.run("mailings.labels", &json!({"text": "Bo", "product": "letter-3x10", "single": {"row": 2, "col": 3}})).unwrap();
        assert_eq!(r["filled"], 1);
        let Some(Block::Table(t)) = s.doc.body.first().map(|b| &**b) else { panic!("no table") };
        // Row 2, column 3 (grid column 5: label, gap, label, gap, label).
        let cell = t.rows.get(1).and_then(|r| r.cells.get(4)).and_then(|c| c.blocks.first());
        assert!(matches!(cell.map(|b| &**b), Some(Block::Para(p)) if p.text == "Bo"));
        assert!(s.run("mailings.labels", &json!({"product": "nope"})).is_err());
    }
}
