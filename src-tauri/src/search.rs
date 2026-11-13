//! Full-text search over the archive. The archive is small (tens or hundreds of lessons, a few
//! MB of text), so the files are read at each search: no index to keep in sync with a folder
//! that Drive or the user can change at any time. Accents and case are ignored; every word of
//! the query must be in the same document.
use std::fs;
use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::library::{GENERATED_DIR, Library};
use crate::pipeline::extract_rel;

pub const MAX_HITS: usize = 200;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub course_id: String,
    pub course: String,
    pub lesson_id: String,
    pub lesson: String,
    pub date: String,
    /// Where: "title", "notes", "summary", "study", "extract".
    pub area: String,
    /// The source, for extracts.
    pub source_id: Option<String>,
    pub label: String,
    pub snippet: String,
    /// Seconds of the nearest `[hh:mm:ss]` before the match (transcripts).
    pub time: Option<u64>,
    pub score: usize,
}

/// Lowercase without accents, one char per input char (so byte offsets can be mapped back
/// through char indices).
fn fold(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ä' | 'À' | 'Á' | 'Â' | 'Ä' => 'a',
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => 'i',
        'ò' | 'ó' | 'ô' | 'ö' | 'Ò' | 'Ó' | 'Ô' | 'Ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => 'u',
        _ => c.to_lowercase().next().unwrap_or(c),
    }
}

fn folded(s: &str) -> Vec<char> {
    s.chars().map(fold).collect()
}

fn find(hay: &[char], needle: &[char]) -> Vec<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return Vec::new();
    }
    (0..=hay.len() - needle.len()).filter(|&i| hay[i..i + needle.len()] == *needle).collect()
}

/// Last `[hh:mm:ss]` before char index `at`.
fn time_before(chars: &[char], at: usize) -> Option<u64> {
    let s: String = chars[..at].iter().collect();
    let i = s.rfind('[')?;
    let inner = s[i + 1..].split(']').next()?;
    let p: Vec<u64> = inner.split(':').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    match p.as_slice() {
        [h, m, s] => Some(h * 3600 + m * 60 + s),
        [m, s] => Some(m * 60 + s),
        _ => None,
    }
}

/// Text around the match without Markdown marks and `[hh:mm:ss]` (the time is shown apart).
fn snippet(chars: &[char], at: usize, len: usize) -> String {
    static TIMES: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"\[\d{1,2}:\d{2}(?::\d{2})?\]").unwrap());
    let from = at.saturating_sub(80);
    let to = (at + len + 120).min(chars.len());
    let raw: String = chars[from..to].iter().collect();
    let raw = TIMES.replace_all(&raw, "");
    let mut s: String = raw.chars().filter(|c| !"#*`$_>|".contains(*c)).collect();
    s = s.split_whitespace().filter(|w| *w != "-").collect::<Vec<_>>().join(" ");
    format!("{}{}{}", if from > 0 { "…" } else { "" }, s.trim(), if to < chars.len() { "…" } else { "" })
}

pub struct Query {
    terms: Vec<Vec<char>>,
}

impl Query {
    pub fn new(q: &str) -> Self {
        Self { terms: q.split_whitespace().filter(|t| t.chars().count() >= 2).map(folded).collect() }
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    /// (score, char index of the first match) if every term is in `text`.
    fn matches(&self, text: &str) -> Option<(Vec<char>, usize, usize)> {
        let hay = folded(text);
        let mut score = 0;
        let mut first = usize::MAX;
        for t in &self.terms {
            let at = find(&hay, t);
            if at.is_empty() {
                return None;
            }
            score += at.len();
            first = first.min(at[0]);
        }
        Some((text.chars().collect(), first, score))
    }
}

fn read(p: &Path) -> Option<String> {
    fs::read_to_string(p).ok()
}

pub fn search(lib: &Library, q: &str) -> Result<Vec<Hit>> {
    let q = Query::new(q);
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let mut hits = Vec::new();
    for c in lib.courses()? {
        for l in lib.lessons(&c.meta.id)? {
            let base = |area: &str, label: String, source_id: Option<String>, text: &str, times: bool| {
                q.matches(text).map(|(chars, at, score)| Hit {
                    course_id: c.meta.id.clone(),
                    course: c.meta.name.clone(),
                    lesson_id: l.meta.id.clone(),
                    lesson: l.meta.title.clone(),
                    date: l.meta.date.to_string(),
                    area: area.into(),
                    source_id,
                    label,
                    snippet: snippet(&chars, at, q.terms[0].len()),
                    time: if times { time_before(&chars, at) } else { None },
                    score,
                })
            };
            let gdir = l.path.join(GENERATED_DIR);
            let mut found = vec![
                base("title", crate::tr!("Titolo", "Title"), None, &l.meta.title, false),
                base("notes", crate::tr!("Note", "Notes"), None, &l.meta.notes, false),
                read(&gdir.join("sintesi.md")).and_then(|t| base("summary", crate::tr!("Sintesi", "Summary"), None, &t, false)),
                read(&gdir.join("studio.json")).and_then(|t| base("study", crate::tr!("Studio", "Study"), None, &t, false)),
            ];
            for s in &l.meta.sources {
                let media = matches!(s.kind, crate::library::SourceKind::Audio | crate::library::SourceKind::Video);
                found.push(read(&l.path.join(extract_rel(s))).and_then(|t| base("extract", s.original_name.clone(), Some(s.id.clone()), &t, media)));
            }
            hits.extend(found.into_iter().flatten());
        }
    }
    // Titles first, then by number of occurrences, then the most recent lesson.
    hits.sort_by(|a, b| (b.area == "title").cmp(&(a.area == "title")).then(b.score.cmp(&a.score)).then(b.date.cmp(&a.date)));
    hits.truncate(MAX_HITS);
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_across_lessons() {
        let t = tempfile::tempdir().unwrap();
        let lib = Library::open(t.path()).unwrap();
        let c = lib.create_course("Economia", "", "").unwrap();
        let l1 = lib.create_lesson(&c.meta.id, "Valore attuale netto", "2026-11-03".parse().unwrap()).unwrap();
        let l2 = lib.create_lesson(&c.meta.id, "Break-even", "2026-11-10".parse().unwrap()).unwrap();
        let wav = t.path().join("reg.m4a");
        fs::write(&wav, b"x").unwrap();
        let (l2, _) = lib.add_files(&l2.meta.id, &[wav]).unwrap();
        fs::write(l2.path.join(extract_rel(&l2.meta.sources[0])), "[00:00:05] Introduzione.\n\n[00:12:30] Il punto di pareggio è dove ricavi e costi si eguagliano. Il VAN qui non serve.").unwrap();
        fs::write(l1.path.join("generati/sintesi.md"), "## VAN\nIl valore attuale netto (VAN) attualizza i flussi di cassa.").unwrap();

        let h = search(&lib, "van").unwrap();
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].area, "summary", "two occurrences first");
        let tr = h.iter().find(|x| x.area == "extract").unwrap();
        assert_eq!(tr.time, Some(750));
        assert!(tr.snippet.contains("Il VAN qui non serve"), "{}", tr.snippet);
        assert!(!tr.snippet.contains("[00:12:30]"), "{}", tr.snippet);
        assert!(h[0].snippet.starts_with("VAN Il valore"), "{}", h[0].snippet);

        // Accents and case ignored; all words must be there.
        assert_eq!(search(&lib, "PAREGGIO costi").unwrap().len(), 1);
        assert_eq!(search(&lib, "pareggio attualizza").unwrap().len(), 0);
        assert_eq!(search(&lib, "è").unwrap().len(), 0, "one-letter words ignored");
        assert_eq!(search(&lib, "valore").unwrap()[0].area, "title");
        lib.update_lesson(&l1.meta.id, crate::library::LessonPatch { notes: Some("Ripassare la città".into()), ..Default::default() }).unwrap();
        assert_eq!(search(&lib, "citta").unwrap()[0].area, "notes");
    }
}
