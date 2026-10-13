# Layout and pagination parity with Microsoft Word

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-12 · **Change:** trivial (symbol-font text drawn as Unicode symbols, #776; previously trivial: large-table scrolling measured and fixed, #811; previously trivial: text boxes use their own margins; Resize shape to fit text never cuts text, #743; previously trivial: complex-script shaping and Myanmar line breaking, #604; previously trivial: body page totals, table-cell footnotes and bookmark link pages fixed, #450 #451 #455; previously major: first version; line breaking, pagination and object placement checked against Word's behaviour from the layout source on origin/main) · **Target:** Microsoft Word (Microsoft 365) for Mac 16.113.4

A document only "opens correctly" if its pages break where Word breaks them: the same lines, the
same page count, footnotes and pictures on the same pages. This checklist covers line breaking,
pagination and object placement in `crates/layout`. Typing behaviour is in
[`typing-parity.md`](typing-parity.md); the reading and writing of the properties is in
[`file-format-parity.md`](file-format-parity.md).

**Dimension: ~55% (estimated), 90–145 h to full** (the sum of the rows below). The engine implements most of Word's layout
rules, but nothing has been compared page-for-page with Word on real documents, and the default
font can't match Word's metrics.

## Checklist

✅ matches Word as far as our tests show · 🟡 implemented, differs or untested against Word · ❌ missing

| Feature | State | Evidence / gap | Hours |
|---|---|---|---|
| Line breaking (UAX #14 with Word's exceptions: no break after `/`) | ✅ | `layout::tests::no_line_break_right_after_a_slash` | — |
| Justification, Word 2013+ space shrinking (compat mode 15) | ✅ | #109 | — |
| Complex scripts (Myanmar, Khmer, Indic): shaped per script run with OpenType fallback fonts, one cluster per syllable, Myanmar wraps between syllables | 🟡 | `layout::tests_complex` (#604). Khmer, Thai and Lao still wrap only at spaces (need a dictionary) | 4–8 |
| Kashida justification (Arabic) | ❌ | no kashida in `crates/layout` | 3–5 |
| Distributed / Thai distributed alignment | 🟡 | read; layout untested | 1–2 |
| Default font metrics (Aptos, Calibri, Cambria…) | 🟡 | Carlito/Caladea/Liberation are metric-compatible for Calibri/Cambria/Arial/Times/Courier; **Aptos (Word's default since 2024) has no metric-compatible open substitute**, so new Word documents wrap differently ([typing-parity.md](typing-parity.md) Known gaps). Needs an owner decision (commission or license a metric-matched font) | 6–10 + owner |
| Symbol fonts (Symbol, Wingdings, Wingdings 2/3, Webdings) without the proprietary font: each character, stored as U+F020–U+F0FF or a plain 8-bit code, is drawn as its Unicode equivalent from a face that has it; PDF text and copies read as those symbols | ✅ | `doc::symbol_fonts` tables (Adobe Symbol table and Unicode L2/12-368), `layout::tests::wingdings_text_draws_and_reads_as_its_unicode_symbols` (#776). Shapes come from Noto Sans Symbols / Symbols 2, DejaVu Sans or Segoe UI Symbol, so they differ from Word's; Wingdings 0xFF (not in Unicode) still has no symbol | — |
| Indents in characters (`w:firstLineChars`, `w:leftChars`…), measured in the paragraph mark's font size | ✅ | #534, `layout::tests::character_indents_use_the_paragraph_mark_size`; spacing in lines (`w:beforeLines`/`w:afterLines`) uses the stored points (needs the document grid) | — |
| Line spacing (single, 1.5, double, at least, exactly, multiple), spacing before/after, contextual spacing | 🟡 | Word puts multiple-spacing extra below the text and collapses spacing differently (#135, #108 open) | 3–5 |
| Kerning only where Kerning for fonts asks for it (`w:kern`, at or above its size) | ✅ | Measured in Word for Windows: no `w:kern` or `w:kern="0"` is unkerned; WordCraft kerned every run, so its lines held more text than Word's. `layout::tests::kerning_follows_kerning_for_fonts_like_word` | — |
| Automatic (HTML-style) paragraph spacing (`w:beforeAutospacing`, `w:afterAutospacing`) | ✅ | Measured in Word for Windows: 14 pt at any size, none at the top of the document or a cell or after a cell's last paragraph, the larger of two neighbouring spaces. `layout::tests::automatic_paragraph_spacing_like_word`, `…_drops_at_cell_edges` | — |
| Widow/orphan control, keep with next, keep lines together, page break before | ✅ | `lib.rs` keep_next/keep_lines; a line holding only a page break stays at the bottom of a full page instead of leaving a blank page (#534) | — |
| Tabs (left, center, right, decimal, bar), leaders | 🟡 | Bar tabs untested | 1–2 |
| Hyphenation (automatic, zone from the document, words in capitals, soft hyphens, consecutive limit, Manual Hyphenation, #407) | 🟡 | English patterns only; other languages hyphenate wrongly or not at all | in localization |
| Drop caps | ✅ | 2026-10-06 | — |
| Columns, column breaks, separators, balancing | ✅ | Columns balance on a section's last page when the next section starts continuous (also at the document end, which Word only balances when the document ends with a continuous break); not after next-page/even/odd breaks or on a page with a column break; keep-together, keep-with-next, widow/orphan control and unsplittable rows can leave them slightly uneven (#350) | — |
| Section breaks (next page, continuous, even, odd), different first page, odd/even headers | ✅ | Section breaks don't show in Draft view (#42). A break inserted with text selected goes before the selection and keeps it, as in Word (#852) | 1–2 |
| List labels: number formats (decimal, letters, roman, ordinal text, Chinese/Japanese/Korean counting and legal, 一〇一 digits, 甲乙丙, ①…⑳, full-width), `%n` text such as `第%1章` / `（%1）`, legal numbering | ✅ | `doc::cjk_numbers` tests, `layout::tests::chinese_counting_list_labels_and_their_font_runs`; a label mixing faces (digit + CJK comma) is drawn run by run (#534). Hebrew, Arabic, Hindi, Thai, Vietnamese and Russian formats show decimal | — |
| Body top below a tall header, footer pushes body up | ✅ | #138; a footer taller than the bottom margin moves the body bottom up (#453), and a header or footer linked to the previous section takes its room too (#454) | — |
| Footnotes at the page bottom, endnotes at the end, note numbering | 🟡 | A long note continues onto the next page at a line boundary (its first line stays with the reference), first in that page's note area under a full-width continuation separator (#352); the document's own separator, continuation separator and continuation notice are drawn (the notice under a note that continues); a long endnote continues on the next column or page at a line boundary (#456), and the caret finds note text on its continuation page (#457); per-section restart partial | 1–3 |
| Body top below a tall header, footer pushes body up | ✅ | #138 | — |
| Footnotes at the page bottom, endnotes at the end, note numbering | 🟡 | A long note continues onto the next page at a line boundary (its first line stays with the reference), first in that page's note area under a full-width continuation separator (#352); the document's own separator, continuation separator and continuation notice are drawn (the notice under a note that continues; endnotes use the document's separator but don't continue onto a page of their own); each section's note options: footnotes below the text, endnotes at the end of each section, start at, numbering restarted each section or each page (counted from where the notes land, at most three passes) (#385) | 1–2 |
| Tables: row heights (at least/exact), rows split across pages, header rows repeat, Can't Split | ✅ | 2026-10-06, #138 | — |
| Tables: autofit to contents/window, fixed widths, Word 2013 edge | 🟡 | #137; #44: AutoFit Contents measures each column's narrowest and widest text and shares the width like Word, once (not live as you type); cell preferred widths and spans still differ | 3–5 |
| Floating tables (`w:tblpPr`) | 🟡 | #137; one placed on the page or margin that would cover the text before it and doesn't fit below it moves to the next page (#534); overlap rules untested | 2–3 |
| Nested tables | 🟡 | render; polish missing | 2–4 |
| Floating pictures/shapes placement (relative to page/margin/column/paragraph/line/character) | ✅ | #136 | — |
| Text wrap: square, top-and-bottom, behind, in front | ✅ | 2026-10-06 | — |
| Text wrap: tight, through (contour, wrap polygon) | 🟡 | #354: lines follow the wrap polygon, or the outline worked out from the object (shape geometry; a picture's opaque pixels as a convex hull; otherwise the rectangle), turned and flipped with it, kept the left/right distance from text; Through also uses gaps inside the outline. Wrap side (`wrapText`: both sides, left only, right only, largest side per line) applies to square, tight and through wraps, turned objects included; picture outlines are convex; not compared page-for-page with Word | 1–2 |
| Text boxes, overflow, linked text boxes | ✅ | #46; linked text boxes (#369): the text runs on through each box in turn at that box's line length, overflow past the last box hidden; Align Text top / middle / bottom; the box's own internal margins, and a box set to Resize shape to fit text shows all its text (#743) | — |
| Rotated objects and text | 🟡 | #332: pictures, shapes, charts and groups drawn turned and flipped about their centre (screen and PDF); square wrap and inline lines keep clear of the rotated bounds; hit testing on the turned shape. Text in a rotated text box stays upright | 2–3 |
| Page borders (from page edge or text), page colour, watermark | ✅ | | — |
| Line numbers (restart per page/section, count by) | ✅ | | — |
| Vertical page alignment | ✅ | | — |
| Gutter, mirror margins, book fold, 2 pages per sheet | 🟡 | gutter and mirror margins (even pages put the inside margin and gutter on the right, #452; page-relative floats move with the text); book fold missing | 2–3 |
| Document grid (`w:docGrid`, lines per page, characters per line) | ❌ | East Asian documents paginate differently without it | 4–6 |
| Vertical text (`tbRl`) in sections and text boxes; cell text direction | ✅ | cell text direction (#245); text-box text direction (#369); vertical sections (`tbRl`/`btLr`, #491): the page's text area turned, lines top to bottom stacking right to left, ideographs, kana and Hangul upright, Latin on its side, caret and clicks through the turn. Missing: OpenType `vert` alternates (punctuation is moved, not substituted), arrow keys along vertical lines, upright pictures take turned room | 2–4 |
| Gutter, mirror margins, book fold, 2 pages per sheet | 🟡 | gutter and mirror margins; book fold missing | 2–3 |
| Document grid (`w:docGrid`, lines per page, characters per line) | 🟡 | #391: line grid snaps line heights to whole pitches (text centred in them; `w:snapToGrid` off opts a paragraph out); character grid puts East Asian characters on the character pitch (`linesAndChars`) or all text (`snapToChars`), Latin text proportional; View › Gridlines draws it. Not yet: run-level `w:snapToGrid`, grid in headers/footers, paragraph spacing in grid lines, Word's exact glyph placement in a cell | 1–2 |
| Ruby / phonetic guide, enclose characters, combined characters | 🟡 | ruby (Phonetic Guide) landed (#288): ruby text over its base by alignment, line grows to fit, in PDF; enclose and combined characters missing; ruby not yet compared with Word page by page | 2–4 |
| Bidirectional paragraphs, mixed-direction lines; RTL tables and sections | 🟡 | #207; RTL tables (#362): grid columns from the right edge, start alignment and indent from the right margin, cell margins and left/right borders mirrored, merged and turned cells and nested tables included; RTL sections (#362): newspaper columns right to left with the line between mirrored. Right-hand page gutter (`w:rtlGutter`) missing | 1 |
| Equations: display layout, numbering, wrapping long display equations | ✅ | #191; long display equations break at top-level operators (`m:brkBin`, `m:brkBinSub`), manual breaks and `m:alnAt`, continuation lines indented by `m:wrapIndent` or set right (`m:wrapRight`) (#326) | — |
| Hidden text excluded from breaking and hyphenation | ✅ | #173 | — |
| Fields: PAGE, NUMPAGES, SECTIONPAGES, TOC page numbers | ✅ | #52; totals in the body need a second pass once the page count is known (#451) | — |
| Compatibility modes (Word 2003/2007/2010 layout for old files: `w:compat` options) | 🟡 | mode 15 behaviour; the ~60 legacy compat options are mostly ignored | 10–20 |
| Page-by-page comparison harness against Word | ❌ | Run Word locally on our own synthetic documents, export PDF, compare line breaks; Word output stays under `plan/` (never committed) | 8–12 |

## Performance

| Measure | WordCraft | Word | Kind |
|---|---|---|---|
| Cold layout, 188-page document | 61 ms | not measured | measured 2026-10-06 (not re-run this pass) |
| Relayout after an edit | 1.4 ms | not measured | measured 2026-10-06 |
| Font picker with many installed fonts | fixed freezes (#233) | instant | issue #31, #59 |
| Large real-world documents (500+ pages, many pictures) | not measured | | gap |
| Scrolling a 1,096-page document that is one 5,300-row table (#811) | ~2.4 ms idle, ~4.5 ms median scroll frame (was ~600 ms); opening still lays out for ~4–8 s | not measured | measured 2026-10-12, release, headless (`tests_perf::big_table_scroll_bench`) |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-12 | trivial | Spaces, tabs and ideographic spaces don't count toward a line's height, as in Word (a line of only them takes the paragraph mark's) |
| 2026-10-12 | trivial | A list label's font raises the first line's ascent but never its descent (Word keeps the text's descent below the baseline) |
| 2026-10-12 | trivial | Hidden end-of-cell mark (`w:hideMark`): a cell's empty last paragraph takes no room, spacing and all; a row of only such empty cells is just its borders |
| 2026-10-12 | trivial | Text frames in headers, footers, cells and text boxes are placed like floating objects with the text wrapping around them; body frames still lay out in line |
| 2026-10-12 | trivial | List indents and style indents combine like Word's: a numbered style's own indents (and those of styles based on it) beat the list level's, its base styles' don't, the paragraph's own list beats every style; a paragraph that switches its list off (`numId` 0) drops the styles' indents |
| 2026-10-12 | trivial | Keep with next: a paragraph that doesn't fit on the page anyway breaks as usual (its last lines go over with the next paragraph) instead of moving whole; a chain of kept paragraphs moves together |
| 2026-10-12 | trivial | Continuous section breaks: an empty paragraph carrying only the break takes no room (text after it follows on; balanced columns ignore it) |
| 2026-10-12 | trivial | Floats relative to a paragraph (or its line) are measured from the paragraph's start above its space before |
| 2026-10-12 | trivial | Text after a column break in unequal columns is broken for its new column's width |
| 2026-10-12 | trivial | Space before at the top of a page after page breaks, measured in Word for Windows in compatibility modes 12 and 15 (32 cases) |
| 2026-10-12 | trivial | No automatic (HTML-style) spacing between two list items, of one list or two, at any level; the first item keeps its automatic space before and the last its space after |
| 2026-10-12 | trivial | Kerning only where the document asks for it, and automatic (HTML-style) paragraph spacing, both measured in Word for Windows against real .gov documents |
| 2026-10-12 | trivial | A section break inserted over a selection keeps the selected text: the first section no longer comes out empty (#852) |
| 2026-10-12 | trivial | Large-table scrolling (#811): per-frame page fingerprinting fixed; measurement added |
| 2026-10-12 | trivial | Symbol-font text (Wingdings, Wingdings 2/3, Webdings, Symbol) drawn as Unicode symbols (#776) |
| 2026-10-11 | trivial | Chinese documents (#534): indents in characters, a page-anchored floating table after a full page moves to the next page, no blank page from a page break at the bottom of a full page |
| 2026-10-11 | trivial | East Asian list number formats (一、二、三, 第一章, ①); list labels whose characters come from two fonts draw each part with its own font (the digits of "1、" no longer show as other letters); a list level without indents leaves the label at the paragraph edge (#534) |
| 2026-10-11 | trivial | Vertical sections: `w:textDirection` in `w:sectPr`, upright East Asian glyphs (#491) |
| 2026-10-11 | trivial | Mirror indents (`w:mirrorIndents`): left/right indents swap on even pages; suppress line numbers inherited from styles (#512) |
| 2026-10-11 | trivial | NUMPAGES/SECTIONPAGES in the body show the real totals (the body is laid out again once the page count is known, at most three extra passes, #451); footnotes referenced in table cells go to the page bottom (#450); PDF links to a bookmark go to the page its start is on, not its paragraph's first page (#455) |
| 2026-10-11 | minor | Complex scripts: script itemisation, OpenType fallback fonts, syllable clusters, Myanmar syllable breaks (#604) |
| 2026-10-11 | trivial | Mirrored margins on even pages, tall and linked footers/headers push the body, long endnotes continue, caret in continued notes (#452, #453, #454, #456, #457) |
| 2026-10-11 | trivial | Hyphenation zone, words in capitals and consecutive-hyphen limit follow the document settings; Manual Hyphenation (#407) |
| 2026-10-11 | trivial | Document grid: line and character pitch (#391) |
| 2026-10-12 | trivial | Text boxes use their own internal margins; Resize shape to fit text never cuts text (#743) |
| 2026-10-11 | trivial | Linked text boxes, Align Text and text-box text direction (#369) |
| 2026-10-11 | trivial | Right-to-left tables and sections laid out mirrored (#362) |
| 2026-10-10 | trivial | The document's own note separators and continuation notice are drawn (#352) |
| 2026-10-11 | trivial | Newspaper columns balance before a continuous section break (#350) |
| 2026-10-11 | trivial | Group and Ungroup keep turned members' orientation and placement (#441, #443); Size resizes a group (#442); the eraser hits turned ink where it is drawn (#445) |
| 2026-10-10 | trivial | Long footnotes continue onto the next page with a continuation separator (#352) |
| 2026-10-11 | trivial | `EQ \o` fields overstrike their arguments; enclosed characters draw their circle, square, triangle or diamond as vectors (#297) |
| 2026-10-11 | trivial | Ruby (Phonetic Guide) layout landed (#288) |
| 2026-10-10 | trivial | Wrap side: text on both sides, left only, right only or the largest side of an object (#354) |
| 2026-10-10 | trivial | Tight and Through wrapping follow the object's outline (#354) |
| 2026-10-10 | trivial | Rotated and flipped objects laid out and drawn (#332) |
| 2026-10-11 | trivial | Footnotes below text, endnotes at the end of each section, note numbering restarted per section or page (#385) |
| 2026-10-10 | trivial | Long display equations wrap across lines (#326) |
| 2026-10-10 | trivial | AutoFit Contents measures the text (#44) |
| 2026-10-10 | major | First version: layout and pagination checklist with evidence from `crates/layout`, PRs and issues |
