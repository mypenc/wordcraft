//! File format dispatch by extension. Format crates plug in here.

use wordcraft_doc::Document;

#[cfg(not(target_arch = "wasm32"))]
mod safe_save;

/// Formats WordCraft opens.
pub const OPEN_EXTS: &[&str] = &[
    "docx",
    "docm",
    "dotx",
    "dotm",
    "doc",
    "dot",
    "txt",
    "md",
    "markdown",
    "html",
    "htm",
    "rtf",
    "odt",
    "tex",
    "latex",
    "ltx",
    "xml",
    "wcraft.json",
    "json",
];
/// Formats WordCraft saves (Save As).
pub const SAVE_EXTS: &[&str] = &["docx", "docm", "dotx", "dotm", "pdf", "txt", "md", "html", "rtf", "odt", "tex", "png", "json", "xml"];

/// The extension of a file name or path, lower-case; empty when the file name has none (a dot
/// in a folder name doesn't count).
fn ext_of(name: &str) -> String {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    file.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase()).unwrap_or_default()
}

/// Is `ext` one of Word's package formats (.docx/.docm/.dotx/.dotm), which save without loss?
pub fn is_word_package(ext: &str) -> bool {
    wordcraft_docx::Flavor::from_ext(ext).is_some()
}

/// How a copy in another format is written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExportOptions {
    /// Print Markup: PDFs show tracked changes marked up and, with `comments`, each page scaled
    /// down beside a markup area with the comments in balloons (page images get the area too);
    /// HTML, Markdown and text copies end with the comments.
    pub markup: bool,
    /// With `markup`, show the comments (Track Changes Options › Comments).
    pub comments: bool,
    /// An `.xml` copy is a Word 2003 XML Document (WordprocessingML 2003) instead of a Word XML
    /// Document (Flat OPC): `format: "wordml2003"` on the save commands.
    pub word2003: bool,
}

/// A document password: wiped from memory when dropped, never printed (`Debug` shows `***`).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Password(zeroize::Zeroizing<String>);

impl Password {
    pub fn new(s: &str) -> Password {
        Password(zeroize::Zeroizing::new(s.to_string()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    /// The text, for a password box to edit in place.
    pub fn as_mut_string(&mut self) -> &mut String {
        &mut self.0
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Debug for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Password(***)")
    }
}

/// Does this open error mean the file is password-protected and no password was given?
pub fn needs_password(err: &str) -> bool {
    err.contains(wordcraft_docx::PASSWORD_REQUIRED)
}

/// Does this open error mean the password given was wrong?
pub fn wrong_password(err: &str) -> bool {
    err.contains(wordcraft_docx::WRONG_PASSWORD)
}

/// Can files with this extension be saved with a password (Word's packages)?
pub fn can_encrypt(name: &str) -> bool {
    is_word_package(&ext_of(name))
}

/// Parse a document from bytes; `name` gives the format by extension.
pub fn open_bytes(name: &str, bytes: &[u8]) -> Result<Document, String> {
    open_bytes_with(name, bytes, None).map(|(doc, _)| doc)
}

/// [`open_bytes`] for a file that may be password-protected: a Word document encrypted with a
/// password ([MS-OFFCRYPTO] agile encryption) is decrypted with `password` first. Also says
/// whether the file was encrypted. Without the password needed the error satisfies
/// [`needs_password`]; with a wrong one, [`wrong_password`].
pub fn open_bytes_with(name: &str, bytes: &[u8], password: Option<&str>) -> Result<(Document, bool), String> {
    let ext = ext_of(name);
    if matches!(ext.as_str(), "docx" | "docm" | "dotx" | "dotm" | "doc" | "dot") && wordcraft_docx::is_encrypted(bytes) {
        let mut doc = wordcraft_docx::read_with_password(bytes, password).map_err(|e| e.to_string())?;
        doc.ensure_nonempty();
        return Ok((doc, true));
    }
    if ext.is_empty() || signature_contradicts(&ext, bytes) {
        return open_by_content(name, &ext, bytes, password);
    }
    let mut doc = match ext.as_str() {
        // Plain text goes through the formats importer: BOMs, unmarked UTF-16, Windows-1252.
        "txt" | "text" | "" => wordcraft_formats::txt::import(bytes),
        "json" => serde_json::from_slice::<Document>(bytes).map_err(|e| format!("{name}: {e}"))?,
        other => match crate::io_ext::open(other, bytes) {
            Some(r) => r?,
            None => return open_by_content(name, other, bytes, password),
        },
    };
    doc.ensure_nonempty();
    Ok((doc, false))
}

/// The narrowest text width of the document's sections, points: what a picture must fit.
pub(crate) fn text_width(doc: &Document) -> f32 {
    doc.sections().iter().map(|(_, sp)| sp.text_width()).fold(f32::INFINITY, f32::min).clamp(36.0, 4000.0)
}

/// Shrink inline pictures wider than `max_w` to it, keeping their proportions. Layout keeps a
/// picture's size (as Word does), so formats whose pictures come at their pixel size (HTML,
/// Markdown, ODT, RTF) are fitted when they are read, as Insert › Pictures does.
pub(crate) fn fit_block_pictures(b: &mut wordcraft_doc::Block, max_w: f32) {
    crate::cmd::paste::each_para(b, 0, &mut |p| {
        let mut changed = false;
        for o in &mut p.objects {
            if let wordcraft_doc::InlineObject::Image { w, h, float, .. } = o
                && float.wrap == wordcraft_doc::para::Wrap::Inline
                && w.is_finite()
                && *w > max_w
            {
                let k = max_w / *w;
                *w = max_w;
                *h = (*h * k).max(1.0);
                changed = true;
            }
        }
        if changed {
            p.touch();
        }
    });
}

/// [`fit_block_pictures`] for a whole document read from a file, against its own text width.
pub(crate) fn fit_pictures(doc: &mut Document) {
    let max_w = text_width(doc);
    for b in &mut doc.body {
        fit_block_pictures(std::sync::Arc::make_mut(b), max_w);
    }
}

/// Does the file hold a different document format than its Word or RTF extension says? An RTF
/// file saved as `.doc` (as accounting systems and report generators often save their
/// documents), a `.docx` package renamed `.doc` or a Word 97-2003 file renamed `.docx`: Word opens
/// each by what it holds, so they go through [`open_by_content`] instead of failing.
fn signature_contradicts(ext: &str, bytes: &[u8]) -> bool {
    const ZIP: &[u8] = b"PK\x03\x04";
    const OLE: &[u8] = &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    let held = if bytes.starts_with(ZIP) {
        "zip"
    } else if bytes.starts_with(OLE) {
        "ole"
    } else if bytes.starts_with(b"{\\rtf") {
        "rtf"
    } else {
        return false;
    };
    let expected = match ext {
        "docx" | "docm" | "dotx" | "dotm" => "zip",
        "doc" | "dot" => "ole",
        "rtf" => "rtf",
        _ => return false,
    };
    held != expected
}

/// A file whose extension names no format WordCraft knows (`.m3u`, `.log`, `.ini`, none…),
/// opened by what it holds, as Word does: a Word document or RTF file by its signature, any
/// other text through the plain-text import with encoding detection (#569). Anything else is
/// refused with a clear error rather than shown as garbage.
fn open_by_content(name: &str, ext: &str, bytes: &[u8], password: Option<&str>) -> Result<(Document, bool), String> {
    const ZIP: &[u8] = b"PK\x03\x04";
    const OLE: &[u8] = &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    let (mut doc, encrypted) = if bytes.starts_with(OLE) && wordcraft_docx::is_encrypted(bytes) {
        (wordcraft_docx::read_with_password(bytes, password).map_err(|e| e.to_string())?, true)
    } else if bytes.starts_with(ZIP) {
        (wordcraft_docx::read(bytes).map_err(|e| format!("{name}: not a document WordCraft can open ({e})"))?, false)
    } else if bytes.starts_with(OLE) {
        (wordcraft_docbin::read(bytes).map_err(|e| format!("{name}: not a document WordCraft can open ({e})"))?, false)
    } else if bytes.starts_with(b"{\\rtf") {
        (wordcraft_formats::rtf::import(bytes)?, false)
    } else if wordcraft_formats::txt::is_text(bytes) {
        (wordcraft_formats::txt::import(bytes), false)
    } else if ext.is_empty() {
        return Err(format!("{name}: not a document or text file WordCraft can open"));
    } else {
        return Err(format!("{name}: unsupported format `.{ext}` (not a document or text file)"));
    };
    doc.ensure_nonempty();
    Ok((doc, encrypted))
}

/// Serialise a document; `name` gives the format by extension.
pub fn save_bytes(name: &str, doc: &Document) -> Result<Vec<u8>, String> {
    save_bytes_opts(name, doc, &ExportOptions::default())
}

/// [`save_bytes`] with export options (Print Markup).
pub fn save_bytes_opts(name: &str, doc: &Document, opts: &ExportOptions) -> Result<Vec<u8>, String> {
    let ext = ext_of(name);
    match ext.as_str() {
        // List paragraphs get their label and a tab, as in Word's plain-text save.
        "txt" | "text" => {
            let bytes = wordcraft_formats::txt::export(doc);
            Ok(if opts.markup && opts.comments { wordcraft_formats::comments::append("txt", doc, bytes) } else { bytes })
        }
        "json" => serde_json::to_vec_pretty(doc).map_err(|e| e.to_string()),
        other => match crate::io_ext::save(other, doc, opts) {
            Some(r) => r,
            None => Err(format!("{name}: can't save as `.{other}`")),
        },
    }
}

/// [`save_bytes`], encrypted with `password` when there is one and the format is a Word package
/// (other formats are copies and are written as they are).
pub fn save_bytes_with(name: &str, doc: &Document, password: Option<&str>, opts: &ExportOptions) -> Result<Vec<u8>, String> {
    let bytes = save_bytes_opts(name, doc, opts)?;
    match password {
        Some(pw) if can_encrypt(name) => wordcraft_docx::encrypt(&bytes, pw).map_err(|e| e.to_string()),
        _ => Ok(bytes),
    }
}

pub fn open_path(path: &std::path::Path) -> Result<Document, String> {
    open_path_with(path, None).map(|(doc, _)| doc)
}

/// [`open_path`] for a file that may be password-protected (see [`open_bytes_with`]).
#[cfg(not(target_arch = "wasm32"))]
pub fn open_path_with(path: &std::path::Path, password: Option<&str>) -> Result<(Document, bool), String> {
    crate::file_access::check_read(&path.to_string_lossy())?;
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.len() > 2 << 30 {
        return Err(format!("{}: file is larger than 2 GB", path.display()));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let name = path.to_string_lossy();
    if matches!(ext_of(&name).as_str(), "html" | "htm" | "xhtml") {
        // Pictures referenced by a relative path load from beside the HTML file.
        let images = LocalImages::new(path.parent().unwrap_or(std::path::Path::new("")));
        let mut doc = wordcraft_formats::html::import_with(&wordcraft_formats::html::decode(&bytes), &|src| images.load(src));
        fit_pictures(&mut doc);
        doc.ensure_nonempty();
        return Ok((doc, false));
    }
    open_bytes_with(&name, &bytes, password)
}

/// The pictures an HTML file names by a path relative to its folder.
///
/// Only regular files inside the folder (after resolving `..` and symlinks) are read. The folder is
/// the HTML file's own, which `open_path_with` has already checked against the automation read root. URLs
/// (`http:`, `file:`…) and absolute paths are not followed. Each file is read at most once and
/// at most `max_file` bytes of it; every reference, cached or not, counts against a budget for
/// the whole document, so one large picture can't be multiplied by referencing it many times.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct LocalImages {
    /// The folder, canonical; `None` when it can't be resolved (nothing loads then).
    dir: Option<std::path::PathBuf>,
    max_file: u64,
    /// Bytes the document may still load.
    left: std::cell::Cell<u64>,
    cache: std::cell::RefCell<Vec<(std::path::PathBuf, std::sync::Arc<Vec<u8>>)>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl LocalImages {
    /// Largest picture file read.
    const MAX_FILE: u64 = 256 << 20;
    /// Picture bytes one document may load, counting every reference.
    const BUDGET: u64 = 1 << 30;
    /// Files whose bytes are kept for further references.
    const CACHE: usize = 64;

    pub(crate) fn new(dir: &std::path::Path) -> LocalImages {
        LocalImages::with_limits(dir, LocalImages::MAX_FILE, LocalImages::BUDGET)
    }

    pub(crate) fn with_limits(dir: &std::path::Path, max_file: u64, budget: u64) -> LocalImages {
        let dir = if dir.as_os_str().is_empty() { std::path::Path::new(".") } else { dir };
        LocalImages { dir: std::fs::canonicalize(dir).ok(), max_file, left: std::cell::Cell::new(budget), cache: Default::default() }
    }

    /// Bytes of the file `src` names, or `None` when it isn't a regular file inside the folder,
    /// is too large, or the budget is spent.
    pub(crate) fn load(&self, src: &str) -> Option<std::sync::Arc<Vec<u8>>> {
        let path = self.resolve(src)?;
        let cached = self.cache.borrow().iter().find(|(p, _)| *p == path).map(|(_, d)| d.clone());
        if let Some(data) = cached {
            self.charge(data.len())?;
            return Some(data);
        }
        let limit = self.max_file.min(self.left.get());
        let bytes = read_regular_file(&path, limit)?;
        // What was read is spent even when it's over the limit.
        self.charge(bytes.len())?;
        if bytes.len() as u64 > limit {
            return None;
        }
        let data = std::sync::Arc::new(bytes);
        let mut cache = self.cache.borrow_mut();
        if cache.len() < LocalImages::CACHE {
            cache.push((path, data.clone()));
        }
        Some(data)
    }

    /// Take `n` bytes from the budget; `None` (and the budget emptied) when there aren't enough.
    fn charge(&self, n: usize) -> Option<()> {
        let left = self.left.get().checked_sub(n as u64);
        self.left.set(left.unwrap_or(0));
        left.map(|_| ())
    }

    /// The canonical path `src` names, if it's inside the folder.
    fn resolve(&self, src: &str) -> Option<std::path::PathBuf> {
        let src = src.split(['#', '?']).next().unwrap_or("");
        if src.is_empty() || src.starts_with("//") || src.split_once(':').is_some_and(|(scheme, _)| !scheme.contains('/')) {
            return None;
        }
        let rel = String::from_utf8(wordcraft_formats::model::percent_decode(src)).ok()?;
        let rel = std::path::Path::new(&rel);
        if rel.components().any(|c| matches!(c, std::path::Component::Prefix(_) | std::path::Component::RootDir)) {
            return None;
        }
        let dir = self.dir.as_ref()?;
        // Canonical paths have `..` and symlinks resolved, so this keeps both inside the folder.
        let path = std::fs::canonicalize(dir.join(rel)).ok()?;
        path.starts_with(dir).then_some(path)
    }
}

/// Up to `limit + 1` bytes of the regular file at `path` (more than `limit` means too large), or
/// `None` for anything else. A named pipe blocks when opened and a device reports a length of 0
/// and may never end, so the path is checked before opening, the opened handle is checked again,
/// and the read is bounded whatever length the file reports.
#[cfg(not(target_arch = "wasm32"))]
fn read_regular_file(path: &std::path::Path, limit: u64) -> Option<Vec<u8>> {
    use std::io::Read;
    if !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let file = std::fs::File::open(path).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() {
        return None;
    }
    let mut out = Vec::with_capacity(usize::try_from(meta.len().min(limit)).unwrap_or(0));
    file.take(limit.saturating_add(1)).read_to_end(&mut out).ok()?;
    Some(out)
}

/// The bytes of the file at `path`, for commands that take a path (pictures, mail-merge data).
/// Every such read goes through here so the automation read root applies ([`crate::file_access`]).
#[cfg(not(target_arch = "wasm32"))]
pub fn read_file(path: &str) -> Result<Vec<u8>, String> {
    crate::file_access::check_read(path)?;
    std::fs::read(path).map_err(|e| format!("{path}: {e}"))
}

#[cfg(target_arch = "wasm32")]
pub fn read_file(path: &str) -> Result<Vec<u8>, String> {
    Err(format!("{path}: files are opened through the browser on the web"))
}

/// [`read_file`] for a file that must be a regular file of at most `max` bytes, checked before
/// anything is read (a font named by the user, which could be any size).
#[cfg(not(target_arch = "wasm32"))]
pub fn read_file_capped(path: &str, max: u64, what: &str) -> Result<Vec<u8>, String> {
    crate::file_access::check_read(path)?;
    let meta = std::fs::metadata(path).map_err(|e| format!("{path}: {e}"))?;
    if !meta.is_file() {
        return Err(format!("{path}: not a file"));
    }
    if meta.len() > max {
        return Err(format!("{path}: the {what} is larger than {} MB", max >> 20));
    }
    std::fs::read(path).map_err(|e| format!("{path}: {e}"))
}

/// Write `bytes` to the file at `path`, inside the automation write root when there is one.
#[cfg(not(target_arch = "wasm32"))]
pub fn write_file(path: &str, bytes: &[u8]) -> Result<(), String> {
    crate::file_access::check_write(path)?;
    std::fs::write(path, bytes).map_err(|e| format!("{path}: {e}"))
}

#[cfg(target_arch = "wasm32")]
pub fn write_file(path: &str, _bytes: &[u8]) -> Result<(), String> {
    Err(format!("{path}: files are saved through the browser on the web"))
}

#[cfg(target_arch = "wasm32")]
pub fn open_path_with(path: &std::path::Path, _password: Option<&str>) -> Result<(Document, bool), String> {
    Err(format!("{}: files are opened through the browser on the web", path.display()))
}

pub fn save_path(path: &std::path::Path, doc: &Document) -> Result<(), String> {
    save_path_with(path, doc, None, &ExportOptions::default())
}

/// [`save_path`], encrypted with `password` as [`save_bytes_with`] does.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_path_with(path: &std::path::Path, doc: &Document, password: Option<&str>, opts: &ExportOptions) -> Result<(), String> {
    crate::file_access::check_write(&path.to_string_lossy())?;
    let bytes = save_bytes_with(&path.to_string_lossy(), doc, password, opts)?;
    safe_save::write(path, &bytes).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(target_arch = "wasm32")]
pub fn save_path_with(path: &std::path::Path, _doc: &Document, _password: Option<&str>, _opts: &ExportOptions) -> Result<(), String> {
    Err(format!("{}: files are saved through the browser on the web", path.display()))
}

/// Decode text bytes: UTF-8 (with or without BOM), UTF-16 LE/BE (BOM or sniffed), else
/// Windows-1252 (see [`wordcraft_formats::txt::decode`]).
pub fn decode_text(b: &[u8]) -> String {
    wordcraft_formats::txt::decode(b)
}
