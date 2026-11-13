//! Spaced review of the flashcards (Leitner boxes), per lesson or over a whole course. The
//! progress is in `ripasso.json` of each lesson, keyed by the question: Rigenera keeps it for the
//! cards that stay the same. Personal: it does not go in packages.
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Result, anyhow};
use chrono::{Days, NaiveDate};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::library::{Lesson, Library};

pub const REVIEW_FILE: &str = "ripasso.json";
/// Days until the next review, by box: a card known every time goes 1, 3, 7, 14, 30 days on.
const INTERVALS: [u64; 6] = [0, 1, 3, 7, 14, 30];
/// Cards never seen that join a session: a course of 20 lessons would start with 300.
pub const NEW_PER_SESSION: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CardState {
    #[serde(rename = "box")]
    pub level: u8,
    pub due: NaiveDate,
    pub seen: u32,
    pub wrong: u32,
}

/// Stable key of a card: its question, ignoring case and spacing.
pub fn key(question: &str) -> String {
    let norm = question.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    Sha256::digest(norm.as_bytes()).iter().take(8).map(|b| format!("{b:02x}")).collect()
}

fn read(lesson_dir: &Path) -> BTreeMap<String, CardState> {
    fs::read(lesson_dir.join(REVIEW_FILE)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DueCard {
    pub lesson_id: String,
    pub lesson_title: String,
    pub key: String,
    pub domanda: String,
    pub risposta: String,
    /// None = never seen.
    #[serde(rename = "box")]
    pub level: Option<u8>,
}

#[derive(Debug, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Due {
    /// Seen before and due today (oldest due first), then at most `NEW_PER_SESSION` new ones.
    pub cards: Vec<DueCard>,
    pub due: usize,
    /// All the new cards, also those beyond today's.
    pub new: usize,
}

/// The cards to review today in some lessons (oldest lesson first for the new ones).
fn due_in(mut lessons: Vec<Lesson>, today: NaiveDate) -> Due {
    lessons.sort_by_key(|l| (l.meta.date, l.meta.created));
    let mut old: Vec<(NaiveDate, DueCard)> = Vec::new();
    let mut new: Vec<DueCard> = Vec::new();
    for l in &lessons {
        let Some(study) = crate::pipeline::outputs_study(l) else { continue };
        let states = read(&l.path);
        let mut keys = std::collections::HashSet::new();
        for c in study.flashcard {
            let k = key(&c.domanda);
            if !keys.insert(k.clone()) {
                continue;
            }
            let state = states.get(&k);
            let card = DueCard { lesson_id: l.meta.id.clone(), lesson_title: l.meta.title.clone(), key: k, domanda: c.domanda, risposta: c.risposta, level: state.map(|s| s.level) };
            match state {
                Some(s) if s.due <= today => old.push((s.due, card)),
                Some(_) => {}
                None => new.push(card),
            }
        }
    }
    old.sort_by_key(|(d, _)| *d);
    let (due, new_total) = (old.len(), new.len());
    let mut cards: Vec<DueCard> = old.into_iter().map(|(_, c)| c).collect();
    cards.extend(new.into_iter().take(NEW_PER_SESSION));
    Due { cards, due, new: new_total }
}

pub fn due_lesson(lib: &Library, lesson_id: &str, today: NaiveDate) -> Result<Due> {
    Ok(due_in(vec![lib.lesson(lesson_id)?], today))
}

pub fn due_course(lib: &Library, course_id: &str, today: NaiveDate) -> Result<Due> {
    Ok(due_in(lib.lessons(course_id)?, today))
}

/// Records an answer: known → one box up, not known → back to the first box (due today).
/// The caller holds the archive's write lock.
pub fn answer(lib: &Library, lesson_id: &str, card_key: &str, knew: bool, today: NaiveDate) -> Result<CardState> {
    let l = lib.lesson(lesson_id)?;
    let study = crate::pipeline::outputs_study(&l).ok_or_else(|| anyhow!(crate::tr!("la lezione non ha flashcard", "the lecture has no flashcards")))?;
    if !study.flashcard.iter().any(|c| key(&c.domanda) == card_key) {
        return Err(anyhow!(crate::tr!("flashcard non trovata: forse è stata corretta o rigenerata", "flashcard not found: it may have been corrected or generated again")));
    }
    let mut states = read(&l.path);
    let s = states.entry(card_key.to_string()).or_insert(CardState { level: 0, due: today, seen: 0, wrong: 0 });
    s.level = if knew { (s.level + 1).min(INTERVALS.len() as u8 - 1) } else { 0 };
    s.due = today + Days::new(INTERVALS[s.level as usize]);
    s.seen += 1;
    if !knew {
        s.wrong += 1;
    }
    let out = s.clone();
    crate::pipeline::write_atomic(&l.path, Path::new(REVIEW_FILE), &serde_json::to_vec_pretty(&states)?)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{Card, Study};

    fn lesson_with_cards(lib: &Library, course: &str, date: u32, cards: &[&str]) -> Lesson {
        let l = lib.create_lesson(course, &format!("L{date}"), NaiveDate::from_ymd_opt(2026, 11, date).unwrap()).unwrap();
        let study = Study { glossario: vec![], flashcard: cards.iter().map(|q| Card { domanda: q.to_string(), risposta: "r".into() }).collect(), domande_esame: vec![] };
        fs::create_dir_all(l.path.join("generati")).unwrap();
        fs::write(l.path.join("generati/studio.json"), serde_json::to_vec(&study).unwrap()).unwrap();
        l
    }

    #[test]
    fn boxes_intervals_and_stable_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = Library::open(tmp.path()).unwrap();
        let c = lib.create_course("RO", "", "").unwrap();
        let l = lesson_with_cards(&lib, &c.meta.id, 3, &["Cos'è una base?", "Cos'è il pivot?"]);
        let day = NaiveDate::from_ymd_opt(2026, 11, 11).unwrap();
        let d = due_lesson(&lib, &l.meta.id, day).unwrap();
        assert_eq!((d.due, d.new, d.cards.len()), (0, 2, 2));
        let k = d.cards[0].key.clone();
        assert_eq!(k, key("  cos'è UNA  base? "));
        // Known: tomorrow, then 3 days on. Not known: back to today.
        assert_eq!(answer(&lib, &l.meta.id, &k, true, day).unwrap().due, day + Days::new(1));
        assert_eq!(due_lesson(&lib, &l.meta.id, day).unwrap().cards.len(), 1);
        let s = answer(&lib, &l.meta.id, &k, true, day + Days::new(1)).unwrap();
        assert_eq!((s.level, s.due), (2, day + Days::new(4)));
        let s = answer(&lib, &l.meta.id, &k, false, day + Days::new(4)).unwrap();
        assert_eq!((s.level, s.due, s.seen, s.wrong), (0, day + Days::new(4), 3, 1));
        let d = due_lesson(&lib, &l.meta.id, day + Days::new(4)).unwrap();
        assert_eq!((d.due, d.new), (1, 1));
        assert_eq!(d.cards[0].level, Some(0), "seen cards first");
        // Rigenera with the same question (other case): the progress stays.
        let study = Study { glossario: vec![], flashcard: vec![Card { domanda: "COS'È UNA BASE?".into(), risposta: "nuova".into() }], domande_esame: vec![] };
        fs::write(l.path.join("generati/studio.json"), serde_json::to_vec(&study).unwrap()).unwrap();
        let d = due_lesson(&lib, &l.meta.id, day + Days::new(4)).unwrap();
        assert_eq!((d.due, d.new, d.cards[0].risposta.as_str()), (1, 0, "nuova"));
        assert!(answer(&lib, &l.meta.id, "nessuna", true, day).is_err());
    }

    #[test]
    fn a_course_mixes_lessons_and_limits_the_new_cards() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = Library::open(tmp.path()).unwrap();
        let c = lib.create_course("RO", "", "").unwrap();
        let qs: Vec<String> = (0..15).map(|i| format!("domanda {i}")).collect();
        let qs: Vec<&str> = qs.iter().map(String::as_str).collect();
        let late = lesson_with_cards(&lib, &c.meta.id, 10, &qs);
        let early = lesson_with_cards(&lib, &c.meta.id, 3, &qs);
        let day = NaiveDate::from_ymd_opt(2026, 11, 11).unwrap();
        answer(&lib, &late.meta.id, &key("domanda 0"), false, day).unwrap();
        let d = due_course(&lib, &c.meta.id, day).unwrap();
        assert_eq!((d.due, d.new), (1, 29));
        assert_eq!(d.cards.len(), 1 + NEW_PER_SESSION);
        assert_eq!(d.cards[0].lesson_id, late.meta.id, "the one to see again first");
        assert_eq!(d.cards[1].lesson_id, early.meta.id, "then new cards of the oldest lesson");
        assert_eq!(d.cards.last().unwrap().lesson_id, late.meta.id);
    }
}
