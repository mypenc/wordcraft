//! Italian grammar and punctuation checks: cheap, rule-based and tuned to stay quiet when
//! unsure (a missed mistake costs less than a wrong mark on correct text). Messages are in
//! Italian.
//!
//! - **Repeated word** (`la la`), except words Italian doubles on purpose (`piano piano`,
//!   `sì sì`, interjections).
//! - **Space before punctuation** (`bene ,`) and **double spaces**; emoticons (` :)`) and
//!   ellipses are left alone.
//! - **Capital letter at the start of a sentence**, knowing Italian abbreviations (`ecc.`,
//!   `dott.`, `prof.`, `sig.`), initials, ordinal numbers (`1. posto`) and ellipses.
//! - **Doubled punctuation** (`,,`, `..`, `;;`).
//!
//! No comma rule and no English rule (such as `a`/`an`) applies to Italian text.

use crate::{Issue, IssueKind, ProofLang, words};

/// Words Italian repeats on purpose: emphatic doubling, interjections, onomatopoeia.
const ALLOWED_REPEATS: &[&str] = &[
    "sì", "no", "piano", "bene", "male", "ecco", "oh", "ah", "eh", "mh", "bah", "ehi", "urrà", "evviva", "ciao", "addio", "adagio", "forte", "lento",
    "chiaro",
];

/// Abbreviations written with a final period after which a sentence goes on in lower case
/// (lower case, without the period).
const ABBREVIATIONS: &[&str] = &[
    "ecc",
    "es",
    "dott",
    "prof",
    "profssa",
    "ing",
    "avv",
    "sig",
    "sigg",
    "sigra",
    "sigr",
    "geom",
    "rag",
    "dottssa",
    "avvoc",
    "pag",
    "pagg",
    "tel",
    "art",
    "artt",
    "fig",
    "figg",
    "vol",
    "voll",
    "ed",
    "sez",
    "cap",
    "capp",
    "par",
    "n",
    "num",
    "vs",
    "cfr",
    "cfrt",
    "op",
    "cit",
    "ibid",
    "sez",
    "all",
    "tav",
    "pag",
    "ditta",
    "soc",
    "spa",
    "srl",
    "on",
    "sen",
    "spett",
    "eg",
    "ca",
    "v",
    "vv",
    "sec",
    "min",
    "max",
    "lun",
    "mar",
    "mer",
    "gio",
    "ven",
    "sab",
    "dom",
    "gen",
    "feb",
    "mar",
    "apr",
    "mag",
    "giu",
    "lug",
    "ago",
    "set",
    "ott",
    "nov",
    "dic",
    "genn",
    "febb",
    "sett",
    "dottor",
    "professor",
    "cav",
    "commend",
    "mons",
    "fr",
    "suor",
    "s",
    "ss",
    "b",
    "beato",
];

fn issue(start: usize, end: usize, message: String, suggestions: Vec<String>) -> Issue {
    Issue { start, end, kind: IssueKind::Grammar, message, suggestions, lang: ProofLang::It }
}

/// Is `prev`, before a period, an abbreviation, an initial, a number or a Roman numeral (after
/// which a lower-case word doesn't start a sentence)?
fn abbreviation_or_number(prev: &str) -> bool {
    prev.chars().count() <= 1
        || prev.chars().any(|c| c.is_ascii_digit())
        || (prev.chars().all(|c| matches!(c, 'I' | 'V' | 'X' | 'L' | 'C' | 'D' | 'M')))
        || ABBREVIATIONS.contains(&prev.to_lowercase().as_str())
}

/// Does the gap between two words end a sentence: a `.`, `!` or `?` (not an ellipsis, not after
/// an abbreviation), then space, with nothing but quotes and brackets around?
fn ends_sentence(gap: &str, prev: &str) -> bool {
    let t = gap.trim_start_matches(['"', '”', '’', '\'', ')', '»', ']', '\u{1}']);
    let Some(end) = t.chars().next() else { return false };
    if !matches!(end, '.' | '!' | '?') || t.starts_with("..") {
        return false;
    }
    if end == '.' && abbreviation_or_number(prev) {
        return false;
    }
    let rest = t.trim_start_matches(['.', '!', '?']).trim_start_matches(['"', '”', '’', '\'', ')', '»', ']']);
    rest.starts_with(char::is_whitespace) && rest.trim_start().chars().all(|c| matches!(c, '"' | '“' | '«' | '(' | '[' | '\'' | '—'))
}

fn repeated_words(text: &str, ws: &[(usize, usize)], v: &mut Vec<Issue>) {
    for pair in ws.windows(2) {
        let [(a0, b0), (a1, b1)] = pair else { continue };
        let (Some(w0), Some(w1), Some(gap)) = (text.get(*a0..*b0), text.get(*a1..*b1), text.get(*b0..*a1)) else { continue };
        if !gap.is_empty()
            && gap.chars().all(char::is_whitespace)
            && w0.chars().all(char::is_alphabetic)
            && w0.to_lowercase() == w1.to_lowercase()
            && !ALLOWED_REPEATS.contains(&w0.to_lowercase().as_str())
        {
            v.push(issue(*a0, *b1, format!("Parola ripetuta: \"{w1}\""), vec![w0.to_string()]));
        }
    }
}

fn spaces(text: &str, v: &mut Vec<Issue>) {
    for (i, c) in text.char_indices() {
        if c != ' ' {
            continue;
        }
        let rest = text.get(i + 1..).unwrap_or("");
        let mut it = rest.chars();
        let Some(p) = it.next() else { continue };
        let after = it.next();
        let flag = match p {
            ',' => true,
            // `;)` and `;-)` are emoticons.
            ';' => !matches!(after, Some(')' | '(' | '-' | 'P' | 'D')),
            // ` ...` is an ellipsis, ` .5` a number.
            '.' => !matches!(after, Some('.')) && !after.is_some_and(|a| a.is_ascii_digit()),
            // ` :)`, ` :-)`, ` :D` and similar are emoticons; ` ?!` is still a stray space.
            ':' | '!' | '?' => after.is_none_or(|a| a.is_whitespace() || matches!(a, '"' | '”' | '’' | '!' | '?')),
            _ => false,
        };
        if flag {
            v.push(issue(i, i + 1 + p.len_utf8(), "Spazio prima della punteggiatura".into(), vec![p.to_string()]));
        }
    }
    for (i, _) in text.match_indices("  ") {
        if i > 0 && text.get(..i).is_some_and(|t| t.ends_with(' ')) {
            continue;
        }
        v.push(issue(i, i + 2, "Spazio doppio".into(), vec![" ".into()]));
    }
}

fn sentence_capitals(text: &str, ws: &[(usize, usize)], v: &mut Vec<Issue>) {
    let mut after_end = true;
    for (a, b) in ws {
        let Some(w) = text.get(*a..*b) else { continue };
        if after_end && w.chars().next().is_some_and(char::is_lowercase) && w.chars().all(char::is_alphabetic) {
            let fixed: String = w.chars().next().map(|f| f.to_uppercase().chain(w.chars().skip(1)).collect()).unwrap_or_default();
            v.push(issue(*a, *b, "Maiuscola a inizio frase".into(), vec![fixed]));
        }
        after_end = ends_sentence(text.get(*b..).unwrap_or(""), w);
    }
}

fn doubled_punctuation(text: &str, v: &mut Vec<Issue>) {
    for pat in [",,", "..", ";;", "::", "??", "!!"] {
        for (i, _) in text.match_indices(pat) {
            if pat == ".." && text.get(i + 2..).is_some_and(|r| r.starts_with('.')) {
                continue; // ellipsis
            }
            v.push(issue(i, i + pat.len(), "Punteggiatura doppia".into(), vec![pat[..1].into()]));
        }
    }
}

/// Grammar issues in a paragraph of Italian text, in order.
pub fn check(text: &str) -> Vec<Issue> {
    let ws: Vec<(usize, usize)> = words(text);
    let mut v = Vec::new();
    repeated_words(text, &ws, &mut v);
    spaces(text, &mut v);
    sentence_capitals(text, &ws, &mut v);
    doubled_punctuation(text, &mut v);
    v.sort_by_key(|i| (i.start, i.end));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn italian_grammar_flags() {
        test_dict_install();
        let t = "questo questo è un test ,  fatto male.. Vedi dott. rossi";
        let v = check(t);
        let msgs: Vec<&str> = v.iter().map(|i| i.message.as_str()).collect();
        assert!(msgs.iter().any(|m| m.contains("ripetuta")), "{msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("prima della punteggiatura")), "{msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("doppio")), "{msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("doppia")), "{msgs:?}");
        assert!(v.iter().all(|i| i.lang == ProofLang::It), "{v:?}");
    }

    #[test]
    fn italian_grammar_stays_quiet() {
        test_dict_install();
        assert!(
            check("Vado a Roma. Il dott. Rossi è già lì... piano piano!").is_empty(),
            "{:?}",
            check("Vado a Roma. Il dott. Rossi è già lì... piano piano!")
        );
        assert!(check("Sì sì, bene bene :)").is_empty());
    }

    fn test_dict_install() {
        crate::it::test_dict::install();
    }
}
