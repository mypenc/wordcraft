//! Rich Text Format.
//!
//! Export: font and colour tables, a small stylesheet (headings with outline levels, Title,
//! Quote, Code), list tables (`\listtable` / `\listoverridetable` with `\listtext` labels for
//! older readers), document info, page setup, paragraph formatting (`\pard`, alignment, page
//! break before), character formatting (`\b \i \ul \strike \super \sub \f \fs \cf \chcbpat`),
//! hyperlinks as `HYPERLINK` fields, bookmarks, pictures (`\pngblip` / `\jpegblip`) and tables
//! (`\trowd … \cellx … \cell … \row`, header rows, vertical merges, shading).
//!
//! Import: a tolerant reader for groups and destinations (unknown `\*` destinations, headers,
//! footers, notes and objects are skipped), `\par \line \tab \page`, `\uN` with `\ucN`
//! fallback skipping, `\'hh` and 8-bit text in the document's code page (`\ansicpg`) or the
//! current font's (`\fcharset`, `\cpg`: Cyrillic, Central European, Greek…; Windows-1252 when
//! none is named), the formatting words above, stylesheet-based
//! headings and `\outlinelevel`, lists through `\ls`/`\ilvl`, fields, bookmarks, pictures and
//! tables. Group nesting and picture sizes are capped.

use std::collections::HashMap;

use wordcraft_doc::{Align, Document, Rgb};

use crate::model::{self, Cell, FBlock, FTable, Flow, Fmt, Inline, Kind, ListInfo, Meta, Para, make_img};

/// Deepest group nesting we track; deeper groups share the innermost state.
const MAX_GROUPS: usize = 512;
/// Largest picture we decode (bytes).
const MAX_PICT: usize = 64 << 20;

// ---------------------------------------------------------------------------------------------
// Export

/// RTF-escape text (non-ASCII as `\uN?`).
pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '\\' => o.push_str("\\\\"),
            '{' => o.push_str("\\{"),
            '}' => o.push_str("\\}"),
            '\t' => o.push_str("\\tab "),
            '\n' => o.push_str("\\line "),
            '\u{000C}' => o.push_str("\\page "),
            '\u{00A0}' => o.push_str("\\~"),
            '\u{00AD}' => o.push_str("\\-"),
            '\u{2011}' => o.push_str("\\_"),
            c if c.is_ascii_control() => {}
            c if c.is_ascii() => o.push(c),
            c => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    o.push_str(&format!("\\u{}?", *u as i16));
                }
            }
        }
    }
    o
}

struct Tables {
    fonts: Vec<String>,
    colors: Vec<Rgb>,
}

impl Tables {
    fn font(&mut self, name: &str) -> usize {
        match self.fonts.iter().position(|f| f.eq_ignore_ascii_case(name)) {
            Some(i) => i,
            None => {
                self.fonts.push(name.to_string());
                self.fonts.len() - 1
            }
        }
    }
    /// Colour table index (1-based; 0 is "auto").
    fn color(&mut self, c: Rgb) -> usize {
        match self.colors.iter().position(|x| *x == c) {
            Some(i) => i + 1,
            None => {
                self.colors.push(c);
                self.colors.len()
            }
        }
    }
}

struct Writer {
    t: Tables,
    /// List definitions: per list, the ordered flag per level.
    lists: Vec<[bool; 9]>,
}

const HEAD_SIZES: [f32; 6] = [20.0, 16.0, 14.0, 12.0, 12.0, 11.0];

fn style_num(k: Kind) -> Option<u32> {
    match k {
        Kind::Heading(n) => Some(u32::from(n.clamp(1, 6))),
        Kind::Title => Some(7),
        Kind::Quote => Some(8),
        Kind::Code => Some(9),
        _ => None,
    }
}

impl Writer {
    fn run(&mut self, t: &str, f: &Fmt, out: &mut String) {
        let mut ctl = String::new();
        if f.bold {
            ctl.push_str("\\b");
        }
        if f.italic {
            ctl.push_str("\\i");
        }
        if f.underline || f.link.is_some() {
            ctl.push_str("\\ul");
        }
        if f.strike {
            ctl.push_str("\\strike");
        }
        if f.sup {
            ctl.push_str("\\super");
        } else if f.sub {
            ctl.push_str("\\sub");
        }
        if f.code {
            let i = self.t.font(model::MONO_FONT);
            ctl.push_str(&format!("\\f{i}"));
        } else if let Some(fam) = &f.font {
            let i = self.t.font(fam);
            ctl.push_str(&format!("\\f{i}"));
        }
        if let Some(s) = f.size {
            ctl.push_str(&format!("\\fs{}", (s * 2.0).round().clamp(2.0, 3276.0) as i32));
        }
        let color = f.color.or(if f.link.is_some() { Some(Rgb(0x05, 0x63, 0xC1)) } else { None });
        if let Some(c) = color {
            let i = self.t.color(c);
            ctl.push_str(&format!("\\cf{i}"));
        }
        if let Some(c) = f.background {
            let i = self.t.color(c);
            ctl.push_str(&format!("\\chcbpat{i}"));
        }
        let body = if ctl.is_empty() { esc(t) } else { format!("{{{ctl} {}}}", esc(t)) };
        match &f.link {
            Some(l) => {
                out.push_str(&format!("{{\\field{{\\*\\fldinst{{HYPERLINK \"{}\"}}}}{{\\fldrslt{{{}}}}}}}", esc(&l.replace('"', "%22")), body));
            }
            None => out.push_str(&body),
        }
    }

    fn picture(&mut self, img: &model::Img, out: &mut String) {
        let (data, blip) = match img.ext.as_str() {
            "png" => (img.data.to_vec(), "\\pngblip"),
            "jpeg" | "jpg" => (img.data.to_vec(), "\\jpegblip"),
            _ => {
                // Re-encode other formats as PNG.
                let Ok(dec) = image::load_from_memory(&img.data) else { return };
                let mut buf = Vec::new();
                if dec.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png).is_err() {
                    return;
                }
                (buf, "\\pngblip")
            }
        };
        let (pw, ph) = model::image_px(&data).unwrap_or((1, 1));
        out.push_str(&format!(
            "{{\\pict{blip}\\picw{pw}\\pich{ph}\\picwgoal{}\\pichgoal{}\n",
            (img.w * 20.0).round() as i64,
            (img.h * 20.0).round() as i64
        ));
        for chunk in data.chunks(64) {
            for b in chunk {
                out.push_str(&format!("{b:02x}"));
            }
            out.push('\n');
        }
        out.push('}');
    }

    fn inlines(&mut self, inl: &[Inline], out: &mut String) {
        let inl = model::equations_as_text(inl);
        for i in inl.iter() {
            match i {
                Inline::Text(t, f) => self.run(t, f, out),
                Inline::Image(img) => self.picture(img, out),
                Inline::Anchor(a) => {
                    let a = esc(a);
                    out.push_str(&format!("{{\\*\\bkmkstart {a}}}{{\\*\\bkmkend {a}}}"));
                }
                Inline::Figure(_) | Inline::Equation { .. } => {}
            }
        }
    }

    fn para(&mut self, p: &Para, intbl: bool, ls: Option<(usize, String)>, end: &str, out: &mut String) {
        out.push_str("\\pard\\plain");
        if intbl {
            out.push_str("\\intbl");
        }
        if let Some(s) = style_num(p.kind) {
            out.push_str(&format!("\\s{s}"));
        }
        if let Kind::Heading(n) = p.kind {
            out.push_str(&format!("\\outlinelevel{}", n.clamp(1, 6) - 1));
        }
        match p.align {
            Some(Align::Center) => out.push_str("\\qc"),
            Some(Align::Right) => out.push_str("\\qr"),
            Some(Align::Justify) | Some(Align::Distribute) => out.push_str("\\qj"),
            _ => out.push_str("\\ql"),
        }
        if p.page_break {
            out.push_str("\\pagebb");
        }
        match p.kind {
            Kind::Quote => out.push_str("\\li720\\ri720\\i"),
            Kind::Code => {
                let f = self.t.font(model::MONO_FONT);
                out.push_str(&format!("\\sa0\\f{f}\\fs20"));
            }
            Kind::Rule => out.push_str("\\brdrb\\brdrs\\brdrw15\\brsp20"),
            Kind::Heading(n) => {
                let sz = HEAD_SIZES.get(usize::from(n.clamp(1, 6)) - 1).copied().unwrap_or(12.0);
                out.push_str(&format!("\\sb240\\sa80\\keepn\\b\\fs{}", (sz * 2.0) as i32));
            }
            Kind::Title => out.push_str("\\sa80\\fs56"),
            Kind::Normal => {}
        }
        if let (Some(li), Some((n, label))) = (p.list, &ls) {
            let lv = li.level.min(8);
            out.push_str(&format!("\\ls{n}\\ilvl{lv}\\fi-360\\li{}", 720 * (u32::from(lv) + 1)));
            out.push_str(&format!(" {{\\listtext\\pard\\plain {}\\tab}}", esc(label)));
        }
        out.push(' ');
        self.inlines(&p.inlines, out);
        out.push_str(end);
        out.push('\n');
    }

    fn blocks(&mut self, blocks: &[FBlock], intbl: bool, last_end: &str, out: &mut String, depth: usize) {
        let mut counter = model::ListCounter::default();
        let mut cur_list: Option<usize> = None;
        let mut top_ordered = false;
        let n = blocks.len();
        for (i, b) in blocks.iter().enumerate() {
            let end = if i + 1 == n { last_end } else { "\\par" };
            match b {
                FBlock::Para(p) => {
                    let ls = match p.list {
                        Some(li) => {
                            // A top-level item of the other kind starts a new list.
                            if li.level == 0 && cur_list.is_some() && top_ordered != li.ordered {
                                cur_list = None;
                            }
                            if li.level == 0 {
                                top_ordered = li.ordered;
                            }
                            let id = match cur_list {
                                Some(id) => id,
                                None => {
                                    // Level kinds from the whole run of list paragraphs.
                                    let mut kinds = [li.ordered; 9];
                                    let mut seen = [false; 9];
                                    for q in blocks.iter().skip(i) {
                                        let FBlock::Para(q) = q else { break };
                                        let Some(ql) = q.list else { break };
                                        let l = ql.level.min(8) as usize;
                                        if l == 0 && seen[0] && kinds[0] != ql.ordered {
                                            break;
                                        }
                                        if let (Some(s), Some(k)) = (seen.get_mut(l), kinds.get_mut(l))
                                            && !*s
                                        {
                                            *s = true;
                                            *k = ql.ordered;
                                        }
                                    }
                                    self.lists.push(kinds);
                                    counter.reset();
                                    self.lists.len()
                                }
                            };
                            cur_list = Some(id);
                            Some((id, counter.label(li)))
                        }
                        None => {
                            cur_list = None;
                            None
                        }
                    };
                    self.para(p, intbl, ls, end, out);
                }
                FBlock::Table(t) => {
                    cur_list = None;
                    if intbl || depth > 0 {
                        // Nested table: its cells' paragraphs flow into this cell.
                        let inner: Vec<FBlock> = t.rows.iter().flatten().flat_map(|c| c.blocks.clone()).collect();
                        if depth < model::MAX_DEPTH {
                            self.blocks(&inner, intbl, end, out, depth + 1);
                        }
                    } else {
                        self.table(t, out, depth);
                    }
                }
            }
        }
        if blocks.is_empty() && !last_end.is_empty() {
            out.push_str(&format!("\\pard\\plain{} {last_end}\n", if intbl { "\\intbl" } else { "" }));
        }
    }

    fn table(&mut self, t: &FTable, out: &mut String, depth: usize) {
        let cols = t.cols().max(1);
        let total = 9360.0f32; // 6.5" in twips
        let widths: Vec<f32> = if t.widths.len() == cols && t.widths.iter().all(|w| w.is_finite() && *w > 0.0) {
            t.widths.iter().map(|w| w * 20.0).collect()
        } else {
            vec![total / cols as f32; cols]
        };
        for row in &t.rows {
            out.push_str("\\trowd\\trgaph108\\trleft0");
            if !row.is_empty() && row.iter().all(|c| c.header) {
                out.push_str("\\trhdr");
            }
            let mut g = 0usize;
            let mut x = 0.0f32;
            for c in row {
                let span = c.colspan.clamp(1, 63) as usize;
                for k in g..(g + span).min(widths.len()) {
                    x += widths.get(k).copied().unwrap_or(0.0);
                }
                g += span;
                if c.covered {
                    out.push_str("\\clvmrg");
                } else if c.rowspan > 1 {
                    out.push_str("\\clvmgf");
                }
                out.push_str("\\clbrdrt\\brdrs\\brdrw10\\clbrdrl\\brdrs\\brdrw10\\clbrdrb\\brdrs\\brdrw10\\clbrdrr\\brdrs\\brdrw10");
                if let Some(s) = c.shading {
                    let i = self.t.color(s);
                    out.push_str(&format!("\\clcbpat{i}"));
                }
                out.push_str(&format!("\\cellx{}", x.round() as i64));
            }
            out.push('\n');
            for c in row {
                if c.covered {
                    out.push_str("\\pard\\plain\\intbl \\cell\n");
                } else {
                    let blocks: Vec<FBlock> = c
                        .blocks
                        .iter()
                        .cloned()
                        .map(|b| match b {
                            FBlock::Para(mut p) if c.header => {
                                for i in &mut p.inlines {
                                    if let Inline::Text(_, f) = i {
                                        f.bold = true;
                                    }
                                }
                                FBlock::Para(p)
                            }
                            other => other,
                        })
                        .collect();
                    self.blocks(&blocks, true, "\\cell", out, depth);
                }
            }
            out.push_str("\\row\n");
        }
    }
}

fn level_def(ordered: bool, l: usize) -> String {
    let indent = 720 * (l + 1);
    if ordered {
        let nfc = [0, 4, 2].get(l % 3).copied().unwrap_or(0);
        format!(
            "{{\\listlevel\\levelnfc{nfc}\\levelnfcn{nfc}\\leveljc0\\leveljcn0\\levelfollow0\\levelstartat1\\levelspace0\\levelindent0{{\\leveltext\\'02\\'{l:02x}.;}}{{\\levelnumbers\\'01;}}\\fi-360\\li{indent}\\lin{indent}}}"
        )
    } else {
        let ch = ['\u{2022}', '\u{25E6}', '\u{25AA}'].get(l % 3).copied().unwrap_or('\u{2022}');
        format!(
            "{{\\listlevel\\levelnfc23\\levelnfcn23\\leveljc0\\leveljcn0\\levelfollow0\\levelstartat1\\levelspace0\\levelindent0{{\\leveltext\\'01\\u{}?;}}{{\\levelnumbers;}}\\fi-360\\li{indent}\\lin{indent}}}",
            ch as u32 as i16
        )
    }
}

fn rtf_date(s: &str) -> Option<String> {
    let num = |a: usize, b: usize| s.get(a..b).and_then(|x| x.parse::<u32>().ok());
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi) = (num(11, 13).unwrap_or(0), num(14, 16).unwrap_or(0));
    Some(format!("\\yr{y}\\mo{mo}\\dy{d}\\hr{h}\\min{mi}"))
}

/// RTF text of a document.
pub fn export(doc: &Document) -> String {
    let flow = model::from_doc(doc);
    let base_font = doc.styles.default_chr.font.clone().unwrap_or_else(|| wordcraft_doc::styles::BODY_FONT.to_string());
    let mut w = Writer { t: Tables { fonts: vec![base_font], colors: Vec::new() }, lists: Vec::new() };
    let mut body = String::new();
    w.blocks(&flow.blocks, false, "\\par", &mut body, 0);
    export_with(&flow.meta, &w, &body, &doc.last_section)
}

fn export_with(m: &Meta, w: &Writer, body: &str, sec: &wordcraft_doc::SectionProps) -> String {
    let mut out = String::from("{\\rtf1\\ansi\\ansicpg1252\\deff0\\deflang1033\\uc1\n{\\fonttbl");
    for (i, f) in w.t.fonts.iter().enumerate() {
        let fam = if model::is_mono(f) { "\\fmodern" } else { "\\fswiss" };
        out.push_str(&format!("{{\\f{i}{fam}\\fcharset0 {};}}", esc(f)));
    }
    out.push_str("}\n{\\colortbl;");
    for c in &w.t.colors {
        out.push_str(&format!("\\red{}\\green{}\\blue{};", c.0, c.1, c.2));
    }
    out.push_str("}\n{\\*\\generator WordCraft;}\n{\\stylesheet{\\s0\\snext0 Normal;}");
    for n in 1..=6u32 {
        let sz = HEAD_SIZES.get(n as usize - 1).copied().unwrap_or(12.0);
        out.push_str(&format!("{{\\s{n}\\sbasedon0\\snext0\\outlinelevel{}\\keepn\\b\\fs{} heading {n};}}", n - 1, (sz * 2.0) as i32));
    }
    out.push_str(
        "{\\s7\\sbasedon0\\snext0\\fs56 Title;}{\\s8\\sbasedon0\\snext0\\li720\\ri720\\i Quote;}{\\s9\\sbasedon0\\snext9\\sa0\\fs20 Code;}}\n",
    );
    if !w.lists.is_empty() {
        out.push_str("{\\*\\listtable");
        for (i, kinds) in w.lists.iter().enumerate() {
            let id = i + 1;
            out.push_str(&format!("\n{{\\list\\listtemplateid{id}\\listhybrid"));
            for (l, k) in kinds.iter().enumerate() {
                out.push_str(&level_def(*k, l));
            }
            out.push_str(&format!("{{\\listname ;}}\\listid{id}}}"));
        }
        out.push_str("}\n{\\*\\listoverridetable");
        for i in 1..=w.lists.len() {
            out.push_str(&format!("{{\\listoverride\\listid{i}\\listoverridecount0\\ls{i}}}"));
        }
        out.push_str("}\n");
    }
    out.push_str("{\\info");
    for (k, v) in [("title", &m.title), ("author", &m.author), ("subject", &m.subject), ("keywords", &m.keywords), ("doccomm", &m.description)] {
        if !v.is_empty() {
            out.push_str(&format!("{{\\{k} {}}}", esc(v)));
        }
    }
    if let Some(d) = rtf_date(&m.created) {
        out.push_str(&format!("{{\\creatim{d}}}"));
    }
    out.push_str("}\n");
    let tw = |v: f32| (v * 20.0).round() as i64;
    out.push_str(&format!(
        "\\paperw{}\\paperh{}\\margl{}\\margr{}\\margt{}\\margb{}\\viewkind4\n",
        tw(sec.page_w),
        tw(sec.page_h),
        tw(sec.margin_left),
        tw(sec.margin_right),
        tw(sec.margin_top),
        tw(sec.margin_bottom)
    ));
    out.push_str(body);
    out.push_str("}\n");
    out
}

// ---------------------------------------------------------------------------------------------
// Import

#[derive(Clone, Copy, Debug, PartialEq)]
enum Dest {
    Body,
    Skip,
    FontTbl,
    ColorTbl,
    StyleSheet,
    Info(u8),
    ListTable,
    ListOverride,
    FldInst,
    Pict,
    Bookmark,
}

/// Which header or footer a `\header…`/`\footer…` group is: footer or header, and the pages
/// (`\headerf` first, `\headerl` left = even, `\header`/`\headerr` the others).
#[derive(Clone, Copy, Debug, PartialEq)]
struct HeaderFooter {
    footer: bool,
    pages: HfPages,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum HfPages {
    Default,
    First,
    Even,
}

/// The reader's output so far, set aside while a header or footer is read.
#[derive(Default)]
struct Output {
    body: Vec<FBlock>,
    para: Para,
    para_has: bool,
    table: Vec<Vec<Cell>>,
    row: Vec<Cell>,
    cell: Vec<FBlock>,
    defs: Vec<CellDef>,
    def_cur: CellDef,
    row_header: bool,
    rule_pending: bool,
    table_rights: Vec<Vec<i32>>,
}

#[derive(Clone, Debug)]
struct St {
    dest: Dest,
    fmt: Fmt,
    uc: usize,
    hidden: bool,
    // Paragraph properties.
    align: Option<Align>,
    style: i32,
    intbl: bool,
    ls: Option<i32>,
    ilvl: u8,
    outline: Option<u8>,
    pagebb: bool,
    field: Option<usize>,
    /// The code page of the current font's charset (`\fcharset`), when it names one.
    cp: Option<u16>,
    /// Space before/after (`\sb`, `\sa`), line spacing (`\sl`, `\slmult`) and indents
    /// (`\li`, `\ri`, `\fi`), twips.
    /// The paragraph's borders (`\brdrt`, `\brdrb`, `\brdrl`, `\brdrr`) and the side being
    /// described.
    borders: wordcraft_doc::props::Borders,
    border_side: Option<char>,
    sb: Option<i32>,
    sa: Option<i32>,
    sl: i32,
    slmult: bool,
    li: Option<i32>,
    ri: Option<i32>,
    fi: Option<i32>,
}

impl Default for St {
    fn default() -> Self {
        St {
            dest: Dest::Body,
            fmt: Fmt::default(),
            uc: 1,
            hidden: false,
            align: None,
            style: 0,
            intbl: false,
            ls: None,
            ilvl: 0,
            outline: None,
            pagebb: false,
            field: None,
            cp: None,
            borders: Default::default(),
            border_side: None,
            sb: None,
            sa: None,
            sl: 0,
            slmult: false,
            li: None,
            ri: None,
            fi: None,
        }
    }
}

#[derive(Default, Clone)]
struct CellDef {
    vmerge: u8,
    hmerge: u8,
    shading: Option<Rgb>,
    right: i32,
    /// The borders the cell names (`\clbrdrt` … `\clbrdrr`, each with its line style, `\brdrw`
    /// width and `\brdrcf` colour).
    borders: wordcraft_doc::props::Borders,
}

/// Page size and margins in twips (`\paperw`, `\margl`… and their `…sxn` section forms).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct PageGeom {
    w: Option<i32>,
    h: Option<i32>,
    left: Option<i32>,
    right: Option<i32>,
    top: Option<i32>,
    bottom: Option<i32>,
    header: Option<i32>,
    footer: Option<i32>,
    landscape: bool,
}

impl PageGeom {
    /// The section properties these give, Word's defaults where the file names none.
    fn section(&self) -> wordcraft_doc::SectionProps {
        let mut s = wordcraft_doc::SectionProps::default();
        // Twips to points, ignoring nonsense (zero, negative, or beyond Word's 22").
        let pt = |v: Option<i32>, lo: f32, hi: f32| v.map(|t| t as f32 / 20.0).filter(|v| v.is_finite() && (lo..=hi).contains(v));
        if let Some(w) = pt(self.w, 72.0, 1584.0) {
            s.page_w = w;
        }
        if let Some(h) = pt(self.h, 72.0, 1584.0) {
            s.page_h = h;
        }
        if self.landscape && s.page_w < s.page_h {
            std::mem::swap(&mut s.page_w, &mut s.page_h);
        }
        s.landscape = s.page_w > s.page_h;
        for (v, f) in [
            (self.left, &mut s.margin_left),
            (self.right, &mut s.margin_right),
            (self.top, &mut s.margin_top),
            (self.bottom, &mut s.margin_bottom),
            (self.header, &mut s.header),
            (self.footer, &mut s.footer),
        ] {
            if let Some(v) = pt(v, 0.0, 720.0) {
                *f = v;
            }
        }
        // Margins that leave no room for text fall back to Word's.
        let d = wordcraft_doc::SectionProps::default();
        if s.page_w - s.margin_left - s.margin_right < 72.0 {
            (s.margin_left, s.margin_right) = (d.margin_left, d.margin_right);
        }
        if s.page_h - s.margin_top - s.margin_bottom < 72.0 {
            (s.margin_top, s.margin_bottom) = (d.margin_top, d.margin_bottom);
        }
        s
    }
}

#[derive(Default)]
struct Reader {
    st: St,
    stack: Vec<St>,
    overflow: usize,
    fonts: HashMap<i32, String>,
    font_cur: i32,
    font_name: String,
    /// Code pages of the fonts whose charset names one (`\fcharset204` → 1251).
    font_cps: HashMap<i32, u16>,
    font_cp_cur: Option<u16>,
    /// The document's code page (`\ansicpg`); 0 until it names one (Windows-1252 then).
    ansicpg: u16,
    /// The default font (`\deff`), whose charset applies before any `\f`.
    deff: i32,
    page: PageGeom,
    /// The header or footer being read: the group depth it started at, which one, and the
    /// body's output, put back when the group closes.
    hf: Option<(usize, HeaderFooter, Box<Output>)>,
    /// Headers and footers read, in order.
    hf_parts: Vec<(HeaderFooter, Vec<FBlock>)>,
    /// `\titlepg`: the first page has its own header and footer.
    title_page: bool,
    /// The cell border side being described (`\clbrdrt`, `l`, `b`, `r`): its line style, width
    /// and colour follow.
    cell_side: Option<char>,
    /// Each table row's cell right edges (`\cellx`, twips), for the table's column grid.
    table_rights: Vec<Vec<i32>>,
    colors: Vec<Option<Rgb>>,
    color_cur: (u8, u8, u8, bool),
    styles: HashMap<i32, String>,
    style_cur: i32,
    style_name: String,
    meta: Meta,
    info_text: String,
    // Lists: list id → ordered per level; override ls → list id.
    lists: HashMap<i32, Vec<bool>>,
    list_cur: Vec<bool>,
    list_cur_id: i32,
    overrides: HashMap<i32, i32>,
    ov_list: i32,
    ov_ls: i32,
    fields: Vec<String>,
    // Picture being read.
    pict_depth: Option<usize>,
    pict_hex: Vec<u8>,
    pict_half: Option<u8>,
    pict_kind: u8,
    pict_w: Option<f32>,
    pict_h: Option<f32>,
    pict_sx: f32,
    pict_sy: f32,
    bkmk: Option<(usize, String)>,
    skip_chars: usize,
    pending_high: Option<u16>,
    // Output.
    body: Vec<FBlock>,
    para: Para,
    para_has: bool,
    table: Vec<Vec<Cell>>,
    row: Vec<Cell>,
    cell: Vec<FBlock>,
    defs: Vec<CellDef>,
    def_cur: CellDef,
    row_header: bool,
    created: Option<(u32, u32, u32, u32, u32)>,
    in_creatim: bool,
    rule_pending: bool,
    info_depth: Option<usize>,
}

impl Reader {
    fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Set the output aside (a header or footer starts) or put it back (it ended), returning the
    /// other one.
    fn swap_output(&mut self, mut o: Output) -> Output {
        std::mem::swap(&mut o.body, &mut self.body);
        std::mem::swap(&mut o.para, &mut self.para);
        std::mem::swap(&mut o.para_has, &mut self.para_has);
        std::mem::swap(&mut o.table, &mut self.table);
        std::mem::swap(&mut o.row, &mut self.row);
        std::mem::swap(&mut o.cell, &mut self.cell);
        std::mem::swap(&mut o.defs, &mut self.defs);
        std::mem::swap(&mut o.def_cur, &mut self.def_cur);
        std::mem::swap(&mut o.row_header, &mut self.row_header);
        std::mem::swap(&mut o.rule_pending, &mut self.rule_pending);
        std::mem::swap(&mut o.table_rights, &mut self.table_rights);
        o
    }

    /// The border of the cell side being described, else of the paragraph side.
    fn cell_border_slot(&mut self) -> Option<&mut Option<wordcraft_doc::props::Border>> {
        let (b, side) = match self.cell_side {
            Some(c) => (&mut self.def_cur.borders, c),
            None => (&mut self.st.borders, self.st.border_side?),
        };
        match side {
            't' => Some(&mut b.top),
            'l' => Some(&mut b.left),
            'b' => Some(&mut b.bottom),
            'r' => Some(&mut b.right),
            _ => None,
        }
    }

    /// A header or footer group starts: read it into an output of its own.
    fn start_header_footer(&mut self, which: HeaderFooter) {
        let body = self.swap_output(Output::default());
        self.hf = Some((self.depth(), which, Box::new(body)));
        self.st.intbl = false;
    }

    /// The header or footer group closed: keep what it held and go back to the body.
    fn end_header_footer(&mut self) {
        let Some((_, which, body)) = self.hf.take() else { return };
        if self.para_has || !self.para.inlines.is_empty() {
            self.end_para(false);
        }
        self.close_table();
        let part = self.swap_output(*body);
        // A document names each one once; a repeat (another section's) replaces it.
        self.hf_parts.retain(|(w, _)| *w != which);
        if self.hf_parts.len() < 16 {
            self.hf_parts.push((which, part.body));
        }
    }

    fn open(&mut self) {
        self.skip_chars = 0;
        if self.stack.len() >= MAX_GROUPS {
            self.overflow += 1;
            return;
        }
        self.stack.push(self.st.clone());
    }

    fn close(&mut self) {
        self.skip_chars = 0;
        if self.overflow > 0 {
            self.overflow -= 1;
            return;
        }
        let closing = self.st.dest;
        // Destination-level commits.
        match closing {
            Dest::FontTbl if !self.font_name.is_empty() => self.commit_font(),
            Dest::StyleSheet if !self.style_name.is_empty() => self.commit_style(),
            Dest::Info(k) => {
                let t = std::mem::take(&mut self.info_text).trim().to_string();
                match k {
                    1 => self.meta.title = t,
                    2 => self.meta.author = t,
                    3 => self.meta.subject = t,
                    4 => self.meta.keywords = t,
                    5 => self.meta.description = t,
                    _ => {}
                }
                self.in_creatim = false;
            }
            _ => {}
        }
        let Some(prev) = self.stack.pop() else { return };
        if self.pict_depth.is_some_and(|d| self.depth() < d) {
            self.pict_depth = None;
            self.finish_pict();
        }
        if let Some((d, _)) = &self.bkmk
            && self.depth() < *d
            && let Some((_, name)) = self.bkmk.take()
        {
            let name = name.trim().to_string();
            if !name.is_empty() {
                self.para.inlines.push(Inline::Anchor(name));
                self.para_has = true;
            }
        }
        self.st = prev;
        if self.hf.as_ref().is_some_and(|(d, _, _)| self.depth() < *d) {
            self.end_header_footer();
        }
    }

    fn commit_font(&mut self) {
        let name = std::mem::take(&mut self.font_name).trim().trim_end_matches(';').trim().to_string();
        if !name.is_empty() && self.fonts.len() < 4096 {
            self.fonts.insert(self.font_cur, name);
            if let Some(cp) = self.font_cp_cur.take() {
                self.font_cps.insert(self.font_cur, cp);
            }
        }
    }

    /// A byte of 8-bit text (`\'hh` or raw) as a char, in the current font's code page, else the
    /// default font's, else the document's.
    fn byte_char(&self, b: u8) -> char {
        let cp = self.st.cp.or_else(|| self.font_cps.get(&self.deff).copied()).unwrap_or(if self.ansicpg == 0 { 1252 } else { self.ansicpg });
        wordcraft_doc::encoding::windows_byte(cp, b)
    }

    fn commit_style(&mut self) {
        let name = std::mem::take(&mut self.style_name).trim().trim_end_matches(';').trim().to_string();
        if self.styles.len() < 4096 {
            self.styles.insert(self.style_cur, name);
        }
    }

    fn finish_pict(&mut self) {
        if self.pict_half.take().is_some() {
            // Odd hex digit count: drop the dangling nibble.
        }
        let data = std::mem::take(&mut self.pict_hex);
        if self.pict_kind == 0 || data.is_empty() {
            return;
        }
        let sx = if self.pict_sx > 0.0 { self.pict_sx / 100.0 } else { 1.0 };
        let sy = if self.pict_sy > 0.0 { self.pict_sy / 100.0 } else { 1.0 };
        let w = self.pict_w.map(|v| v * sx);
        let h = self.pict_h.map(|v| v * sy);
        if let Some(img) = make_img(data, w, h, "") {
            self.para.inlines.push(Inline::Image(img));
            self.para_has = true;
        }
    }

    fn text(&mut self, s: &str) {
        match self.st.dest {
            Dest::Body => {
                if self.st.hidden {
                    return;
                }
                let f = self.st.fmt.clone();
                self.para.push_text(s, &f);
                self.para_has = true;
            }
            Dest::FontTbl => {
                for c in s.chars() {
                    if c == ';' {
                        self.commit_font();
                    } else {
                        self.font_name.push(c);
                    }
                }
            }
            Dest::ColorTbl => {
                for c in s.chars() {
                    if c == ';' {
                        let (r, g, b, set) = self.color_cur;
                        if self.colors.len() < 4096 {
                            self.colors.push(if set { Some(Rgb(r, g, b)) } else { None });
                        }
                        self.color_cur = (0, 0, 0, false);
                    }
                }
            }
            Dest::StyleSheet => {
                for c in s.chars() {
                    if c == ';' {
                        self.commit_style();
                    } else {
                        self.style_name.push(c);
                    }
                }
            }
            Dest::Info(_) => self.info_text.push_str(s),
            Dest::FldInst => {
                if let Some(f) = self.st.field.and_then(|i| self.fields.get_mut(i))
                    && f.len() < 4096
                {
                    f.push_str(s);
                }
            }
            Dest::Pict => {
                for b in s.bytes() {
                    let v = match b {
                        b'0'..=b'9' => b - b'0',
                        b'a'..=b'f' => b - b'a' + 10,
                        b'A'..=b'F' => b - b'A' + 10,
                        _ => continue,
                    };
                    match self.pict_half.take() {
                        Some(h) => {
                            if self.pict_hex.len() < MAX_PICT {
                                self.pict_hex.push((h << 4) | v);
                            }
                        }
                        None => self.pict_half = Some(v),
                    }
                }
            }
            Dest::Bookmark => {
                if let Some((_, n)) = &mut self.bkmk
                    && n.len() < 256
                {
                    n.push_str(s);
                }
            }
            Dest::Skip | Dest::ListTable | Dest::ListOverride => {}
        }
    }

    fn ch(&mut self, c: char) {
        if self.skip_chars > 0 {
            self.skip_chars -= 1;
            return;
        }
        let mut buf = [0u8; 4];
        self.text(c.encode_utf8(&mut buf));
    }

    fn para_kind(&self) -> Kind {
        let name = self.styles.get(&self.st.style).map(|s| s.to_ascii_lowercase()).unwrap_or_default();
        if let Some(n) = name.strip_prefix("heading").and_then(|r| r.trim().parse::<u8>().ok()) {
            return Kind::Heading(n.clamp(1, 6));
        }
        if self.st.style != 0 {
            match name.as_str() {
                "title" => return Kind::Title,
                "quote" | "intense quote" => return Kind::Quote,
                "code" | "html preformatted" | "plain text" => return Kind::Code,
                _ => {}
            }
        }
        if let Some(o) = self.st.outline.filter(|o| *o < 9) {
            return Kind::Heading((o + 1).min(6));
        }
        Kind::Normal
    }

    /// End the current paragraph (`\par`, `\cell`).
    fn end_para(&mut self, in_cell: bool) {
        let mut p = std::mem::take(&mut self.para);
        p.kind = self.para_kind();
        if p.kind == Kind::Normal && p.inlines.is_empty() && self.rule_pending {
            p.kind = Kind::Rule;
        }
        self.rule_pending = false;
        p.align = self.st.align;
        p.page_break = self.st.pagebb;
        p.spacing = self.spacing();
        p.borders = self.st.borders.any_visible().then_some(self.st.borders);
        if let Some(ls) = self.st.ls {
            let list = self.overrides.get(&ls).and_then(|id| self.lists.get(id));
            let lv = self.st.ilvl.min(8);
            let ordered = list.and_then(|l| l.get(lv as usize).copied()).unwrap_or(false);
            p.list = Some(ListInfo { ordered, level: lv });
        }
        // Headings and code keep their style formatting implicit.
        if matches!(p.kind, Kind::Heading(_) | Kind::Code | Kind::Title) {
            for i in &mut p.inlines {
                if let Inline::Text(_, f) = i {
                    f.size = None;
                    if p.kind != Kind::Title {
                        f.bold = false;
                    }
                    if p.kind == Kind::Code {
                        f.code = false;
                        f.font = None;
                    }
                }
            }
        }
        if p.kind == Kind::Quote {
            for i in &mut p.inlines {
                if let Inline::Text(_, f) = i {
                    f.italic = false;
                }
            }
        }
        self.para_has = false;
        if self.st.intbl || in_cell {
            self.cell.push(FBlock::Para(p));
        } else {
            self.close_table();
            self.body.push(FBlock::Para(p));
        }
    }

    /// The current paragraph's spacing and indents in points.
    fn spacing(&self) -> model::Spacing {
        use wordcraft_doc::props::LineSpacing;
        let pt = |v: i32| (v as f32 / 20.0).clamp(-1584.0, 1584.0);
        let gap = |v: Option<i32>| v.map(|v| pt(v).max(0.0));
        let sl = self.st.sl;
        let line = if sl == 0 {
            None
        } else if sl < 0 {
            Some(LineSpacing::Exactly(pt(sl.saturating_neg()).max(1.0)))
        } else if self.st.slmult {
            // \sl240\slmult1 is single spacing.
            Some(LineSpacing::Multiple((sl as f32 / 240.0).clamp(0.06, 132.0)))
        } else {
            Some(LineSpacing::AtLeast(pt(sl)))
        };
        model::Spacing {
            before: gap(self.st.sb),
            after: gap(self.st.sa),
            line,
            left: self.st.li.map(pt),
            right: self.st.ri.map(pt),
            first: self.st.fi.map(pt),
        }
    }

    fn end_cell(&mut self) {
        self.end_para(true);
        let blocks = std::mem::take(&mut self.cell);
        if self.row.len() < 63 {
            self.row.push(Cell { blocks, ..Default::default() });
        }
    }

    fn end_row(&mut self) {
        if self.para_has {
            self.end_cell();
        }
        let mut cells = std::mem::take(&mut self.row);
        let defs = self.defs.clone();
        let mut out: Vec<Cell> = Vec::new();
        let mut rights: Vec<i32> = Vec::new();
        for (i, mut c) in cells.drain(..).enumerate() {
            let d = defs.get(i).cloned().unwrap_or_default();
            // A merged-in cell widens the cell it joins.
            if d.hmerge == 2
                && let Some(last) = rights.last_mut()
            {
                *last = d.right;
            } else {
                rights.push(d.right);
            }
            c.shading = d.shading;
            c.borders = d.borders.any_visible().then_some(d.borders);
            c.header = self.row_header;
            match d.vmerge {
                1 => c.rowspan = 2,
                2 => c.covered = true,
                _ => {}
            }
            if d.hmerge == 2
                && let Some(prev) = out.last_mut()
            {
                prev.colspan = (prev.colspan + 1).min(63);
                continue;
            }
            out.push(c);
        }
        if !out.is_empty() && self.table.len() < wordcraft_doc::table::MAX_ROWS {
            rights.truncate(out.len());
            self.table.push(out);
            self.table_rights.push(rights);
        }
    }

    fn close_table(&mut self) {
        if !self.row.is_empty() || !self.cell.is_empty() {
            if !self.cell.is_empty() {
                let blocks = std::mem::take(&mut self.cell);
                self.row.push(Cell { blocks, ..Default::default() });
            }
            self.end_row();
        }
        if !self.table.is_empty() {
            let mut rows = std::mem::take(&mut self.table);
            let rights = std::mem::take(&mut self.table_rights);
            let widths = column_grid(&mut rows, &rights);
            // Word draws only the borders the cells name: no grid style, each cell its own.
            self.body.push(FBlock::Table(FTable { rows, widths, borderless: true }));
        }
    }

    fn word(&mut self, w: &str, p: Option<i32>, starred: bool, first_in_group: bool) {
        let on = p != Some(0);
        let pv = p.unwrap_or(0);
        if self.st.dest == Dest::Skip && !(self.in_info() && matches!(w, "title" | "author" | "subject" | "keywords" | "doccomm" | "creatim")) {
            if self.in_creatim {
                let v = pv.clamp(0, 9999) as u32;
                let c = self.created.get_or_insert((0, 1, 1, 0, 0));
                match w {
                    "yr" => c.0 = v,
                    "mo" => c.1 = v,
                    "dy" => c.2 = v,
                    "hr" => c.3 = v,
                    "min" => c.4 = v,
                    _ => {}
                }
            }
            return;
        }
        // Destinations.
        let dest = match w {
            "fonttbl" => Some(Dest::FontTbl),
            "colortbl" => Some(Dest::ColorTbl),
            "stylesheet" => Some(Dest::StyleSheet),
            "info" => Some(Dest::Skip),
            "title" if self.st.dest == Dest::Skip && self.in_info() => Some(Dest::Info(1)),
            "author" if self.in_info() => Some(Dest::Info(2)),
            "subject" if self.in_info() => Some(Dest::Info(3)),
            "keywords" if self.in_info() => Some(Dest::Info(4)),
            "doccomm" if self.in_info() => Some(Dest::Info(5)),
            "listtable" => Some(Dest::ListTable),
            "listoverridetable" => Some(Dest::ListOverride),
            "fldinst" => Some(Dest::FldInst),
            // Transparent wrapper: read the nested pict without changing the destination.
            "shppict" => return,
            "pict" => Some(Dest::Pict),
            "bkmkstart" => Some(Dest::Bookmark),
            "header" | "footer" | "headerl" | "headerr" | "headerf" | "footerl" | "footerr" | "footerf"
                if self.hf.is_none() && self.st.dest == Dest::Body && !self.st.intbl =>
            {
                let footer = w.starts_with("footer");
                let pages = match w.strip_prefix(if footer { "footer" } else { "header" }) {
                    Some("f") => HfPages::First,
                    Some("l") => HfPages::Even,
                    _ => HfPages::Default,
                };
                self.start_header_footer(HeaderFooter { footer, pages });
                return;
            }
            "header" | "footer" | "headerl" | "headerr" | "headerf" | "footerl" | "footerr" | "footerf" | "footnote" | "annotation" | "pntext"
            | "listtext" | "object" | "nonshppict" | "xe" | "tc" | "txe" | "private" | "bkmkend" | "atnid" | "atnauthor" | "comment" | "shpinst"
            | "do" | "operator" | "company" | "manager" | "category" | "hlinkbase" | "revtime" | "printim" | "buptim" | "version" | "nofpages"
            | "nofwords" | "nofchars" | "edmins" | "vern" | "themedata" | "colorschememapping" | "latentstyles" | "datastore" | "xmlnstbl"
            | "rsidtbl" | "generator" | "mmathPr" | "pgdsctbl" | "filetbl" | "revtbl" | "listpicture" | "userprops" | "docvar" | "objdata"
            | "fontemb" | "fontfile" | "panose" | "falt" | "mhtmltag" | "htmltag" | "pnseclvl" | "background" | "sp" | "pn" | "pgptbl" => {
                Some(Dest::Skip)
            }
            "creatim" if self.in_info() => {
                self.in_creatim = true;
                Some(Dest::Skip)
            }
            _ => None,
        };
        if let Some(d) = dest {
            match d {
                Dest::Bookmark => {
                    self.bkmk = Some((self.depth(), String::new()));
                }
                Dest::Pict => {
                    self.pict_depth = Some(self.depth());
                    self.pict_hex.clear();
                    self.pict_half = None;
                    self.pict_kind = 0;
                    self.pict_w = None;
                    self.pict_h = None;
                    self.pict_sx = 0.0;
                    self.pict_sy = 0.0;
                }
                Dest::FldInst if self.st.field.is_none() => self.start_field(),
                _ => {}
            }
            self.st.dest = d;
            return;
        }
        if starred && first_in_group {
            // Unknown ignorable destination.
            if !matches!(w, "fldinst" | "bkmkstart") {
                self.st.dest = Dest::Skip;
                return;
            }
        }
        match self.st.dest {
            Dest::Skip => {
                if self.in_creatim {
                    let v = pv.clamp(0, 9999) as u32;
                    let c = self.created.get_or_insert((0, 1, 1, 0, 0));
                    match w {
                        "yr" => c.0 = v,
                        "mo" => c.1 = v,
                        "dy" => c.2 = v,
                        "hr" => c.3 = v,
                        "min" => c.4 = v,
                        _ => {}
                    }
                }
                return;
            }
            Dest::FontTbl => {
                match w {
                    "f" => {
                        self.font_cur = pv;
                        self.font_name.clear();
                        self.font_cp_cur = None;
                    }
                    "fcharset" => self.font_cp_cur = wordcraft_doc::encoding::charset_code_page(pv),
                    // A font's explicit code page (`\cpg1251`) wins over its charset.
                    "cpg" if pv > 0 && wordcraft_doc::encoding::SINGLE_BYTE_CODE_PAGES.contains(&(pv as u16)) => self.font_cp_cur = Some(pv as u16),
                    _ => {}
                }
                return;
            }
            Dest::ColorTbl => {
                let v = pv.clamp(0, 255) as u8;
                match w {
                    "red" => {
                        self.color_cur.0 = v;
                        self.color_cur.3 = true;
                    }
                    "green" => {
                        self.color_cur.1 = v;
                        self.color_cur.3 = true;
                    }
                    "blue" => {
                        self.color_cur.2 = v;
                        self.color_cur.3 = true;
                    }
                    _ => {}
                }
                return;
            }
            Dest::StyleSheet => {
                match w {
                    "s" | "cs" | "ds" | "ts" => {
                        self.style_cur = if w == "s" { pv } else { -1 - pv.saturating_abs() };
                        self.style_name.clear();
                    }
                    _ => {}
                }
                return;
            }
            Dest::ListTable => {
                match w {
                    "list" => {
                        self.list_cur.clear();
                        self.list_cur_id = 0;
                    }
                    "listlevel" if self.list_cur.len() < 9 => {
                        self.list_cur.push(true);
                    }
                    "levelnfc" | "levelnfcn" => {
                        if let Some(l) = self.list_cur.last_mut() {
                            *l = !matches!(pv, 23 | 255);
                        }
                    }
                    "listid" => {
                        self.list_cur_id = pv;
                        if self.lists.len() < 4096 {
                            self.lists.insert(pv, self.list_cur.clone());
                        }
                    }
                    _ => {}
                }
                return;
            }
            Dest::ListOverride => {
                match w {
                    "listid" => self.ov_list = pv,
                    "ls" => {
                        self.ov_ls = pv;
                        if self.overrides.len() < 4096 {
                            self.overrides.insert(pv, self.ov_list);
                        }
                    }
                    _ => {}
                }
                return;
            }
            Dest::Pict => {
                match w {
                    "pngblip" => self.pict_kind = 1,
                    "jpegblip" => self.pict_kind = 2,
                    "picwgoal" => self.pict_w = Some(pv.clamp(1, 31_680) as f32 / 20.0),
                    "pichgoal" => self.pict_h = Some(pv.clamp(1, 31_680) as f32 / 20.0),
                    "picscalex" => self.pict_sx = pv.clamp(1, 1000) as f32,
                    "picscaley" => self.pict_sy = pv.clamp(1, 1000) as f32,
                    _ => {}
                }
                return;
            }
            // Text destinations take Unicode escapes (with fallback skipping) like the body.
            Dest::Info(_) | Dest::FldInst | Dest::Bookmark if !matches!(w, "u" | "uc") => return,
            Dest::Info(_) | Dest::FldInst | Dest::Bookmark => {}
            Dest::Body => {}
        }
        match w {
            "par" => self.end_para(false),
            "sect" => self.end_para(false),
            "cell" => self.end_cell(),
            "nestcell" => self.end_para(true),
            "row" => self.end_row(),
            "line" => self.ch('\n'),
            "tab" => self.ch('\t'),
            "page" => self.ch('\u{000C}'),
            "emdash" => self.ch('—'),
            "endash" => self.ch('–'),
            "bullet" => self.ch('•'),
            "lquote" => self.ch('‘'),
            "rquote" => self.ch('’'),
            "ldblquote" => self.ch('“'),
            "rdblquote" => self.ch('”'),
            "emspace" => self.ch('\u{2003}'),
            "enspace" => self.ch('\u{2002}'),
            "qmspace" => self.ch('\u{2005}'),
            "zwj" => self.ch('\u{200D}'),
            "zwnj" => self.ch('\u{200C}'),
            "u" => {
                let u = (pv as i64).rem_euclid(65_536) as u16;
                if (0xD800..0xDC00).contains(&u) {
                    self.pending_high = Some(u);
                } else {
                    let c = match self.pending_high.take() {
                        Some(h) if (0xDC00..0xE000).contains(&u) => char::decode_utf16([h, u]).next().and_then(|r| r.ok()).unwrap_or('\u{FFFD}'),
                        _ => char::from_u32(u32::from(u)).unwrap_or('\u{FFFD}'),
                    };
                    self.ch(c);
                }
                self.skip_chars = self.st.uc;
            }
            "uc" => self.st.uc = pv.clamp(0, 8) as usize,
            "plain" => {
                self.st.fmt = Fmt { link: self.st.fmt.link.clone(), ..Default::default() };
                self.st.hidden = false;
            }
            "pard" => {
                let field = self.st.field;
                let (dest, fmt, uc, hidden) = (self.st.dest, self.st.fmt.clone(), self.st.uc, self.st.hidden);
                self.st = St { dest, fmt, uc, hidden, field, ..Default::default() };
            }
            "b" => self.st.fmt.bold = on,
            "i" => self.st.fmt.italic = on,
            "ul" | "uld" | "uldash" | "uldashd" | "uldashdd" | "uldb" | "ulth" | "ulw" | "ulwave" | "ulhwave" | "ulldash" | "ulthd" => {
                self.st.fmt.underline = on
            }
            "ulnone" => self.st.fmt.underline = false,
            "strike" | "striked" => self.st.fmt.strike = on,
            "super" | "up" => {
                self.st.fmt.sup = on;
                self.st.fmt.sub = false;
            }
            "sub" | "dn" => {
                self.st.fmt.sub = on;
                self.st.fmt.sup = false;
            }
            "nosupersub" => {
                self.st.fmt.sup = false;
                self.st.fmt.sub = false;
            }
            "fs" => self.st.fmt.size = if pv > 0 { Some((pv as f32 / 2.0).clamp(1.0, 1638.0)) } else { None },
            "ansicpg" => {
                if pv > 0 && wordcraft_doc::encoding::SINGLE_BYTE_CODE_PAGES.contains(&(pv as u16)) {
                    self.ansicpg = pv as u16;
                }
            }
            "deff" => self.deff = pv,
            "f" => {
                self.st.cp = self.font_cps.get(&pv).copied();
                let name = self.fonts.get(&pv).cloned();
                match name {
                    Some(n) if model::is_mono(&n) => {
                        self.st.fmt.code = true;
                        self.st.fmt.font = None;
                    }
                    Some(n) => {
                        self.st.fmt.code = false;
                        self.st.fmt.font = Some(n);
                    }
                    None => {}
                }
            }
            "cf" => self.st.fmt.color = self.colors.get(pv.max(0) as usize).copied().flatten(),
            "highlight" | "chcbpat" | "cb" => self.st.fmt.background = self.colors.get(pv.max(0) as usize).copied().flatten(),
            "v" => self.st.hidden = on,
            "ql" => self.st.align = Some(Align::Left),
            "qc" => self.st.align = Some(Align::Center),
            "qr" => self.st.align = Some(Align::Right),
            "qj" | "qd" => self.st.align = Some(Align::Justify),
            "s" => self.st.style = pv,
            "outlinelevel" => self.st.outline = Some(pv.clamp(0, 9) as u8),
            "pagebb" => self.st.pagebb = on,
            "intbl" => self.st.intbl = true,
            "ls" => self.st.ls = Some(pv),
            "ilvl" => self.st.ilvl = pv.clamp(0, 8) as u8,
            "brdrb" | "brdrt" | "brdrl" | "brdrr" if self.cell_side.is_none() => {
                if w == "brdrb" {
                    self.rule_pending = true;
                }
                self.st.border_side = w.chars().last();
            }
            "trowd" => {
                self.defs.clear();
                self.def_cur = CellDef::default();
                self.row_header = false;
            }
            "trhdr" => self.row_header = true,
            "clvmgf" => self.def_cur.vmerge = 1,
            "clvmrg" => self.def_cur.vmerge = 2,
            "clmgf" => self.def_cur.hmerge = 1,
            "clmrg" => self.def_cur.hmerge = 2,
            "clcbpat" => self.def_cur.shading = self.colors.get(pv.max(0) as usize).copied().flatten(),
            "sb" => self.st.sb = Some(pv),
            "sa" => self.st.sa = Some(pv),
            "sl" => self.st.sl = pv,
            "slmult" => self.st.slmult = on,
            "li" | "lin" => self.st.li = Some(pv),
            "ri" | "rin" => self.st.ri = Some(pv),
            "fi" => self.st.fi = Some(pv),
            "clbrdrt" => self.cell_side = Some('t'),
            "clbrdrl" => self.cell_side = Some('l'),
            "clbrdrb" => self.cell_side = Some('b'),
            "clbrdrr" => self.cell_side = Some('r'),
            "brdrs" | "brdrth" | "brdrsh" | "brdrdb" | "brdrdot" | "brdrdash" | "brdrdashsm" | "brdrdashd" | "brdrdashdd" | "brdrhair"
            | "brdrtriple" | "brdrthtnsg" | "brdrtnthsg" | "brdrthtnmg" | "brdrtnthmg" | "brdrwavy" | "brdrwavydb" | "brdrnone" | "brdrnil"
            | "brdrtbl"
                if self.cell_side.is_some() || self.st.border_side.is_some() =>
            {
                use wordcraft_doc::props::{Border, BorderStyle};
                let style = match w {
                    "brdrnone" | "brdrnil" | "brdrtbl" => BorderStyle::None,
                    "brdrth" => BorderStyle::Thick,
                    "brdrdb" | "brdrthtnsg" | "brdrtnthsg" | "brdrthtnmg" | "brdrtnthmg" | "brdrwavydb" => BorderStyle::Double,
                    "brdrdot" => BorderStyle::Dotted,
                    "brdrdash" | "brdrdashsm" => BorderStyle::Dashed,
                    "brdrdashd" | "brdrdashdd" => BorderStyle::DotDash,
                    "brdrtriple" => BorderStyle::Triple,
                    "brdrwavy" => BorderStyle::Wave,
                    _ => BorderStyle::Single,
                };
                let b = (style != BorderStyle::None).then_some(Border { style, ..Border::single(0.5) });
                if let Some(slot) = self.cell_border_slot() {
                    *slot = b;
                }
            }
            "brdrw" if self.cell_side.is_some() || self.st.border_side.is_some() => {
                let width = (pv as f32 / 20.0).clamp(0.25, 6.0);
                if let Some(Some(b)) = self.cell_border_slot() {
                    b.width = width;
                }
            }
            "brdrcf" if self.cell_side.is_some() || self.st.border_side.is_some() => {
                let color = self.colors.get(pv.max(0) as usize).copied().flatten();
                if let Some(Some(b)) = self.cell_border_slot() {
                    b.color = color;
                }
            }
            "titlepg" => self.title_page = on,
            "paperw" | "pgwsxn" => self.page.w = Some(pv),
            "paperh" | "pghsxn" => self.page.h = Some(pv),
            "margl" | "marglsxn" => self.page.left = Some(pv),
            "margr" | "margrsxn" => self.page.right = Some(pv),
            "margt" | "margtsxn" => self.page.top = Some(pv),
            "margb" | "margbsxn" => self.page.bottom = Some(pv),
            "headery" => self.page.header = Some(pv),
            "footery" => self.page.footer = Some(pv),
            "landscape" | "lndscpsxn" => self.page.landscape = on,
            "cellx" => {
                self.cell_side = None;
                let mut d = std::mem::take(&mut self.def_cur);
                d.right = pv;
                if self.defs.len() < 63 {
                    self.defs.push(d);
                }
            }
            "field" => self.start_field(),
            "fldrslt" => {
                let link = self.st.field.and_then(|i| self.fields.get(i)).and_then(|s| hyperlink(s));
                if link.is_some() {
                    self.st.fmt.link = link;
                }
            }
            _ => {}
        }
    }

    fn start_field(&mut self) {
        if self.fields.len() < 100_000 {
            self.fields.push(String::new());
            self.st.field = Some(self.fields.len() - 1);
        }
    }

    fn in_info(&self) -> bool {
        self.info_depth.is_some_and(|d| self.depth() > d)
    }
}

/// The target of a `HYPERLINK "url"` (or `HYPERLINK \l "bookmark"`) field instruction.
fn hyperlink(instr: &str) -> Option<String> {
    let t = instr.trim();
    let rest = t.strip_prefix("HYPERLINK").or_else(|| t.strip_prefix("hyperlink"))?;
    let local = rest.contains("\\l");
    let q0 = rest.find('"');
    let target = match q0 {
        Some(a) => {
            let r = rest.get(a + 1..)?;
            r.get(..r.find('"').unwrap_or(r.len()))?.to_string()
        }
        None => rest.split_whitespace().find(|w| !w.starts_with('\\'))?.to_string(),
    };
    let target = target.replace("%22", "\"");
    if target.is_empty() {
        return None;
    }
    Some(if local && !target.starts_with('#') { format!("#{target}") } else { target })
}

/// Parse RTF bytes into the flow model.
pub fn parse(bytes: &[u8]) -> Result<Flow, String> {
    parse_with_page(bytes).map(|(f, _, _)| f)
}

/// The flow, the page, and the default font (`\deff`, font 0 when the file names none), which
/// is the font of all text that doesn't select one.
fn parse_with_page(bytes: &[u8]) -> Result<(Flow, PageGeom, Option<String>), String> {
    parse_full(bytes).map(|r| (r.flow, r.page, r.default_font))
}

/// Everything an import takes from an RTF file.
struct Parsed {
    flow: Flow,
    page: PageGeom,
    default_font: Option<String>,
    headers_footers: Vec<(HeaderFooter, Vec<FBlock>)>,
    title_page: bool,
}

fn parse_full(bytes: &[u8]) -> Result<Parsed, String> {
    let start = bytes.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(0);
    if !bytes.get(start..).is_some_and(|b| b.starts_with(b"{\\rtf")) {
        return Err("not an RTF document (it doesn't start with {\\rtf)".into());
    }
    let mut r = Reader::default();
    let mut i = start;
    let mut first_in_group = false;
    let mut starred = false;
    while let Some(&c) = bytes.get(i) {
        match c {
            b'{' => {
                r.open();
                first_in_group = true;
                starred = false;
                i += 1;
                continue;
            }
            b'}' => {
                r.close();
                if r.info_depth.is_some_and(|d| r.depth() < d) {
                    r.info_depth = None;
                }
                first_in_group = false;
                starred = false;
                i += 1;
                continue;
            }
            b'\\' => {
                let Some(&n) = bytes.get(i + 1) else { break };
                if n.is_ascii_alphabetic() {
                    let mut j = i + 1;
                    while bytes.get(j).is_some_and(|b| b.is_ascii_alphabetic()) && j - i <= 32 {
                        j += 1;
                    }
                    let word = std::str::from_utf8(bytes.get(i + 1..j).unwrap_or(b"")).unwrap_or("");
                    let mut k = j;
                    let neg = bytes.get(k) == Some(&b'-');
                    if neg {
                        k += 1;
                    }
                    let d0 = k;
                    while bytes.get(k).is_some_and(|b| b.is_ascii_digit()) && k - d0 < 10 {
                        k += 1;
                    }
                    let param = if k > d0 {
                        let v = std::str::from_utf8(bytes.get(d0..k).unwrap_or(b"0")).ok().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
                        let v = if neg { -v } else { v };
                        Some(v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
                    } else {
                        if neg {
                            k -= 1;
                        }
                        None
                    };
                    if bytes.get(k) == Some(&b' ') {
                        k += 1;
                    }
                    i = k;
                    if word == "bin" {
                        let n = param.unwrap_or(0).max(0) as usize;
                        let end = i.saturating_add(n).min(bytes.len());
                        if r.st.dest == Dest::Pict
                            && let Some(data) = bytes.get(i..end)
                            && r.pict_hex.len() + data.len() <= MAX_PICT
                        {
                            r.pict_hex.extend_from_slice(data);
                        }
                        i = end;
                        continue;
                    }
                    if word == "info" {
                        r.info_depth = Some(r.depth());
                    }
                    if r.skip_chars > 0
                        && matches!(r.st.dest, Dest::Body | Dest::Info(_) | Dest::FldInst | Dest::Bookmark)
                        && !matches!(word, "par" | "pard" | "cell" | "row")
                    {
                        r.skip_chars -= 1;
                        first_in_group = false;
                        continue;
                    }
                    r.word(word, param, starred, first_in_group);
                    first_in_group = false;
                    starred = false;
                    continue;
                }
                match n {
                    b'\'' => {
                        let h = bytes.get(i + 2..i + 4).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
                        match h {
                            Some(v) => {
                                if r.st.dest == Dest::Pict {
                                    r.text(&format!("{v:02x}"));
                                } else {
                                    let c = r.byte_char(v);
                                    r.ch(c);
                                }
                                i += 4;
                            }
                            None => i += 2,
                        }
                    }
                    b'*' => {
                        starred = true;
                        i += 2;
                        continue;
                    }
                    b'~' => {
                        r.ch('\u{00A0}');
                        i += 2;
                    }
                    b'-' => {
                        r.ch('\u{00AD}');
                        i += 2;
                    }
                    b'_' => {
                        r.ch('\u{2011}');
                        i += 2;
                    }
                    b'\n' | b'\r' => {
                        r.word("par", None, false, false);
                        i += 2;
                    }
                    b'\\' | b'{' | b'}' => {
                        r.ch(n as char);
                        i += 2;
                    }
                    _ => i += 2,
                }
                first_in_group = false;
            }
            b'\r' | b'\n' | 0 => i += 1,
            _ => {
                // A run of plain text.
                let mut j = i;
                while bytes.get(j).is_some_and(|b| !matches!(b, b'{' | b'}' | b'\\' | b'\r' | b'\n' | 0)) {
                    j += 1;
                }
                let chunk = bytes.get(i..j).unwrap_or(b"");
                if r.skip_chars == 0 && chunk.is_ascii() {
                    if let Ok(s) = std::str::from_utf8(chunk) {
                        r.text(s);
                    }
                } else {
                    for b in chunk {
                        let c = r.byte_char(*b);
                        r.ch(c);
                    }
                }
                i = j;
                first_in_group = false;
            }
        }
    }
    // A header or footer left open by a truncated file ends here.
    r.end_header_footer();
    if r.para_has || !r.para.inlines.is_empty() {
        r.end_para(false);
    }
    r.close_table();
    if let Some((y, mo, d, h, mi)) = r.created
        && y > 0
    {
        r.meta.created = format!("{y:04}-{:02}-{:02}T{:02}:{:02}:00Z", mo.clamp(1, 12), d.clamp(1, 31), h.min(23), mi.min(59));
    }
    let mut blocks = std::mem::take(&mut r.body);
    if blocks.is_empty() {
        blocks.push(FBlock::Para(Para::default()));
    }
    let default_font = r.fonts.get(&r.deff).filter(|n| !model::is_mono(n)).cloned();
    Ok(Parsed { flow: Flow { blocks, meta: r.meta }, page: r.page, default_font, headers_footers: r.hf_parts, title_page: r.title_page })
}

/// The column grid of a table from its rows' cell right edges: every distinct edge starts a grid
/// column, and each cell spans the grid columns between its left and right edges, as Word lays
/// out RTF rows whose cells don't line up. Returns the widths in points, or none (equal columns)
/// when the edges don't make a grid.
fn column_grid(rows: &mut [Vec<Cell>], rights: &[Vec<i32>]) -> Vec<f32> {
    let mut edges: Vec<i32> = rights.iter().flatten().copied().filter(|&x| x > 0).collect();
    edges.sort_unstable();
    edges.dedup();
    let ok = rights.len() == rows.len()
        && rows
            .iter()
            .zip(rights)
            .all(|(r, x)| r.len() == x.len() && x.windows(2).all(|w| w.first() < w.get(1)) && x.first().is_some_and(|&v| v > 0))
        && !edges.is_empty()
        && edges.len() <= 63;
    if !ok {
        return Vec::new();
    }
    for (row, x) in rows.iter_mut().zip(rights) {
        let mut left = 0;
        for (c, &right) in row.iter_mut().zip(x) {
            let span = edges.iter().filter(|&&e| e > left && e <= right).count().max(1);
            c.colspan = span as u32;
            left = right;
        }
    }
    let mut prev = 0;
    edges
        .iter()
        .map(|&e| {
            let w = (e - prev) as f32 / 20.0;
            prev = e;
            w
        })
        .collect()
}

/// Parse RTF bytes into a document.
pub fn import(bytes: &[u8]) -> Result<Document, String> {
    let Parsed { flow, page, default_font, headers_footers, title_page } = parse_full(bytes)?;
    let mut doc = model::to_doc_in(&flow, page.section());
    for (which, blocks) in &headers_footers {
        let blocks = model::part_blocks(&mut doc, blocks);
        let id = doc.add_part(if which.footer { wordcraft_doc::PartKind::Footer } else { wordcraft_doc::PartKind::Header }, blocks);
        let set = if which.footer { &mut doc.last_section.footers } else { &mut doc.last_section.headers };
        match which.pages {
            HfPages::Default => set.default = Some(id),
            HfPages::First => set.first = Some(id),
            HfPages::Even => set.even = Some(id),
        }
    }
    doc.last_section.title_page = title_page;
    // RTF's paragraph defaults are single spacing and no space around paragraphs; what a file
    // wants beyond that it states on each paragraph.
    let dp = &mut doc.styles.default_para;
    dp.space_before = Some(0.0);
    dp.space_after = Some(0.0);
    dp.line_spacing = Some(wordcraft_doc::props::LineSpacing::Multiple(1.0));
    if default_font.is_some() {
        doc.styles.default_chr.font = default_font;
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(f: &Flow) -> Vec<String> {
        f.blocks
            .iter()
            .map(|b| match b {
                FBlock::Para(p) => format!("{:?}:{}", p.kind, p.text()),
                FBlock::Table(t) => {
                    format!("table{}x{}:{}", t.rows.len(), t.cols(), t.rows.iter().flatten().map(|c| c.text()).collect::<Vec<_>>().join("|"))
                }
            })
            .collect()
    }

    #[test]
    fn basic_reader() {
        let rtf = br#"{\rtf1\ansi{\fonttbl{\f0 Arial;}{\f1 Courier New;}}{\colortbl;\red255\green0\blue0;}{\stylesheet{\s0 Normal;}{\s1 heading 1;}}
{\info{\title My Doc}{\author Ann}}
\pard\s1 Head\par
\pard Plain {\b bold} \i it\i0  {\cf1 red} caf\'e9 \u8364? and\line next\tab x\par
{\*\unknown skip me}{\field{\*\fldinst{HYPERLINK "http://a.b"}}{\fldrslt{link}}}\par
\trowd\cellx100\cellx200 \pard\intbl a\cell b\cell\row
\pard after\par}"#;
        let f = parse(rtf).unwrap();
        assert_eq!(f.meta.title, "My Doc");
        assert_eq!(f.meta.author, "Ann");
        let t = texts(&f);
        assert_eq!(t[0], "Heading(1):Head");
        assert_eq!(t[1], "Normal:Plain bold it red café € and\nnext\tx");
        assert_eq!(t[2], "Normal:link");
        assert_eq!(t[3], "table1x2:a|b");
        assert_eq!(t[4], "Normal:after");
        let FBlock::Para(p) = &f.blocks[1] else { panic!() };
        assert!(p.inlines.iter().any(|i| matches!(i, Inline::Text(t, f) if t == "bold" && f.bold)));
        assert!(p.inlines.iter().any(|i| matches!(i, Inline::Text(t, f) if t == "red" && f.color == Some(Rgb(255, 0, 0)))));
        let FBlock::Para(l) = &f.blocks[2] else { panic!() };
        assert!(matches!(&l.inlines[0], Inline::Text(_, f) if f.link.as_deref() == Some("http://a.b")));
    }

    #[test]
    fn paragraph_borders() {
        // A heading with a grey rule under it, as hh.ru résumés write their section titles.
        let rtf = br"{\rtf1\ansi{\colortbl;\red0\green0\blue0;\red216\green216\blue216;}
\pard\brdrb\brdrs\brdrw10\brdrcf2 Section\par
\pard Body\par
\pard\brdrt\brdrdb\brdrl\brdrs\brdrr\brdrs Boxed\par
\pard\brdrb\brdrs\par}";
        let f = parse(rtf).unwrap();
        let paras: Vec<&Para> = f.blocks.iter().filter_map(|b| if let FBlock::Para(p) = b { Some(p) } else { None }).collect();
        let b = paras[0].borders.unwrap();
        let bottom = b.bottom.unwrap();
        assert_eq!((bottom.width, bottom.color), (0.5, Some(Rgb(216, 216, 216))));
        assert!(b.top.is_none());
        assert_eq!(paras[0].kind, Kind::Normal);
        // \pard resets them.
        assert!(paras[1].borders.is_none());
        let b = paras[2].borders.unwrap();
        assert_eq!(b.top.map(|x| x.style), Some(wordcraft_doc::props::BorderStyle::Double));
        assert!(b.left.is_some() && b.right.is_some() && b.bottom.is_none());
        // An empty paragraph with only a bottom border is still a rule.
        assert_eq!(paras[3].kind, Kind::Rule);
        // The document's paragraph carries them.
        let d = import(rtf).unwrap();
        let first = d.body.iter().find_map(|b| if let wordcraft_doc::Block::Para(p) = &**b { Some(p) } else { None }).unwrap();
        assert!(first.props.borders.and_then(|b| b.bottom).is_some());
    }

    #[test]
    fn headers_and_footers() {
        let rtf = br"{\rtf1\ansi\titlepg
{\header \pard\qr Running head\par}
{\headerf \pard First head {\b bold}\par}
{\footer \pard Page \chpgn\par}
\pard Body text\par
\trowd\cellx2000\pard\intbl cell\cell\row
\pard after\par}";
        let d = import(rtf).unwrap();
        let s = &d.last_section;
        assert!(s.title_page);
        let text = |id: Option<u32>| {
            let p = id.and_then(|id| d.parts.get(&id)).unwrap();
            p.blocks
                .iter()
                .filter_map(|b| if let wordcraft_doc::Block::Para(p) = &**b { Some(p.text.clone()) } else { None })
                .collect::<Vec<_>>()
                .join("|")
        };
        assert_eq!(text(s.headers.default), "Running head");
        assert_eq!(text(s.headers.first), "First head bold");
        assert!(text(s.footers.default).starts_with("Page"));
        assert!(s.footers.first.is_none() && s.headers.even.is_none());
        // The body keeps only its own text, and its table survives the header groups.
        let f = parse(rtf).unwrap();
        let t = texts(&f);
        assert_eq!(t, ["Normal:Body text", "table1x1:cell", "Normal:after"]);
        // Without \titlepg there is no separate first page.
        let d = import(br"{\rtf1\ansi{\headerf x\par} y\par}").unwrap();
        assert!(!d.last_section.title_page && d.last_section.headers.first.is_some());
        // An unterminated header still ends, and the body text after it isn't lost.
        assert!(import(br"{\rtf1\ansi{\header x").is_ok());
    }

    #[test]
    fn paragraph_spacing_and_indents() {
        use wordcraft_doc::props::LineSpacing;
        let rtf = br"{\rtf1\ansi
\pard\sb120\sa150\sl360\slmult1\li720\ri360\fi-360 a\par
\pard\sl-280 b\par
\pard\sl300\slmult0 c\par
\pard d\par}";
        let f = parse(rtf).unwrap();
        let sp: Vec<model::Spacing> = f.blocks.iter().filter_map(|b| if let FBlock::Para(p) = b { Some(p.spacing) } else { None }).collect();
        assert_eq!(sp[0].before, Some(6.0));
        assert_eq!(sp[0].after, Some(7.5));
        assert_eq!(sp[0].line, Some(LineSpacing::Multiple(1.5)));
        assert_eq!((sp[0].left, sp[0].right, sp[0].first), (Some(36.0), Some(18.0), Some(-18.0)));
        assert_eq!(sp[1].line, Some(LineSpacing::Exactly(14.0)));
        assert_eq!(sp[2].line, Some(LineSpacing::AtLeast(15.0)));
        // \pard resets them.
        assert_eq!(sp[3], model::Spacing::default());
        // The document's defaults are RTF's: single, no space after; the paragraph's own win.
        let d = import(rtf).unwrap();
        assert_eq!(d.styles.default_para.space_after, Some(0.0));
        assert_eq!(d.styles.default_para.line_spacing, Some(LineSpacing::Multiple(1.0)));
        let first = d.body.iter().find_map(|b| if let wordcraft_doc::Block::Para(p) = &**b { Some(p) } else { None }).unwrap();
        assert_eq!(first.props.space_after, Some(7.5));
        assert_eq!(first.props.indent_first, Some(-18.0));
    }

    #[test]
    fn default_font_is_deff_or_font_zero() {
        // Text that never selects a font is in the default font: \deff, else font 0.
        let d = import(br"{\rtf1\ansi{\fonttbl{\f0\fnil Times New Roman Cyr;}{\f1 Arial;}}\plain x\par}").unwrap();
        assert_eq!(d.styles.default_chr.font.as_deref(), Some("Times New Roman Cyr"));
        let d = import(br"{\rtf1\ansi\deff1{\fonttbl{\f0 Times New Roman;}{\f1 Arial;}} x\par}").unwrap();
        assert_eq!(d.styles.default_chr.font.as_deref(), Some("Arial"));
        // No font table: our default stays.
        let d = import(br"{\rtf1\ansi x\par}").unwrap();
        assert_eq!(d.styles.default_chr.font, wordcraft_doc::StyleSheet::builtin().default_chr.font);
    }

    #[test]
    fn page_size_and_margins() {
        let d = import(br"{\rtf1\ansi\paperw11906\paperh16838\margl1134\margr850\margt1134\margb1134 x\par}").unwrap();
        let s = &d.last_section;
        assert!((s.page_w - 595.3).abs() < 0.1 && (s.page_h - 841.9).abs() < 0.1, "{} x {}", s.page_w, s.page_h);
        assert!((s.margin_left - 56.7).abs() < 0.1 && (s.margin_right - 42.5).abs() < 0.1);
        assert!(!s.landscape);
        let d = import(br"{\rtf1\ansi\paperw11906\paperh16838\landscape x\par}").unwrap();
        assert!(d.last_section.landscape && d.last_section.page_w > d.last_section.page_h);
        // Nonsense keeps Word's defaults.
        let d = import(br"{\rtf1\ansi\paperw0\margl99999 x\par}").unwrap();
        assert_eq!(d.last_section, wordcraft_doc::SectionProps::default());
    }

    #[test]
    fn table_grid_from_cell_edges_and_borders() {
        // Rows whose cells don't line up share one grid: edges 2000, 3000, 9000.
        let rtf = br"{\rtf1\ansi
\trowd\cellx2000\cellx9000 \pard\intbl a\cell b\cell\row
\trowd\cellx3000\cellx9000 \pard\intbl c\cell d\cell\row
\pard after\par}";
        let f = parse(rtf).unwrap();
        let FBlock::Table(t) = &f.blocks[0] else { panic!() };
        assert_eq!(t.widths, [100.0, 50.0, 300.0]);
        assert_eq!(t.rows[0].iter().map(|c| c.colspan).collect::<Vec<_>>(), [1, 2]);
        assert_eq!(t.rows[1].iter().map(|c| c.colspan).collect::<Vec<_>>(), [2, 1]);
        assert_eq!(t.cols(), 3);
        // RTF tables have no grid style: each cell draws the borders it names, nothing else.
        assert!(t.borderless);
        assert!(t.rows.iter().flatten().all(|c| c.borders.is_none()));
        // A rule under a cell (a letterhead's line): the bottom side only, its width and colour.
        let f =
            parse(br"{\rtf1\ansi{\colortbl;\red174\green174\blue174;}\trowd\clbrdrb\brdrs\brdrw15\brdrcf1\cellx2000\pard\intbl a\cell\row}").unwrap();
        let FBlock::Table(t) = &f.blocks[0] else { panic!() };
        let b = t.rows[0][0].borders.unwrap();
        assert!(b.top.is_none() && b.left.is_none() && b.right.is_none());
        let bottom = b.bottom.unwrap();
        assert_eq!((bottom.style, bottom.width, bottom.color), (wordcraft_doc::props::BorderStyle::Single, 0.75, Some(Rgb(174, 174, 174))));
        // Every side, in different styles, per cell.
        let f = parse(br"{\rtf1\ansi\trowd\clbrdrt\brdrdb\clbrdrl\brdrs\brdrw10\clbrdrb\brdrdot\clbrdrr\brdrth\cellx2000\cellx4000\pard\intbl a\cell b\cell\row}").unwrap();
        let FBlock::Table(t) = &f.blocks[0] else { panic!() };
        let b = t.rows[0][0].borders.unwrap();
        use wordcraft_doc::props::BorderStyle as S;
        assert_eq!(
            [b.top.map(|x| x.style), b.left.map(|x| x.style), b.bottom.map(|x| x.style), b.right.map(|x| x.style)],
            [Some(S::Double), Some(S::Single), Some(S::Dotted), Some(S::Thick)]
        );
        assert_eq!(b.left.map(|x| x.width), Some(0.5));
        assert!(t.rows[0][1].borders.is_none(), "the second cell names no borders");
        // A border side set to none isn't a border.
        let f = parse(br"{\rtf1\ansi\trowd\clbrdrl\brdrnone\cellx2000\pard\intbl a\cell\row}").unwrap();
        let FBlock::Table(t) = &f.blocks[0] else { panic!() };
        assert!(t.rows[0][0].borders.is_none());
        // The document's cells carry them.
        let d = import(br"{\rtf1\ansi\trowd\clbrdrb\brdrs\cellx2000\pard\intbl a\cell\row}").unwrap();
        let t = d.body.iter().find_map(|b| if let wordcraft_doc::Block::Table(t) = &**b { Some(t) } else { None }).unwrap();
        assert!(t.props.style.is_none());
        assert!(t.rows[0].cells[0].props.borders.and_then(|b| b.bottom).is_some());
        // The table sizes to the document's page, not Letter.
        let d = import(br"{\rtf1\ansi\paperw11906\margl1134\margr1134\trowd\cellx4818\cellx9638\pard\intbl a\cell b\cell\row}").unwrap();
        let t = d.body.iter().find_map(|b| if let wordcraft_doc::Block::Table(t) = &**b { Some(t) } else { None }).unwrap();
        assert_eq!(t.grid, [240.9, 241.0]);
    }

    #[test]
    fn code_pages_from_ansicpg_and_font_charsets() {
        // The document's code page: Russian RTF as accounting systems write it.
        let f = parse(br"{\rtf1\ansi\ansicpg1251\deff0{\fonttbl{\f0 Times New Roman;}}\f0 \'cf\'f0\'e8\'e2\'e5\'f2, \'b8\'eb\'ea\'e0 \'b9 5\par}")
            .unwrap();
        assert_eq!(texts(&f), ["Normal:Привет, ёлка № 5"]);
        // Font charsets switch inside a Windows-1252 document: Western, Cyrillic, Central
        // European, then back to the first font.
        let f = parse(
            br"{\rtf1\ansi\ansicpg1252\deff0{\fonttbl{\f0 Arial;}{\f1\fcharset204 Arial CYR;}{\f2\fcharset238 Arial CE;}}\f0 caf\'e9 {\f1 \'c4\'e0} \f2 \'a3\'f3d\'9f \f0 na\'efve\par}",
        )
        .unwrap();
        assert_eq!(texts(&f), ["Normal:café Да Łódź naïve"]);
        // The default font's charset applies before any \f; an explicit \cpg wins over \fcharset.
        let f = parse(br"{\rtf1\ansi\deff1{\fonttbl{\f1\fcharset204 Times;}{\f2\fcharset0\cpg1253 Greek;}}\'cc\'e8\'f0 \f2 \'c1\'e2\par}").unwrap();
        assert_eq!(texts(&f), ["Normal:Мир Αβ"]);
        // Raw 8-bit text (not escaped) follows the same code page.
        let f = parse(b"{\\rtf1\\ansi\\ansicpg1251 \xc4\xee\xec\\par}").unwrap();
        assert_eq!(texts(&f), ["Normal:Дом"]);
    }

    #[test]
    fn shppict_imports_one_image_and_skips_legacy_duplicate() -> Result<(), String> {
        let pixels = image::RgbaImage::from_pixel(1, 1, image::Rgba([40, 80, 160, 255]));
        let mut png = Vec::new();
        pixels.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).map_err(|e| e.to_string())?;
        let hex: String = png.iter().map(|b| format!("{b:02x}")).collect();
        let pict = format!("{{\\pict\\pngblip\\picwgoal1440\\pichgoal720 {hex}}}");
        // Also use a supported PNG as the legacy duplicate: it must still be skipped.
        for legacy in [format!("{{\\pict\\wmetafile8 {hex}}}"), pict.clone()] {
            for picture in [format!("{{\\*\\shppict{pict}}}"), pict.clone()] {
                let f = parse(format!("{{\\rtf1\\ansi Before{picture}{{\\nonshppict{legacy}}}After\\par}}").as_bytes())?;
                let images: Vec<_> = f
                    .blocks
                    .iter()
                    .filter_map(|b| match b {
                        FBlock::Para(p) => Some(&p.inlines),
                        FBlock::Table(_) => None,
                    })
                    .flatten()
                    .filter_map(|i| match i {
                        Inline::Image(img) => Some(img),
                        _ => None,
                    })
                    .collect();
                assert_eq!(images.len(), 1, "{picture}");
                let img = images.first().ok_or("missing picture")?;
                assert_eq!(img.data.as_ref(), &png);
                assert_eq!(img.ext, "png");
                assert_eq!((img.w, img.h), (72.0, 36.0));
                assert_eq!(texts(&f), vec!["Normal:BeforeAfter"]);
            }
        }
        // Recognizing shppict must not resurrect pictures inside skipped destinations.
        for dest in ["nonshppict", "unknown"] {
            let f = parse(format!("{{\\rtf1{{\\*\\{dest}{{\\*\\shppict{pict}}}}}Before\\par}}").as_bytes())?;
            assert_eq!(texts(&f), vec!["Normal:Before"]);
            assert!(f.blocks.iter().all(|b| match b {
                FBlock::Para(p) => p.inlines.iter().all(|i| !matches!(i, Inline::Image(_))),
                FBlock::Table(_) => false,
            }));
        }
        Ok(())
    }

    #[test]
    fn unicode_in_info_bookmarks_and_field_instructions() {
        // #435: `\uN` (with its `\ucN` fallback skipped) decodes in every text destination.
        let rtf = br#"{\rtf1\ansi\uc1{\info{\title Caf\u233?}{\author Caf\u233\'3f}}{\*\bkmkstart caf\u233?}{\*\bkmkend caf\u233?}{\field{\*\fldinst{HYPERLINK "https://example.test/caf\u233?"}}{\fldrslt{link}}}\par Caf\u233?\par}"#;
        let f = parse(rtf).unwrap();
        assert_eq!(f.meta.title, "Café");
        assert_eq!(f.meta.author, "Café");
        let inl: Vec<&Inline> = f.blocks.iter().flat_map(|b| if let FBlock::Para(p) = b { p.inlines.iter().collect() } else { vec![] }).collect();
        assert!(inl.iter().any(|i| matches!(i, Inline::Anchor(n) if n == "café")), "{inl:?}");
        assert!(
            inl.iter().any(|i| matches!(i, Inline::Text(t, f) if t == "link" && f.link.as_deref() == Some("https://example.test/café"))),
            "{inl:?}"
        );
        assert_eq!(texts(&f).last().map(String::as_str), Some("Normal:Café"));
    }

    #[test]
    fn rejects_non_rtf() {
        assert!(parse(b"hello").is_err());
        assert!(parse(b"{\\rtf1").is_ok());
    }

    #[test]
    fn escapes() {
        assert_eq!(esc("a{b}\\c\té😀"), "a\\{b\\}\\\\c\\tab \\u233?\\u-10179?\\u-8704?");
    }
}
