//! Dialogs: Font, Paragraph, Find & Replace, Go To, Insert Table, Page Setup, Link, Bookmark,
//! Word Count, Zoom, Watermark, New/Modify Style, Manage Styles, New/Modify Table Style, Table
//! Properties, Enclose Characters, Command search, Icons, Paste Special, About, Save Changes, the password to open a
//! document and Encrypt with Password, and the mail-merge Recipient List, Insert Merge Field, Find
//! Recipient, merge rules, Match Fields, Check for Errors and Select Table (a workbook's sheet) (Customize Keyboard is in
//! `dialogs_keyboard`, Customize Ribbon in `dialogs_ribbon`). Every dialog ends by running a
//! command (or shows one's result), so agents get the same result without the dialog.

use egui::{Sense, Ui, vec2};
use serde::Serialize;
use serde_json::{Value, json};

use wordcraft_engine::Password;
use wordcraft_engine::cmd::mailings::{FilterOp, FilterRule, SortKey};

use crate::WordApp;
use crate::options::MeasureExt;
use crate::theme::{Tokens, semibold};

/// The Find and Replace dialog's search options beyond the first row (#543).
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindExtra {
    pub wildcards: bool,
    pub prefix: bool,
    pub suffix: bool,
    pub ignore_punct: bool,
    pub ignore_space: bool,
    /// The search options are shown.
    pub more: bool,
    /// Special ▾ inserts into Replace with (it last had the focus), not Find what.
    pub to_replace: bool,
}

/// The tabs of the Find and Replace dialog (#712).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FindTab {
    #[default]
    Find,
    Replace,
    GoTo,
}

/// What Go To moves to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GoToWhat {
    #[default]
    Page,
    Bookmark,
}

/// The Special ▾ menu: (name, code) for Find what, without and with wildcards.
const SPECIAL_FIND: &[(&str, &str)] = &[
    ("Paragraph Mark", "^p"),
    ("Tab Character", "^t"),
    ("Any Character", "^?"),
    ("Any Digit", "^#"),
    ("Any Letter", "^$"),
    ("Caret Character", "^^"),
    ("Section Character", "^%"),
    ("Paragraph Character", "^v"),
    ("Column Break", "^n"),
    ("Em Dash", "^+"),
    ("En Dash", "^="),
    ("Graphic", "^g"),
    ("Manual Line Break", "^l"),
    ("Manual Page Break", "^m"),
    ("Nonbreaking Hyphen", "^~"),
    ("Nonbreaking Space", "^s"),
    ("Optional Hyphen", "^-"),
    ("Section Break", "^b"),
    ("White Space", "^w"),
];
const SPECIAL_FIND_WILD: &[(&str, &str)] = &[
    ("Any Character", "?"),
    ("Character in Range", "[-]"),
    ("Beginning of Word", "<"),
    ("End of Word", ">"),
    ("Expression", "()"),
    ("Not", "[!]"),
    ("Num Occurrences", "{,}"),
    ("Previous 1 or More", "@"),
    ("0 or More Characters", "*"),
    ("Paragraph Mark", "^13"),
    ("Tab Character", "^t"),
    ("Caret Character", "^^"),
    ("Section Character", "^%"),
    ("Paragraph Character", "^v"),
    ("Column Break", "^n"),
    ("Em Dash", "^+"),
    ("En Dash", "^="),
    ("Graphic", "^g"),
    ("Manual Line Break", "^l"),
    ("Manual Page Break", "^m"),
    ("Nonbreaking Hyphen", "^~"),
    ("Nonbreaking Space", "^s"),
    ("Optional Hyphen", "^-"),
];
/// Replace with; with wildcards, "Find What Expression" (`\1`) comes first.
const SPECIAL_REPLACE: &[(&str, &str)] = &[
    ("Paragraph Mark", "^p"),
    ("Tab Character", "^t"),
    ("Caret Character", "^^"),
    ("Section Character", "^%"),
    ("Paragraph Character", "^v"),
    ("Clipboard Contents", "^c"),
    ("Column Break", "^n"),
    ("Em Dash", "^+"),
    ("En Dash", "^="),
    ("Find What Text", "^&"),
    ("Manual Line Break", "^l"),
    ("Manual Page Break", "^m"),
    ("Nonbreaking Hyphen", "^~"),
    ("Nonbreaking Space", "^s"),
    ("Optional Hyphen", "^-"),
];

/// The Special ▾ menu button; returns the code picked.
fn special_menu(ui: &mut Ui, extra: &FindExtra) -> Option<&'static str> {
    let mut picked = None;
    ui.menu_button(format!("{} ▾", tl!("Special")), |ui| {
        let wild_replace: &[(&str, &str)] = &[("Find What Expression", "\\1")];
        let lists: [&[(&str, &str)]; 2] = match (extra.to_replace, extra.wildcards) {
            (false, false) => [SPECIAL_FIND, &[]],
            (false, true) => [SPECIAL_FIND_WILD, &[]],
            (true, false) => [SPECIAL_REPLACE, &[]],
            (true, true) => [wild_replace, SPECIAL_REPLACE],
        };
        for (name, code) in lists.into_iter().flatten() {
            if ui.button(format!("{}   {code}", tl!(name))).clicked() {
                picked = Some(*code);
                ui.close();
            }
        }
    });
    picked
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "dialog", rename_all = "camelCase")]
pub enum Dialog {
    RenameDocument {
        name: String,
        message: String,
        focus: bool,
    },
    Font {
        /// The font (Latin text) and the Asian text font, as the font lists name them, and their
        /// values when the dialog opened: OK changes only the ones changed (#505).
        font: String,
        font_was: String,
        font_ea: String,
        font_ea_was: String,
        /// Show the Asian text font box: a Chinese or Japanese interface, or East Asian text or
        /// an Asian font in the selection.
        asian: bool,
        size: String,
        bold: bool,
        italic: bool,
        underline: bool,
        strike: bool,
        sup: bool,
        sub: bool,
        small_caps: bool,
        caps: bool,
        hidden: bool,
        color: String,
        spacing: f32,
        /// The properties the fields stood for when the dialog opened: OK sends only what the
        /// user changed, so formatting the dialog can't show (a double underline) or left
        /// alone stays as it is.
        #[serde(skip)]
        start: serde_json::Map<String, Value>,
    },
    Paragraph {
        /// Right-to-left reading order (Direction in Word's Paragraph dialog).
        rtl: bool,
        /// Alignment and indents as seen on the page: `left` is the left side whatever the
        /// direction (the model stores them from the paragraph's start edge).
        align: String,
        left: f32,
        right: f32,
        first: f32,
        before: f32,
        after: f32,
        /// Line spacing as a multiple; Exactly and At Least show as 1.0.
        line: f32,
        /// `line` when the dialog opened: OK keeps the paragraph's spacing, whatever its rule,
        /// unless this changed (#467).
        #[serde(skip)]
        line_was: f32,
        keep_next: bool,
        keep_lines: bool,
        page_break: bool,
        widow: bool,
        /// Page: 0 Indents and Spacing, 1 Line and Page Breaks, 2 Asian Typography.
        tab: u8,
        /// Asian Typography as the model has it (`para.asianTypography`): kinsoku, word wrap,
        /// hanging punctuation, compress punctuation at line start, space between Asian and
        /// Latin text, space between Asian text and numbers.
        asian: [bool; 6],
        /// `asian` when the dialog opened: OK sets only the flags that changed.
        #[serde(skip)]
        asian_was: [bool; 6],
        /// Mirror indents, don't add space between paragraphs of the same style, suppress line
        /// numbers, don't hyphenate (#512).
        opts: [bool; 4],
        /// `opts` when the dialog opened: OK sets only the ones that changed.
        #[serde(skip)]
        opts_was: [bool; 4],
    },
    /// Tabs (#320).
    Tabs {
        form: Box<crate::dialogs_para::TabsForm>,
    },
    /// Borders and Shading (#320).
    Borders {
        form: Box<crate::dialogs_para::BordersForm>,
    },
    Find {
        query: String,
        replace: String,
        match_case: bool,
        whole_word: bool,
        regex: bool,
        /// Search options under "Search Options" (#543).
        extra: FindExtra,
        /// Find, Replace or Go To, one dialog as in Word (#712).
        tab: FindTab,
        goto_what: GoToWhat,
        /// Go To's box: a page number (`+2`, `-1` move from the current page) or a bookmark.
        goto_target: String,
        message: String,
        /// Put the caret in Find what on the first frame, so typing searches (#772).
        focus: bool,
    },
    InsertTable {
        rows: u32,
        cols: u32,
    },
    PageSetup {
        top: f32,
        bottom: f32,
        left: f32,
        right: f32,
        landscape: bool,
        /// Document grid: kind (0 none, 1 lines, 2 lines and characters, 3 snap to characters),
        /// lines per page and characters per line; `grid_was` as opened (applied only if changed).
        grid: (u8, u32, u32),
        grid_was: (u8, u32, u32),
        /// Section direction: right to left (`layout.sectionRtl`).
        rtl: bool,
        /// Paper width and height in the measurement unit as shown (landscape: the wide side
        /// first); `paper_was` as opened (`layout.size` runs only if it changed, #1433).
        paper: (f32, f32),
        paper_was: (f32, f32),
        /// Custom size chosen in Paper size: shown even while the numbers match a named size.
        paper_custom: bool,
        /// Why OK didn't apply (the margins don't fit the paper).
        message: String,
    },
    Link {
        url: String,
        text: String,
    },
    Bookmark {
        name: String,
    },
    WordCount {
        stats: Value,
    },
    Zoom {
        percent: f32,
    },
    Watermark {
        text: String,
        diagonal: bool,
    },
    /// Home › Font › Phonetic Guide: ruby text over the selected text (`format.phonetic`).
    Phonetic {
        base: String,
        ruby: String,
        /// OOXML alignment name (`center`, `distributeLetter`…).
        align: String,
        size: f32,
        offset: f32,
        /// The selection already has a phonetic guide (Remove is offered).
        existing: bool,
    },
    NewStyle {
        name: String,
        based_on: String,
        /// Opened from Manage Styles, which comes back when this closes.
        #[serde(skip)]
        back: bool,
    },
    ModifyStyle {
        #[serde(skip)]
        back: bool,
        /// The dialog that comes back when this closes (Table of Contents › Modify…).
        #[serde(skip)]
        resume: Option<Box<Dialog>>,
        id: String,
        name: String,
        font: String,
        size: f32,
        bold: bool,
        italic: bool,
        color: String,
        before: f32,
        after: f32,
    },
    /// Manage Styles: every style with its preview and description; modify, create, delete and
    /// show or hide them.
    ManageStyles {
        alphabetical: bool,
        selected: String,
    },
    /// Table Design › New / Modify Table Style: one formatting region at a time.
    TableStyle {
        /// The style being modified; `None` creates one.
        id: Option<String>,
        name: String,
        /// Style id.
        based_on: String,
        /// Index into `regions` (see `REGION_LABELS`).
        region: usize,
        regions: Box<[TableRegion; 7]>,
        /// What the regions showed when the dialog opened (or the base style changed): only
        /// changes are sent, so everything else stays inherited.
        #[serde(skip)]
        basis: Box<[TableRegion; 7]>,
    },
    Commands {
        query: String,
        /// Opened by a shortcut that may also send its key's text (Alt+Q's "q", in the same frame
        /// or the next): that text isn't typed into the box; the next key pressed ends this (#819).
        #[serde(skip)]
        swallow: bool,
    },
    /// Columns, Symbol and Field ([`crate::dialogs_insert`], #321).
    Insert(Box<crate::dialogs_insert::InsertDialog>),
    /// Caption, Index, Mark Index Entry, Table of Contents, Source Manager and Create / Edit
    /// Source ([`crate::dialogs_refs`], #402).
    Refs(Box<crate::dialogs_refs::RefsDialog>),
    /// Insert › Icons: the icon library, searchable and filtered by category (empty for all).
    Icons {
        query: String,
        category: String,
        selected: String,
    },
    /// Paste Special: the clipboard's formats and the chosen one. The clipboard payload stays out
    /// of the serialized dialog state (it can be megabytes).
    PasteSpecial {
        formats: Vec<String>,
        choice: usize,
        #[serde(skip)]
        payload: Value,
        /// Shown when nothing can be pasted: what the clipboard holds, or that it is empty (#833).
        #[serde(skip)]
        note: String,
    },
    /// Tabs: 0 About, 1 Contributors, 2 Models.
    About {
        tab: u8,
    },
    /// Confirm before deleting a recovery copy.
    DiscardRecovery {
        id: String,
        name: String,
    },
    /// "Do you want to save changes?" before a user's New, Open, Close, Envelopes, Labels or
    /// Finish & Merge replaces the document (see [`WordApp::run`]); `then` runs once it is
    /// answered (`ui.saveChanges`), and only while the document it asked about
    /// (`Session::document_id`) is still the one open. `message`: why the last Save failed, shown
    /// in the prompt when it comes back after that failure (#1445); empty otherwise.
    SaveChanges {
        name: String,
        then: String,
        #[serde(skip)]
        params: Value,
        #[serde(skip)]
        document: u64,
        #[serde(default)]
        message: String,
    },
    /// Mailings › Select Recipients / Edit Recipient List: a table of recipients to type or
    /// edit (field names on top), or a file to load instead (#240).
    RecipientList {
        fields: Vec<String>,
        rows: Vec<Vec<String>>,
        message: String,
        /// Filter and Sort (#778): applied with `mailings.filterRecipients` on OK.
        filter: RecipientFilter,
    },
    /// Mailings › Select Recipients › Use an Existing List… picked a workbook with several sheets
    /// (#334): which sheet holds the recipients, and whether its first row names the fields.
    /// OK runs `mailings.recipients` with `params` (the file) plus `sheet` and `headers`.
    SelectTable {
        sheets: Vec<String>,
        selected: usize,
        headers: bool,
        message: String,
        #[serde(skip)]
        params: Value,
    },
    /// Mailings › Insert Merge Field: one of the recipient list's fields (or a typed name).
    InsertMergeField {
        fields: Vec<String>,
        field: String,
    },
    /// A field's code: a new field (Ctrl+F9, `edit` false) or the selected field's code to edit
    /// (double-click a field showing its code, right-click › Edit Field…).
    FieldCode {
        code: String,
        edit: bool,
        message: String,
    },
    /// Mailings › Find Recipient.
    FindRecipient {
        text: String,
        message: String,
    },
    /// Mailings › Rules › If…Then…Else… (`IF`) or Skip Record If… (`SKIPIF`): the condition, and
    /// for If the text to insert either way.
    MergeRule {
        rule: String,
        fields: Vec<String>,
        field: String,
        value: String,
        then: String,
        els: String,
    },
    /// Mailings › Match Fields: which recipient-list field fills each address part.
    MatchFields {
        fields: Vec<String>,
        address: Value,
    },
    /// Mailings › Check for Errors: merge fields the recipient list doesn't have.
    CheckErrors {
        unknown: Vec<String>,
        records: u64,
    },
    /// Table Layout › Properties: the table, the caret's row, column and cell. Only what changed
    /// from `basis` (the values when it opened) is applied, as one `table.properties` step.
    TableProperties {
        form: Box<TableForm>,
        #[serde(skip)]
        basis: Box<TableForm>,
    },
    /// Opening a password-protected document (#55): the password, then `file.open` with
    /// `params` again. The password is never serialised or printed.
    Password {
        name: String,
        #[serde(skip)]
        params: Value,
        #[serde(skip)]
        password: Password,
        message: String,
    },
    /// File › Info › Protect Document › Encrypt with Password: a password typed twice
    /// (`file.encrypt`); left empty, it removes the password.
    EncryptPassword {
        #[serde(skip)]
        password: Password,
        #[serde(skip)]
        confirm: Password,
        message: String,
    },
    /// Home › Paragraph › Multilevel List › Define New Multilevel List (#328).
    DefineList {
        form: Box<crate::dialogs_lists::ListForm>,
    },
    /// Review › Tracking › Track Changes Options (#328).
    TrackOptions {
        form: Box<crate::dialogs_lists::TrackForm>,
    },
    /// Compare / Combine Documents and AutoCorrect Options ([`crate::dialogs_review`], #412).
    Review(Box<crate::dialogs_review::ReviewDialog>),
    /// Tools › Customize Keyboard (#368).
    CustomizeKeyboard {
        form: Box<crate::dialogs_keyboard::KeyboardForm>,
    },
    /// Tools › Customize Ribbon and the Quick Access Toolbar (#379).
    CustomizeRibbon {
        form: Box<crate::dialogs_ribbon::RibbonForm>,
    },
    /// Developer › Controls › Properties.
    Control {
        form: Box<crate::dialogs_controls::ControlForm>,
    },
    /// Home › Font › Enclose Characters: the style (`none` takes the enclosure away, `shrink`,
    /// `enlarge`), the text and the shape (`circle`, `square`, `triangle`, `diamond`).
    Enclose {
        style: String,
        text: String,
        shape: String,
    },
    /// References › Footnotes › Footnote and Endnote (#385).
    NoteOptions {
        form: Box<crate::dialogs_notes::NoteForm>,
    },
    /// View › Templates › Templates and Add-ins (#377).
    Templates {
        form: Box<crate::dialogs_templates::TemplatesForm>,
    },
    /// Templates and Add-ins › Organizer: copy, delete and rename styles (#377).
    Organizer {
        form: Box<crate::dialogs_templates::OrganizerForm>,
    },
    /// Line Numbers, Hyphenation, Language, Envelopes and Labels (#407).
    Layout(Box<crate::dialogs_layout::LayoutDialog>),
}

/// The Table Properties dialog's fields. Lengths are in the interface unit
/// ([`wordcraft_geom::Unit`], inches by default).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableForm {
    pub width_on: bool,
    pub width: f32,
    /// `left`, `center` or `right`.
    pub align: String,
    pub indent: f32,
    /// Table direction: right to left (`w:bidiVisual`).
    pub rtl: bool,
    pub row_height_on: bool,
    pub row_height: f32,
    pub row_exact: bool,
    pub allow_break: bool,
    pub header_row: bool,
    pub column_width: f32,
    pub cell_width_on: bool,
    pub cell_width: f32,
    /// `top`, `center` or `bottom`.
    pub valign: String,
}

impl TableForm {
    /// The caret's table, row, column and cell as the dialog shows them.
    pub fn read(app: &WordApp) -> Option<TableForm> {
        use wordcraft_doc::props::{Align, HeightRule, VAlign};
        let s = &app.session;
        let (tp, r, c) = s.sel.focus.path.cell()?;
        let t = s.doc.table(s.sel.focus.story, &tp)?;
        let k = crate::options::unit().pt_per_unit();
        let row = t.rows.get(r).map(|x| x.props.clone()).unwrap_or_default();
        let cell = t.rows.get(r).and_then(|x| x.cells.get(c)).map(|x| x.props.clone()).unwrap_or_default();
        let tw = wordcraft_engine::cmd::page::sect(s).text_width();
        let width = t.props.width_pct.filter(|p| *p > 0.0).map(|p| tw * p.min(100.0) / 100.0).or(t.props.width.filter(|w| *w > 0.0));
        let fin = |v: f32| if v.is_finite() { v } else { 0.0 };
        Some(TableForm {
            width_on: width.is_some(),
            width: fin(width.unwrap_or(tw)) / k,
            align: match t.props.align {
                Some(Align::Center) => "center",
                Some(Align::Right) => "right",
                _ => "left",
            }
            .into(),
            indent: fin(t.props.indent.unwrap_or(0.0)) / k,
            rtl: t.props.bidi_visual,
            row_height_on: row.height.is_some() && row.height_rule != HeightRule::Auto,
            row_height: fin(row.height.unwrap_or(18.0)) / k,
            row_exact: row.height_rule == HeightRule::Exact,
            allow_break: !row.cant_split,
            header_row: row.header,
            column_width: fin(t.grid.get(t.grid_col(r, c)).copied().unwrap_or(72.0)) / k,
            cell_width_on: cell.width.is_some(),
            cell_width: fin(cell.width.unwrap_or(72.0)) / k,
            valign: match cell.valign {
                VAlign::Center => "center",
                VAlign::Bottom => "bottom",
                _ => "top",
            }
            .into(),
        })
    }

    /// `table.properties` parameters for what differs from `basis` (empty when nothing does).
    pub fn changes(&self, basis: &TableForm) -> Value {
        let k = crate::options::unit().pt_per_unit();
        let moved = |a: f32, b: f32| (a - b).abs() > 1e-4;
        let mut v = serde_json::Map::new();
        if self.width_on != basis.width_on || (self.width_on && moved(self.width, basis.width)) {
            v.insert("width".into(), if self.width_on { json!(self.width * k) } else { Value::Null });
        }
        if self.align != basis.align {
            v.insert("align".into(), json!(self.align));
        }
        if moved(self.indent, basis.indent) {
            v.insert("indent".into(), json!(self.indent * k));
        }
        if self.rtl != basis.rtl {
            v.insert("rtl".into(), json!(self.rtl));
        }
        if self.row_height_on != basis.row_height_on
            || (self.row_height_on && (moved(self.row_height, basis.row_height) || self.row_exact != basis.row_exact))
        {
            v.insert("rowHeight".into(), if self.row_height_on { json!(self.row_height * k) } else { Value::Null });
            v.insert("rowHeightRule".into(), json!(if self.row_exact { "exact" } else { "atLeast" }));
        }
        if self.allow_break != basis.allow_break {
            v.insert("allowBreak".into(), json!(self.allow_break));
        }
        if self.header_row != basis.header_row {
            v.insert("headerRow".into(), json!(self.header_row));
        }
        if moved(self.column_width, basis.column_width) {
            v.insert("columnWidth".into(), json!(self.column_width * k));
        }
        if self.cell_width_on != basis.cell_width_on || (self.cell_width_on && moved(self.cell_width, basis.cell_width)) {
            v.insert("cellWidth".into(), if self.cell_width_on { json!(self.cell_width * k) } else { Value::Null });
        }
        if self.valign != basis.valign {
            v.insert("valign".into(), json!(self.valign));
        }
        Value::Object(v)
    }
}

/// The address parts Match Fields lists, with their keys in `mailings.matchFields`' `address`.
pub const ADDRESS_PARTS: [(&str, &str); 9] = [
    ("Courtesy Title", "title"),
    ("First Name", "firstName"),
    ("Last Name", "lastName"),
    ("Company", "company"),
    ("Address", "address"),
    ("City", "city"),
    ("State", "state"),
    ("Postal Code", "postal"),
    ("Country", "country"),
];

/// The `mailings.rules` parameters a Merge Rule dialog stands for, once it names a field.
pub fn rule_params(d: &Dialog) -> Option<Value> {
    let Dialog::MergeRule { rule, field, value, then, els, .. } = d else { return None };
    let field = field.trim();
    if field.is_empty() {
        return None;
    }
    Some(if rule == "SKIPIF" {
        json!({"rule": "SKIPIF", "field": field, "value": value})
    } else {
        json!({"rule": "IF", "field": field, "value": value, "then": then, "else": els})
    })
}

/// The fields a new recipient list starts with.
pub const NEW_LIST_FIELDS: [&str; 5] = ["First Name", "Last Name", "Address", "City", "Postal Code"];

/// A typed recipient list ready for `mailings.recipients`: field names trimmed, blank rows
/// dropped. An error says what to fix.
pub fn recipient_table(fields: &[String], rows: &[Vec<String>]) -> Result<Value, &'static str> {
    let fields: Vec<&str> = fields.iter().map(|f| f.trim()).collect();
    if fields.is_empty() {
        return Err("Add at least one field.");
    }
    if fields.iter().any(|f| f.is_empty()) {
        return Err("Every field needs a name.");
    }
    for (i, f) in fields.iter().enumerate() {
        if fields.iter().skip(i + 1).any(|g| g.eq_ignore_ascii_case(f)) {
            return Err("Field names must be different.");
        }
    }
    let rows: Vec<&Vec<String>> = rows.iter().filter(|r| r.iter().any(|c| !c.trim().is_empty())).collect();
    if rows.is_empty() {
        return Err("Type at least one recipient.");
    }
    Ok(json!({"fields": fields, "rows": rows}))
}

/// The Recipient List dialog's Filter and Sort section (#778).
#[derive(Clone, Debug, Default, Serialize)]
pub struct RecipientFilter {
    /// Whether the section is shown.
    pub open: bool,
    pub rules: Vec<FilterRow>,
    /// Sort by, then by, then by: a field (empty: none) and descending.
    pub sort: [(String, bool); 3],
}

/// One filter condition row: `field op value`, joined to the row above with And or Or.
#[derive(Clone, Debug, Serialize)]
pub struct FilterRow {
    pub field: String,
    #[serde(serialize_with = "op_id")]
    pub op: FilterOp,
    pub value: String,
    pub or: bool,
}

fn op_id<S: serde::Serializer>(op: &FilterOp, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(op.id())
}

impl RecipientFilter {
    /// The session's current filter and sort.
    pub fn from_session(m: &wordcraft_engine::cmd::mailings::MergeState) -> Self {
        let mut sort: [(String, bool); 3] = Default::default();
        for (slot, k) in sort.iter_mut().zip(&m.sort) {
            *slot = (k.field.clone(), k.descending);
        }
        let rules: Vec<FilterRow> =
            m.filter.iter().map(|r| FilterRow { field: r.field.clone(), op: r.op, value: r.value.clone(), or: r.or }).collect();
        RecipientFilter { open: !rules.is_empty() || !m.sort.is_empty(), rules, sort }
    }

    fn engine(&self) -> (Vec<FilterRule>, Vec<SortKey>) {
        let rules = self
            .rules
            .iter()
            .filter(|r| !r.field.is_empty())
            .map(|r| FilterRule { field: r.field.clone(), op: r.op, value: r.value.clone(), or: r.or })
            .collect();
        let sort = self.sort.iter().filter(|(f, _)| !f.is_empty()).map(|(f, d)| SortKey { field: f.clone(), descending: *d }).collect();
        (rules, sort)
    }

    fn is_empty(&self) -> bool {
        self.rules.iter().all(|r| r.field.is_empty()) && self.sort.iter().all(|(f, _)| f.is_empty())
    }

    /// `mailings.filterRecipients` params.
    pub fn params(&self) -> Value {
        let (rules, sort) = self.engine();
        json!({
            "rules": rules.iter().map(|r| json!({"field": r.field, "op": r.op.id(), "value": r.value, "join": if r.or { "or" } else { "and" }})).collect::<Vec<_>>(),
            "sort": sort.iter().map(|k| json!({"field": k.field, "descending": k.descending})).collect::<Vec<_>>(),
        })
    }
}

/// One table style region in the Table Style dialog: colours are hex, empty for none.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableRegion {
    pub borders: bool,
    pub fill: String,
    pub bold: bool,
    pub color: String,
}

/// The Table Style dialog's regions, in the order of `table_style::REGIONS` (the param names).
const REGION_LABELS: [&str; 7] = ["Whole Table", "Header Row", "Total Row", "First Column", "Last Column", "Banded Rows", "Banded Columns"];

/// The regions of table style `id` as resolved through its based-on chain.
fn table_regions(app: &WordApp, id: &str) -> Box<[TableRegion; 7]> {
    let Some(st) = app.session.doc.styles.table_style(id) else { return Default::default() };
    let p = &st.parts;
    let hex = |c: Option<wordcraft_doc::Rgb>| c.map(|c| c.hex()).unwrap_or_default();
    let text = |c: &wordcraft_doc::CharProps| match c.color {
        Some(wordcraft_doc::TextColor::Rgb(c)) => c.hex(),
        _ => String::new(),
    };
    let lines = |b: Option<wordcraft_doc::props::Borders>| b.is_some_and(|b| b.any_visible());
    // A region's text: the whole table's formatting with the region's over it.
    let region = |borders: Option<wordcraft_doc::props::Borders>, fill: Option<wordcraft_doc::Rgb>, chr: &wordcraft_doc::CharProps| {
        let mut c = st.chr.clone();
        c.overlay(chr);
        TableRegion { borders: lines(borders), fill: hex(fill), bold: c.bold.unwrap_or(false), color: text(&c) }
    };
    let none = wordcraft_doc::CharProps::default();
    Box::new([
        region(p.borders, p.fill, &none),
        region(p.header_borders, p.header_fill, &p.header_chr),
        region(p.total_borders, p.total_fill, &p.total_chr),
        region(p.first_col_borders, p.first_col_fill, &p.first_col_chr),
        region(p.last_col_borders, p.last_col_fill, &p.last_col_chr),
        region(p.band_borders, p.band_fill, &p.band_chr),
        region(p.col_band_borders, p.col_band_fill, &p.col_band_chr),
    ])
}

/// The params `table.newStyle` / `table.modifyStyle` take for the Table Style dialog's state:
/// only what changed from `basis`, so the rest stays inherited.
fn table_style_params(id: Option<&str>, name: &str, based_on: &str, regions: &[TableRegion; 7], basis: &[TableRegion; 7]) -> Value {
    let mut v = json!({"name": name.trim(), "basedOn": based_on});
    for ((key, r), b) in wordcraft_engine::cmd::table_style::REGIONS.into_iter().zip(regions.iter()).zip(basis.iter()) {
        let ch = region_changes(r, b);
        if ch.as_object().is_some_and(|o| !o.is_empty()) {
            v[key] = ch;
        }
    }
    if let Some(id) = id {
        v["style"] = json!(id);
    }
    v
}

/// A small sample table drawn with `style` (5 columns, a header, three body rows and a total
/// row), showing the regions `look` turns on: the Table Style dialog's live preview.
fn table_style_preview(ui: &mut Ui, style: Option<&wordcraft_doc::styles::TableStyleProps>, look: wordcraft_doc::props::TableLook) {
    use wordcraft_doc::props::{Border, Borders};
    const COLS: usize = 5;
    const ROWS: usize = 5;
    const TEXT: [[&str; COLS]; ROWS] = [
        ["", "Mon", "Tue", "Wed", "Sum"],
        ["North", "4", "7", "2", "13"],
        ["South", "6", "1", "5", "12"],
        ["West", "3", "8", "4", "15"],
        ["Total", "13", "16", "11", "40"],
    ];
    let (rect, _) = ui.allocate_exact_size(vec2(300.0, 120.0), Sense::hover());
    let painter = ui.painter_at(rect);
    // The page is white whatever the interface theme.
    painter.rect_filled(rect, 2.0, egui::Color32::WHITE);
    let t = Tokens::get(ui.ctx());
    painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    let Some(st) = style else { return };
    let p = &st.parts;
    let table = rect.shrink2(vec2(14.0, 10.0));
    let (cw, rh) = (table.width() / COLS as f32, table.height() / ROWS as f32);
    let rgb = |c: wordcraft_doc::Rgb| egui::Color32::from_rgb(c.0, c.1, c.2);
    let tb = p.borders.unwrap_or_default();
    for (ri, row) in TEXT.iter().enumerate() {
        let header = look.header_row && ri == 0;
        let total = look.total_row && ri + 1 == ROWS;
        let band = look.banded_rows
            && !header
            && (ri.saturating_sub(usize::from(look.header_row)) / p.band_size.unwrap_or(1).clamp(1, 1000) as usize).is_multiple_of(2);
        for (ci, txt) in row.iter().enumerate() {
            // Sample words in the interface language; numbers as they are.
            let txt = &if txt.chars().any(char::is_alphabetic) { tl!(txt) } else { *txt };
            let first = look.first_column && ci == 0;
            let last = look.last_column && ci + 1 == COLS;
            let col_band = look.banded_columns && !first && ci.saturating_sub(usize::from(look.first_column)).is_multiple_of(2);
            let cell = egui::Rect::from_min_size(table.min + vec2(ci as f32 * cw, ri as f32 * rh), vec2(cw, rh));
            // Regions from lowest to highest priority, as layout applies them.
            let regions = [
                (col_band, p.col_band_fill, &p.col_band_chr, p.col_band_borders),
                (band, p.band_fill, &p.band_chr, p.band_borders),
                (first, p.first_col_fill, &p.first_col_chr, p.first_col_borders),
                (last, p.last_col_fill, &p.last_col_chr, p.last_col_borders),
                (header, p.header_fill, &p.header_chr, p.header_borders),
                (total, p.total_fill, &p.total_chr, p.total_borders),
            ];
            let mut fill = p.fill;
            let mut chr = st.chr.clone();
            let mut b = Borders {
                top: if ri == 0 { tb.top } else { tb.between },
                bottom: if ri + 1 == ROWS { tb.bottom } else { tb.between },
                left: if ci == 0 { tb.left } else { tb.inside_v },
                right: if ci + 1 == COLS { tb.right } else { tb.inside_v },
                between: None,
                inside_v: None,
            };
            for (on, f, c, rb) in regions {
                if !on {
                    continue;
                }
                fill = f.or(fill);
                chr.overlay(c);
                if let Some(rb) = rb {
                    b.overlay(&Borders { between: None, inside_v: None, ..rb });
                }
            }
            if total && p.total_borders.is_none_or(|x| x.top.is_none()) {
                b.top = p.total_border_top.or(b.top);
            }
            if let Some(f) = fill {
                painter.rect_filled(cell, 0.0, rgb(f));
            }
            let edge = |from: egui::Pos2, to: egui::Pos2, e: Option<Border>| {
                if let Some(e) = e.filter(Border::is_visible) {
                    let color = e.color.map_or(egui::Color32::BLACK, rgb);
                    painter.line_segment([from, to], egui::Stroke::new(e.width.clamp(0.5, 3.0) * 1.3, color));
                }
            };
            edge(cell.left_top(), cell.right_top(), b.top);
            edge(cell.left_bottom(), cell.right_bottom(), b.bottom);
            edge(cell.left_top(), cell.left_bottom(), b.left);
            edge(cell.right_top(), cell.right_bottom(), b.right);
            let color = match chr.color {
                Some(wordcraft_doc::TextColor::Rgb(c)) => rgb(c),
                _ => egui::Color32::BLACK,
            };
            let font = egui::FontId::proportional(11.0);
            let pos = cell.left_center() + vec2(4.0, 0.0);
            painter.text(pos, egui::Align2::LEFT_CENTER, tl!(*txt), font.clone(), color);
            if chr.bold.unwrap_or(false) {
                // A second pass a hair to the right reads as bold at this size.
                painter.text(pos + vec2(0.6, 0.0), egui::Align2::LEFT_CENTER, tl!(*txt), font, color);
            }
        }
    }
}

/// The style of the table at the caret, when it is a table style.
fn current_table_style(app: &WordApp) -> Option<String> {
    let s = &app.session;
    let (tp, _, _) = s.sel.focus.path.cell()?;
    let id = s.doc.table(s.sel.focus.story, &tp)?.props.style.clone()?;
    s.doc.styles.get(&id).filter(|st| st.kind == wordcraft_doc::StyleKind::Table).map(|st| st.id.clone())
}

/// The params `table.newStyle` / `table.modifyStyle` need for what changed in one region.
fn region_changes(r: &TableRegion, basis: &TableRegion) -> Value {
    let color = |c: &str| if c.is_empty() { Value::Null } else { json!(c) };
    let mut v = json!({});
    if r.borders != basis.borders {
        v["borders"] = json!(r.borders);
    }
    if r.fill != basis.fill {
        v["fill"] = color(&r.fill);
    }
    if r.bold != basis.bold {
        v["bold"] = json!(r.bold);
    }
    if r.color != basis.color {
        v["color"] = color(&r.color);
    }
    v
}

/// A colour menu: a swatch with the colour grid and No Color. `value` is hex, empty for none.
pub(crate) fn color_menu(ui: &mut Ui, theme: &[wordcraft_doc::Rgb], value: &mut String) {
    color_field(ui, theme, value, "No Color", "");
}

/// A colour field: a swatch and a drop-down with `none_label` (which sets `none_value`), the
/// colour grid, Recent Colors and More Colors… (#606). `value` is hex or `none_value`.
pub(crate) fn color_field(ui: &mut Ui, theme: &[wordcraft_doc::Rgb], value: &mut String, none_label: &str, none_value: &str) {
    ui.horizontal(|ui| {
        let id = ui.id().with("colorField");
        crate::colors::take_field(ui.ctx(), id, value);
        let c = wordcraft_doc::Rgb::parse(value);
        let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
        let t = Tokens::get(ui.ctx());
        match c {
            Some(c) => {
                ui.painter().rect_filled(r, 2.0, crate::theme::c32(c));
            }
            None => {
                ui.painter().line_segment([r.left_bottom(), r.right_top()], egui::Stroke::new(1.0, t.text_dim));
            }
        }
        ui.painter().rect_stroke(r, 2.0, egui::Stroke::new(1.0, t.border_strong), egui::StrokeKind::Inside);
        let label = if c.is_some() { value.clone() } else { tl!(none_label).to_string() };
        ui.menu_button(label, |ui| {
            if ui.button(tl!(none_label)).clicked() {
                *value = none_value.to_string();
                ui.close();
            }
            crate::colors::field_menu(ui, theme, id, value);
        });
    });
}

impl Dialog {
    pub fn name(&self) -> &'static str {
        match self {
            Dialog::RenameDocument { .. } => "renameDocument",
            Dialog::Font { .. } => "font",
            Dialog::Paragraph { .. } => "paragraph",
            Dialog::Tabs { .. } => "tabs",
            Dialog::Borders { .. } => "borders",
            Dialog::Find { tab: FindTab::Find, .. } => "find",
            Dialog::Find { tab: FindTab::Replace, .. } => "replace",
            Dialog::Find { tab: FindTab::GoTo, .. } => "goto",
            Dialog::InsertTable { .. } => "insertTable",
            Dialog::PageSetup { .. } => "pageSetup",
            Dialog::Link { .. } => "link",
            Dialog::Bookmark { .. } => "bookmark",
            Dialog::WordCount { .. } => "wordCount",
            Dialog::Zoom { .. } => "zoom",
            Dialog::Watermark { .. } => "watermark",
            Dialog::Phonetic { .. } => "phonetic",
            Dialog::NewStyle { .. } => "newStyle",
            Dialog::ModifyStyle { .. } => "modifyStyle",
            Dialog::ManageStyles { .. } => "manageStyles",
            Dialog::TableStyle { id: None, .. } => "newTableStyle",
            Dialog::TableStyle { .. } => "modifyTableStyle",
            Dialog::Commands { .. } => "commands",
            Dialog::Insert(d) => d.name(),
            Dialog::Refs(d) => d.name(),
            Dialog::Icons { .. } => "icons",
            Dialog::PasteSpecial { .. } => "pasteSpecial",
            Dialog::About { .. } => "about",
            Dialog::SaveChanges { .. } => "saveChanges",
            Dialog::DiscardRecovery { .. } => "discardRecovery",
            Dialog::RecipientList { .. } => "recipientList",
            Dialog::SelectTable { .. } => "selectTable",
            Dialog::InsertMergeField { .. } => "insertMergeField",
            Dialog::FindRecipient { .. } => "findRecipient",
            Dialog::FieldCode { edit: false, .. } => "fieldCode",
            Dialog::FieldCode { .. } => "editFieldCode",
            Dialog::MergeRule { rule, .. } if rule == "SKIPIF" => "ruleSkipIf",
            Dialog::MergeRule { .. } => "ruleIf",
            Dialog::MatchFields { .. } => "matchFields",
            Dialog::CheckErrors { .. } => "checkErrors",
            Dialog::TableProperties { .. } => "tableProperties",
            Dialog::Password { .. } => "password",
            Dialog::EncryptPassword { .. } => "encryptPassword",
            Dialog::DefineList { .. } => "defineList",
            Dialog::TrackOptions { .. } => "trackChangesOptions",
            Dialog::Review(d) => d.name(),
            Dialog::CustomizeKeyboard { .. } => "customizeKeyboard",
            Dialog::CustomizeRibbon { .. } => "customizeRibbon",
            Dialog::Control { .. } => "controlProperties",
            Dialog::Enclose { .. } => "enclose",
            Dialog::NoteOptions { .. } => "noteOptions",
            Dialog::Templates { .. } => "templates",
            Dialog::Organizer { .. } => "organizer",
            Dialog::Layout(d) => d.name(),
        }
    }

    /// The dialog that shows a report command's result: Match Fields, Check for Errors.
    pub fn report(id: &str, v: &Value) -> Option<Dialog> {
        let strings = |k: &str| -> Vec<String> {
            v.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
        };
        match id {
            "mailings.matchFields" => {
                Some(Dialog::MatchFields { fields: strings("fields"), address: v.get("address").cloned().unwrap_or(Value::Null) })
            }
            "mailings.checkErrors" => {
                Some(Dialog::CheckErrors { unknown: strings("unknownFields"), records: v.get("records").and_then(Value::as_u64).unwrap_or(0) })
            }
            _ => None,
        }
    }

    pub fn open(name: &str, app: &mut WordApp) -> Option<Dialog> {
        if let Some(d) = crate::dialogs_insert::open(name, app) {
            return Some(Dialog::Insert(Box::new(d)));
        }
        if let Some(d) = crate::dialogs_refs::open(name, app) {
            return Some(Dialog::Refs(Box::new(d)));
        }
        if let Some(d) = crate::dialogs_layout::open(name, app) {
            return Some(Dialog::Layout(Box::new(d)));
        }
        if matches!(name, "compare" | "combine" | "autoCorrect" | "restrictEditing") {
            return crate::dialogs_review::open(name, app).map(|d| Dialog::Review(Box::new(d)));
        }
        let st = app.session.run("format.state", &json!({})).unwrap_or_default();
        let s = |k: &str| st.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let b = |k: &str| st.get(k).and_then(Value::as_bool).unwrap_or(false);
        Some(match name {
            "font" => {
                let font = app.previews.font_shown(&s("font"));
                let font_ea = app.previews.font_shown(&s("fontEastAsia"));
                let lang = crate::i18n::current().code();
                let text = app.session.selected_text();
                let text = if text.is_empty() {
                    app.session.doc.para_at(&app.session.sel.focus).map(|p| p.plain_text().to_string()).unwrap_or_default()
                } else {
                    text
                };
                let asian = ["zh", "ja", "ko"].iter().any(|l| lang.starts_with(l)) || !font_ea.is_empty() || text.chars().any(east_asian_char);
                let mut d = Dialog::Font {
                    font_was: font.clone(),
                    font,
                    font_ea_was: font_ea.clone(),
                    font_ea,
                    asian,
                    size: st.get("size").and_then(Value::as_f64).map(|v| v.to_string()).unwrap_or_default(),
                    bold: b("bold"),
                    italic: b("italic"),
                    underline: b("underline"),
                    strike: b("strike"),
                    sup: b("superscript"),
                    sub: b("subscript"),
                    small_caps: b("smallCaps"),
                    caps: b("caps"),
                    hidden: b("hidden"),
                    color: s("color"),
                    spacing: st.get("spacing").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                    start: serde_json::Map::new(),
                };
                let props = font_props(&d);
                if let Dialog::Font { start, .. } = &mut d {
                    *start = props;
                }
                d
            }
            "paragraph" | "asianTypography" => {
                let rp = app.session.doc.para_at(&app.session.sel.focus).map(|p| app.session.doc.styles.resolve_para(&p.props));
                let rp = rp?;
                let line = match rp.line_spacing {
                    wordcraft_doc::props::LineSpacing::Multiple(m) => m,
                    _ => 1.0,
                };
                let (vl, vr) = rp.visual_indents();
                Dialog::Paragraph {
                    rtl: rp.bidi,
                    align: format!("{:?}", rp.align.visual(rp.bidi)).to_lowercase(),
                    left: vl / crate::options::unit().pt_per_unit(),
                    right: vr / crate::options::unit().pt_per_unit(),
                    first: rp.indent_first / crate::options::unit().pt_per_unit(),
                    before: rp.space_before,
                    after: rp.space_after,
                    line,
                    line_was: line,
                    keep_next: rp.keep_next,
                    keep_lines: rp.keep_lines,
                    page_break: rp.page_break_before,
                    widow: rp.widow_control,
                    tab: if name == "asianTypography" { 2 } else { 0 },
                    asian: [rp.kinsoku, rp.word_wrap, rp.overflow_punct, rp.top_line_punct, rp.auto_space_de, rp.auto_space_dn],
                    asian_was: [rp.kinsoku, rp.word_wrap, rp.overflow_punct, rp.top_line_punct, rp.auto_space_de, rp.auto_space_dn],
                    opts: [rp.mirror_indents, rp.contextual_spacing, rp.suppress_line_numbers, rp.suppress_hyphens],
                    opts_was: [rp.mirror_indents, rp.contextual_spacing, rp.suppress_line_numbers, rp.suppress_hyphens],
                }
            }
            "tabs" => Dialog::Tabs { form: Box::new(crate::dialogs_para::TabsForm::read(app)?) },
            "borders" | "pageBorders" | "shading" => {
                let tab = match name {
                    "pageBorders" => 1,
                    "shading" => 2,
                    _ => 0,
                };
                Dialog::Borders { form: Box::new(crate::dialogs_para::BordersForm::read(app, tab)?) }
            }
            "find" | "replace" | "goto" => Dialog::Find {
                query: if app.session.sel.is_collapsed() { app.session.find.query.clone() } else { app.session.selected_text() },
                replace: app.session.find.replace.clone(),
                match_case: app.session.find.match_case,
                whole_word: app.session.find.whole_word,
                regex: app.session.find.regex,
                extra: FindExtra {
                    wildcards: app.session.find.wildcards,
                    prefix: app.session.find.match_prefix,
                    suffix: app.session.find.match_suffix,
                    ignore_punct: app.session.find.ignore_punct,
                    ignore_space: app.session.find.ignore_space,
                    more: app.session.find.wildcards
                        || app.session.find.match_prefix
                        || app.session.find.match_suffix
                        || app.session.find.ignore_punct
                        || app.session.find.ignore_space,
                    to_replace: false,
                },
                tab: match name {
                    "replace" => FindTab::Replace,
                    "goto" => FindTab::GoTo,
                    _ => FindTab::Find,
                },
                goto_what: GoToWhat::Page,
                goto_target: String::new(),
                message: String::new(),
                focus: true,
            },
            "insertTable" => Dialog::InsertTable { rows: 2, cols: 5 },
            "pageSetup" => {
                let sp = wordcraft_engine::cmd::page::sect(&app.session);
                use wordcraft_doc::section::DocGridType;
                let kind = match sp.doc_grid.map(|g| g.kind) {
                    Some(DocGridType::Lines) => 1,
                    Some(DocGridType::LinesAndChars) => 2,
                    Some(DocGridType::SnapToChars) => 3,
                    _ => 0,
                };
                let size = app.session.doc.styles.resolve_char(Some("Normal"), &Default::default()).size;
                let grid = (kind, sp.grid_lines_per_page().min(1000), sp.grid_chars_per_line(size).min(1000));
                let paper = (sp.page_w / crate::options::unit().pt_per_unit(), sp.page_h / crate::options::unit().pt_per_unit());
                Dialog::PageSetup {
                    paper,
                    paper_was: paper,
                    paper_custom: false,
                    message: String::new(),
                    top: sp.margin_top / crate::options::unit().pt_per_unit(),
                    bottom: sp.margin_bottom / crate::options::unit().pt_per_unit(),
                    left: sp.margin_left / crate::options::unit().pt_per_unit(),
                    right: sp.margin_right / crate::options::unit().pt_per_unit(),
                    landscape: sp.landscape,
                    grid,
                    grid_was: grid,
                    rtl: sp.rtl,
                }
            }
            "link" => Dialog::Link { url: "https://".into(), text: app.session.selected_text() },
            "bookmark" => Dialog::Bookmark { name: String::new() },
            "wordCount" => Dialog::WordCount { stats: app.session.run("review.wordCount", &json!({})).unwrap_or_default() },
            "zoom" => Dialog::Zoom { percent: (app.session.view.zoom * 100.0).round() },
            "watermark" => Dialog::Watermark { text: "CONFIDENTIAL".into(), diagonal: true },
            "phonetic" => {
                let v = wordcraft_engine::cmd::phonetic::state(&app.session);
                let f = |k: &str| v.get(k).and_then(Value::as_f64).unwrap_or(0.0) as f32;
                let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                Dialog::Phonetic {
                    base: s("base"),
                    ruby: s("ruby"),
                    align: s("align"),
                    size: f("size"),
                    offset: f("offset"),
                    existing: v.get("existing").and_then(Value::as_bool).unwrap_or(false),
                }
            }
            "newStyle" => Dialog::NewStyle { name: "Style1".into(), based_on: "Normal".into(), back: false },
            "manageStyles" => Dialog::ManageStyles { alphabetical: false, selected: s("style") },
            "newTableStyle" => {
                let based_on = current_table_style(app).unwrap_or_else(|| "TableGrid".into());
                let styles = &app.session.doc.styles;
                let name = (1..1000).map(|i| format!("Table Style {i}")).find(|n| styles.find(n).is_none()).unwrap_or_default();
                let regions = table_regions(app, &based_on);
                Dialog::TableStyle { id: None, name, based_on, region: 0, basis: regions.clone(), regions }
            }
            "modifyTableStyle" => {
                let id = current_table_style(app)?;
                let st = app.session.doc.styles.get(&id)?;
                let (name, based_on) = (st.name.clone(), st.based_on.clone().unwrap_or_default());
                let regions = table_regions(app, &id);
                Dialog::TableStyle { id: Some(id), name, based_on, region: 0, basis: regions.clone(), regions }
            }
            "commands" => Dialog::Commands { query: String::new(), swallow: true },
            "icons" => Dialog::Icons { query: String::new(), category: String::new(), selected: String::new() },
            "tableProperties" => {
                let form = Box::new(TableForm::read(app)?);
                Dialog::TableProperties { basis: form.clone(), form }
            }
            "enclose" => {
                // The selection's text and its enclosure, if it has one.
                let t = wordcraft_engine::cmd::format::enclose_target(&app.session).ok();
                let cur = t.as_ref().and_then(|t| t.current.clone());
                Dialog::Enclose {
                    style: cur.as_ref().map_or("shrink", |e| e.style.name()).into(),
                    shape: cur.as_ref().map_or("circle", |e| e.shape.name()).into(),
                    text: t.map(|t| t.text).unwrap_or_default(),
                }
            }
            "pasteSpecial" => Dialog::paste_special(app, &json!({})),
            "renameDocument" => Dialog::RenameDocument { name: app.title_stem(), message: String::new(), focus: true },
            "about" => Dialog::About { tab: 0 },
            "contributors" => Dialog::About { tab: 1 },
            "models" => Dialog::About { tab: 2 },
            // The current list to edit, or a new one to type.
            "recipientList" | "newRecipientList" => {
                let m = &app.session.merge;
                if name == "recipientList" && !m.headers.is_empty() {
                    // Every record (not only the filtered ones), with the filter to edit.
                    Dialog::RecipientList {
                        fields: m.headers.clone(),
                        rows: m.source.clone(),
                        message: String::new(),
                        filter: RecipientFilter::from_session(m),
                    }
                } else {
                    let fields: Vec<String> = NEW_LIST_FIELDS.iter().map(|f| f.to_string()).collect();
                    let rows = vec![vec![String::new(); fields.len()]];
                    Dialog::RecipientList { fields, rows, message: String::new(), filter: RecipientFilter::default() }
                }
            }
            "insertMergeField" => {
                let fields = app.session.merge.headers.clone();
                Dialog::InsertMergeField { field: fields.first().cloned().unwrap_or_default(), fields }
            }
            "findRecipient" => Dialog::FindRecipient { text: String::new(), message: String::new() },
            "fieldCode" => Dialog::FieldCode { code: String::new(), edit: false, message: String::new() },
            "editFieldCode" => {
                let field = wordcraft_engine::cmd::fields::selected(&app.session).into_iter().next()?;
                Dialog::FieldCode { code: field.instr, edit: true, message: String::new() }
            }
            "encryptPassword" => Dialog::EncryptPassword { password: Password::default(), confirm: Password::default(), message: String::new() },
            "ruleIf" | "ruleSkipIf" => {
                let fields = app.session.merge.headers.clone();
                Dialog::MergeRule {
                    rule: if name == "ruleSkipIf" { "SKIPIF" } else { "IF" }.into(),
                    field: fields.first().cloned().unwrap_or_default(),
                    fields,
                    value: String::new(),
                    then: String::new(),
                    els: String::new(),
                }
            }
            "matchFields" | "checkErrors" => {
                let id = if name == "matchFields" { "mailings.matchFields" } else { "mailings.checkErrors" };
                return Self::report(id, &app.session.run(id, &json!({})).unwrap_or_default());
            }
            "defineList" => Dialog::DefineList { form: Box::new(crate::dialogs_lists::ListForm::read(app)) },
            "trackChangesOptions" => Dialog::TrackOptions { form: Box::new(crate::dialogs_lists::TrackForm::read(app)) },
            "customizeKeyboard" => Dialog::CustomizeKeyboard { form: Box::new(crate::dialogs_keyboard::KeyboardForm::read(app)) },
            "customizeRibbon" => {
                Dialog::CustomizeRibbon { form: Box::new(crate::dialogs_ribbon::RibbonForm::read(app, crate::dialogs_ribbon::Page::Ribbon)) }
            }
            "customizeQuickAccess" => {
                Dialog::CustomizeRibbon { form: Box::new(crate::dialogs_ribbon::RibbonForm::read(app, crate::dialogs_ribbon::Page::QuickAccess)) }
            }
            "noteOptions" => Dialog::NoteOptions { form: Box::new(crate::dialogs_notes::NoteForm::read(app)) },
            "templates" => Dialog::Templates { form: Box::new(crate::dialogs_templates::TemplatesForm::read(app)) },
            "organizer" => Dialog::Organizer { form: Box::new(crate::dialogs_templates::OrganizerForm::read(app, None)) },
            "controlProperties" => match crate::dialogs_controls::ControlForm::read(app) {
                Some(form) => Dialog::Control { form: Box::new(form) },
                None => {
                    app.status(tl!("Put the caret in a content control first."));
                    return None;
                }
            },
            _ => return None,
        })
    }

    /// Paste Special for a clipboard payload (`text`, `html`, `rtf`; empty = WordCraft's own copy).
    pub fn paste_special(app: &WordApp, payload: &Value) -> Dialog {
        let formats: Vec<String> = wordcraft_engine::cmd::paste::available(&app.session, payload).into_iter().map(str::to_string).collect();
        let mut kept = json!({});
        for k in ["text", "html", "rtf", "picture"] {
            if let Some(t) = payload.get(k).and_then(Value::as_str) {
                kept[k] = json!(t);
            }
        }
        let note = if formats.is_empty() { crate::system_clipboard::nothing_to_paste(app) } else { String::new() };
        Dialog::PasteSpecial { formats, choice: 0, payload: kept, note }
    }

    pub fn modify_style(app: &WordApp, id: &str) -> Option<Dialog> {
        let st = app.session.doc.styles.get(id)?;
        let rc = app.session.doc.styles.resolve_char(
            if st.kind == wordcraft_doc::StyleKind::Paragraph { Some(id) } else { None },
            &wordcraft_doc::CharProps {
                style: if st.kind == wordcraft_doc::StyleKind::Character { Some(id.into()) } else { None },
                ..Default::default()
            },
        );
        let rp = app.session.doc.styles.resolve_para(&wordcraft_doc::ParaProps { style: Some(id.into()), ..Default::default() });
        Some(Dialog::ModifyStyle {
            back: false,
            resume: None,
            id: id.into(),
            name: st.name.clone(),
            font: rc.font,
            size: rc.size,
            bold: rc.bold,
            italic: rc.italic,
            color: match rc.color {
                wordcraft_doc::TextColor::Rgb(c) => c.hex(),
                wordcraft_doc::TextColor::Auto => "auto".into(),
            },
            before: rp.space_before,
            after: rp.space_after,
        })
    }
}

/// Grid picker for Insert › Table (hover to size, click to insert).
pub fn table_grid_picker(ui: &mut Ui, app: &mut WordApp) {
    let t = Tokens::get(ui.ctx());
    let id = egui::Id::new("table_picker_hover");
    let press_id = egui::Id::new("table_picker_press");
    let hover: (usize, usize) = ui.data(|d| d.get_temp(id)).unwrap_or((0, 0));
    let pressing: bool = ui.data(|d| d.get_temp(press_id)).unwrap_or(false);
    ui.label(if hover.0 > 0 {
        crate::i18n::fmt(tl!("{cols}×{rows} Table"), &[("cols", &hover.1.to_string()), ("rows", &hover.0.to_string())])
    } else {
        tl!("Insert Table").to_string()
    });
    let mut new_hover = (0, 0);
    let mut clicked = None;
    let mut cells: Vec<((usize, usize), egui::Rect)> = Vec::new();
    egui::Grid::new("tgp").spacing(vec2(2.0, 2.0)).show(ui, |ui| {
        for r in 1..=8 {
            for c in 1..=10 {
                let (rect, resp) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::click_and_drag());
                cells.push(((r, c), rect));
                let on = r <= hover.0 && c <= hover.1;
                ui.painter().rect(
                    rect,
                    1.0,
                    if on { t.checked } else { t.input },
                    egui::Stroke::new(1.0, if on { t.accent } else { t.border_strong }),
                    egui::StrokeKind::Inside,
                );
                if resp.hovered() {
                    new_hover = (r, c);
                }
                if resp.clicked() {
                    clicked = Some((r, c));
                }
            }
            ui.end_row();
        }
    });
    // Fingers send no hover, so a finger dragged across the grid is tracked by the pointer
    // position while the button is held: the cell under it is the size, and lifting inserts it.
    let (pointer, down, pressed, released) =
        ui.input(|i| (i.pointer.interact_pos(), i.pointer.primary_down(), i.pointer.primary_pressed(), i.pointer.primary_released()));
    let under = |p: egui::Pos2| cells.iter().find(|(_, r)| r.contains(p)).map(|(rc, _)| *rc);
    let mut now_pressing = pressing;
    if pressed {
        now_pressing = pointer.is_some_and(|p| under(p).is_some());
    }
    if now_pressing && down
        && let Some(cell) = pointer.and_then(under)
    {
        new_hover = cell;
    } else if now_pressing && down {
        new_hover = hover;
    }
    if now_pressing && released {
        now_pressing = false;
        if clicked.is_none() {
            clicked = pointer.and_then(under).or(if hover.0 > 0 { Some(hover) } else { None });
        }
    }
    ui.data_mut(|d| d.insert_temp(press_id, now_pressing));
    ui.data_mut(|d| d.insert_temp(id, if down || released { new_hover } else { hover }));
    if let Some((r, c)) = clicked {
        let _ = app.run("insert.table", json!({"rows": r, "cols": c}));
        ui.close();
    }
}

/// The open dialog, as (window id, frames it has been centred for).
fn dialog_top_key() -> egui::Id {
    egui::Id::new("dialog_top")
}

/// Frames a dialog stays centred: the first one measures it, the second centres that size.
const DIALOG_CENTRED_FRAMES: u8 = 2;

/// Places a dialog: centred for its first frames, then left where it is with its top-centre point
/// as the pivot, so a taller or shorter tab (Paragraph, Font, Borders and Shading…) only moves
/// the bottom edge and the tab strip stays under the mouse (#507). It is no longer anchored after
/// that, so dragging the title bar moves it (#732). egui keeps it on screen if it grows past the
/// bottom.
fn place_dialog<'a>(ctx: &egui::Context, id: egui::Id, w: egui::Window<'a>) -> egui::Window<'a> {
    let w = w.pivot(egui::Align2::CENTER_TOP);
    match ctx.data(|d| d.get_temp::<(egui::Id, u8)>(dialog_top_key())) {
        Some((open, frames)) if open == id && frames >= DIALOG_CENTRED_FRAMES => w,
        _ => w.anchor(egui::Align2::CENTER_CENTER, vec2(0.0, -40.0)),
    }
}

/// After a frame of a dialog: counts the frames it has been centred for.
fn count_dialog_frame(ctx: &egui::Context, id: egui::Id) {
    let frames = match ctx.data(|d| d.get_temp::<(egui::Id, u8)>(dialog_top_key())) {
        Some((open, frames)) if open == id => frames,
        _ => 0,
    };
    if frames < DIALOG_CENTRED_FRAMES {
        ctx.data_mut(|d| d.insert_temp(dialog_top_key(), (id, frames + 1)));
    }
}

pub fn show(app: &mut WordApp, ctx: &egui::Context) {
    let Some(mut d) = app.dialog.take() else {
        ctx.data_mut(|d| d.remove::<(egui::Id, u8)>(dialog_top_key()));
        return;
    };
    let mut open = true;
    let mut close = false;
    let title = match &d {
        Dialog::RenameDocument { .. } => "Rename Document",
        Dialog::Font { .. } => "Font",
        Dialog::Paragraph { .. } => "Paragraph",
        Dialog::Tabs { .. } => "Tabs",
        Dialog::Borders { .. } => "Borders and Shading",
        Dialog::Find { .. } => "Find and Replace",
        Dialog::InsertTable { .. } => "Insert Table",
        Dialog::PageSetup { .. } => "Page Setup",
        Dialog::Link { .. } => "Insert Hyperlink",
        Dialog::Bookmark { .. } => "Bookmark",
        Dialog::WordCount { .. } => "Word Count",
        Dialog::Zoom { .. } => "Zoom",
        Dialog::Watermark { .. } => "Custom Watermark",
        Dialog::Phonetic { .. } => "Phonetic Guide",
        Dialog::NewStyle { .. } => "Create New Style",
        Dialog::ModifyStyle { .. } => "Modify Style",
        Dialog::ManageStyles { .. } => "Manage Styles",
        Dialog::TableStyle { id: None, .. } => "New Table Style",
        Dialog::TableStyle { .. } => "Modify Table Style",
        Dialog::Commands { .. } => "Search Commands",
        Dialog::Insert(d) => d.title(),
        Dialog::Refs(d) => d.title(),
        Dialog::Icons { .. } => "Icons",
        Dialog::PasteSpecial { .. } => "Paste Special",
        Dialog::About { .. } => "About WordCraft",
        Dialog::SaveChanges { .. } => "WordCraft",
        Dialog::DiscardRecovery { .. } => "Discard recovery copy",
        Dialog::RecipientList { .. } => "Recipient List",
        Dialog::SelectTable { .. } => "Select Table",
        Dialog::InsertMergeField { .. } => "Insert Merge Field",
        Dialog::FindRecipient { .. } => "Find Recipient",
        Dialog::FieldCode { edit: false, .. } => "Field",
        Dialog::FieldCode { .. } => "Edit Field Code",
        Dialog::MergeRule { rule, .. } if rule == "SKIPIF" => "Skip Record If",
        Dialog::MergeRule { .. } => "If…Then…Else",
        Dialog::MatchFields { .. } => "Match Fields",
        Dialog::CheckErrors { .. } => "Check for Errors",
        Dialog::TableProperties { .. } => "Table Properties",
        Dialog::Password { .. } => "Password",
        Dialog::EncryptPassword { .. } => "Encrypt with Password",
        Dialog::DefineList { .. } => "Define New Multilevel List",
        Dialog::TrackOptions { .. } => "Track Changes Options",
        Dialog::Review(d) => d.title(),
        Dialog::CustomizeKeyboard { .. } => "Customize Keyboard",
        Dialog::CustomizeRibbon { .. } => "Customize Ribbon",
        Dialog::Control { .. } => "Content Control Properties",
        Dialog::Enclose { .. } => "Enclose Characters",
        Dialog::NoteOptions { .. } => "Footnote and Endnote",
        Dialog::Templates { .. } => "Templates and Add-ins",
        Dialog::Organizer { .. } => "Organizer",
        Dialog::Layout(d) => d.title(),
    };
    let id = egui::Id::new(("dialog", title));
    let shown = place_dialog(ctx, id, egui::Window::new(tl!(title)).id(id).collapsible(false).resizable(false)).open(&mut open).show(ctx, |ui| {
        ui.set_min_width(340.0);
        close = body(app, ui, &mut d);
    });
    if shown.is_some() {
        count_dialog_frame(ctx, id);
    }
    if open && !close {
        app.dialog = Some(d);
    } else {
        app.canvas.want_focus = true;
    }
}

/// Labels of Page Setup's paper size (#1433), in one place in case Word's wording differs.
pub(crate) const PAPER_LABELS: PaperLabels =
    PaperLabels { group: "Paper", size: "Paper size:", custom: "Custom size", width: "Width:", height: "Height:", more: "More Paper Sizes…" };

pub(crate) struct PaperLabels {
    pub group: &'static str,
    pub size: &'static str,
    pub custom: &'static str,
    pub width: &'static str,
    pub height: &'static str,
    /// The Size menu's last item, opening Page Setup.
    pub more: &'static str,
}

/// The named paper size `paper` (in the measurement unit, either orientation) is, or None for a
/// custom size.
pub(crate) fn paper_entry(paper: (f32, f32)) -> Option<&'static str> {
    let ppu = crate::options::unit().pt_per_unit();
    wordcraft_geom::paper_name(paper.0 * ppu, paper.1 * ppu)
}

/// The commands Page Setup's OK runs for the paper, orientation and margins (#1433), all in the
/// measurement unit as the dialog shows them. Refuses when the margins don't fit the paper.
/// Otherwise it orders them so that none is refused on the way: the size first
/// when the section's margins fit it, else the margins first, else margins of 0 in between.
/// `layout.size` keeps the section's orientation; `layout.orientation` then turns the page and
/// its margins to what the dialog shows. A size equal to the opened one (either way round,
/// within 0.01 pt) doesn't run `layout.size`.
fn page_layout_commands(
    app: &WordApp,
    (paper, was): ((f32, f32), (f32, f32)),
    landscape: bool,
    (top, bottom, left, right): (f32, f32, f32, f32),
) -> Result<Vec<(&'static str, Value)>, &'static str> {
    use wordcraft_engine::cmd::page::{MARGINS_DONT_FIT, margins_fit, sect};
    let ppu = crate::options::unit().pt_per_unit();
    let (paper, was) = ((paper.0 * ppu, paper.1 * ppu), (was.0 * ppu, was.1 * ppu));
    let m = (top * ppu, bottom * ppu, left * ppu, right * ppu);
    let cur = sect(&app.session);
    if !margins_fit(paper, m, cur.gutter) {
        return Err(MARGINS_DONT_FIT);
    }
    let turned = |on: bool, (a, b): (f32, f32)| if on { (a.max(b), a.min(b)) } else { (a.min(b), a.max(b)) };
    let ((w0, h0), (w1, h1)) = (turned(false, paper), turned(false, was));
    let new_size = (w0 - w1).abs() >= 0.01 || (h0 - h1).abs() >= 0.01;
    let size = ("layout.size", json!({"width": paper.0, "height": paper.1}));
    let orientation = ("layout.orientation", json!({"value": if landscape { "landscape" } else { "portrait" }}));
    let margins = ("layout.margins", json!({"top": m.0, "bottom": m.1, "left": m.2, "right": m.3}));
    let old_margins = (cur.margin_top, cur.margin_bottom, cur.margin_left, cur.margin_right);
    Ok(if !new_size || margins_fit(turned(cur.landscape, paper), old_margins, cur.gutter) {
        let mut v = if new_size { vec![size] } else { Vec::new() };
        v.extend([orientation, margins]);
        v
    } else if margins_fit(turned(landscape, (cur.page_w, cur.page_h)), m, cur.gutter) {
        vec![orientation, margins, size]
    } else {
        vec![orientation, ("layout.margins", json!({"top": 0.0, "bottom": 0.0, "left": 0.0, "right": 0.0})), size, margins]
    })
}

/// Run a dialog's commands as one undo step. If one fails, put the document and the undo stack
/// back as they were (nothing applied) and return its error.
fn run_as_one_step(app: &mut WordApp, commands: Vec<(&str, Value)>) -> Result<(), String> {
    let snapshot = app.session.edit_snapshot();
    for (i, (id, params)) in commands.into_iter().enumerate() {
        if i > 0 {
            app.session.join_next_undo();
        }
        if let Err(e) = app.run(id, params) {
            app.session.restore(snapshot);
            return Err(e);
        }
    }
    Ok(())
}

/// Page Setup's Paper group (#1433): a Paper size list (the named sizes and Custom size) and
/// the width and height. The fields follow the orientation (the wide side is the width in
/// landscape), and a typed size that is wider than tall turns the orientation to landscape.
fn page_paper_group(ui: &mut Ui, paper: &mut (f32, f32), landscape: &mut bool, custom: &mut bool) {
    if (*landscape && paper.0 < paper.1) || (!*landscape && paper.0 > paper.1) {
        *paper = (paper.1, paper.0);
    }
    let unit = crate::options::unit();
    let ppu = unit.pt_per_unit();
    let l = &PAPER_LABELS;
    ui.label(egui::RichText::new(tl!(l.group)).font(semibold(12.5)));
    ui.horizontal(|ui| {
        let label = ui.label(tl!(l.size));
        let entry = paper_entry(*paper).filter(|_| !*custom);
        let shown = tl!(entry.unwrap_or(l.custom));
        let combo = egui::ComboBox::from_id_salt("ps_paper_size").width(180.0).height(400.0).selected_text(shown).show_ui(ui, |ui| {
            for (n, w, h) in wordcraft_geom::PAPER_SIZES {
                if ui.selectable_label(entry == Some(*n), tl!(n)).clicked() {
                    let (w, h) = (w / ppu, h / ppu);
                    *paper = if *landscape { (w.max(h), w.min(h)) } else { (w.min(h), w.max(h)) };
                    *custom = false;
                }
            }
            // Custom size keeps the numbers to edit, and stays chosen while they match a name.
            if ui.selectable_label(entry.is_none(), tl!(l.custom)).clicked() {
                *custom = true;
            }
        });
        combo.response.labelled_by(label.id);
    });
    egui::Grid::new("ps_paper").num_columns(4).show(ui, |ui| {
        use wordcraft_engine::cmd::page::{MAX_PAGE_SIDE, MIN_PAGE_SIDE};
        let range = MIN_PAGE_SIDE / ppu..=MAX_PAGE_SIDE / ppu;
        ui.label(tl!(l.width));
        let w = ui.add(egui::DragValue::new(&mut paper.0).speed(3.6 / ppu).range(range.clone()).measure(unit).max_decimals(2));
        ui.label(tl!(l.height));
        let h = ui.add(egui::DragValue::new(&mut paper.1).speed(3.6 / ppu).range(range).measure(unit).max_decimals(2));
        if (w.changed() || h.changed()) && paper.0 != paper.1 {
            *landscape = paper.0 > paper.1;
        }
        ui.end_row();
    });
}

pub(crate) fn buttons(ui: &mut Ui, ok: &str) -> (bool, bool) {
    let mut r = (false, false);
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Enter and Escape belong to the Colors dialog while it is open above this one.
            let keys = !crate::colors::is_open(ui.ctx());
            if ui.button(tl!("Cancel")).clicked() || (keys && ui.input(|i| i.key_pressed(egui::Key::Escape))) {
                r.1 = true;
            }
            let okb = ui.add(crate::widgets::primary_button(ui, ok));
            if okb.clicked() || (keys && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                r.0 = true;
            }
        });
    });
    r
}

/// The character properties the Font dialog's fields stand for.
fn font_props(d: &Dialog) -> serde_json::Map<String, Value> {
    let Dialog::Font { font, font_ea, size, bold, italic, underline, strike, sup, sub, small_caps, caps, hidden, color, spacing, .. } = d else {
        return serde_json::Map::new();
    };
    let mut props = serde_json::Map::new();
    props.insert("bold".into(), json!(*bold));
    props.insert("italic".into(), json!(*italic));
    props.insert("strike".into(), json!(*strike));
    props.insert("underline".into(), json!(if *underline { "single" } else { "none" }));
    let vert = if *sup {
        "superscript"
    } else if *sub {
        "subscript"
    } else {
        "baseline"
    };
    props.insert("vertAlign".into(), json!(vert));
    props.insert("smallCaps".into(), json!(*small_caps));
    props.insert("caps".into(), json!(*caps));
    props.insert("hidden".into(), json!(*hidden));
    props.insert("spacing".into(), json!(*spacing));
    if !font.trim().is_empty() {
        props.insert("font".into(), json!(font.trim()));
    }
    if !font_ea.trim().is_empty() {
        props.insert("fontEa".into(), json!(font_ea.trim()));
    }
    if let Ok(s) = size.trim().parse::<f64>() {
        props.insert("size".into(), json!(s));
    }
    if let Some(c) = wordcraft_doc::Rgb::parse(color) {
        props.insert("color".into(), json!({"Rgb": [c.0, c.1, c.2]}));
    } else if color.eq_ignore_ascii_case("auto") {
        // Automatic, from the colour field's drop-down (#606).
        props.insert("color".into(), json!("Auto"));
    }
    props
}

/// Is `c` Chinese, Japanese or Korean text (ideographs, kana, Hangul, CJK punctuation, fullwidth
/// forms)?
fn east_asian_char(c: char) -> bool {
    matches!(u32::from(c), 0x1100..=0x11FF | 0x2E80..=0x9FFF | 0xA960..=0xA97F | 0xAC00..=0xD7FF | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFFEF | 0x20000..=0x3FFFF)
}

/// The Font dialog's OK: one formatting step that sets only the properties the user changed, so
/// formatting the dialog can't show or left alone stays as it is. Fonts are set by their stored
/// (English) names, the Latin font for Persian/Arabic text too (as `format.font`).
fn apply_font(app: &mut WordApp, d: &Dialog) {
    let Dialog::Font { start, .. } = d else { return };
    let mut changed: serde_json::Map<String, Value> = font_props(d).into_iter().filter(|(k, v)| start.get(k) != Some(v)).collect();
    if let Some(f) = changed.get("font").and_then(Value::as_str).map(str::to_string) {
        let f = app.previews.font_stored(&f);
        changed.insert("font".into(), json!(f));
        changed.insert("fontCs".into(), json!(f));
    }
    if let Some(f) = changed.get("fontEa").and_then(Value::as_str).map(str::to_string) {
        changed.insert("fontEa".into(), json!(app.previews.font_stored(&f)));
    }
    if !changed.is_empty() {
        let _ = app.run("format.set", json!({"props": changed}));
    }
}

/// Returns true to close.
fn body(app: &mut WordApp, ui: &mut Ui, d: &mut Dialog) -> bool {
    match d {
        Dialog::RenameDocument { name, message, focus } => {
            ui.label(tl!("Document name"));
            let r = ui.add(egui::TextEdit::singleline(name).desired_width(320.0).char_limit(200));
            if *focus {
                r.request_focus();
                *focus = false;
            }
            if !message.is_empty() {
                ui.colored_label(ui.visuals().error_fg_color, message.as_str());
            }
            let (ok, cancel) = buttons(ui, tl!("Rename"));
            if cancel {
                return true;
            }
            if ok && name.trim() == app.title_stem() {
                return true;
            }
            if ok {
                match app.run("file.rename", json!({"name": name})) {
                    Ok(_) => return true,
                    Err(e) => *message = e,
                }
            }
            false
        }

        Dialog::Font { font, font_ea, asian, size, bold, italic, underline, strike, sup, sub, small_caps, caps, hidden, color, spacing, .. } => {
            // Build the sample text format for the preview pane below.
            let preview_size = size.trim().parse::<f32>().unwrap_or(12.0).clamp(6.0, 72.0);
            let preview_color = wordcraft_doc::Rgb::parse(color).map(crate::theme::c32).unwrap_or_else(|| ui.visuals().text_color());
            let mut fmt = egui::TextFormat {
                font_id: egui::FontId::new(preview_size, egui::FontFamily::Proportional),
                color: preview_color,
                ..Default::default()
            };
            if *bold {
                fmt.font_id = crate::theme::semibold(preview_size);
            }
            if *italic {
                fmt.italics = true;
            }
            if *underline {
                fmt.underline = egui::Stroke::new(1.0, preview_color);
            }
            if *strike {
                fmt.strikethrough = egui::Stroke::new(1.0, preview_color);
            }
            egui::Grid::new("fontdlg").num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
                // Word's two font boxes when East Asian text is in use: the Latin text font and
                // the Asian text font, each with its own list (type to find a font).
                let prev = app.previews.font_preview_fn();
                if *asian {
                    ui.label(tl!("Asian text font:"));
                    let fams = app.previews.east_asian_families();
                    if let Some(f) = crate::widgets::font_combo(ui, "fontdlg_asian", 220.0, font_ea, &fams, Some(&*prev)) {
                        *font_ea = f;
                    }
                    ui.end_row();
                }
                ui.label(if *asian { tl!("Latin text font:") } else { tl!("Font:") });
                let fams = app.previews.families();
                if let Some(f) = crate::widgets::font_combo(ui, "fontdlg_font", 220.0, font, &fams, Some(&*prev)) {
                    *font = f;
                }
                ui.end_row();
                ui.label(tl!("Size:"));
                ui.text_edit_singleline(size);
                ui.end_row();
                ui.label(tl!("Font color:"));
                let theme = app.session.doc.settings.theme_colors.clone();
                color_field(ui, &theme, color, "Automatic", "auto");
                ui.end_row();
                ui.label(tl!("Spacing (pt):"));
                ui.add(egui::DragValue::new(spacing).speed(0.1).range(-20.0..=20.0));
                ui.end_row();
            });
            ui.separator();
            ui.label(egui::RichText::new(tl!("Effects")).font(semibold(12.5)));
            egui::Grid::new("fx").num_columns(2).show(ui, |ui| {
                ui.checkbox(bold, tl!("Bold"));
                ui.checkbox(italic, tl!("Italic"));
                ui.end_row();
                ui.checkbox(underline, tl!("Underline"));
                ui.checkbox(strike, tl!("Strikethrough"));
                ui.end_row();
                ui.checkbox(sup, tl!("Superscript"));
                ui.checkbox(sub, tl!("Subscript"));
                ui.end_row();
                ui.checkbox(small_caps, tl!("Small caps"));
                ui.checkbox(caps, tl!("All caps"));
                ui.end_row();
                ui.checkbox(hidden, tl!("Hidden"));
                ui.end_row();
            });
            // Live preview of the chosen font/size/colour/effects.
            ui.separator();
            ui.label(egui::RichText::new(tl!("Preview")).small().weak());
            let sample = if *caps { tl!("AaBbYyZz").to_uppercase() } else { tl!("AaBbYyZz").to_string() };
            let mut job = egui::text::LayoutJob::single_section(sample, fmt);
            job.wrap.max_width = ui.available_width().min(400.0);
            ui.label(job);
            let (ok, cancel) = buttons(ui, tl!("OK"));
            if ok {
                apply_font(app, d);
            }
            ok || cancel
        }
        Dialog::Paragraph {
            rtl,
            align,
            left,
            right,
            first,
            before,
            after,
            line,
            line_was,
            keep_next,
            keep_lines,
            page_break,
            widow,
            tab,
            asian,
            asian_was,
            opts,
            opts_was,
        } => {
            ui.horizontal(|ui| {
                for (k, name) in ["Indents and Spacing", "Line and Page Breaks", "Asian Typography"].into_iter().enumerate() {
                    if ui.selectable_label(*tab as usize == k, tl!(name)).clicked() {
                        *tab = k as u8;
                    }
                }
            });
            ui.separator();
            match *tab {
                0 => {
                    ui.label(egui::RichText::new(tl!("General")).font(semibold(12.5)));
                    ui.horizontal(|ui| {
                        ui.label(tl!("Direction:"));
                        ui.radio_value(rtl, true, tl!("Right-to-left"));
                        ui.radio_value(rtl, false, tl!("Left-to-right"));
                    });
                    const ALIGNS: [(&str, &str); 4] = [("left", "Left"), ("center", "Center"), ("right", "Right"), ("justify", "Justify")];
                    ui.horizontal(|ui| {
                        ui.label(tl!("Alignment:"));
                        let shown = ALIGNS.iter().find(|(v, _)| v == align).map_or(align.as_str(), |(_, l)| tl!(l));
                        egui::ComboBox::from_id_salt("para_align").selected_text(shown).show_ui(ui, |ui| {
                            for (v, l) in ALIGNS {
                                ui.selectable_value(align, v.to_string(), tl!(l));
                            }
                        });
                    });
                    ui.label(egui::RichText::new(tl!("Indentation")).font(semibold(12.5)));
                    // Mirrored indents are the inside and outside ones (they swap on even pages).
                    let (l_label, r_label) = if opts[0] { ("Inside:", "Outside:") } else { ("Left:", "Right:") };
                    egui::Grid::new("ind").num_columns(4).show(ui, |ui| {
                        ui.label(tl!(l_label));
                        ui.add(
                            egui::DragValue::new(left)
                                .speed(3.6 / crate::options::unit().pt_per_unit())
                                .measure(crate::options::unit())
                                .max_decimals(2),
                        );
                        ui.label(tl!(r_label));
                        ui.add(
                            egui::DragValue::new(right)
                                .speed(3.6 / crate::options::unit().pt_per_unit())
                                .measure(crate::options::unit())
                                .max_decimals(2),
                        );
                        ui.end_row();
                        ui.label(tl!("First line:"));
                        ui.add(
                            egui::DragValue::new(first)
                                .speed(3.6 / crate::options::unit().pt_per_unit())
                                .measure(crate::options::unit())
                                .max_decimals(2),
                        );
                        ui.end_row();
                    });
                    ui.checkbox(&mut opts[0], tl!("Mirror indents"));
                    ui.label(egui::RichText::new(tl!("Spacing")).font(semibold(12.5)));
                    egui::Grid::new("sp").num_columns(4).show(ui, |ui| {
                        ui.label(tl!("Before:"));
                        ui.add(egui::DragValue::new(before).speed(1.0).range(0.0..=1584.0).suffix(format!(" {}", tl!("pt"))));
                        ui.label(tl!("Line spacing:"));
                        ui.add(egui::DragValue::new(line).speed(0.05).range(0.5..=5.0));
                        ui.end_row();
                        ui.label(tl!("After:"));
                        ui.add(egui::DragValue::new(after).speed(1.0).range(0.0..=1584.0).suffix(format!(" {}", tl!("pt"))));
                        ui.end_row();
                    });
                    ui.checkbox(&mut opts[1], tl!("Don't add space between paragraphs of the same style"));
                }
                1 => {
                    ui.label(egui::RichText::new(tl!("Line and Page Breaks")).font(semibold(12.5)));
                    ui.checkbox(widow, tl!("Widow/Orphan control"));
                    ui.checkbox(keep_next, tl!("Keep with next"));
                    ui.checkbox(keep_lines, tl!("Keep lines together"));
                    ui.checkbox(page_break, tl!("Page break before"));
                    ui.label(egui::RichText::new(tl!("Formatting exceptions")).font(semibold(12.5)));
                    ui.checkbox(&mut opts[2], tl!("Suppress line numbers"));
                    ui.checkbox(&mut opts[3], tl!("Don't hyphenate"));
                }
                _ => {
                    let [kinsoku, word_wrap, overflow, top_line, de, dn] = asian;
                    ui.label(egui::RichText::new(tl!("Line breaking")).font(semibold(12.5)));
                    ui.checkbox(kinsoku, tl!("Apply Asian line-breaking rules (kinsoku)"));
                    // The model's flag is "wrap whole words"; the box offers the opposite.
                    let mut mid_word = !*word_wrap;
                    if ui.checkbox(&mut mid_word, tl!("Allow Latin text to wrap in the middle of a word")).changed() {
                        *word_wrap = !mid_word;
                    }
                    ui.checkbox(overflow, tl!("Allow hanging punctuation"));
                    ui.label(egui::RichText::new(tl!("Character Spacing")).font(semibold(12.5)));
                    ui.checkbox(top_line, tl!("Compress punctuation at the start of a line"));
                    ui.checkbox(de, tl!("Add space between Asian and Latin text"));
                    ui.checkbox(dn, tl!("Add space between Asian text and numbers"));
                }
            }
            ui.add_space(6.0);
            let tabs = ui.button(tl!("Tabs…")).clicked();
            let (ok, cancel) = buttons(ui, tl!("OK"));
            if ok || tabs {
                apply_paragraph(
                    app,
                    *rtl,
                    align,
                    *left,
                    *right,
                    *first,
                    *before,
                    *after,
                    (*line, *line_was),
                    [*keep_next, *keep_lines, *page_break, *widow],
                    (*opts, *opts_was),
                );
                apply_asian(app, *asian, *asian_was);
            }
            // Tabs… keeps what was set here and moves on to the Tabs dialog.
            if tabs {
                app.dialog = Dialog::open("tabs", app);
            }
            ok || cancel || tabs
        }
        Dialog::Tabs { form } => crate::dialogs_para::tabs(app, ui, form),
        Dialog::Borders { form } => crate::dialogs_para::borders(app, ui, form),
        Dialog::Find { query, replace, match_case, whole_word, regex, extra, tab, goto_what, goto_target, message, focus } => {
            ui.horizontal(|ui| {
                for (t, label) in [(FindTab::Find, "Find"), (FindTab::Replace, "Replace"), (FindTab::GoTo, "Go To")] {
                    if ui.selectable_label(*tab == t, tl!(label)).clicked() && *tab != t {
                        *tab = t;
                        message.clear();
                    }
                }
            });
            ui.separator();
            if *tab == FindTab::GoTo {
                return goto_body(app, ui, goto_what, goto_target, message);
            }
            let replace_mode = *tab == FindTab::Replace;
            ui.horizontal(|ui| {
                ui.label(tl!("Find what:"));
                let r = ui.add(egui::TextEdit::singleline(query).id(egui::Id::new("find_what")).desired_width(240.0));
                if std::mem::take(focus) {
                    r.request_focus();
                }
                if r.gained_focus() {
                    extra.to_replace = false;
                }
            });
            if replace_mode {
                ui.horizontal(|ui| {
                    ui.label(tl!("Replace with:"));
                    if ui.add(egui::TextEdit::singleline(replace).desired_width(228.0)).gained_focus() {
                        extra.to_replace = true;
                    }
                });
            } else {
                extra.to_replace = false;
            }
            // Word greys out Match case and the word options with wildcards (they always match case).
            let plain = !extra.wildcards;
            ui.horizontal(|ui| {
                ui.add_enabled(plain, egui::Checkbox::new(match_case, tl!("Match case")));
                ui.add_enabled(plain, egui::Checkbox::new(whole_word, tl!("Whole words")));
                ui.checkbox(&mut extra.wildcards, tl!("Use wildcards"));
            });
            let arrow = if extra.more { "▾" } else { "▸" };
            if ui.selectable_label(extra.more, format!("{arrow} {}", tl!("Search Options"))).clicked() {
                extra.more = !extra.more;
            }
            if extra.more {
                ui.horizontal(|ui| {
                    ui.add_enabled(plain, egui::Checkbox::new(&mut extra.prefix, tl!("Match prefix")));
                    ui.add_enabled(plain, egui::Checkbox::new(&mut extra.suffix, tl!("Match suffix")));
                });
                ui.horizontal(|ui| {
                    ui.add_enabled(plain, egui::Checkbox::new(&mut extra.ignore_punct, tl!("Ignore punctuation characters")));
                    ui.add_enabled(plain, egui::Checkbox::new(&mut extra.ignore_space, tl!("Ignore white-space characters")));
                });
                ui.horizontal(|ui| {
                    ui.checkbox(regex, tl!("Regular expressions"));
                    if let Some(code) = special_menu(ui, extra) {
                        if extra.to_replace { replace.push_str(code) } else { query.push_str(code) }
                    }
                });
            }
            let opts = json!({
                "text": query,
                "with": replace,
                "matchCase": *match_case,
                "wholeWord": *whole_word,
                "regex": *regex,
                "wildcards": extra.wildcards,
                "matchPrefix": extra.prefix,
                "matchSuffix": extra.suffix,
                "ignorePunct": extra.ignore_punct,
                "ignoreSpace": extra.ignore_space,
            });
            if !replace_mode {
                ui.horizontal(|ui| {
                    let lit = app.session.find.highlight;
                    if ui.selectable_label(lit, tl!("Reading Highlight")).clicked() {
                        let mut v = opts.clone();
                        v["highlight"] = json!(!lit);
                        match app.run("edit.advancedFind", v) {
                            Ok(r) if !lit => *message = crate::i18n::fmt(tl!("{count} items highlighted"), &[("count", &r["count"].to_string())]),
                            Ok(_) => message.clear(),
                            Err(e) => *message = e,
                        }
                    }
                });
            }
            if !message.is_empty() {
                ui.label(egui::RichText::new(message.as_str()).weak());
            }
            let mut close = false;
            ui.horizontal(|ui| {
                if replace_mode {
                    if ui.button(tl!("Replace All")).clicked() {
                        match app.run("edit.replaceAll", opts.clone()) {
                            Ok(r) => *message = format!("All done. We made {} replacements.", r["replaced"]),
                            Err(e) => *message = e,
                        }
                    }
                    if ui.button(tl!("Replace")).clicked() {
                        let _ = app.run("edit.replace", opts.clone());
                    }
                }
                if ui.button(tl!("Find Next")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    // One step per click: `edit.find` would select a match itself, and the step after it skip one.
                    match app.run("edit.findNext", opts.clone()) {
                        Ok(r) => *message = format!("Match {} of {}", r["index"].as_u64().unwrap_or(0) + 1, r["count"]),
                        Err(e) => *message = e,
                    }
                }
                if ui.button(tl!("Close")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    close = true;
                }
            });
            close
        }
        Dialog::Enclose { style, text, shape } => {
            ui.label(egui::RichText::new(tl!("Style")).font(semibold(12.5)));
            ui.horizontal(|ui| {
                for (v, l) in [("none", "None"), ("shrink", "Shrink text"), ("enlarge", "Enlarge symbol")] {
                    ui.radio_value(style, v.to_string(), tl!(l));
                }
            });
            ui.add_space(6.0);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.label(tl!("Text:"));
                    ui.add(egui::TextEdit::singleline(text).desired_width(110.0).char_limit(2));
                    ui.add_space(6.0);
                    enclose_preview(ui, style, text, shape);
                });
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.label(tl!("Enclosure"));
                    for (v, l) in [("circle", "Circle"), ("square", "Square"), ("triangle", "Triangle"), ("diamond", "Diamond")] {
                        ui.add_enabled_ui(style != "none", |ui| ui.selectable_value(shape, v.to_string(), tl!(l)));
                    }
                });
            });
            let (ok, cancel) = buttons(ui, tl!("OK"));
            if ok {
                let params = if style == "none" { json!({"shape": null}) } else { json!({"shape": shape, "style": style, "text": text}) };
                let _ = app.run("format.enclose", params);
            }
            ok || cancel
        }
        Dialog::TableProperties { form, basis } => {
            table_properties(ui, form);
            let (ok, cancel) = buttons(ui, tl!("OK"));
            if ok {
                let changes = form.changes(basis);
                if changes.as_object().is_some_and(|m| !m.is_empty()) {
                    let _ = app.run("table.properties", changes);
                }
            }
            ok || cancel
        }
        Dialog::InsertTable { rows, cols } => {
            egui::Grid::new("it").num_columns(2).show(ui, |ui| {
                ui.label(tl!("Number of columns:"));
                ui.add(egui::DragValue::new(cols).range(1..=63));
                ui.end_row();
                ui.label(tl!("Number of rows:"));
                ui.add(egui::DragValue::new(rows).range(1..=1000));
                ui.end_row();
            });
            let (ok, cancel) = buttons(ui, tl!("OK"));
            if ok {
                let _ = app.run("insert.table", json!({"rows": *rows, "cols": *cols}));
            }
            ok || cancel
        }
        Dialog::PageSetup { top, bottom, left, right, landscape, rtl, grid, grid_was, paper, paper_was, paper_custom, message } => {
            ui.label(egui::RichText::new(tl!("Margins")).font(semibold(12.5)));
            egui::Grid::new("ps").num_columns(4).show(ui, |ui| {
                ui.label(tl!("Top:"));
                ui.add(egui::DragValue::new(top).speed(3.6 / crate::options::unit().pt_per_unit()).measure(crate::options::unit()).max_decimals(2));
                ui.label(tl!("Bottom:"));
                ui.add(
                    egui::DragValue::new(bottom).speed(3.6 / crate::options::unit().pt_per_unit()).measure(crate::options::unit()).max_decimals(2),
                );
                ui.end_row();
                ui.label(tl!("Left:"));
                ui.add(egui::DragValue::new(left).speed(3.6 / crate::options::unit().pt_per_unit()).measure(crate::options::unit()).max_decimals(2));
                ui.label(tl!("Right:"));
                ui.add(egui::DragValue::new(right).speed(3.6 / crate::options::unit().pt_per_unit()).measure(crate::options::unit()).max_decimals(2));
                ui.end_row();
            });
            ui.label(egui::RichText::new(tl!("Orientation")).font(semibold(12.5)));
            ui.horizontal(|ui| {
                ui.radio_value(landscape, false, tl!("Portrait"));
                ui.radio_value(landscape, true, tl!("Landscape"));
            });
            page_paper_group(ui, paper, landscape, paper_custom);
            ui.label(egui::RichText::new(tl!("Document Grid")).font(semibold(12.5)));
            ui.horizontal_wrapped(|ui| {
                ui.radio_value(&mut grid.0, 0, tl!("No grid"));
                ui.radio_value(&mut grid.0, 1, tl!("Lines only"));
                ui.radio_value(&mut grid.0, 2, tl!("Lines and characters"));
                ui.radio_value(&mut grid.0, 3, tl!("Text snaps to characters"));
            });
            egui::Grid::new("ps_grid").num_columns(4).show(ui, |ui| {
                ui.add_enabled(grid.0 >= 2, egui::Label::new(tl!("Characters per line:")));
                ui.add_enabled(grid.0 >= 2, egui::DragValue::new(&mut grid.2).range(1..=1000));
                ui.add_enabled(grid.0 >= 1, egui::Label::new(tl!("Lines per page:")));
                ui.add_enabled(grid.0 >= 1, egui::DragValue::new(&mut grid.1).range(1..=1000));
                ui.end_row();
            });
            ui.horizontal(|ui| {
                ui.label(tl!("Section direction:"));
                ui.radio_value(rtl, false, tl!("Left-to-right"));
                ui.radio_value(rtl, true, tl!("Right-to-left"));
            });
            if !message.is_empty() {
                ui.label(egui::RichText::new(message.as_str()).small().color(ui.visuals().error_fg_color));
            }
            let (mut ok, cancel) = buttons(ui, tl!("OK"));
            if ok {
                // Everything OK changes is one undo step; a refused command applies nothing.
                let result = page_layout_commands(app, (*paper, *paper_was), *landscape, (*top, *bottom, *left, *right))
                    .map_err(|e| tl!(e).to_string())
                    .and_then(|mut commands| {
                        if grid != grid_was {
                            let kind = ["none", "lines", "linesAndChars", "snapToChars"].get(grid.0 as usize).copied().unwrap_or("none");
                            let mut v = json!({"type": kind});
                            if grid.1 != grid_was.1 {
                                v["linesPerPage"] = json!(grid.1);
                            }
                            if grid.2 != grid_was.2 {
                                v["charsPerLine"] = json!(grid.2);
                            }
                            commands.push(("layout.documentGrid", v));
                        }
                        if *rtl != wordcraft_engine::cmd::page::sect(&app.session).rtl {
                            commands.push(("layout.sectionRtl", json!({"value": *rtl})));
                        }
                        run_as_one_step(app, commands)
                    });
                if let Err(e) = result {
                    *message = e;
                    ok = false;
                }
            }
            ok || cancel
        }
        Dialog::Link { url, text } => {
            egui::Grid::new("lnk").num_columns(2).show(ui, |ui| {
                ui.label(tl!("Text to display:"));
                ui.text_edit_singleline(text);
                ui.end_row();
                ui.label(tl!("Address:"));
                ui.text_edit_singleline(url);
                ui.end_row();
            });
            let (ok, cancel) = buttons(ui, tl!("OK"));
            if ok {
                let _ = app.run("insert.link", json!({"url": url, "text": text}));
            }
            ok || cancel
        }
        Dialog::Bookmark { name } => {
            ui.label(tl!("Bookmark name:"));
            ui.text_edit_singleline(name);
            let marks = app.session.doc.bookmarks();
            for (n, _) in &marks {
                if ui.selectable_label(false, n).clicked() {
                    let _ = app.run("edit.goto", json!({"bookmark": n}));
                }
            }
            let (ok, cancel) = buttons(ui, tl!("Add"));
            if ok {
                let _ = app.run("insert.bookmark", json!({"name": name}));
            }
            ok || cancel
        }
        Dialog::WordCount { stats } => {
            egui::Grid::new("wc").num_columns(2).spacing(vec2(30.0, 6.0)).show(ui, |ui| {
                for (l, k) in [
                    ("Pages", "pages"),
                    ("Words", "words"),
                    ("Characters (no spaces)", "characters"),
                    ("Characters (with spaces)", "charactersWithSpaces"),
                    ("Paragraphs", "paragraphs"),
                    ("Lines", "lines"),
                ] {
                    ui.label(tl!(l));
                    ui.label(stats.get(k).map(|v| v.to_string()).unwrap_or_default());
                    ui.end_row();
                }
            });
            let mut include = stats.get("includeTextBoxes").and_then(Value::as_bool).unwrap_or(true);
            if ui.checkbox(&mut include, tl!("Count words in text boxes and notes")).changed()
                && let Ok(v) = app.run("review.wordCount", json!({"includeTextBoxes": include}))
            {
                *stats = v;
            }
            let (ok, cancel) = buttons(ui, tl!("Close"));
            ok || cancel
        }
        Dialog::Zoom { percent } => {
            ui.horizontal(|ui| {
                for p in [200.0, 100.0, 75.0] {
                    ui.radio_value(percent, p, format!("{p}%"));
                }
            });
            // Logarithmic like the status bar slider, and wide enough to aim: the default 100 pt
            // linear slider jumped from 100% to 480% within a short drag (issue #67).
            ui.spacing_mut().slider_width = 220.0;
            ui.add(egui::Slider::new(percent, 10.0..=500.0).logarithmic(true).step_by(1.0).suffix("%"));
            let mut fit = None;
            ui.horizontal(|ui| {
                if ui.button(tl!("Page width")).clicked() {
                    fit = Some("pageWidth");
                }
                if ui.button(tl!("Whole page")).clicked() {
                    fit = Some("onePage");
                }
                if ui.button(tl!("Many pages")).clicked() {
                    fit = Some("multiplePages");
                }
            });
            if let Some(f) = fit {
                let _ = app.run("view.zoom", json!({"value": f}));
                return true;
            }
            let (ok, cancel) = buttons(ui, tl!("OK"));
            if ok {
                let _ = app.run("view.zoom", json!({"value": *percent}));
            }
            ok || cancel
        }
        Dialog::Watermark { text, diagonal } => {
            ui.horizontal(|ui| {
                ui.label(tl!("Text:"));
                ui.text_edit_singleline(text);
            });
            ui.horizontal(|ui| {
                ui.radio_value(diagonal, true, tl!("Diagonal"));
                ui.radio_value(diagonal, false, tl!("Horizontal"));
            });
            let (ok, cancel) = buttons(ui, tl!("OK"));
            if ok {
                let _ = app.run("design.watermark", json!({"text": text, "diagonal": *diagonal}));
            }
            ok || cancel
        }
        Dialog::Phonetic { base, ruby, align, size, offset, existing } => {
            // Base and ruby text are usually Chinese or Japanese: fonts for them load from the next frame.
            app.want_system_cjk = true;
            const ALIGNS: [(&str, &str); 6] = [
                ("center", "Center"),
                ("distributeLetter", "Distribute letters"),
                ("distributeSpace", "Distribute spacing"),
                ("left", "Left"),
                ("right", "Right"),
                ("rightVertical", "Right (vertical)"),
            ];
            egui::Grid::new("phonetic").num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
                ui.label(tl!("Base text:"));
                ui.label(egui::RichText::new(base.as_str()).strong());
                ui.end_row();
                ui.label(tl!("Ruby text:"));
                ui.text_edit_singleline(ruby);
                ui.end_row();
                ui.label(tl!("Alignment:"));
                let shown = ALIGNS.iter().find(|(k, _)| k == align).map_or("Center", |(_, l)| l);
                egui::ComboBox::from_id_salt("phonetic_align").selected_text(tl!(shown)).show_ui(ui, |ui| {
                    for (k, l) in ALIGNS {
                        ui.selectable_value(align, k.to_string(), tl!(l));
                    }
                });
                ui.end_row();
                ui.label(tl!("Offset (pt):"));
                ui.add(egui::DragValue::new(offset).speed(0.5).range(-100.0..=100.0));
                ui.end_row();
                ui.label(tl!("Size (pt):"));
                ui.add(egui::DragValue::new(size).speed(0.5).range(1.0..=200.0));
                ui.end_row();
            });
            let mut remove = false;
            if *existing {
                ui.add_space(4.0);
                remove = ui.button(tl!("Remove")).clicked();
            }
            let (ok, cancel) = buttons(ui, tl!("OK"));
            if remove {
                let _ = app.run("format.phonetic", json!({"ruby": null}));
            } else if ok {
                let _ = app.run("format.phonetic", json!({"ruby": ruby, "align": align, "size": *size, "offset": *offset}));
            }
            ok || cancel || remove
        }
        Dialog::NewStyle { name, based_on, back } => {
            egui::Grid::new("ns").num_columns(2).show(ui, |ui| {
                ui.label(tl!("Name:"));
                ui.text_edit_singleline(name);
                ui.end_row();
                ui.label(tl!("Style based on:"));
                ui.text_edit_singleline(based_on);
                ui.end_row();
            });
            ui.label(egui::RichText::new(tl!("The new style takes the formatting of the current paragraph.")).small().weak());
            let (ok, cancel) = buttons(ui, tl!("OK"));
            let mut created = None;
            if ok {
                created =
                    app.run("styles.create", json!({"name": name, "basedOn": based_on})).ok().and_then(|v| v.get("id")?.as_str().map(str::to_string));
            }
            if (ok || cancel) && *back {
                app.dialog = Some(Dialog::ManageStyles { alphabetical: false, selected: created.unwrap_or_default() });
            }
            ok || cancel
        }
        Dialog::ModifyStyle { back, resume, id, name, font, size, bold, italic, color, before, after } => {
            egui::Grid::new("ms").num_columns(2).show(ui, |ui| {
                ui.label(tl!("Name:"));
                ui.text_edit_singleline(name);
                ui.end_row();
                ui.label(tl!("Font:"));
                ui.text_edit_singleline(font);
                ui.end_row();
                ui.label(tl!("Size:"));
                ui.add(egui::DragValue::new(size).range(1.0..=1638.0).speed(0.5));
                ui.end_row();
                ui.label(tl!("Color:"));
                ui.text_edit_singleline(color);
                ui.end_row();
                ui.label(tl!("Space before/after:"));
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(before).range(0.0..=1584.0));
                    ui.add(egui::DragValue::new(after).range(0.0..=1584.0));
                });
                ui.end_row();
            });
            ui.horizontal(|ui| {
                ui.checkbox(bold, tl!("Bold"));
                ui.checkbox(italic, tl!("Italic"));
            });
            let (ok, cancel) = buttons(ui, tl!("OK"));
            if ok {
                let mut chr = json!({"font": font, "size": *size, "bold": *bold, "italic": *italic});
                if let Some(c) = wordcraft_doc::Rgb::parse(color) {
                    chr["color"] = json!({"Rgb": [c.0, c.1, c.2]});
                }
                let _ =
                    app.run("styles.modify", json!({"style": id, "name": name, "chr": chr, "para": {"spaceBefore": *before, "spaceAfter": *after}}));
            }
            if (ok || cancel) && *back {
                app.dialog = Some(Dialog::ManageStyles { alphabetical: false, selected: id.clone() });
            }
            if (ok || cancel)
                && let Some(r) = resume.take()
            {
                app.dialog = Some(*r);
            }
            ok || cancel
        }
        Dialog::ManageStyles { alphabetical, selected } => manage_styles(app, ui, alphabetical, selected),
        Dialog::TableStyle { id, name, based_on, region, regions, basis } => {
            let styles: Vec<(String, String)> = app
                .session
                .doc
                .styles
                .styles
                .iter()
                .filter(|s| s.kind == wordcraft_doc::StyleKind::Table && Some(&s.id) != id.as_ref())
                .map(|s| (s.id.clone(), s.name.clone()))
                .collect();
            let base_name = styles.iter().find(|(i, _)| i == based_on).map(|(_, n)| n.clone()).unwrap_or_else(|| based_on.clone());
            let mut rebased = false;
            egui::Grid::new("tstyle").num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
                ui.label(tl!("Name:"));
                ui.text_edit_singleline(name);
                ui.end_row();
                ui.label(tl!("Style based on:"));
                egui::ComboBox::from_id_salt("tstyle_base").width(200.0).selected_text(base_name).show_ui(ui, |ui| {
                    for (sid, sname) in &styles {
                        if ui.selectable_label(sid == based_on, sname).clicked() && sid != based_on {
                            *based_on = sid.clone();
                            rebased = true;
                        }
                    }
                });
                ui.end_row();
                ui.label(tl!("Apply formatting to:"));
                egui::ComboBox::from_id_salt("tstyle_region")
                    .width(200.0)
                    .selected_text(tl!(REGION_LABELS.get(*region).copied().unwrap_or("Whole Table")))
                    .show_ui(ui, |ui| {
                        for (i, n) in REGION_LABELS.iter().enumerate() {
                            ui.selectable_value(region, i, tl!(n));
                        }
                    });
                ui.end_row();
            });
            // A new style shows its new base's formatting; a modified one keeps its own.
            if rebased && id.is_none() {
                *regions = table_regions(app, based_on);
                *basis = regions.clone();
            }
            ui.separator();
            let theme = app.session.doc.settings.theme_colors.clone();
            if let Some(r) = regions.get_mut(*region) {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut r.borders, tl!("Borders"));
                    ui.checkbox(&mut r.bold, tl!("Bold"));
                });
                egui::Grid::new("tstyle_colors").num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
                    ui.label(tl!("Fill color:"));
                    color_menu(ui, &theme, &mut r.fill);
                    ui.end_row();
                    ui.label(tl!("Font color:"));
                    color_menu(ui, &theme, &mut r.color);
                    ui.end_row();
                });
            }
            // Live preview: the style as OK would leave it, on the current table's options with
            // the region being edited turned on.
            let v = table_style_params(id.as_deref(), name, based_on, regions, basis);
            let mut look = app
                .session
                .sel
                .focus
                .path
                .cell()
                .and_then(|(tp, _, _)| app.session.doc.table(app.session.sel.focus.story, &tp).map(|t| t.props.look))
                .unwrap_or_default();
            match *region {
                1 => look.header_row = true,
                2 => look.total_row = true,
                3 => look.first_column = true,
                4 => look.last_column = true,
                5 => look.banded_rows = true,
                6 => look.banded_columns = true,
                _ => {}
            }
            ui.add_space(4.0);
            ui.label(egui::RichText::new(tl!("Preview")).small().weak());
            table_style_preview(ui, wordcraft_engine::cmd::table_style::preview(&app.session, &v).as_ref(), look);
            let (ok, cancel) = buttons(ui, tl!("OK"));
            if ok {
                let cmd = if id.is_some() { "table.modifyStyle" } else { "table.newStyle" };
                if let Err(e) = app.run(cmd, v) {
                    app.status(e);
                    return false;
                }
            }
            ok || cancel
        }
        Dialog::Insert(f) => crate::dialogs_insert::body(app, ui, f),
        Dialog::Refs(f) => crate::dialogs_refs::body(app, ui, f),
        Dialog::Commands { query, swallow } => {
            if *swallow {
                ui.ctx().input_mut(|i| i.events.retain(|e| !swallow_shortcut_text(swallow, e)));
            }
            let r = ui.add(egui::TextEdit::singleline(query).hint_text(tl!("Type a command, e.g. \"insert table\"")).desired_width(380.0));
            r.request_focus();
            let reg = app.session.registry.clone();
            // An empty box suggests a few common commands, as Word's Search does.
            let mut hits: Vec<&wordcraft_engine::CommandSpec> = if query.trim().is_empty() {
                ui.label(egui::RichText::new(tl!("Suggested")).font(semibold(12.5)));
                SEARCH_SUGGESTIONS.iter().filter_map(|id| reg.get(id)).collect()
            } else {
                reg.all().iter().filter(|c| command_matches(c, query)).collect()
            };
            hits.truncate(14);
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            let mut run: Option<&str> = enter.then(|| hits.first().map(|c| c.id)).flatten();
            for c in &hits {
                let sc = crate::widgets::shortcut_text(app, c.id);
                let label = format!("{}   —   {}", tl!(c.label), crate::i18n::location(c.location));
                if ui.add(egui::Button::new(label).shortcut_text(sc).min_size(vec2(380.0, 0.0))).clicked() {
                    run = Some(c.id);
                }
            }
            // Search also finds the words in the document (the Navigation pane's results).
            let text = query.trim().to_string();
            let mut find = false;
            if !text.is_empty() {
                ui.separator();
                let label = crate::i18n::fmt(tl!("Find “{text}” in the document"), &[("text", &text)]);
                find = ui.add(egui::Button::new(label).min_size(vec2(380.0, 0.0))).clicked() || (enter && hits.is_empty());
            }
            if let Some(id) = run {
                let _ = app.run(id, json!({}));
            } else if find {
                find_in_navigation_pane(app, ui.ctx(), &text);
            }
            run.is_some() || find || ui.input(|i| i.key_pressed(egui::Key::Escape))
        }
        Dialog::Icons { query, category, selected } => icons_body(app, ui, query, category, selected),
        Dialog::PasteSpecial { formats, choice, payload, note } => {
            ui.label(egui::RichText::new(tl!("Paste as:")).font(semibold(12.5)));
            if formats.is_empty() {
                // The engine's wording, sentence-cased; the format names in it aren't translated.
                let text = if note.is_empty() || note == "the clipboard is empty" {
                    tl!("The clipboard is empty.").to_string()
                } else {
                    let mut c = note.chars();
                    c.next().map(|f| f.to_uppercase().chain(c).collect::<String>() + ".").unwrap_or_default()
                };
                ui.label(egui::RichText::new(text).weak());
            }
            for (i, f) in formats.iter().enumerate() {
                let label = wordcraft_engine::cmd::paste::FORMATS.iter().find(|(id, _)| id == f).map(|(_, l)| *l).unwrap_or(f.as_str());
                ui.radio_value(choice, i, tl!(label));
            }
            if let Some(f) = formats.get(*choice) {
                ui.add_space(6.0);
                ui.label(egui::RichText::new(tl!("Result")).font(semibold(12.5)));
                let hint = match f.as_str() {
                    "formatted" => "Keeps the fonts and formatting of the copied text.",
                    "rtf" => "Reads the clipboard's rich text and keeps its formatting.",
                    "html" => "Reads the clipboard's web content and keeps its formatting.",
                    "picture" => "Pastes the clipboard's picture.",
                    _ => "Drops all formatting and pastes plain text.",
                };
                ui.label(egui::RichText::new(tl!(hint)).weak());
            }
            let (ok, cancel) = buttons(ui, tl!("OK"));
            if ok && let Some(f) = formats.get(*choice) {
                let mut params = payload.clone();
                params["as"] = json!(f);
                let _ = app.run("edit.pasteSpecial", params);
            }
            ok || cancel
        }
        Dialog::About { tab } => {
            ui.set_width(660.0);
            ui.horizontal(|ui| {
                for (i, l) in ["About", "Contributors", "Models"].into_iter().enumerate() {
                    let i = i as u8;
                    if ui.selectable_label(*tab == i, tl!(l)).clicked() {
                        *tab = i;
                    }
                }
            });
            ui.separator();
            match *tab {
                1 => crate::credits::contributors_ui(ui),
                2 => crate::credits::models_ui(ui),
                _ => {
                    ui.label(egui::RichText::new(tl!("WordCraft")).font(semibold(22.0)));
                    let build = option_env!("WORDCRAFT_BUILD_SHA").map(|s| s.get(..8).unwrap_or(s)).unwrap_or(tl!("development build"));
                    ui.label(crate::i18n::fmt(tl!("Version {version} ({build})"), &[("version", env!("CARGO_PKG_VERSION")), ("build", build)]));
                    ui.label(tl!(
                        "A free, open-source word processor written from scratch in Rust.\nPart of the Crafting Apps from the ArtCraft team."
                    ));
                    ui.add_space(6.0);
                    ui.hyperlink_to(tl!("getartcraft.com/apps/wordcraft"), "https://getartcraft.com/apps/wordcraft");
                    ui.hyperlink_to(tl!("Join us on Discord: discord.gg/artcraft"), "https://discord.gg/artcraft");
                    ui.hyperlink_to(tl!("Source code: github.com/storytold/wordcraft"), "https://github.com/storytold/wordcraft");
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(tl!("MIT OR Apache-2.0. Copyright (c) 2026 ArtCraft Team and the WordCraft contributors."))
                            .small()
                            .weak(),
                    );
                }
            }
            let (ok, cancel) = buttons(ui, tl!("Close"));
            ok || cancel
        }
        Dialog::DiscardRecovery { name, .. } => {
            ui.label(crate::i18n::fmt(tl!("Discard the recovery copy of {name}?"), &[("name", name)]));
            ui.label(tl!("This cannot be undone. Your original file stays unchanged."));
            let answer = ui
                .horizontal(|ui| {
                    let cancel = ui.button(tl!("Cancel"));
                    let discard = ui.button(tl!("Discard"));
                    if cancel.clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        Some("cancel")
                    } else if discard.clicked() {
                        Some("discard")
                    } else {
                        None
                    }
                })
                .inner;
            if let Some(answer) = answer {
                app.dialog = Some(d.clone());
                let result = app.run("ui.recoveryDiscard", json!({"answer": answer}));
                if let Err(e) = &result {
                    app.status(e.clone());
                }
                return result.is_ok();
            }
            false
        }
        Dialog::SaveChanges { name, message, .. } => {
            ui.label(egui::RichText::new(crate::i18n::fmt(tl!("Do you want to save changes to {name}?"), &[("name", name)])).font(semibold(15.0)));
            ui.label(tl!("Your changes will be lost if you don't save them."));
            // Why the last Save failed (#1445): the prompt came back so the user can't miss it.
            if !message.is_empty() {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(message.as_str()).color(ui.visuals().error_fg_color));
            }
            ui.add_space(8.0);
            let buttons = ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let cancel = ui.button(tl!("Cancel"));
                    let dont_save = ui.button(tl!("Don't Save"));
                    let save = ui.add(crate::widgets::primary_button(ui, tl!("Save")));
                    [(cancel, "cancel"), (dont_save, "dontSave"), (save, "save")]
                })
                .inner
            });
            let buttons = buttons.inner;
            // The keyboard alone answers, as in Word (#861): Save has focus when the prompt opens,
            // so Enter saves, and the arrow keys (egui's focus navigation) or Tab move between the
            // buttons, shown as Save, Don't Save, Cancel.
            if ui.memory(|m| m.focused().is_none())
                && let Some((save, _)) = buttons.get(2)
            {
                save.request_focus();
            }
            // Enter clicks the focused button; it means Save only when none of them has focus.
            let focused = buttons.iter().any(|(b, _)| b.has_focus());
            let answer = if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                Some("cancel")
            } else if let Some((_, a)) = buttons.iter().find(|(b, _)| b.clicked()) {
                Some(*a)
            } else if !focused && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                Some("save")
            } else {
                None
            };
            let Some(answer) = answer else { return false };
            // `ui.saveChanges` reads the prompt from `app.dialog`, which `show` has taken out.
            app.dialog = Some(d.clone());
            let _ = app.run("ui.saveChanges", json!({"answer": answer}));
            true
        }
        Dialog::RecipientList { fields, rows, message, filter } => recipient_list(app, ui, fields, rows, message, filter),
        Dialog::InsertMergeField { fields, field } => {
            if fields.is_empty() {
                ui.label(egui::RichText::new(tl!("Select recipients first to choose from their fields.")).small().weak());
            } else {
                ui.label(tl!("Fields:"));
                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                    for f in fields.iter() {
                        let resp = ui.selectable_label(field == f, f);
                        if resp.clicked() {
                            field.clone_from(f);
                        }
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.label(tl!("Field:"));
                ui.text_edit_singleline(field);
            });
            let (ok, cancel) = buttons(ui, tl!("Insert"));
            if ok && !field.trim().is_empty() {
                let _ = app.run("mailings.insertField", json!({"field": field.trim()}));
                return true;
            }
            cancel
        }
        Dialog::Password { name, params, password, message } => {
            ui.label(crate::i18n::fmt(tl!("{name} is protected with a password."), &[("name", name)]));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(tl!("Password:"));
                let r = ui.add(egui::TextEdit::singleline(password.as_mut_string()).password(true).desired_width(220.0));
                if password.is_empty() && !r.has_focus() {
                    r.request_focus();
                }
            });
            if !message.is_empty() {
                ui.label(egui::RichText::new(message.as_str()).small().color(ui.visuals().error_fg_color));
            }
            let (ok, cancel) = buttons(ui, tl!("OK"));
            if ok && !password.is_empty() {
                let mut p = params.clone();
                if let Some(o) = p.as_object_mut() {
                    o.insert("password".into(), json!(password.as_str()));
                }
                // `execute`, not `run`: Save Changes was asked when the open began.
                match app.execute("file.open", p) {
                    Ok(_) => {
                        app.ui.backstage = false;
                        return true;
                    }
                    Err(e) if wordcraft_engine::io::wrong_password(&e) => {
                        *message = tl!("That password isn't right. Check it (passwords are case-sensitive) and try again.").to_string();
                        password.as_mut_string().clear();
                    }
                    Err(e) => *message = e,
                }
            }
            cancel
        }
        Dialog::EncryptPassword { password, confirm, message } => {
            ui.label(tl!("Anyone who opens the document will need this password."));
            ui.add_space(4.0);
            egui::Grid::new("encrypt_password").num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
                ui.label(tl!("Password:"));
                let r = ui.add(egui::TextEdit::singleline(password.as_mut_string()).password(true).desired_width(220.0));
                if password.is_empty() && confirm.is_empty() && !r.has_focus() {
                    r.request_focus();
                }
                ui.end_row();
                ui.label(tl!("Confirm password:"));
                ui.add(egui::TextEdit::singleline(confirm.as_mut_string()).password(true).desired_width(220.0));
                ui.end_row();
            });
            ui.label(
                egui::RichText::new(tl!(
                    "Keep the password somewhere safe: without it nobody, WordCraft included, can open the document. Leave both boxes empty to remove the password."
                ))
                .small()
                .weak(),
            );
            if !message.is_empty() {
                ui.label(egui::RichText::new(message.as_str()).small().color(ui.visuals().error_fg_color));
            }
            let (ok, cancel) = buttons(ui, tl!("OK"));
            if ok {
                if password != confirm {
                    *message = tl!("The passwords don't match.").to_string();
                    confirm.as_mut_string().clear();
                    return false;
                }
                let pw = if password.is_empty() { Value::Null } else { json!(password.as_str()) };
                match app.run("file.encrypt", json!({"password": pw})) {
                    Ok(_) => {
                        app.status(if password.is_empty() {
                            tl!("The password is removed. Save the document to keep the change.")
                        } else {
                            tl!("The document will be saved with a password. Save it to protect it.")
                        });
                        return true;
                    }
                    Err(e) => *message = e,
                }
            }
            cancel
        }
        Dialog::DefineList { form } => crate::dialogs_lists::define_list(app, ui, form),
        Dialog::TrackOptions { form } => crate::dialogs_lists::track_options(app, ui, form),
        Dialog::Review(d) => crate::dialogs_review::body(app, ui, d),
        Dialog::CustomizeKeyboard { form } => crate::dialogs_keyboard::customize_keyboard(app, ui, form),
        Dialog::CustomizeRibbon { form } => crate::dialogs_ribbon::customize_ribbon(app, ui, form),
        Dialog::FieldCode { code, edit, message } => {
            ui.label(tl!("Field codes:"));
            let r = ui.add(egui::TextEdit::singleline(code).desired_width(320.0).font(egui::TextStyle::Monospace));
            if !r.has_focus() && code.is_empty() {
                r.request_focus();
            }
            ui.label(egui::RichText::new("PAGE · NUMPAGES · DATE \\@ \"MMMM d, yyyy\" · AUTHOR · TITLE · SEQ Figure").small().weak());
            if !message.is_empty() {
                ui.label(egui::RichText::new(message.as_str()).small().weak());
            }
            let (ok, cancel) = buttons(ui, tl!("OK"));
            if ok && !code.trim().is_empty() {
                let r = if *edit {
                    app.run("fields.setCode", json!({"code": code.trim()}))
                } else {
                    app.run("insert.field", json!({"instr": code.trim()}))
                };
                match r {
                    Ok(_) => return true,
                    Err(e) => *message = e,
                }
            }
            cancel
        }
        Dialog::Control { form } => crate::dialogs_controls::body(app, ui, form),
        Dialog::SelectTable { sheets, selected, headers, message, params } => {
            ui.label(tl!("Choose the sheet that holds the recipients."));
            ui.add_space(4.0);
            // A double-click picks the sheet as OK does.
            let mut picked = false;
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_min_width(300.0);
                egui::ScrollArea::vertical().id_salt("select_table").max_height(220.0).show(ui, |ui| {
                    for (i, name) in sheets.iter().enumerate() {
                        let r = ui.selectable_label(*selected == i, name.as_str());
                        if r.clicked() || r.double_clicked() {
                            *selected = i;
                        }
                        picked |= r.double_clicked();
                    }
                });
            });
            ui.checkbox(headers, tl!("First row of data contains column headers"));
            if !message.is_empty() {
                ui.label(egui::RichText::new(message.as_str()).color(Tokens::get(ui.ctx()).red));
            }
            let (ok, cancel) = buttons(ui, tl!("OK"));
            if ok || picked {
                let mut p = params.clone();
                if let Some(o) = p.as_object_mut() {
                    o.insert("sheet".into(), json!(*selected + 1));
                    o.insert("headers".into(), json!(*headers));
                }
                match app.load_recipients(p) {
                    Ok(_) => return true,
                    Err(e) => *message = e,
                }
            }
            cancel
        }
        Dialog::NoteOptions { form } => crate::dialogs_notes::note_options(app, ui, form),
        Dialog::Templates { form } => crate::dialogs_templates::templates(app, ui, form),
        Dialog::Organizer { form } => crate::dialogs_templates::organizer(app, ui, form),
        Dialog::Layout(d) => crate::dialogs_layout::body(app, ui, d),
        Dialog::FindRecipient { text, message } => {
            ui.horizontal(|ui| {
                ui.label(tl!("Find:"));
                ui.text_edit_singleline(text);
            });
            if !message.is_empty() {
                ui.label(egui::RichText::new(message.as_str()).small().weak());
            }
            let (ok, cancel) = buttons(ui, tl!("Find"));
            if ok && !text.trim().is_empty() {
                match app.run("mailings.findRecipient", json!({"text": text.trim()})) {
                    Ok(_) => return true,
                    Err(e) => *message = e,
                }
            }
            cancel
        }
        Dialog::MergeRule { rule, fields, field, value, then, els } => {
            let skip = rule == "SKIPIF";
            ui.label(tl!(if skip {
                "Leave a recipient out of the merge when a field has this value."
            } else {
                "Insert one text or another, depending on a field's value."
            }));
            ui.add_space(4.0);
            egui::Grid::new("merge_rule").num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
                ui.label(tl!("Field:"));
                if fields.is_empty() {
                    ui.text_edit_singleline(field);
                } else {
                    egui::ComboBox::from_id_salt("merge_rule_field").selected_text(field.as_str()).width(200.0).show_ui(ui, |ui| {
                        for f in fields.iter() {
                            ui.selectable_value(field, f.clone(), f);
                        }
                    });
                }
                ui.end_row();
                ui.label(tl!("Equals:"));
                ui.text_edit_singleline(value);
                ui.end_row();
                if !skip {
                    ui.label(tl!("Then insert:"));
                    ui.text_edit_singleline(then);
                    ui.end_row();
                    ui.label(tl!("Otherwise insert:"));
                    ui.text_edit_singleline(els);
                    ui.end_row();
                }
            });
            let (ok, cancel) = buttons(ui, tl!("OK"));
            let params = rule_params(d);
            if ok && let Some(params) = params {
                let _ = app.run("mailings.rules", params);
                return true;
            }
            cancel
        }
        Dialog::MatchFields { fields, address } => {
            if fields.is_empty() {
                ui.label(egui::RichText::new(tl!("Select recipients first to match their fields.")).small().weak());
            } else {
                ui.label(tl!("The recipient-list field that fills each part of an address block and greeting line."));
                ui.add_space(4.0);
                egui::Grid::new("match_fields").num_columns(2).spacing(vec2(24.0, 6.0)).striped(true).show(ui, |ui| {
                    for (part, key) in ADDRESS_PARTS {
                        ui.label(tl!(part));
                        match address.get(key).and_then(Value::as_str) {
                            Some(f) => ui.label(egui::RichText::new(f).strong()),
                            None => ui.label(egui::RichText::new(tl!("(not matched)")).weak()),
                        };
                        ui.end_row();
                    }
                });
            }
            let (ok, cancel) = buttons(ui, tl!("Close"));
            ok || cancel
        }
        Dialog::CheckErrors { unknown, records } => {
            if unknown.is_empty() {
                ui.label(tl!("No errors found: every merge field is in the recipient list."));
            } else {
                ui.label(tl!("These merge fields aren't in the recipient list:"));
                egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                    for f in unknown.iter() {
                        ui.label(egui::RichText::new(format!("• {f}")).color(Tokens::get(ui.ctx()).red));
                    }
                });
            }
            ui.add_space(4.0);
            ui.label(egui::RichText::new(crate::i18n::fmt(tl!("Recipients: {count}"), &[("count", &records.to_string())])).weak());
            let (ok, cancel) = buttons(ui, tl!("Close"));
            ok || cancel
        }
    }
}

/// The Recipient List dialog's body: a file button, the editable table, OK/Cancel. True to close.
fn recipient_list(
    app: &mut WordApp,
    ui: &mut Ui,
    fields: &mut Vec<String>,
    rows: &mut Vec<Vec<String>>,
    message: &mut String,
    filter: &mut RecipientFilter,
) -> bool {
    const CELL_W: f32 = 120.0;
    const X_W: f32 = 18.0;
    const ROW_H: f32 = 24.0;
    const NUM_W: f32 = 34.0;
    // Each record's place in the merge (None: filtered out), when there is a filter or sort.
    let order: Option<Vec<Option<usize>>> = (!filter.is_empty()).then(|| {
        let (rules, sort) = filter.engine();
        let mut order = vec![None; rows.len()];
        for (k, i) in wordcraft_engine::cmd::mailings::select_records(fields, rows, &rules, &sort).into_iter().enumerate() {
            if let Some(slot) = order.get_mut(i) {
                *slot = Some(k + 1);
            }
        }
        order
    });
    // Wide enough for every field (up to a limit; more scroll sideways).
    let gap = ui.spacing().item_spacing.x;
    let table_w = NUM_W + gap + fields.len() as f32 * (CELL_W + X_W + 2.0 * gap) + X_W + 2.0 * gap;
    ui.set_min_width(table_w.clamp(340.0, 720.0));
    ui.label(tl!("Type the recipients, or load them from a file."));
    if ui.button(tl!("Use an Existing List…")).clicked() {
        // The picker takes over; the list it loads replaces this one.
        let _ = app.run("ui.openRecipientList", json!({}));
        return true;
    }
    ui.add_space(6.0);
    let mut remove_field = None;
    let can_remove = fields.len() > 1;
    let mut remove_row = None;
    let mut x_w = X_W;
    egui::ScrollArea::horizontal().id_salt("recipients_h").max_width(720.0).show(ui, |ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.allocate_exact_size(vec2(NUM_W, ROW_H - 4.0), Sense::hover());
                for (i, f) in fields.iter_mut().enumerate() {
                    ui.add_sized([CELL_W, ROW_H - 4.0], egui::TextEdit::singleline(f).font(egui::TextStyle::Button).hint_text(tl!("Field name")));
                    let x = ui.add_enabled(can_remove, egui::Button::new("×").small().min_size(vec2(X_W, X_W))).on_hover_text(tl!("Remove field"));
                    x_w = x.rect.width();
                    if x.clicked() {
                        remove_field = Some(i);
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().id_salt("recipients_v").max_height(260.0).show_rows(ui, ROW_H, rows.len(), |ui, range| {
                for r in range {
                    let Some(row) = rows.get_mut(r) else { continue };
                    ui.horizontal(|ui| {
                        // The record's number in the merge, a dash when the filter leaves it out.
                        let (num, kept) = match order.as_ref().map(|o| o.get(r).copied().flatten()) {
                            None => ((r + 1).to_string(), true),
                            Some(Some(k)) => (k.to_string(), true),
                            Some(None) => ("–".to_string(), false),
                        };
                        let num = egui::RichText::new(num).weak();
                        let (rect, _) = ui.allocate_exact_size(vec2(NUM_W, ROW_H - 4.0), Sense::hover());
                        ui.put(rect, egui::Label::new(if kept { num } else { num.strikethrough() }));
                        for c in row.iter_mut() {
                            ui.add_sized([CELL_W, ROW_H - 4.0], egui::TextEdit::singleline(c));
                            // Under the field's remove button.
                            ui.allocate_exact_size(vec2(x_w, X_W), Sense::hover());
                        }
                        if ui.add(egui::Button::new("×").small().min_size(vec2(X_W, X_W))).on_hover_text(tl!("Remove recipient")).clicked() {
                            remove_row = Some(r);
                        }
                    });
                }
            });
        });
    });
    if let Some(i) = remove_field.filter(|_| fields.len() > 1) {
        fields.remove(i);
        for r in rows.iter_mut() {
            if i < r.len() {
                r.remove(i);
            }
        }
    }
    if let Some(r) = remove_row.filter(|r| *r < rows.len()) {
        rows.remove(r);
    }
    ui.horizontal(|ui| {
        if ui.button(tl!("New Entry")).clicked() {
            rows.push(vec![String::new(); fields.len()]);
        }
        if ui.button(tl!("Add Field")).clicked() {
            fields.push(String::new());
            for r in rows.iter_mut() {
                r.push(String::new());
            }
        }
    });
    ui.add_space(4.0);
    let toggle = egui::Button::new(tl!("Filter and Sort")).selected(filter.open);
    if ui.add(toggle).clicked() {
        filter.open = !filter.open;
    }
    if filter.open {
        recipient_filter(ui, fields, filter);
    }
    if let Some(order) = &order {
        let count = order.iter().flatten().count().to_string();
        let total = rows.len().to_string();
        ui.label(
            egui::RichText::new(crate::i18n::fmt(tl!("{count} of {total} recipients will be merged."), &[("count", &count), ("total", &total)]))
                .weak(),
        );
    }
    if !message.is_empty() {
        ui.label(egui::RichText::new(message.as_str()).color(Tokens::get(ui.ctx()).red));
    }
    let (ok, cancel) = buttons(ui, tl!("OK"));
    if !ok {
        return cancel;
    }
    let r = recipient_table(fields, rows).map_err(|e| tl!(e).to_string()).and_then(|params| app.load_recipients(params));
    // A new list starts unfiltered; the dialog's filter and sort apply to it.
    let r = r.and_then(|_| if filter.is_empty() { Ok(Value::Null) } else { app.run("mailings.filterRecipients", filter.params()) });
    match r {
        Ok(_) => true,
        Err(e) => {
            *message = e;
            false
        }
    }
}

/// Filter and Sort: condition rows (And/Or, field, comparison, value) and three sort keys.
fn recipient_filter(ui: &mut Ui, fields: &[String], filter: &mut RecipientFilter) {
    let names: Vec<String> = fields.iter().map(|f| f.trim().to_string()).filter(|f| !f.is_empty()).collect();
    let field_combo = |ui: &mut Ui, id: (&str, usize), value: &mut String, none: bool| {
        let shown = if value.is_empty() { tl!("(none)").to_string() } else { value.clone() };
        egui::ComboBox::from_id_salt(id).width(130.0).selected_text(shown).show_ui(ui, |ui| {
            if none {
                ui.selectable_value(value, String::new(), tl!("(none)"));
            }
            for n in &names {
                ui.selectable_value(value, n.clone(), n.as_str());
            }
        });
    };
    let mut remove = None;
    egui::Grid::new("recipient_filter").num_columns(5).spacing(vec2(6.0, 4.0)).show(ui, |ui| {
        ui.label("");
        ui.label(egui::RichText::new(tl!("Field")).weak());
        ui.label(egui::RichText::new(tl!("Comparison")).weak());
        ui.label(egui::RichText::new(tl!("Compare to")).weak());
        ui.end_row();
        for (i, r) in filter.rules.iter_mut().enumerate() {
            if i == 0 {
                ui.label("");
            } else {
                egui::ComboBox::from_id_salt(("rf_join", i)).width(60.0).selected_text(tl!(if r.or { "Or" } else { "And" })).show_ui(ui, |ui| {
                    ui.selectable_value(&mut r.or, false, tl!("And"));
                    ui.selectable_value(&mut r.or, true, tl!("Or"));
                });
            }
            field_combo(ui, ("rf_field", i), &mut r.field, false);
            egui::ComboBox::from_id_salt(("rf_op", i)).width(150.0).selected_text(tl!(r.op.label())).show_ui(ui, |ui| {
                for op in FilterOp::ALL {
                    ui.selectable_value(&mut r.op, op, tl!(op.label()));
                }
            });
            ui.add_enabled(r.op.has_value(), egui::TextEdit::singleline(&mut r.value).desired_width(130.0));
            if ui.add(egui::Button::new("×").small()).on_hover_text(tl!("Remove condition")).clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(i) = remove.filter(|i| *i < filter.rules.len()) {
        filter.rules.remove(i);
    }
    ui.horizontal(|ui| {
        if ui.button(tl!("Add Condition")).clicked() && filter.rules.len() < 64 {
            let field = names.first().cloned().unwrap_or_default();
            filter.rules.push(FilterRow { field, op: FilterOp::Equal, value: String::new(), or: false });
        }
        if ui.button(tl!("Clear All")).clicked() {
            filter.rules.clear();
            filter.sort = Default::default();
        }
    });
    ui.add_space(4.0);
    egui::Grid::new("recipient_sort").num_columns(3).spacing(vec2(6.0, 4.0)).show(ui, |ui| {
        for (i, (field, desc)) in filter.sort.iter_mut().enumerate() {
            ui.label(tl!(if i == 0 { "Sort by" } else { "Then by" }));
            field_combo(ui, ("rs_field", i), field, true);
            ui.add_enabled_ui(!field.is_empty(), |ui| {
                egui::ComboBox::from_id_salt(("rs_dir", i)).width(110.0).selected_text(tl!(if *desc { "Descending" } else { "Ascending" })).show_ui(
                    ui,
                    |ui| {
                        ui.selectable_value(desc, false, tl!("Ascending"));
                        ui.selectable_value(desc, true, tl!("Descending"));
                    },
                );
            });
            ui.end_row();
        }
    });
}

/// The Table Properties dialog's fields: Table, Row, Column and Cell sections.
fn table_properties(ui: &mut Ui, f: &mut TableForm) {
    let unit = crate::options::unit();
    fn len(v: &mut f32, lo: f32, unit: wordcraft_geom::Unit) -> egui::DragValue<'_> {
        egui::DragValue::new(v).speed(0.01).range(lo..=22.0).measure(unit).max_decimals(2)
    }
    let heading = |ui: &mut Ui, s: &str| {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(tl!(s)).font(semibold(12.5)));
    };
    heading(ui, "Table");
    egui::Grid::new("tp_table").num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
        ui.checkbox(&mut f.width_on, tl!("Preferred width:"));
        ui.add_enabled(f.width_on, len(&mut f.width, 0.1, unit));
        ui.end_row();
        ui.label(tl!("Alignment:"));
        ui.horizontal(|ui| {
            for (v, l) in [("left", "Left"), ("center", "Center"), ("right", "Right")] {
                ui.radio_value(&mut f.align, v.to_string(), tl!(l));
            }
        });
        ui.end_row();
        ui.label(tl!("Indent from left:"));
        ui.add_enabled(f.align == "left", egui::DragValue::new(&mut f.indent).speed(0.01).range(-11.0..=22.0).measure(unit).max_decimals(2));
        ui.end_row();
        ui.label(tl!("Table direction:"));
        ui.horizontal(|ui| {
            ui.radio_value(&mut f.rtl, false, tl!("Left-to-right"));
            ui.radio_value(&mut f.rtl, true, tl!("Right-to-left"));
        });
        ui.end_row();
    });
    heading(ui, "Row");
    egui::Grid::new("tp_row").num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
        ui.checkbox(&mut f.row_height_on, tl!("Specify height:"));
        ui.horizontal(|ui| {
            ui.add_enabled(f.row_height_on, len(&mut f.row_height, 0.02, unit));
            ui.add_enabled_ui(f.row_height_on, |ui| {
                egui::ComboBox::from_id_salt("tp_rule").selected_text(tl!(if f.row_exact { "Exactly" } else { "At least" })).show_ui(ui, |ui| {
                    ui.selectable_value(&mut f.row_exact, false, tl!("At least"));
                    ui.selectable_value(&mut f.row_exact, true, tl!("Exactly"));
                });
            });
        });
        ui.end_row();
    });
    ui.checkbox(&mut f.allow_break, tl!("Allow row to break across pages"));
    ui.checkbox(&mut f.header_row, tl!("Repeat as header row on each page"));
    heading(ui, "Column");
    egui::Grid::new("tp_col").num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
        ui.label(tl!("Preferred width:"));
        ui.add(len(&mut f.column_width, 0.1, unit));
        ui.end_row();
    });
    heading(ui, "Cell");
    egui::Grid::new("tp_cell").num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
        ui.checkbox(&mut f.cell_width_on, tl!("Preferred width:"));
        ui.add_enabled(f.cell_width_on, len(&mut f.cell_width, 0.1, unit));
        ui.end_row();
        ui.label(tl!("Vertical alignment:"));
        ui.horizontal(|ui| {
            for (v, l) in [("top", "Top"), ("center", "Center"), ("bottom", "Bottom")] {
                ui.radio_value(&mut f.valign, v.to_string(), tl!(l));
            }
        });
        ui.end_row();
    });
}

/// The Paragraph dialog's OK. `align`, `left` and `right` are as seen on the page; `flags` are
/// keep with next, keep lines together, page break before, widow/orphan control; `opts` (now,
/// when opened) are mirror indents, contextual spacing, suppress line numbers and don't
/// hyphenate, of which only the changed ones are set.
#[allow(clippy::too_many_arguments)]
fn apply_paragraph(
    app: &mut WordApp,
    rtl: bool,
    align: &str,
    left: f32,
    right: f32,
    first: f32,
    before: f32,
    after: f32,
    line: (f32, f32),
    flags: [bool; 4],
    opts: ([bool; 4], [bool; 4]),
) {
    let [keep_next, keep_lines, page_break, widow] = flags;
    let (line, line_was) = line;
    // Direction first: the alignment and indents are as seen on the page, and their logical
    // values depend on it.
    let _ = app.run(if rtl { "para.rtl" } else { "para.ltr" }, json!({}));
    let _ = app.run("para.align", json!({"value": align}));
    let (start, end) = if rtl { (right, left) } else { (left, right) };
    let mut props = json!({
        "indentLeft": start * crate::options::unit().pt_per_unit(), "indentRight": end * crate::options::unit().pt_per_unit(), "indentFirst": first * crate::options::unit().pt_per_unit(),
        "spaceBefore": before, "spaceAfter": after,
        "keepNext": keep_next, "keepLines": keep_lines, "pageBreakBefore": page_break, "widowControl": widow,
    });
    // The dialog edits only multiples: an untouched value keeps Exactly and At Least.
    if line != line_was
        && let Some(m) = props.as_object_mut()
    {
        m.insert("lineSpacing".into(), json!({"rule": "multiple", "value": line}));
    }
    let (now, was) = opts;
    for (key, (on, was)) in ["mirrorIndents", "contextualSpacing", "suppressLineNumbers", "suppressHyphens"].into_iter().zip(now.into_iter().zip(was))
    {
        if on != was
            && let Some(m) = props.as_object_mut()
        {
            m.insert(key.into(), json!(on));
        }
    }
    let _ = app.run("para.set", json!({ "props": props }));
}

/// The Paragraph dialog's Asian Typography: sets the flags that changed since it opened, so an
/// untouched tab adds no direct formatting.
fn apply_asian(app: &mut WordApp, asian: [bool; 6], was: [bool; 6]) {
    let keys = ["kinsoku", "wordWrap", "overflowPunct", "topLinePunct", "autoSpaceDE", "autoSpaceDN"];
    let changed: serde_json::Map<String, Value> =
        keys.iter().zip(asian.iter().zip(was)).filter(|(_, (now, was))| **now != *was).map(|(k, (now, _))| (k.to_string(), json!(now))).collect();
    if !changed.is_empty() {
        let _ = app.run("para.asianTypography", Value::Object(changed));
    }
}

/// Manage Styles' body. Reads the list from `styles.manage`'s data (never re-running the command,
/// which would ask to open this dialog again) and acts through commands.
fn manage_styles(app: &mut WordApp, ui: &mut Ui, alphabetical: &mut bool, selected: &mut String) -> bool {
    let list = wordcraft_engine::cmd::para::manage_list(&app.session, *alphabetical);
    let list = list.as_array().cloned().unwrap_or_default();
    let str_of = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    if !list.iter().any(|x| str_of(x, "id") == *selected) {
        *selected = list.first().map(|x| str_of(x, "id")).unwrap_or_default();
    }
    ui.horizontal(|ui| {
        ui.label(tl!("Sort order:"));
        egui::ComboBox::from_id_salt("manage_styles_sort")
            .selected_text(if *alphabetical { tl!("Alphabetical") } else { tl!("As Recommended") })
            .show_ui(ui, |ui| {
                ui.selectable_value(alphabetical, false, tl!("As Recommended"));
                ui.selectable_value(alphabetical, true, tl!("Alphabetical"));
            });
    });
    let t = Tokens::get(ui.ctx());
    let shown = egui::Id::new("manage_styles_shown");
    egui::Frame::NONE.stroke(egui::Stroke::new(1.0, t.border)).inner_margin(4).show(ui, |ui| {
        egui::ScrollArea::vertical().id_salt("manage_styles_list").max_height(200.0).auto_shrink([false, false]).show(ui, |ui| {
            ui.set_width(380.0);
            for st in &list {
                let id = str_of(st, "id");
                let glyph = match st.get("type").and_then(Value::as_str) {
                    Some("character") => "a",
                    Some("linked") => "¶a",
                    Some("table") => "⊞",
                    _ => "¶",
                };
                let hidden = st.get("hidden").and_then(Value::as_bool).unwrap_or(false);
                let mut name = egui::RichText::new(format!("{glyph}  {}", str_of(st, "name")));
                if hidden {
                    name = name.weak();
                }
                let r = ui.selectable_label(*selected == id, name);
                // Bring the selection into view once, when it changes (opening, New Style…).
                if *selected == id && ui.data(|d| d.get_temp::<String>(shown)).as_deref() != Some(id.as_str()) {
                    r.scroll_to_me(Some(egui::Align::Center));
                    ui.data_mut(|d| d.insert_temp(shown, id.clone()));
                }
                if r.clicked() {
                    ui.data_mut(|d| d.insert_temp(shown, id.clone()));
                    *selected = id.clone();
                }
                if r.double_clicked() {
                    *selected = id;
                    if let Some(Dialog::ModifyStyle { id, name, font, size, bold, italic, color, before, after, .. }) =
                        Dialog::modify_style(app, selected)
                    {
                        app.dialog = Some(Dialog::ModifyStyle { back: true, resume: None, id, name, font, size, bold, italic, color, before, after });
                    }
                }
            }
        });
    });
    let leave = |ui: &mut Ui| {
        ui.data_mut(|d| d.remove::<String>(shown));
        true
    };
    if app.dialog.is_some() {
        return leave(ui);
    }
    let Some(cur) = list.iter().find(|x| str_of(x, "id") == *selected).cloned() else { return close_button(ui) && leave(ui) };
    let ty = str_of(&cur, "type");
    ui.add_space(6.0);
    crate::previews::style_preview(app, ui, selected, &ty, vec2(388.0, if ty == "table" { 64.0 } else { 40.0 }));
    ui.add_space(4.0);
    ui.add(egui::Label::new(egui::RichText::new(describe(&cur)).small()).wrap());
    ui.add_space(6.0);
    let builtin = cur.get("builtIn").and_then(Value::as_bool).unwrap_or(true);
    if ty != "table" {
        let mut gallery = cur.get("inGallery").and_then(Value::as_bool).unwrap_or(false);
        if ui.checkbox(&mut gallery, tl!("Show in the Styles gallery")).changed() {
            let _ = app.run("styles.setVisibility", json!({"style": selected, "gallery": gallery}));
        }
    }
    let mut hidden = cur.get("hidden").and_then(Value::as_bool).unwrap_or(false);
    if ui.checkbox(&mut hidden, tl!("Hide from style lists")).changed() {
        let _ = app.run("styles.setVisibility", json!({"style": selected, "hidden": hidden}));
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        // Table styles are edited from Table Design.
        if ui.add_enabled(ty != "table", egui::Button::new(tl!("Modify…"))).clicked()
            && let Some(Dialog::ModifyStyle { id, name, font, size, bold, italic, color, before, after, .. }) = Dialog::modify_style(app, selected)
        {
            app.dialog = Some(Dialog::ModifyStyle { back: true, resume: None, id, name, font, size, bold, italic, color, before, after });
        }
        if ui.button(tl!("New Style…")).clicked() {
            app.dialog = Some(Dialog::NewStyle { name: "Style1".into(), based_on: str_of(&cur, "name"), back: true });
        }
        let del = ui.add_enabled(!builtin, egui::Button::new(tl!("Delete")));
        let del = if builtin { del.on_disabled_hover_text(tl!("Built-in styles can't be deleted.")) } else { del };
        if del.clicked() {
            let _ = app.run("styles.delete", json!({"style": selected}));
            selected.clear();
        }
    });
    (app.dialog.is_some() || close_button(ui)) && leave(ui)
}

/// A dialog's single Close button (Escape too).
fn close_button(ui: &mut Ui) -> bool {
    ui.add_space(8.0);
    let mut close = ui.input(|i| i.key_pressed(egui::Key::Escape));
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        close |= ui.add(crate::widgets::primary_button(ui, tl!("Close"))).clicked();
    });
    close
}

/// A style's description: its type, the formatting it sets and what it is based on.
fn describe(st: &Value) -> String {
    let f = st.get("format").cloned().unwrap_or_default();
    let fs = |k: &str| f.get(k).and_then(Value::as_str);
    let fnum = |k: &str| f.get(k).and_then(Value::as_f64);
    let pt = |x: f64| format!("{}", (x * 10.0).round() / 10.0);
    let mut parts: Vec<String> = vec![
        match st.get("type").and_then(Value::as_str) {
            Some("character") => tl!("Character style"),
            Some("linked") => tl!("Linked style (paragraph and character)"),
            Some("table") => tl!("Table style"),
            _ => tl!("Paragraph style"),
        }
        .to_string(),
    ];
    if st.get("builtIn").and_then(Value::as_bool).unwrap_or(false) {
        parts.push(tl!("Built-in style").to_string());
    }
    if let Some(x) = fs("font") {
        parts.push(crate::i18n::fmt(tl!("Font: {font}"), &[("font", x)]));
    }
    if let Some(x) = fnum("size") {
        parts.push(crate::i18n::fmt(tl!("{size} pt"), &[("size", &pt(x))]));
    }
    if f.get("bold").and_then(Value::as_bool) == Some(true) {
        parts.push(tl!("Bold").to_string());
    }
    if f.get("italic").and_then(Value::as_bool) == Some(true) {
        parts.push(tl!("Italic").to_string());
    }
    if let Some(x) = fs("color") {
        parts.push(crate::i18n::fmt(tl!("Color: {color}"), &[("color", &format!("#{x}"))]));
    }
    if let Some(x) = fnum("spaceBefore") {
        parts.push(crate::i18n::fmt(tl!("Space before: {pt} pt"), &[("pt", &pt(x))]));
    }
    if let Some(x) = fnum("spaceAfter") {
        parts.push(crate::i18n::fmt(tl!("Space after: {pt} pt"), &[("pt", &pt(x))]));
    }
    if let Some(b) = st.get("basedOn").and_then(Value::as_str) {
        parts.push(crate::i18n::fmt(tl!("Based on: {style}"), &[("style", b)]));
    }
    parts.join(", ")
}

/// What Search lists before anything is typed: common commands (#819).
const SEARCH_SUGGESTIONS: [&str; 3] = ["review.spelling", "review.wordCount", "file.print"];

/// Search's "Find … in the document": the Navigation pane opens on its results for `text`.
fn find_in_navigation_pane(app: &mut WordApp, ctx: &egui::Context, text: &str) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("nav_query"), text.to_string()));
    app.ui.nav_tab = "results".into();
    let _ = app.run("view.navigationPane", json!({"value": true}));
    let _ = app.run("edit.find", json!({"text": text}));
}

/// Whether `e` is the text of the shortcut that opened a box (see `Dialog::Commands::swallow`):
/// a plain key press first means the user is typing, and one swallowed text is all there is.
fn swallow_shortcut_text(swallow: &mut bool, e: &egui::Event) -> bool {
    match e {
        egui::Event::Key { pressed: true, modifiers, .. } if !modifiers.alt => {
            *swallow = false;
            false
        }
        egui::Event::Text(_) if *swallow => {
            *swallow = false;
            true
        }
        _ => false,
    }
}

/// Search's filter: every word of the query is in the command's label, location or id, in English
/// or in the interface language, so "insert table" finds Insert › Tables › Table (#819).
fn command_matches(c: &wordcraft_engine::CommandSpec, query: &str) -> bool {
    if query.trim().is_empty() {
        return false;
    }
    let hay = format!(
        "{} {} {} {} {}",
        c.label.to_lowercase(),
        c.location.to_lowercase(),
        c.id.to_lowercase(),
        crate::i18n::lowercase(tl!(c.label)),
        crate::i18n::lowercase(&crate::i18n::location(c.location))
    );
    query.split_whitespace().all(|w| hay.contains(&w.to_lowercase()) || hay.contains(&crate::i18n::lowercase(w)))
}

/// The Find and Replace dialog's Go To tab (#712): a page (a number, or `+2` / `-1` from the
/// current page; empty, Next and Previous step a page) or a bookmark. Returns true to close.
fn goto_body(app: &mut WordApp, ui: &mut Ui, what: &mut GoToWhat, target: &mut String, message: &mut String) -> bool {
    // The document's bookmarks, only while they are listed (finding them walks the document).
    let bookmarks: Vec<String> = if *what == GoToWhat::Bookmark {
        app.session.doc.bookmarks().into_iter().map(|(n, _)| n).filter(|n| !n.starts_with('_')).collect()
    } else {
        Vec::new()
    };
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.label(tl!("Go to what:"));
            for (w, label) in [(GoToWhat::Page, "Page"), (GoToWhat::Bookmark, "Bookmark")] {
                if ui.selectable_label(*what == w, tl!(label)).clicked() && *what != w {
                    *what = w;
                    target.clear();
                    message.clear();
                }
            }
        });
        ui.add_space(16.0);
        ui.vertical(|ui| match what {
            GoToWhat::Page => {
                ui.label(tl!("Enter page number:"));
                ui.add(egui::TextEdit::singleline(target).desired_width(220.0)).request_focus();
                ui.label(egui::RichText::new(tl!("Type +2 or -1 to move from the page you are on.")).weak());
            }
            GoToWhat::Bookmark => {
                ui.label(tl!("Enter bookmark name:"));
                egui::ComboBox::from_id_salt("goto_bookmark").width(220.0).selected_text(target.as_str()).show_ui(ui, |ui| {
                    for b in &bookmarks {
                        ui.selectable_value(target, b.clone(), b.as_str());
                    }
                });
                if bookmarks.is_empty() {
                    ui.label(egui::RichText::new(tl!("This document has no bookmarks.")).weak());
                }
            }
        });
    });
    if !message.is_empty() {
        ui.label(egui::RichText::new(message.as_str()).weak());
    }
    let mut close = false;
    let mut go: Option<Value> = None;
    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let page_step = *what == GoToWhat::Page && target.trim().is_empty();
        if page_step && ui.button(tl!("Previous")).clicked() {
            go = Some(json!({"pageBy": -1}));
        }
        let label = if page_step { tl!("Next") } else { tl!("Go To") };
        if ui.button(label).clicked() || enter {
            go = match what {
                GoToWhat::Page => match goto_page(target) {
                    Some(p) => Some(p),
                    None => {
                        *message = tl!("Type a page number, or +2 / -1.").to_string();
                        None
                    }
                },
                GoToWhat::Bookmark if !target.is_empty() => Some(json!({"bookmark": target.clone()})),
                GoToWhat::Bookmark => None,
            };
        }
        if ui.button(tl!("Close")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
    });
    if let Some(params) = go {
        match app.run("edit.goto", params) {
            Ok(_) => message.clear(),
            Err(e) => *message = e,
        }
    }
    close
}

/// Go To's page box as `edit.goto` params: empty steps one page on, `+n` / `-n` move from the
/// current page, a number is that page. `None` for anything else.
fn goto_page(text: &str) -> Option<Value> {
    let t = text.trim().replace('\u{2212}', "-");
    if t.is_empty() {
        return Some(json!({"pageBy": 1}));
    }
    if let Some(rest) = t.strip_prefix('+') {
        return rest.parse::<i64>().ok().filter(|n| *n >= 0).map(|n| json!({"pageBy": n}));
    }
    if t.starts_with('-') {
        return t.parse::<i64>().ok().map(|n| json!({"pageBy": n}));
    }
    t.parse::<u64>().ok().filter(|n| *n >= 1).map(|n| json!({"page": n}))
}

/// path data; Insert (or a double click) runs `insert.icon`. Returns true to close.
fn icons_body(app: &mut WordApp, ui: &mut Ui, query: &mut String, category: &mut String, selected: &mut String) -> bool {
    use wordcraft_render::icon_lib::{self, Category};
    let t = Tokens::get(ui.ctx());
    ui.set_width(560.0);
    // Typing goes to the search box (never to the document behind the dialog).
    ui.add(egui::TextEdit::singleline(query).hint_text(tl!("Search icons")).desired_width(560.0)).request_focus();
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        if ui.selectable_label(category.is_empty(), tl!("All")).clicked() {
            category.clear();
        }
        for c in Category::ALL {
            if ui.selectable_label(*category == c.name(), tl!(c.name())).clicked() {
                *category = c.name().to_string();
            }
        }
    });
    ui.add_space(4.0);
    let hits = icon_lib::search(query, Category::parse(category));
    let mut insert = false;
    egui::ScrollArea::vertical().max_height(300.0).min_scrolled_height(300.0).auto_shrink([true, false]).show(ui, |ui| {
        if hits.is_empty() {
            ui.label(egui::RichText::new(tl!("No icons match your search.")).color(t.text_dim));
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            for icon in &hits {
                let (rect, resp) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::click());
                let on = *selected == icon.id;
                let fill = if on {
                    t.checked
                } else if resp.hovered() {
                    t.hover
                } else {
                    egui::Color32::TRANSPARENT
                };
                let edge = if on { egui::Stroke::new(1.5, t.accent) } else { egui::Stroke::new(1.0, t.border) };
                ui.painter().rect(rect, 4.0, fill, edge, egui::StrokeKind::Inside);
                crate::icons::library(ui.painter(), rect.shrink(12.0), icon, t.text);
                let resp = resp.on_hover_text(icon.name);
                if resp.clicked() {
                    *selected = icon.id.to_string();
                }
                if resp.double_clicked() {
                    *selected = icon.id.to_string();
                    insert = true;
                }
            }
        });
    });
    let name = icon_lib::find(selected).map(|i| i.name).unwrap_or("");
    ui.add_space(4.0);
    ui.label(egui::RichText::new(name).color(t.text_dim));
    let (ok, cancel) = buttons(ui, tl!("Insert"));
    if (ok || insert) && !selected.is_empty() {
        let _ = app.run("insert.icon", json!({"id": selected.as_str()}));
        return true;
    }
    cancel
}

/// Enclose Characters' preview: the text in the chosen shape, shrunk or with the shape enlarged
/// (drawn, so it doesn't depend on the interface font having ○ □ △ ◇).
fn enclose_preview(ui: &mut Ui, style: &str, text: &str, shape: &str) {
    let t = Tokens::get(ui.ctx());
    let (r, _) = ui.allocate_exact_size(vec2(110.0, 64.0), Sense::hover());
    let p = ui.painter_at(r);
    p.rect_filled(r, 2.0, t.input);
    p.rect_stroke(r, 2.0, egui::Stroke::new(1.0, t.border_strong), egui::StrokeKind::Inside);
    let ink = ui.visuals().text_color();
    let c = r.center();
    let (box_h, text_px) = match style {
        "none" => (0.0, 26.0),
        "enlarge" => (48.0, 26.0),
        _ => (34.0, 20.0),
    };
    let h = box_h / 2.0;
    let stroke = egui::Stroke::new(1.5, ink);
    match (shape, box_h > 0.0) {
        (_, false) => {}
        ("square", _) => {
            p.rect_stroke(egui::Rect::from_center_size(c, vec2(box_h, box_h)), 0.0, stroke, egui::StrokeKind::Middle);
        }
        ("triangle", _) => {
            let pts = vec![egui::pos2(c.x, c.y - h), egui::pos2(c.x + h * 1.1, c.y + h), egui::pos2(c.x - h * 1.1, c.y + h)];
            p.add(egui::Shape::closed_line(pts, stroke));
        }
        ("diamond", _) => {
            let pts = vec![egui::pos2(c.x, c.y - h), egui::pos2(c.x + h, c.y), egui::pos2(c.x, c.y + h), egui::pos2(c.x - h, c.y)];
            p.add(egui::Shape::closed_line(pts, stroke));
        }
        _ => {
            p.circle_stroke(c, h, stroke);
        }
    }
    let px = match (style, shape) {
        ("shrink", "triangle" | "diamond") => text_px * 0.75,
        _ => text_px,
    };
    let dy = if shape == "triangle" && box_h > 0.0 { h * 0.3 } else { 0.0 };
    p.text(c + vec2(0.0, dy), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(px), ink);
}

#[cfg(test)]
mod tests {
    use super::*;
    use wordcraft_doc::Pos;
    use wordcraft_doc::props::Align;
    use wordcraft_engine::Session;

    /// Go To's page box (#712): a number, +n / -n from the current page (a typographic minus
    /// too), empty for the next page; anything else is refused.
    #[test]
    fn go_to_reads_its_page_box() {
        assert_eq!(goto_page("7"), Some(json!({"page": 7})));
        assert_eq!(goto_page(" +2 "), Some(json!({"pageBy": 2})));
        assert_eq!(goto_page("-1"), Some(json!({"pageBy": -1})));
        assert_eq!(goto_page("\u{2212}3"), Some(json!({"pageBy": -3})));
        assert_eq!(goto_page(""), Some(json!({"pageBy": 1})));
        for junk in ["0", "abc", "+x", "1.5", "99999999999999999999999"] {
            assert_eq!(goto_page(junk), None, "{junk}");
        }
    }

    /// Find, Replace and Go To are one dialog with three tabs (#712).
    #[test]
    fn find_replace_and_go_to_are_one_dialog() {
        let mut app = app_with("One");
        for (name, tab) in [("find", FindTab::Find), ("replace", FindTab::Replace), ("goto", FindTab::GoTo)] {
            let _ = app.run("ui.dialog", json!({"name": name}));
            assert!(matches!(&app.dialog, Some(Dialog::Find { tab: t, .. }) if *t == tab), "{name}");
            assert_eq!(app.dialog.as_ref().map(|d| d.name()), Some(name));
        }
    }

    fn app_with(text: &str) -> WordApp {
        let mut app = WordApp::new(Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        let _ = app.run("document.setText", json!({"text": text}));
        app
    }

    fn props(app: &WordApp) -> wordcraft_doc::props::ParaProps {
        app.session.doc.para_at(&app.session.sel.focus).map(|p| p.props.clone()).unwrap_or_default()
    }

    /// #819: the text a shortcut sends with its key isn't typed into the box it opened, whether it
    /// comes in the same frame or the next; real typing (a key press first) is.
    #[test]
    fn shortcut_text_is_swallowed_once() {
        let key = |key, alt| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers { alt, ..Default::default() },
        };
        let text = |t: &str| egui::Event::Text(t.into());
        let run = |events: &[egui::Event]| {
            let mut swallow = true;
            events.iter().filter(|e| !swallow_shortcut_text(&mut swallow, e)).cloned().collect::<Vec<_>>()
        };
        assert_eq!(run(&[key(egui::Key::Q, true), text("q")]), vec![key(egui::Key::Q, true)], "Alt+Q's q, same frame");
        assert_eq!(run(&[text("q"), text("b")]), vec![text("b")], "the q a frame late; then typing");
        assert_eq!(run(&[key(egui::Key::B, false), text("b")]), vec![key(egui::Key::B, false), text("b")], "a click-opened box takes typing");
    }

    /// #819: Search matches every word anywhere in the label, location or id, in any order.
    #[test]
    fn command_search_matches_every_word() {
        let app = app_with("");
        let reg = app.session.registry.clone();
        let table = reg.get("insert.table").unwrap();
        assert!(command_matches(table, "insert table"), "the box's own example");
        assert!(command_matches(table, "  Table   INSERT "), "any order, any case, extra spaces");
        assert!(!command_matches(table, "insert chart"), "every word has to match");
        assert!(!command_matches(table, ""), "an empty box lists nothing");
        assert!(!command_matches(table, "   "));
    }

    /// Ordinary Turkish queries must find dotted-I labels, while canonical English labels and
    /// ASCII command IDs retain their old case-insensitive search in every interface language.
    #[test]
    fn command_search_respects_turkish_i_pairs_and_keeps_canonical_matches() {
        use egui_kittest::kittest::Queryable;
        let mut fonts = false;
        let mut h = egui_kittest::Harness::builder().with_size(vec2(1440.0, 900.0)).build_ui_state(
            move |ui, app: &mut WordApp| {
                let ctx = ui.ctx().clone();
                crate::i18n::set_current(crate::i18n::Lang::from_pref(&app.ui.language));
                if fonts {
                    show(app, &ctx);
                } else {
                    crate::theme::install_fonts_for(&ctx, false);
                    fonts = true;
                }
            },
            app_with("Unchanged document"),
        );
        let before = serde_json::to_value(&h.state().session.doc).unwrap();
        for (language, query, expected) in [
            ("tr", "italik", "İtalik   —   Giriş › Yazı Tipi"),
            ("tr", "İTALİK", "İtalik   —   Giriş › Yazı Tipi"),
            ("tr", "içindekiler", "İçindekiler Tablosu   —   Başvurular › İçindekiler Tablosu"),
            ("tr", "YAZI TİPİ", "İtalik   —   Giriş › Yazı Tipi"),
            ("tr", "INSERT", "Tablo   —   Ekle › Tablolar"),
            ("tr", "INSERT.TABLE", "Tablo   —   Ekle › Tablolar"),
            ("tr", "FORMAT.ITALIC", "İtalik   —   Giriş › Yazı Tipi"),
            ("tr", "TABLE OF CONTENTS", "İçindekiler Tablosu   —   Başvurular › İçindekiler Tablosu"),
            ("en", "ITALIC", "Italic   —   Home › Font"),
            ("en", "FORMAT.ITALIC", "Italic   —   Home › Font"),
            ("en", "INSERT.TABLE", "Table   —   Insert › Tables"),
        ] {
            h.state_mut().ui.language = language.into();
            h.state_mut().dialog = Some(Dialog::Commands { query: query.into(), swallow: false });
            h.run_steps(4);
            assert!(
                h.query(egui_kittest::kittest::By::new().role(egui::accesskit::Role::Button).label_contains(expected)).is_some(),
                "{language}: {query}\n{:?}",
                h.query_all_by_role(egui::accesskit::Role::Button).collect::<Vec<_>>()
            );
            assert_eq!(crate::i18n::lowercase("Iİıi"), if language == "tr" { "ıiıi" } else { "ii\u{307}ıi" });
        }
        assert_eq!(serde_json::to_value(&h.state().session.doc).unwrap(), before, "search does not change the document");
        crate::i18n::set_current(crate::i18n::Lang::EN);
    }

    /// Open the Paragraph dialog and press OK without changing anything.
    fn ok_unchanged(app: &mut WordApp) {
        ok_with(app, |_| {});
    }

    /// Open the Paragraph dialog, change its options (`opts`) with `edit` and press OK.
    fn ok_with(app: &mut WordApp, edit: impl FnOnce(&mut [bool; 4])) {
        let Some(Dialog::Paragraph {
            rtl,
            align,
            left,
            right,
            first,
            before,
            after,
            line,
            line_was,
            keep_next,
            keep_lines,
            page_break,
            widow,
            mut opts,
            ..
        }) = Dialog::open("paragraph", app)
        else {
            panic!("no paragraph dialog")
        };
        let was = opts;
        edit(&mut opts);
        apply_paragraph(
            app,
            rtl,
            &align,
            left,
            right,
            first,
            before,
            after,
            (line, line_was),
            [keep_next, keep_lines, page_break, widow],
            (opts, was),
        );
    }

    #[test]
    fn paragraph_dialog_sets_only_the_options_that_changed() {
        // #512: mirror indents, same-style spacing, suppress line numbers, don't hyphenate.
        let mut app = app_with("Hello");
        ok_unchanged(&mut app);
        let p = props(&app);
        assert_eq!((p.mirror_indents, p.contextual_spacing, p.suppress_line_numbers, p.suppress_hyphens), (None, None, None, None));
        ok_with(&mut app, |o| *o = [true; 4]);
        let p = props(&app);
        assert_eq!(
            (p.mirror_indents, p.contextual_spacing, p.suppress_line_numbers, p.suppress_hyphens),
            (Some(true), Some(true), Some(true), Some(true))
        );
        let Some(Dialog::Paragraph { opts, .. }) = Dialog::open("paragraph", &mut app) else { panic!() };
        assert_eq!(opts, [true; 4], "the dialog shows what is set");
        ok_with(&mut app, |o| o[3] = false);
        assert_eq!(props(&app).suppress_hyphens, Some(false));
        assert_eq!(props(&app).mirror_indents, Some(true));
    }

    #[test]
    fn paragraph_dialog_shows_direction_and_visual_alignment() {
        let mut app = app_with("سلام دنیا");
        let _ = app.run("para.rtl", json!({}));
        let _ = app.run("para.set", json!({"props": {"indentLeft": 36.0}}));
        let Some(Dialog::Paragraph { rtl, align, left, right, .. }) = Dialog::open("paragraph", &mut app) else { panic!() };
        assert!(rtl);
        assert_eq!(align, "right", "a start-aligned right-to-left paragraph shows as right aligned");
        assert_eq!((left, right), (0.0, 0.5), "the start indent is the right one");
    }

    #[test]
    fn paragraph_dialog_ok_keeps_what_it_shows() {
        for (rtl, align) in [(false, "para.alignCenter"), (false, "para.alignRight"), (true, "para.alignLeft"), (true, "para.alignRight")] {
            let mut app = app_with("متن");
            let _ = app.run(if rtl { "para.rtl" } else { "para.ltr" }, json!({}));
            let _ = app.run(align, json!({}));
            let _ = app.run("para.set", json!({"props": {"indentLeft": 18.0, "indentRight": 9.0}}));
            let before = props(&app);
            ok_unchanged(&mut app);
            let after = props(&app);
            assert_eq!((after.bidi, after.align), (before.bidi, before.align), "{align} in rtl={rtl}");
            assert_eq!((after.indent_left, after.indent_right), (Some(18.0), Some(9.0)), "{align} in rtl={rtl}");
        }
    }

    #[test]
    fn paragraph_dialog_keeps_fixed_line_spacing() {
        use wordcraft_doc::props::LineSpacing;
        for (cmd, rule) in [(json!({"exactly": 24}), LineSpacing::Exactly(24.0)), (json!({"atLeast": 24}), LineSpacing::AtLeast(24.0))] {
            // OK without changes, and OK after changing only Keep with next (#467).
            let mut app = app_with("Example");
            let _ = app.run("para.lineSpacing", cmd.clone());
            ok_unchanged(&mut app);
            assert_eq!(props(&app).line_spacing, Some(rule), "unchanged OK, {cmd}");
            let Some(Dialog::Paragraph { rtl, align, left, right, first, before, after, line, line_was, keep_lines, page_break, widow, .. }) =
                Dialog::open("paragraph", &mut app)
            else {
                panic!("no paragraph dialog")
            };
            apply_paragraph(
                &mut app,
                rtl,
                &align,
                left,
                right,
                first,
                before,
                after,
                (line, line_was),
                [true, keep_lines, page_break, widow],
                ([false; 4], [false; 4]),
            );
            let p = props(&app);
            assert_eq!((p.line_spacing, p.keep_next), (Some(rule), Some(true)), "Keep with next, {cmd}");
        }
        // A changed value still sets a multiple.
        let mut app = app_with("Example");
        let _ = app.run("para.lineSpacing", json!({"exactly": 24}));
        let Some(Dialog::Paragraph { rtl, align, left, right, first, before, after, line_was, keep_next, keep_lines, page_break, widow, .. }) =
            Dialog::open("paragraph", &mut app)
        else {
            panic!("no paragraph dialog")
        };
        apply_paragraph(
            &mut app,
            rtl,
            &align,
            left,
            right,
            first,
            before,
            after,
            (1.5, line_was),
            [keep_next, keep_lines, page_break, widow],
            ([false; 4], [false; 4]),
        );
        assert_eq!(props(&app).line_spacing, Some(LineSpacing::Multiple(1.5)));
    }

    #[test]
    fn font_dialog_sets_the_asian_and_latin_fonts_separately() {
        // #505: the Asian text font box and the Latin text font box each set their own font, and
        // OK leaves the one not changed as it was.
        let mut app = app_with("Hello 你好");
        let _ = app.run("select.all", json!({}));
        let fonts = |app: &WordApp| {
            let c = app.session.doc.para_at(&Pos::body(0, 0)).map(|p| p.props_of_char(0).clone()).unwrap_or_default();
            (c.font, c.font_ea)
        };
        let latin = fonts(&app).0;
        let Some(mut d) = Dialog::open("font", &mut app) else { panic!("no font dialog") };
        let Dialog::Font { asian, font_ea, .. } = &mut d else { panic!() };
        assert!(*asian, "Chinese text in the selection shows the Asian text font box");
        *font_ea = "SimSun".into();
        apply_font(&mut app, &d);
        assert_eq!(fonts(&app), (latin, Some("SimSun".into())));
        let Some(mut d) = Dialog::open("font", &mut app) else { panic!() };
        let Dialog::Font { font, font_ea, .. } = &mut d else { panic!() };
        assert_eq!(font_ea, "SimSun");
        *font = "Georgia".into();
        apply_font(&mut app, &d);
        assert_eq!(fonts(&app), (Some("Georgia".into()), Some("SimSun".into())));
        // One undo step each.
        let _ = app.run("edit.undo", json!({}));
        assert_eq!(fonts(&app).1.as_deref(), Some("SimSun"));
        assert_ne!(fonts(&app).0.as_deref(), Some("Georgia"));
    }

    #[test]
    fn paragraph_dialog_changes_direction() {
        let mut app = app_with("Hello");
        let Some(Dialog::Paragraph { align, left, right, first, before, after, line, line_was, keep_next, keep_lines, page_break, widow, .. }) =
            Dialog::open("paragraph", &mut app)
        else {
            panic!()
        };
        // Right to left, aligned right on the page, 1" from the right edge.
        apply_paragraph(
            &mut app,
            true,
            "right",
            left,
            right + 1.0,
            first,
            before,
            after,
            (line, line_was),
            [keep_next, keep_lines, page_break, widow],
            ([false; 4], [false; 4]),
        );
        let _ = align;
        let p = props(&app);
        assert_eq!(p.bidi, Some(true));
        assert_eq!(p.align, Some(Align::Left), "right on the page is the start edge");
        assert_eq!(p.indent_left, Some(72.0), "the right indent is the start indent");
    }

    /// Switching the Paragraph dialog's tabs re-centred it, so the tab strip jumped up and down
    /// by half the height difference (#507): the top edge now stays put and only the bottom moves.
    #[test]
    fn switching_paragraph_tabs_keeps_the_top_edge() {
        use egui_kittest::kittest::Queryable;
        let mut app = app_with("Some text");
        app.dialog = Dialog::open("paragraph", &mut app);
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
            |ui, app: &mut WordApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            app,
        );
        let mut tops = vec![];
        let mut oks = vec![];
        for page in [0u8, 1, 2, 0] {
            if let Some(Dialog::Paragraph { tab, .. }) = &mut h.state_mut().dialog {
                *tab = page;
            }
            for _ in 0..4 {
                h.step();
            }
            tops.push(h.get_by_label("Indents and Spacing").rect().top());
            oks.push(h.get_by_role_and_label(egui::accesskit::Role::Button, "OK").rect().top());
        }
        assert!((oks[0] - oks[1]).abs() > 10.0, "the tabs differ in height: {oks:?}");
        for t in &tops {
            assert!((t - tops[0]).abs() <= 0.5, "the tab strip stays put: {tops:?}");
        }
    }

    #[test]
    fn font_dialog_ok_keeps_what_the_user_left_alone() {
        // Issue #466: OK sent every field, clearing small caps, spacing and double underline
        // when only the size changed.
        let mut app = app_with("Example");
        let _ = app.run("select.all", json!({}));
        let _ = app.run("format.set", json!({"props": {"smallCaps": true, "caps": true, "hidden": true, "spacing": 2, "underline": "double"}}));
        let Some(mut d) = Dialog::open("font", &mut app) else { panic!("no font dialog") };
        if let Dialog::Font { size, small_caps, spacing, .. } = &mut d {
            assert!(*small_caps, "the dialog shows small caps");
            assert_eq!(*spacing, 2.0);
            *size = "14".into();
        }
        apply_font(&mut app, &d);
        let c = app.session.doc.para_at(&wordcraft_doc::Pos::body(0, 0)).map(|p| p.props_of_char(0).clone()).unwrap_or_default();
        assert_eq!(c.size, Some(14.0));
        assert_eq!((c.small_caps, c.caps, c.hidden, c.spacing), (Some(true), Some(true), Some(true), Some(2.0)));
        assert_eq!(c.underline, Some(wordcraft_doc::props::Underline::Double));
        // A field the user does change still applies.
        let Some(mut d) = Dialog::open("font", &mut app) else { panic!("no font dialog") };
        if let Dialog::Font { small_caps, .. } = &mut d {
            *small_caps = false;
        }
        apply_font(&mut app, &d);
        let c = app.session.doc.para_at(&wordcraft_doc::Pos::body(0, 0)).map(|p| p.props_of_char(0).clone()).unwrap_or_default();
        assert_eq!((c.small_caps, c.caps, c.size), (Some(false), Some(true), Some(14.0)));
    }

    /// A dialog was pinned by its anchor, so dragging its title bar did nothing (#732).
    #[test]
    fn dragging_a_dialog_title_bar_moves_it() {
        let mut app = app_with("Some text");
        app.dialog = Dialog::open("paragraph", &mut app);
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
            |ui, app: &mut WordApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            app,
        );
        for _ in 0..4 {
            h.step();
        }
        let id = egui::Id::new(("dialog", "Paragraph"));
        let before = h.ctx.memory(|m| m.area_rect(id)).expect("dialog is open");
        let screen = h.ctx.content_rect();
        assert!((before.center().x - screen.center().x).abs() <= 1.0, "opens centred: {before:?} in {screen:?}");
        let grab = egui::pos2(before.center().x, before.top() + 10.0);
        let drop = grab + vec2(-200.0, 120.0);
        let button =
            |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        h.event(egui::Event::PointerMoved(grab));
        h.step();
        h.event(button(grab, true));
        h.step();
        h.event(egui::Event::PointerMoved(drop));
        h.step();
        h.event(button(drop, false));
        for _ in 0..4 {
            h.step();
        }
        let after = h.ctx.memory(|m| m.area_rect(id)).expect("dialog is still open");
        assert!((after.min.x - (before.min.x - 200.0)).abs() <= 1.0, "moved left: {before:?} -> {after:?}");
        assert!((after.min.y - (before.min.y + 120.0)).abs() <= 1.0, "moved down: {before:?} -> {after:?}");
    }

    fn app_harness(app: WordApp) -> egui_kittest::Harness<'static, WordApp> {
        egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
            |ui, app: &mut WordApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            app,
        )
    }

    /// The Page Setup paper size as opened, in points, and the Paper size entry it shows.
    fn opened_paper(app: &mut WordApp) -> ((f32, f32), Option<&'static str>) {
        let Some(Dialog::PageSetup { paper, .. }) = Dialog::open("pageSetup", app) else { panic!("no page setup dialog") };
        let ppu = crate::options::unit().pt_per_unit();
        ((paper.0 * ppu, paper.1 * ppu), paper_entry(paper))
    }

    /// #1433: Page Setup's Paper size shows the named size the section has, in either
    /// orientation, and Custom size (None) for anything else.
    #[test]
    fn page_setup_reads_the_paper_size() {
        let mut app = app_with("Page");
        let sect = |app: &WordApp| wordcraft_engine::cmd::page::sect(&app.session);
        let ((w, h), entry) = opened_paper(&mut app);
        assert!((w - sect(&app).page_w).abs() < 0.01 && (h - sect(&app).page_h).abs() < 0.01, "{w}×{h}");
        assert_eq!(entry, wordcraft_geom::paper_name(w, h));
        assert!(entry.is_some(), "a new document has a named size");
        let _ = app.run("layout.size", json!({"name": "A4"}));
        let _ = app.run("layout.orientation", json!({"value": "landscape"}));
        let ((w, h), entry) = opened_paper(&mut app);
        assert!(w > h, "landscape shows the wide side as the width: {w}×{h}");
        assert_eq!(entry, Some("A4"));
        let _ = app.run("layout.size", json!({"width": 300.0, "height": 500.0}));
        assert_eq!(opened_paper(&mut app).1, None, "Custom size");
    }

    /// #1433: a custom width and height applied with OK set the section's size (before the
    /// orientation, so a wide custom size ends up landscape); an unchanged size doesn't run
    /// `layout.size`, and switching orientation alone only turns the page.
    #[test]
    fn page_setup_applies_a_custom_paper_size() {
        use egui_kittest::kittest::Queryable;
        let ppu = crate::options::unit().pt_per_unit();
        let size_steps = |app: &WordApp| app.session.undo_labels().iter().filter(|l| l.as_str() == "Size").count();
        let sect = |app: &WordApp| {
            let s = wordcraft_engine::cmd::page::sect(&app.session);
            (s.page_w.round(), s.page_h.round(), s.landscape)
        };
        // OK on an unchanged dialog leaves the size alone.
        let mut h = app_harness(app_with("Page"));
        let before = sect(h.state());
        h.state_mut().run("ui.dialog", json!({"name": "pageSetup"})).unwrap();
        h.run_steps(3);
        h.get_by_label("OK").click();
        h.run_steps(3);
        assert!(h.state().dialog.is_none());
        assert_eq!((sect(h.state()), size_steps(h.state())), (before, 0), "no layout.size for an unchanged size");

        for ((w, ht), want) in [((400.0, 600.0), (400.0, 600.0, false)), ((800.0, 400.0), (800.0, 400.0, true))] {
            h.state_mut().run("ui.dialog", json!({"name": "pageSetup"})).unwrap();
            h.run_steps(2);
            if let Some(Dialog::PageSetup { paper, landscape, .. }) = h.state_mut().dialog.as_mut() {
                *paper = (w / ppu, ht / ppu);
                *landscape = w > ht;
            }
            h.run_steps(2);
            h.get_by_label("OK").click();
            h.run_steps(3);
            assert_eq!(sect(h.state()), want, "{w}×{ht}");
        }
        assert_eq!(size_steps(h.state()), 2);

        // Portrait again with the same paper: the dialog swaps width and height, which is no new size.
        h.state_mut().run("ui.dialog", json!({"name": "pageSetup"})).unwrap();
        h.run_steps(2);
        if let Some(Dialog::PageSetup { paper, landscape, .. }) = h.state_mut().dialog.as_mut() {
            *paper = (paper.1, paper.0);
            *landscape = false;
        }
        h.run_steps(2);
        h.get_by_label("OK").click();
        h.run_steps(3);
        assert_eq!(sect(h.state()), (400.0, 800.0, false));
        assert_eq!(size_steps(h.state()), 2, "turning the page isn't a new size");
    }

    /// #1433: the Paper group's own widgets: choosing A4 in Paper size fills Width and Height,
    /// Landscape swaps them, OK gives an A4 landscape section; typing a width wider than the
    /// height turns the orientation to Landscape and shows Custom size; choosing Custom size
    /// keeps the numbers and stays chosen.
    #[test]
    fn page_setup_paper_widgets_work_together() {
        use egui::accesskit::Role;
        use egui_kittest::kittest::Queryable;
        let ppu = crate::options::unit().pt_per_unit();
        let mut h = app_harness(app_with("Page"));
        h.state_mut().run("ui.dialog", json!({"name": "pageSetup"})).unwrap();
        h.run_steps(3);
        let state = |h: &egui_kittest::Harness<'_, WordApp>| match &h.state().dialog {
            Some(Dialog::PageSetup { paper, landscape, .. }) => {
                ((paper.0 * ppu * 100.0).round() / 100.0, (paper.1 * ppu * 100.0).round() / 100.0, *landscape)
            }
            _ => panic!("no page setup dialog"),
        };
        let combo = |h: &egui_kittest::Harness<'_, WordApp>| h.get_by_role_and_label(Role::ComboBox, "Paper size:").value();
        assert_eq!(combo(&h).as_deref(), Some("Letter"));
        h.get_by_role_and_label(Role::ComboBox, "Paper size:").click();
        h.run_steps(3);
        h.get_by_label("A4").click();
        h.run_steps(3);
        assert_eq!(state(&h), (595.28, 841.89, false), "A4 fills the fields");
        assert_eq!(combo(&h).as_deref(), Some("A4"));
        h.get_by_label("Landscape").click();
        h.run_steps(3);
        assert_eq!(state(&h), (841.89, 595.28, true), "Landscape swaps them");
        assert_eq!(combo(&h).as_deref(), Some("A4"));
        h.get_by_label("OK").click();
        h.run_steps(3);
        let sect = wordcraft_engine::cmd::page::sect(&h.state().session);
        assert_eq!(((sect.page_w * 100.0).round() / 100.0, (sect.page_h * 100.0).round() / 100.0, sect.landscape), (841.89, 595.28, true));

        // Portrait A4 again; a width wider than the height turns it to Landscape, Custom size.
        h.state_mut().run("layout.orientation", json!({"value": "portrait"})).unwrap();
        h.state_mut().run("ui.dialog", json!({"name": "pageSetup"})).unwrap();
        h.run_steps(3);
        h.get_by_value("8.27\"").focus();
        h.run_steps(2);
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        h.event(egui::Event::Text("15".into()));
        h.key_press(egui::Key::Tab);
        h.run_steps(3);
        assert_eq!(state(&h), (1080.0, 841.89, true), "wider than tall: Landscape");
        assert_eq!(combo(&h).as_deref(), Some("Custom size"));

        // Back to A4, then Custom size: the numbers stay and so does the choice.
        h.get_by_role_and_label(Role::ComboBox, "Paper size:").click();
        h.run_steps(3);
        h.get_by_label("A4").click();
        h.run_steps(3);
        h.get_by_role_and_label(Role::ComboBox, "Paper size:").click();
        h.run_steps(3);
        h.get_by_label("Custom size").click();
        h.run_steps(3);
        assert_eq!(state(&h), (841.89, 595.28, true));
        assert_eq!(combo(&h).as_deref(), Some("Custom size"));
    }

    /// #1433: Page Setup checks the new size and margins together: a page its margins don't fit
    /// keeps the dialog open with a message and applies nothing; a smaller page with smaller
    /// margins applies (margins first, so no step is refused on the way).
    #[test]
    fn page_setup_refuses_margins_that_dont_fit_and_applies_smaller_ones_first() {
        use egui_kittest::kittest::Queryable;
        let ppu = crate::options::unit().pt_per_unit();
        let mut h = app_harness(app_with("Page"));
        let before = wordcraft_engine::cmd::page::sect(&h.state().session);
        let undo = h.state().session.undo_depth();
        h.state_mut().run("ui.dialog", json!({"name": "pageSetup"})).unwrap();
        h.run_steps(2);
        if let Some(Dialog::PageSetup { paper, .. }) = h.state_mut().dialog.as_mut() {
            *paper = (100.0 / ppu, 500.0 / ppu);
        }
        h.run_steps(2);
        h.get_by_label("OK").click();
        h.run_steps(3);
        assert!(
            matches!(&h.state().dialog, Some(Dialog::PageSetup { message, .. }) if message.contains("margins don't fit")),
            "stays open with a message"
        );
        assert_eq!((wordcraft_engine::cmd::page::sect(&h.state().session), h.state().session.undo_depth()), (before, undo), "nothing applied");

        if let Some(Dialog::PageSetup { top, bottom, left, right, .. }) = h.state_mut().dialog.as_mut() {
            (*top, *bottom, *left, *right) = (18.0 / ppu, 18.0 / ppu, 18.0 / ppu, 18.0 / ppu);
        }
        h.run_steps(2);
        h.get_by_label("OK").click();
        h.run_steps(3);
        assert!(h.state().dialog.is_none());
        let s = wordcraft_engine::cmd::page::sect(&h.state().session);
        assert_eq!((s.page_w.round(), s.page_h.round(), s.margin_left.round(), s.margin_top.round()), (100.0, 500.0, 18.0, 18.0));
    }

    /// Open Page Setup in `h`, set its paper and margins (points) and press OK.
    fn page_setup_ok(h: &mut egui_kittest::Harness<'_, WordApp>, (w, ht): (f32, f32), (t, b, l, r): (f32, f32, f32, f32)) {
        use egui_kittest::kittest::Queryable;
        let ppu = crate::options::unit().pt_per_unit();
        h.state_mut().run("ui.dialog", json!({"name": "pageSetup"})).unwrap();
        h.run_steps(2);
        if let Some(Dialog::PageSetup { paper, landscape, top, bottom, left, right, .. }) = h.state_mut().dialog.as_mut() {
            *paper = (w / ppu, ht / ppu);
            *landscape = w > ht;
            (*top, *bottom, *left, *right) = (t / ppu, b / ppu, l / ppu, r / ppu);
        }
        h.run_steps(2);
        h.get_by_label("OK").click();
        h.run_steps(3);
    }

    /// #1433: one OK in Page Setup is one undo step, whichever order its commands run in (the
    /// size first; the margins first; margins of 0 in between), and Undo restores the section.
    #[test]
    fn page_setup_ok_is_one_undo_step() {
        let sect = |h: &egui_kittest::Harness<'_, WordApp>| wordcraft_engine::cmd::page::sect(&h.state().session);
        // Size first, then orientation and margins.
        let mut h = app_harness(app_with("Page"));
        let (before, depth) = (sect(&h), h.state().session.undo_depth());
        page_setup_ok(&mut h, (400.0, 600.0), (36.0, 36.0, 36.0, 36.0));
        assert!(h.state().dialog.is_none());
        assert_eq!((sect(&h).page_w, sect(&h).margin_left), (400.0, 36.0));
        assert_eq!(h.state().session.undo_depth(), depth + 1, "one step");
        h.state_mut().run("edit.undo", json!({})).unwrap();
        assert_eq!(sect(&h), before, "one undo restores the section");

        // Neither size first nor margins first fits on the way: margins of 0 in between.
        h.state_mut().run("layout.margins", json!({"top": 72.0, "bottom": 72.0, "left": 250.0, "right": 250.0})).unwrap();
        let (before, depth) = (sect(&h), h.state().session.undo_depth());
        page_setup_ok(&mut h, (300.0, 900.0), (400.0, 400.0, 36.0, 36.0));
        assert!(h.state().dialog.is_none());
        let s = sect(&h);
        assert_eq!((s.page_w, s.page_h, s.margin_top, s.margin_left), (300.0, 900.0, 400.0, 36.0));
        assert_eq!(h.state().session.undo_depth(), depth + 1, "one step");
        h.state_mut().run("edit.undo", json!({})).unwrap();
        assert_eq!(sect(&h), before, "one undo restores the section");
    }

    /// #1433: a command the dialog runs that fails (here `layout.size`, refused by the second
    /// selected section's margins) shows its error, keeps the dialog open and leaves the
    /// document and the undo stack as they were.
    #[test]
    fn page_setup_shows_a_refused_command_and_applies_nothing() {
        let mut app = app_with("one");
        app.run("layout.break", json!({"kind": "nextPage"})).unwrap();
        app.run("text.insert", json!({"text": "two"})).unwrap();
        app.run("layout.margins", json!({"left": 150.0, "right": 150.0})).unwrap();
        // Both sections selected, the first one's settings in the dialog.
        app.run("select.all", json!({})).unwrap();
        let sel = &mut app.session.sel;
        std::mem::swap(&mut sel.anchor, &mut sel.focus);
        let doc = app.session.doc.sections().iter().map(|(_, x)| (*x).clone()).collect::<Vec<_>>();
        let depth = app.session.undo_depth();
        let mut h = app_harness(app);
        page_setup_ok(&mut h, (300.0, 500.0), (72.0, 72.0, 72.0, 72.0));
        assert!(
            matches!(&h.state().dialog, Some(Dialog::PageSetup { message, .. }) if message.contains("margins don't fit")),
            "stays open with the error"
        );
        let after = h.state().session.doc.sections().iter().map(|(_, x)| (*x).clone()).collect::<Vec<_>>();
        assert_eq!((after, h.state().session.undo_depth()), (doc, depth), "nothing applied");
    }

    /// #1433: Layout › Size › More Paper Sizes… opens Page Setup.
    #[test]
    fn more_paper_sizes_opens_page_setup() {
        use egui_kittest::kittest::Queryable;
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(400.0, 600.0))
            .build_ui_state(|ui, app: &mut WordApp| crate::ribbon::size_menu(ui, app), app_with("Page"));
        h.run_steps(2);
        h.get(egui_kittest::kittest::By::new().label_contains("More Paper Sizes…")).click();
        h.run_steps(2);
        assert_eq!(h.state().dialog.as_ref().map(|d| d.name()), Some("pageSetup"));
    }
}
