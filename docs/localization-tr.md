# Turkish interface

> **Last reviewed:** 2026-10-11 · **Last updated:** 2026-10-11 · **Change:** minor (Turkish interface catalog completed, 2,325 entries, #774; previously minor: Turkish interface catalog and regression checks) · **Target:** WordCraft 0.5.0

Choose **File › Options › Interface language › Türkçe**, or follow a Turkish system display language
(`tr-TR`, `tr-CY`, `tr_TR.UTF-8`). The catalog code is `tr`; `ui.language` with `{"value":"tr"}`
selects it explicitly, and `WORDCRAFT_LANGUAGE=tr-TR` does the same for a headless screenshot or a
launch while the preference is `auto`. The choice is saved with the other interface settings.
Document text, command ids, CLI/MCP/control responses and file paths do not change.

`crates/ui-egui/src/i18n/tr.tsv` has all 2,325 entries of the Russian catalog (the same
keys). It covers all ribbon commands, dialogs, built-in style names, equation and math gallery
structures, watermark presets, paper sizes, and rendered preview tiles. It is written from the
meaning of the English labels; no proprietary translation resources were consulted. The catalog
format has no plural forms, and Turkish doesn't need them here: a noun after a number stays
singular (`0 sözcük`, `1 sözcük`, `21 sözcük`). Placeholders, `\n` breaks in ribbon labels and dialog
ellipses are kept, and strings added later fall back to English until they are translated.

## Terminology

| Source | Turkish |
|---|---|
| Font / Paragraph | Yazı Tipi / Paragraf |
| Review / Track Changes | İncele / Değişiklikleri İzle |
| References / Citation / Bibliography | Başvurular / Atıf / Kaynakça |
| Header / Footer / Heading | Üstbilgi / Altbilgi / Başlık |
| Caption / Table of Contents | Resim Yazısı / İçindekiler Tablosu |
| Mail Merge / Start Mail Merge | Posta Birleştirme / Posta Birleştirmeyi Başlat |
| Symbol / Icons | Sembol / Simgeler |
| Undo / Redo / Repeat | Geri Al / Yinele / Tekrarla |
| Undo {action} | Geri Al: {action} |
| Page {page} of {pages} | Sayfa {page} / {pages} |
| Proofing / Word Count | Dil Denetimi / Sözcük Sayısı |
| pt (unit) | pt |

Product and format names stay as they are. Commands use the imperative. Turkish `İ/i` and `I/ı`
are written directly in the labels; code never uppercases a translated label, except the Font
dialog's preview sample, which therefore has no lowercase `i` (`AaBbYyZz ĞğİıŞş` uppercases to
`AABBYYZZ ĞĞİIŞŞ`).

Command search folds translated labels and ribbon locations with the Turkish `İ/i` and `I/ı`
pairs, so `italik`, `içindekiler` and `YAZI TİPİ` find their commands. English labels, locations and
ASCII command ids keep the default case-insensitive match, so `INSERT.TABLE` and `FORMAT.ITALIC`
still work. Other interface languages and document search are unchanged.

## Verification and remaining scope

`cargo test -p wordcraft-ui-egui --lib i18n::tests` covers Turkish locale tags and preference order,
saving and restoring `tr`, unknown preferences, unchanged document content, full key parity with
the Russian catalog (and superset of Spanish and Chinese), placeholders, count wording, the font
sample's capitals, English fallback, and the glyphs `ÇçĞğİıÖöŞşÜü` in the bundled Inter (Regular,
Medium, SemiBold) and JetBrains Mono. A dialog test renders real command-search results in Turkish
and English, including dotted and dotless I and ASCII command ids. The shared catalog checks
(validation, every tab and command translated, and interface audit strings) include Turkish
automatically.

Turkish proofing (`crates/proof/src/tr`) provides spelling against installed Hunspell `tr_TR`
dictionaries, spelling suggestions tuned for Turkish diacritics and detached particles, rule-based grammar
checks (reduplications, conjunction commas, capitalization), and syllable hyphenation by TDK rules.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-13 | minor | Turkish proofing added: spelling, suggestions, grammar rules and TDK syllable hyphenation |
| 2026-10-11 | minor | Turkish catalog completed (2,325 entries, #774), covering all math structures, built-in styles, presets and preview tiles |
| 2026-10-11 | minor | Turkish catalog (1,307 entries), terminology, scope and regression checks |
