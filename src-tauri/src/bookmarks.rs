//! Bookmarks set while listening to a recording ("important", "didn't get it", a note), in
//! `segnaposti.json` of the lesson. They are the student's own notes: they travel in packages,
//! and the summary gives those points more room.
use std::fs;
use std::path::Path;

use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::library::Library;

pub const BOOKMARKS_FILE: &str = "segnaposti.json";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Importante,
    Dubbio,
    Nota,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Importante => "importante",
            Kind::Dubbio => "non capito",
            Kind::Nota => "nota",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    pub id: String,
    pub source_id: String,
    /// Seconds from the start of the recording.
    pub at: f64,
    pub kind: Kind,
    #[serde(default)]
    pub note: String,
    pub created: DateTime<Utc>,
}

/// Bookmarks of a lesson folder, in recording order.
pub fn read(lesson_dir: &Path) -> Vec<Bookmark> {
    let p = lesson_dir.join(BOOKMARKS_FILE);
    let mut v: Vec<Bookmark> = match fs::read(&p) {
        Ok(b) => serde_json::from_slice(&b).unwrap_or_else(|e| {
            log::warn!("{}: {e}", p.display());
            Vec::new()
        }),
        Err(_) => Vec::new(),
    };
    v.sort_by(|a, b| a.source_id.cmp(&b.source_id).then(a.at.total_cmp(&b.at)));
    v
}

fn write(lesson_dir: &Path, v: &[Bookmark]) -> Result<()> {
    if v.is_empty() {
        let p = lesson_dir.join(BOOKMARKS_FILE);
        if p.exists() {
            fs::remove_file(p)?;
        }
        return Ok(());
    }
    crate::pipeline::write_atomic(lesson_dir, Path::new(BOOKMARKS_FILE), &serde_json::to_vec_pretty(v)?)
}

pub fn list(lib: &Library, lesson_id: &str) -> Result<Vec<Bookmark>> {
    Ok(read(&lib.lesson(lesson_id)?.path))
}

/// The caller holds the archive's write lock.
pub fn add(lib: &Library, lesson_id: &str, source_id: &str, at: f64, kind: Kind, note: &str) -> Result<Vec<Bookmark>> {
    let l = lib.lesson(lesson_id)?;
    if !l.meta.sources.iter().any(|s| s.id == source_id) {
        bail!("fonte non trovata");
    }
    if !at.is_finite() || at < 0.0 {
        bail!("minuto non valido");
    }
    let mut v = read(&l.path);
    v.push(Bookmark { id: uuid::Uuid::new_v4().to_string(), source_id: source_id.into(), at, kind, note: note.trim().into(), created: Utc::now() });
    write(&l.path, &v)?;
    Ok(read(&l.path))
}

pub fn update(lib: &Library, lesson_id: &str, id: &str, kind: Kind, note: &str) -> Result<Vec<Bookmark>> {
    let l = lib.lesson(lesson_id)?;
    let mut v = read(&l.path);
    let b = v.iter_mut().find(|b| b.id == id).ok_or_else(|| anyhow!("segnaposto non trovato"))?;
    b.kind = kind;
    b.note = note.trim().into();
    write(&l.path, &v)?;
    Ok(v)
}

pub fn remove(lib: &Library, lesson_id: &str, id: &str) -> Result<Vec<Bookmark>> {
    let l = lib.lesson(lesson_id)?;
    let mut v = read(&l.path);
    v.retain(|b| b.id != id);
    write(&l.path, &v)?;
    Ok(v)
}

/// When a source is removed, its bookmarks go too.
pub fn remove_source(lesson_dir: &Path, source_id: &str) -> Result<()> {
    let mut v = read(lesson_dir);
    let n = v.len();
    v.retain(|b| b.source_id != source_id);
    if v.len() != n {
        write(lesson_dir, &v)?;
    }
    Ok(())
}

fn hms(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// The lines for a prompt: "[F1 00:12:30] importante: nota", only for sources with a code.
pub fn prompt_lines(v: &[Bookmark], code_of: impl Fn(&str) -> Option<String>) -> String {
    v.iter()
        .filter_map(|b| {
            let code = code_of(&b.source_id)?;
            let note = if b.note.is_empty() { String::new() } else { format!(": {}", b.note) };
            Some(format!("- [{code} {}] {}{note}\n", hms(b.at), b.kind.label()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_update_remove_and_source_removal() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = Library::open(tmp.path()).unwrap();
        let c = lib.create_course("RO", "", "").unwrap();
        let l = lib.create_lesson(&c.meta.id, "Simplesso", chrono::NaiveDate::from_ymd_opt(2026, 11, 3).unwrap()).unwrap();
        let l = lib.add_bytes(&l.meta.id, "a.m4a", b"x").unwrap();
        let sid = l.meta.sources[0].id.clone();
        assert!(add(&lib, &l.meta.id, "altro", 1.0, Kind::Nota, "").is_err());
        add(&lib, &l.meta.id, &sid, 750.4, Kind::Importante, " pivot ").unwrap();
        let v = add(&lib, &l.meta.id, &sid, 30.0, Kind::Dubbio, "").unwrap();
        assert_eq!(v[0].at, 30.0, "in recording order");
        let v = update(&lib, &l.meta.id, &v[1].id, Kind::Nota, "regola di Bland").unwrap();
        assert_eq!((v[1].kind, v[1].note.as_str()), (Kind::Nota, "regola di Bland"));
        assert_eq!(prompt_lines(&v, |_| Some("F1".into())), "- [F1 00:00:30] non capito\n- [F1 00:12:30] nota: regola di Bland\n");
        let v = remove(&lib, &l.meta.id, &v[0].id).unwrap();
        assert_eq!(v.len(), 1);
        lib.remove_source(&l.meta.id, &sid).unwrap();
        assert!(list(&lib, &l.meta.id).unwrap().is_empty());
        assert!(!l.path.join(BOOKMARKS_FILE).exists());
    }
}
