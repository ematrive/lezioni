//! What the AI cost: `costi.json` in each lesson folder (processing, chat, cleanup of a piece)
//! and in each course folder (course summary). `generati/info.json` only keeps the last run.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ai::Usage;
use crate::library::{GENERATED_DIR, Library};

pub const COSTS_FILE: &str = "costi.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub when: DateTime<Utc>,
    /// "elaborazione", "chat", "ripulitura", "sintesi-corso"
    pub kind: String,
    pub model: String,
    pub usage: Usage,
    /// Made up from `generati/info.json` of a lesson processed before this file existed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub legacy: bool,
}

impl Entry {
    pub fn new(kind: &str, model: &str, usage: Usage) -> Self {
        Entry { when: Utc::now(), kind: kind.into(), model: model.into(), usage, legacy: false }
    }
}

/// The last processing of a lesson as recorded in `generati/info.json`, for lessons processed
/// before `costi.json` existed.
fn legacy(lesson_dir: &Path) -> Option<Entry> {
    let info: crate::pipeline::Info = serde_json::from_slice(&fs::read(lesson_dir.join(GENERATED_DIR).join("info.json")).ok()?).ok()?;
    Some(Entry { when: info.generated, kind: "elaborazione".into(), model: info.model, usage: info.usage, legacy: true })
}

/// The entries of a lesson (`lesson = true`, with the legacy one when there is no file yet) or
/// of a course folder.
pub fn read(dir: &Path, lesson: bool) -> Vec<Entry> {
    match fs::read(dir.join(COSTS_FILE)) {
        Ok(b) => serde_json::from_slice(&b).unwrap_or_else(|e| {
            log::warn!("{}: {e}", dir.join(COSTS_FILE).display());
            Vec::new()
        }),
        Err(_) if lesson => legacy(dir).into_iter().collect(),
        Err(_) => Vec::new(),
    }
}

/// Adds entries (those with no tokens are skipped). The caller holds the archive's write lock.
pub fn record(dir: &Path, lesson: bool, entries: impl IntoIterator<Item = Entry>) -> Result<()> {
    let new: Vec<Entry> = entries.into_iter().filter(|e| e.usage.prompt_token_count + e.usage.candidates_token_count > 0 || e.usage.cost.is_some_and(|c| c > 0.0)).collect();
    if new.is_empty() {
        return Ok(());
    }
    let mut all = read(dir, lesson);
    all.extend(new);
    crate::pipeline::write_atomic(dir, Path::new(COSTS_FILE), &serde_json::to_vec_pretty(&all)?)
}

/// Records what a job spent, model by model, in the folder of the lesson `lesson_id`.
pub fn record_lesson(lib: &Library, lesson_id: &str, kind: &str, spent: BTreeMap<String, Usage>) -> Result<()> {
    let dir = lib.lesson(lesson_id)?.path;
    record(&dir, true, spent.into_iter().map(|(m, u)| Entry::new(kind, &m, u)))
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LessonCost {
    pub lesson_id: String,
    pub title: String,
    pub date: String,
    pub total: f64,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MonthCost {
    /// "2026-11"
    pub month: String,
    pub total: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseCosts {
    /// Most recent lesson first, as in the lesson list.
    pub lessons: Vec<LessonCost>,
    /// The course summary.
    pub course: f64,
    /// Oldest month first.
    pub months: Vec<MonthCost>,
    pub total: f64,
    /// Some entries had no real cost (Gemini): estimated at the list price.
    pub estimated: bool,
}

/// Totals of a course, by lesson and by month (local time).
pub fn course(lib: &Library, course_id: &str) -> Result<CourseCosts> {
    let course_dir: PathBuf = lib.course(course_id)?.path;
    let mut months: BTreeMap<String, f64> = BTreeMap::new();
    let mut estimated = false;
    let mut add = |entries: &[Entry]| {
        let mut sum = 0.0;
        for e in entries {
            estimated |= e.usage.cost.is_none();
            let c = e.usage.cost_or_estimate();
            sum += c;
            *months.entry(e.when.with_timezone(&chrono::Local).format("%Y-%m").to_string()).or_default() += c;
        }
        sum
    };
    let mut lessons = Vec::new();
    for l in lib.lessons(course_id)? {
        let total = add(&read(&l.path, true));
        lessons.push(LessonCost { lesson_id: l.meta.id, title: l.meta.title, date: l.meta.date.to_string(), total });
    }
    let course = add(&read(&course_dir, false));
    let total = lessons.iter().map(|l| l.total).sum::<f64>() + course;
    Ok(CourseCosts { lessons, course, months: months.into_iter().map(|(month, total)| MonthCost { month, total }).collect(), total, estimated })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(p: u64, c: Option<f64>) -> Usage {
        Usage { prompt_token_count: p, candidates_token_count: 10, thoughts_token_count: 0, cost: c }
    }

    #[test]
    fn entries_add_up_and_keep_the_old_processing() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = Library::open(tmp.path()).unwrap();
        let c = lib.create_course("Idraulica", "", "").unwrap();
        let l = lib.create_lesson(&c.meta.id, "Moto vario", chrono::NaiveDate::from_ymd_opt(2026, 11, 3).unwrap()).unwrap();
        // Processed before costi.json existed: only info.json.
        let info = crate::pipeline::Info { generated: Utc::now(), model: "m".into(), usage: usage(1000, Some(0.03)), suggested_title: String::new(), sources: Default::default(), edited: None };
        fs::create_dir_all(l.path.join(GENERATED_DIR)).unwrap();
        fs::write(l.path.join(GENERATED_DIR).join("info.json"), serde_json::to_vec(&info).unwrap()).unwrap();
        assert_eq!(read(&l.path, true).len(), 1);
        // The first chat must not make the old processing disappear.
        record_lesson(&lib, &l.meta.id, "chat", [("m".to_string(), usage(100, Some(0.01)))].into()).unwrap();
        // Nothing spent: nothing written.
        record_lesson(&lib, &l.meta.id, "chat", [("m".to_string(), Usage::default())].into()).unwrap();
        let e = read(&l.path, true);
        assert_eq!(e.len(), 2);
        assert!(e[0].legacy && !e[1].legacy);
        // Course summary with Gemini (no real cost): estimated.
        record(&c.path, false, [Entry::new("sintesi-corso", "gemini", usage(1_000_000, None))]).unwrap();
        let t = course(&lib, &c.meta.id).unwrap();
        assert!((t.lessons[0].total - 0.04).abs() < 1e-9);
        assert!(t.estimated);
        assert!((t.course - (0.75 + 10.0 * 3.75 / 1e6)).abs() < 1e-9);
        assert!((t.total - t.course - 0.04).abs() < 1e-9);
        assert!((t.months.iter().map(|m| m.total).sum::<f64>() - t.total).abs() < 1e-9);
    }
}
