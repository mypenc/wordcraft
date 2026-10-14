//! Page geometry: mirrored margins, headers and footers taller than their margin (also when
//! linked to the previous section), endnotes continuing over pages, and the caret in a footnote
//! continued on the next page.
use super::*;
use wordcraft_doc::para::{InlineObject, NoteKind};
use wordcraft_doc::{PartKind, Pos, para_block};

fn lay(doc: &Document) -> DocLayout {
    let mut c = LayoutCache::new();
    layout(doc, &mut c, &LayoutOptions::default())
}

fn paras(text: &str, n: usize) -> Vec<Arc<Block>> {
    (1..=n).map(|i| para_block(Paragraph::with_text(&format!("{text} {i}"), Default::default()))).collect()
}

/// Top and bottom of the lines of `story` among `items`.
fn extent(items: &[Placed], story: StoryRef) -> Option<(f32, f32)> {
    items
        .iter()
        .filter_map(|it| match it {
            Placed::Lines { story: s, y, para, l0, l1, .. } if *s == story => Some((*y, item_bottom(*y, para, *l0, *l1)?)),
            _ => None,
        })
        .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
}

/// The left edge of the body text on each page.
fn body_left(l: &DocLayout) -> Vec<f32> {
    l.pages
        .iter()
        .map(|p| p.items.iter().find_map(|it| if let Placed::Lines { story: StoryRef::Body, x, .. } = it { Some(*x) } else { None }).unwrap())
        .collect()
}

#[test]
fn mirrored_margins_put_the_inside_margin_toward_the_binding() {
    // #452: inside 108 (plus a 10pt gutter), outside 36. Odd pages: inside on the left; even
    // pages: on the right.
    let mut d = Document::from_text(&"Body line\n".repeat(90));
    d.last_section.margin_left = 108.0;
    d.last_section.margin_right = 36.0;
    d.last_section.gutter = 10.0;
    let plain = body_left(&lay(&d));
    assert!(plain.len() >= 2 && plain.iter().all(|x| (x - 118.0).abs() < 0.01), "not mirrored: {plain:?}");
    d.settings.mirror_margins = true;
    let l = lay(&d);
    let lefts = body_left(&l);
    for (i, (x, p)) in lefts.iter().zip(&l.pages).enumerate() {
        let want = if p.number % 2 == 1 { 118.0 } else { 36.0 };
        assert!((x - want).abs() < 0.01, "page {i} (number {}): text at {x}, want {want}", p.number);
        assert!((p.body.x - want).abs() < 0.01 && (p.body.w - d.last_section.text_width()).abs() < 0.01, "page {i}: {:?}", p.body);
    }
}

#[test]
fn a_tall_footer_pushes_the_body_up() {
    // #453: an eight-line footer reaches far above the 72pt bottom margin.
    let mut d = Document::from_text(&"Body line\n".repeat(60));
    let id = d.add_part(PartKind::Footer, paras("Footer line", 8));
    d.last_section.footers.default = Some(id);
    let l = lay(&d);
    assert!(l.pages.len() >= 2);
    let foot_top = extent(&l.pages[0].footer, StoryRef::Part(id)).unwrap().0;
    assert!(foot_top < 792.0 - 72.0, "the footer is taller than the margin: {foot_top}");
    for (i, p) in l.pages.iter().enumerate() {
        let (_, body_bottom) = extent(&p.items, StoryRef::Body).unwrap();
        let (top, _) = extent(&p.footer, StoryRef::Part(id)).unwrap();
        assert!(body_bottom <= top + 0.01, "page {i}: body ends at {body_bottom}, footer starts at {top}");
        assert!(p.body.bottom() <= top + 0.01, "page {i}: {:?}", p.body);
    }
    // Every body paragraph is still there, once.
    let placed: usize = l.pages.iter().flat_map(|p| &p.items).filter(|it| matches!(it, Placed::Lines { story: StoryRef::Body, l0: 0, .. })).count();
    assert_eq!(placed, d.body.len());
    // A one-line footer fits in the margin: the body keeps its full height.
    d.parts.insert(id, wordcraft_doc::Part { kind: PartKind::Footer, blocks: paras("Footer", 1), body: Default::default() });
    let l = lay(&d);
    assert!((l.pages[0].body.bottom() - (792.0 - 72.0)).abs() < 0.01, "{:?}", l.pages[0].body);
}

#[test]
fn a_linked_tall_header_pushes_the_next_sections_body_down() {
    // #454: section 2 has no header of its own and shows section 1's (link to previous).
    let mut d = Document::from_text("First section\nSecond section");
    let id = d.add_part(PartKind::Header, paras("Header line", 8));
    let first = SectionProps { headers: wordcraft_doc::section::HeaderSet { default: Some(id), ..Default::default() }, ..d.last_section.clone() };
    d.para_mut(StoryRef::Body, &Path::top(0)).unwrap().section = Some(Box::new(first));
    let l = lay(&d);
    assert_eq!(l.pages.len(), 2);
    for (i, p) in l.pages.iter().enumerate() {
        assert_eq!(p.header_story, Some(id), "page {i}");
        let (_, header_bottom) = extent(&p.header, StoryRef::Part(id)).unwrap();
        assert!(header_bottom > 72.0, "the header reaches past the top margin");
        let (body_top, _) = extent(&p.items, StoryRef::Body).unwrap();
        assert!(body_top >= header_bottom - 0.01, "page {i}: body at {body_top}, header ends at {header_bottom}");
    }
    assert_eq!(l.pages[0].body.y, l.pages[1].body.y, "same header, same body top");
}

#[test]
fn long_endnotes_continue_on_new_pages() {
    // #456: an 80-paragraph endnote after a one-line body.
    let mut d = Document::from_text("Body");
    let id = d.add_part(PartKind::Endnote, paras("Endnote paragraph", 80));
    d.insert_object(&Pos::body(0, 4), InlineObject::NoteRef { kind: NoteKind::Endnote, id, custom: String::new() }, &Default::default()).unwrap();
    let l = lay(&d);
    assert!(l.pages.len() >= 2, "{} pages", l.pages.len());
    let mut seen = Vec::new();
    for (i, p) in l.pages.iter().enumerate() {
        if let Some((top, bottom)) = extent(&p.items, StoryRef::Part(id)) {
            assert!(top >= p.body.y - 0.01 && bottom <= p.body.bottom() + 0.01, "page {i}: note at {top}..{bottom}, body {:?}", p.body);
        }
        for it in &p.items {
            if let Placed::Lines { story: StoryRef::Part(s), path, l0: 0, .. } = it
                && *s == id
            {
                seen.push(path.0.clone());
            }
        }
    }
    assert_eq!(seen, (0..80u32).map(|i| vec![i]).collect::<Vec<_>>(), "every paragraph once, in order");
    // The caret reaches the last paragraph, on a page.
    let c = l.caret(&Pos { story: StoryRef::Part(id), path: Path::top(79), off: 0 }).unwrap();
    assert!(c.top < 792.0 - 72.0, "{c:?}");
}

#[test]
fn caret_in_a_continued_footnote_is_found_on_its_page() {
    // #457: one long footnote paragraph split over pages 1 and 2.
    let mut d = Document::from_text(&"Body text line.\n".repeat(16));
    let id = d.add_part(
        PartKind::Footnote,
        vec![para_block(Paragraph::with_text(&"A long footnote that runs on and on. ".repeat(100), Default::default()))],
    );
    d.insert_object(&Pos::body(4, 4), InlineObject::NoteRef { kind: NoteKind::Footnote, id, custom: String::new() }, &Default::default()).unwrap();
    let l = lay(&d);
    // The first line of the piece on page 2.
    let (para, k) = l.pages[1]
        .items
        .iter()
        .find_map(|it| match it {
            Placed::Lines { story: StoryRef::Part(s), para, l0, .. } if *s == id && *l0 > 0 => Some((para.clone(), *l0)),
            _ => None,
        })
        .expect("the note continues on page 2");
    let at = |off: usize| Pos { story: StoryRef::Part(id), path: Path::top(0), off };
    let cont = at(para.lines[k + 1].start);
    for hint in [0, 1] {
        assert_eq!(l.caret_on(&at(0), hint).map(|c| c.page), Some(0), "start, hint {hint}");
        let c = l.caret_on(&cont, hint).unwrap_or_else(|| panic!("no caret with hint {hint}"));
        assert_eq!(c.page, 1, "hint {hint}");
        // Up goes to the line above, not to the note's start.
        let (up, _) = l.vertical(&cont, None, -1, hint).unwrap();
        assert_eq!(para.line_of(up.off), k, "hint {hint}: {up:?}");
    }
}

/// A letter per section, each restarting its page numbers, with "Page X of Y" in its footer
/// where Y is a `PAGEREF` to a bookmark at the letter's end (how mail-merged letters count their
/// own pages). The footer shows each letter's last page, not the file's stale result.
#[test]
fn page_references_in_footers_show_their_bookmarks_page() {
    let field = |instr: &str| InlineObject::Field { instr: instr.into(), result: "9".into(), locked: false, code: false };
    let footer = |d: &mut Document, mark: &str| {
        let mut p = Paragraph::with_text("Page  of ", Default::default());
        p.insert_object(5, field("PAGE"), &Default::default()).unwrap();
        let n = p.len();
        p.insert_object(n, field(&format!("PAGEREF {mark} \\* Arabic \\* MERGEFORMAT")), &Default::default()).unwrap();
        d.add_part(PartKind::Footer, vec![para_block(p)])
    };
    let mut d = Document::from_text("Letter A 1\nLetter A 2\nLetter A 3\nLetter B 1\nLetter B 2");
    let (fa, fb) = (footer(&mut d, "End_A"), footer(&mut d, "End_B"));
    for i in [1, 2, 4] {
        d.para_mut(StoryRef::Body, &Path::top(i)).unwrap().props.page_break_before = Some(true);
    }
    for (i, mark) in [(2, "End_A"), (4, "End_B")] {
        let p = d.para_mut(StoryRef::Body, &Path::top(i)).unwrap();
        let n = p.len();
        p.insert_object(n, InlineObject::BookmarkStart { name: mark.into() }, &Default::default()).unwrap();
    }
    let restart = |id| SectionProps {
        page_num_start: Some(1),
        footers: wordcraft_doc::section::HeaderSet { default: Some(id), ..Default::default() },
        ..d.last_section.clone()
    };
    let (a, b) = (restart(fa), restart(fb));
    d.para_mut(StoryRef::Body, &Path::top(2)).unwrap().section = Some(Box::new(a));
    d.last_section = b;
    let shown = |l: &DocLayout| -> Vec<String> {
        l.pages
            .iter()
            .map(|p| {
                p.footer
                    .iter()
                    .flat_map(|it| if let Placed::Lines { para, .. } = it { para.shown.iter().map(|s| s.1.clone()).collect() } else { vec![] })
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .collect()
    };
    let mut cache = LayoutCache::new();
    let l = layout(&d, &mut cache, &LayoutOptions::default());
    assert_eq!(shown(&l), ["1/3", "2/3", "3/3", "1/2", "2/2"]);
    // The bookmark moves to the letter's first page: only the reference changes, and the
    // footers laid out before (same page, same totals) aren't reused.
    let last = d.para_mut(StoryRef::Body, &Path::top(4)).unwrap();
    let n = last.len();
    last.delete(n - '\u{FFFC}'.len_utf8(), n).unwrap();
    let first = d.para_mut(StoryRef::Body, &Path::top(3)).unwrap();
    let n = first.len();
    first.insert_object(n, InlineObject::BookmarkStart { name: "End_B".into() }, &Default::default()).unwrap();
    assert_eq!(shown(&layout(&d, &mut cache, &LayoutOptions::default())), ["1/3", "2/3", "3/3", "1/1", "2/1"]);
    // In the body, a page reference keeps its result until fields are updated.
    let mut body = Document::from_text("Intro\nSee page ");
    let p = body.para_mut(StoryRef::Body, &Path::top(0)).unwrap();
    p.insert_object(0, InlineObject::BookmarkStart { name: "Intro".into() }, &Default::default()).unwrap();
    let p = body.para_mut(StoryRef::Body, &Path::top(1)).unwrap();
    let n = p.len();
    p.insert_object(n, field("PAGEREF Intro"), &Default::default()).unwrap();
    // The footer's reference to the same bookmark is resolved.
    let mut fp = Paragraph::new();
    fp.insert_object(0, field("PAGEREF Intro"), &Default::default()).unwrap();
    let id = body.add_part(PartKind::Footer, vec![para_block(fp)]);
    body.last_section.footers.default = Some(id);
    let l = lay(&body);
    assert_eq!(footer_shown(&l), ["1"]);
    let texts: Vec<String> = l.pages[0]
        .items
        .iter()
        .flat_map(|it| if let Placed::Lines { para, .. } = it { para.shown.iter().map(|s| s.1.clone()).collect() } else { vec![] })
        .collect();
    assert_eq!(texts, ["9"]);
}

/// The fields' text in each page's footer, joined with "/".
fn footer_shown(l: &DocLayout) -> Vec<String> {
    l.pages
        .iter()
        .map(|p| {
            p.footer
                .iter()
                .flat_map(|it| if let Placed::Lines { para, .. } = it { para.shown.iter().map(|s| s.1.clone()).collect() } else { vec![] })
                .collect::<Vec<_>>()
                .join("/")
        })
        .collect()
}

/// A footer of `PAGEREF` fields to `marks` (stored result "9"), set on the last section.
fn page_ref_footer(d: &mut Document, marks: &[&str]) -> u32 {
    let mut p = Paragraph::new();
    for mark in marks {
        let n = p.len();
        let field = InlineObject::Field { instr: format!("PAGEREF {mark}"), result: "9".into(), locked: false, code: false };
        p.insert_object(n, field, &Default::default()).unwrap();
    }
    let id = d.add_part(PartKind::Footer, vec![para_block(p)]);
    d.last_section.footers.default = Some(id);
    id
}

fn mark(d: &mut Document, path: &Path, off: usize, name: &str) {
    d.para_mut(StoryRef::Body, path).unwrap().insert_object(off, InlineObject::BookmarkStart { name: name.into() }, &Default::default()).unwrap();
}

/// Bookmarks in the second half of a paragraph split over two pages, in a table cell and in a
/// section numbered in roman numerals are found on the page and in the format they land in.
#[test]
fn page_references_find_bookmarks_in_split_paragraphs_tables_and_roman_sections() {
    // One long paragraph over two pages, marked at its start and its end.
    let mut d = Document::from_text(&"Long paragraph text running on. ".repeat(160));
    let n = d.para(StoryRef::Body, &Path::top(0)).unwrap().len();
    mark(&mut d, &Path::top(0), n, "End");
    mark(&mut d, &Path::top(0), 0, "Start");
    page_ref_footer(&mut d, &["Start", "End"]);
    let l = lay(&d);
    assert_eq!(l.pages.len(), 2, "the paragraph runs over two pages");
    assert_eq!(footer_shown(&l), ["1/2", "1/2"]);

    // A table cell on the second page.
    let mut d = Document::from_text("First\nSecond");
    d.para_mut(StoryRef::Body, &Path::top(1)).unwrap().props.page_break_before = Some(true);
    d.insert_block(StoryRef::Body, &Path::top(2), Block::Table(wordcraft_doc::Table::new(1, 1, 300.0))).unwrap();
    let cell = Path(vec![2, 0, 0, 0]);
    d.para_mut(StoryRef::Body, &cell).unwrap().insert_text(0, "In a cell", &Default::default()).unwrap();
    mark(&mut d, &cell, 0, "Cell");
    page_ref_footer(&mut d, &["Cell"]);
    assert_eq!(footer_shown(&lay(&d)), ["2", "2"]);

    // The second section numbers its pages in lower-case roman numerals.
    let mut d = Document::from_text("Arabic\nRoman 1\nRoman 2");
    d.para_mut(StoryRef::Body, &Path::top(2)).unwrap().props.page_break_before = Some(true);
    let n = d.para(StoryRef::Body, &Path::top(2)).unwrap().len();
    mark(&mut d, &Path::top(2), n, "End");
    let first = d.last_section.clone();
    d.para_mut(StoryRef::Body, &Path::top(0)).unwrap().section = Some(Box::new(first));
    d.last_section.page_num_start = Some(1);
    d.last_section.page_num_format = wordcraft_doc::section::NumFormat::LowerRoman;
    page_ref_footer(&mut d, &["End"]);
    assert_eq!(footer_shown(&lay(&d)), ["", "ii", "ii"], "the first section has no footer of its own");

    // Two bookmarks of the same name: the first in the document counts.
    let mut d = Document::from_text("First\nSecond");
    d.para_mut(StoryRef::Body, &Path::top(1)).unwrap().props.page_break_before = Some(true);
    mark(&mut d, &Path::top(1), 0, "Twice");
    mark(&mut d, &Path::top(0), 0, "Twice");
    page_ref_footer(&mut d, &["Twice"]);
    assert_eq!(footer_shown(&lay(&d)), ["1", "1"]);
}

/// A text box in a footer resolves its page references; one in the body keeps its result.
#[test]
fn page_references_in_footer_text_boxes_are_resolved() {
    use wordcraft_doc::para::{Float, ShapeKind, Wrap};
    let text_box = |d: &mut Document| {
        let mut p = Paragraph::new();
        let field = InlineObject::Field { instr: "PAGEREF End".into(), result: "9".into(), locked: false, code: false };
        p.insert_object(0, field, &Default::default()).unwrap();
        let story = d.add_part(PartKind::TextBox, vec![para_block(p)]);
        InlineObject::Shape {
            kind: ShapeKind::TextBox,
            w: 100.0,
            h: 40.0,
            fill: None,
            stroke: None,
            stroke_width: 0.0,
            float: Float { wrap: Wrap::Inline, ..Default::default() },
            story: Some(story),
            freeform: None,
            effects: Default::default(),
            extra: Default::default(),
        }
    };
    let box_text = |l: &DocLayout, story: Option<u32>| -> Vec<String> {
        l.pages
            .iter()
            .flat_map(|p| p.items.iter().chain(&p.footer))
            .flat_map(|it| match it {
                Placed::Lines { story: StoryRef::Part(id), para, .. } if Some(*id) == story => para.shown.iter().map(|s| s.1.clone()).collect(),
                _ => vec![],
            })
            .collect()
    };
    let mut d = Document::from_text("First\nSecond");
    d.para_mut(StoryRef::Body, &Path::top(1)).unwrap().props.page_break_before = Some(true);
    let n = d.para(StoryRef::Body, &Path::top(1)).unwrap().len();
    mark(&mut d, &Path::top(1), n, "End");
    let in_footer = text_box(&mut d);
    let footer_box = in_footer.text_box();
    let mut fp = Paragraph::with_text("Footer ", Default::default());
    let n = fp.len();
    fp.insert_object(n, in_footer, &Default::default()).unwrap();
    let id = d.add_part(PartKind::Footer, vec![para_block(fp)]);
    d.last_section.footers.default = Some(id);
    let in_body = text_box(&mut d);
    let body_box = in_body.text_box();
    d.para_mut(StoryRef::Body, &Path::top(0)).unwrap().insert_object(0, in_body, &Default::default()).unwrap();
    let l = lay(&d);
    assert!(box_text(&l, footer_box).iter().all(|t| t == "2") && !box_text(&l, footer_box).is_empty(), "{:?}", box_text(&l, footer_box));
    assert_eq!(box_text(&l, body_box), ["9"]);
}
