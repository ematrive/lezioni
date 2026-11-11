//! The summary of a whole course, made from the summaries of its lessons: `<Corso>/generati/`
//! with `sintesi.md`, `domande.json` (the exam questions of the course) and `info.json`. The
//! lessons are cited as [L1], [L2]… and `info.refs` keeps which lesson each code was, so a lesson
//! added later does not shift the links. Not in packages: a package holds lessons, which may get
//! new ids on import, and the summary costs a few cents to make again.
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::ai::{Ai, Part, Request, Usage, prompts};
use crate::library::{GENERATED_DIR, Lesson, Library};

/// The lessons of a course that have a summary, oldest first, with their code (L1, L2…).
pub fn lessons_with_summary(lib: &Library, course_id: &str) -> Result<Vec<(String, Lesson, crate::pipeline::Outputs)>> {
    let mut ls = lib.lessons(course_id)?;
    ls.sort_by_key(|l| (l.meta.date, l.meta.created));
    let mut out = Vec::new();
    for l in ls {
        let o = crate::pipeline::outputs(&l);
        if o.summary.is_some() {
            out.push((format!("L{}", out.len() + 1), l, o));
        }
    }
    Ok(out)
}

/// A term of the glossaries of the lessons, with the lessons it comes from.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GlossaryTerm {
    pub termine: String,
    pub definizione: String,
    /// (lesson id, title), oldest first.
    pub lessons: Vec<(String, String)>,
}

/// The glossaries of all the lessons of a course, merged (same term ignoring case: the first
/// definition, all the lessons), in alphabetical order.
pub fn glossary(lib: &Library, course_id: &str) -> Result<Vec<GlossaryTerm>> {
    let mut ls = lib.lessons(course_id)?;
    ls.sort_by_key(|l| (l.meta.date, l.meta.created));
    let mut by_key: BTreeMap<String, GlossaryTerm> = BTreeMap::new();
    for l in &ls {
        let Some(study) = crate::pipeline::outputs_study(l) else { continue };
        for t in study.glossario {
            let k = t.termine.trim().to_lowercase();
            if k.is_empty() {
                continue;
            }
            let e = by_key.entry(k).or_insert_with(|| GlossaryTerm { termine: t.termine.trim().into(), definizione: t.definizione.trim().into(), lessons: vec![] });
            if !e.lessons.iter().any(|(id, _)| id == &l.meta.id) {
                e.lessons.push((l.meta.id.clone(), l.meta.title.clone()));
            }
        }
    }
    let mut out: Vec<GlossaryTerm> = by_key.into_values().collect();
    out.sort_by_cached_key(|t| t.termine.to_lowercase());
    Ok(out)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CourseQuestion {
    pub domanda: String,
    pub traccia: String,
    /// Codes of the lessons it comes from ("L3").
    #[serde(default)]
    pub lezioni: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Generated {
    sintesi: String,
    #[serde(default)]
    domande: Vec<CourseQuestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LessonStamp {
    pub id: String,
    /// When that lesson's summary was made.
    pub generated: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseInfo {
    pub generated: DateTime<Utc>,
    pub model: String,
    pub usage: Usage,
    /// "L1" → lesson id, as when the summary was made.
    pub refs: BTreeMap<String, String>,
    /// The lessons it was made from: a new or newer lesson summary makes it out of date.
    pub lessons: Vec<LessonStamp>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseSummary {
    pub summary: Option<String>,
    pub questions: Vec<CourseQuestion>,
    pub info: Option<CourseInfo>,
    /// Lessons with a summary now (what a new course summary would use).
    pub ready: usize,
    /// Lessons added or summarized again since the course summary was made.
    pub changed: usize,
}

fn stamps(ls: &[(String, Lesson, crate::pipeline::Outputs)]) -> Vec<LessonStamp> {
    ls.iter().map(|(_, l, o)| LessonStamp { id: l.meta.id.clone(), generated: o.info.as_ref().map(|i| i.generated) }).collect()
}

pub fn read(lib: &Library, course_id: &str) -> Result<CourseSummary> {
    let dir = lib.course(course_id)?.path.join(GENERATED_DIR);
    let text = |f: &str| fs::read_to_string(dir.join(f)).ok();
    let info: Option<CourseInfo> = text("info.json").and_then(|t| serde_json::from_str(&t).ok());
    let now = stamps(&lessons_with_summary(lib, course_id)?);
    let changed = match &info {
        Some(i) => now.iter().filter(|s| !i.lessons.contains(s)).count(),
        None => 0,
    };
    Ok(CourseSummary {
        summary: text("sintesi.md"),
        questions: text("domande.json").and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default(),
        info,
        ready: now.len(),
        changed,
    })
}

/// A lesson summary without its references to the lesson's sources ([F1 00:12:30], [F2; F3]):
/// in the course they would point nowhere, and models copy them.
fn without_source_refs(s: &str) -> String {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r" ?\[F\d+(?:\s+[\d:]+)?(?:\s*[,;]\s*F\d+(?:\s+[\d:]+)?)*\]").unwrap());
    RE.replace_all(s, "").to_string()
}

/// Makes the course summary with `model`. The previous one goes to `generati/versioni/`. The
/// files are written under `lock` (the archive's write lock). Returns what it cost.
pub async fn summarize<A: Ai>(ai: &A, model: &str, lib: &Library, course_id: &str, lock: &std::sync::Mutex<()>, cancel: &CancellationToken) -> Result<(Usage, String)> {
    let course = lib.course(course_id)?;
    let ls = lessons_with_summary(lib, course_id)?;
    if ls.is_empty() {
        bail!("nessuna lezione del corso ha una sintesi: elabora prima le lezioni");
    }
    let mut text = String::new();
    for (code, l, o) in &ls {
        text.push_str(&format!("[{code}] Lezione del {}: {}\n{}\n", l.meta.date, l.meta.title, without_source_refs(o.summary.as_deref().unwrap_or(""))));
        if let Some(s) = &o.study
            && !s.domande_esame.is_empty()
        {
            text.push_str("Domande d'esame proposte per questa lezione:\n");
            for q in &s.domande_esame {
                text.push_str(&format!("- {}\n", q.domanda));
            }
        }
        text.push('\n');
    }
    let req = Request {
        system: Some(prompts::SYSTEM.into()),
        parts: vec![Part::Text(prompts::course_summary(&course.meta.name, &lib.terms_for(&course.meta), ls.len(), &text))],
        schema: Some(prompts::course_summary_schema()),
        max_output_tokens: Some(48_000),
        ..Default::default()
    };
    let mut usage = Usage::default();
    // As for the lessons: an empty skeleton now and then (Haiku), asked again at most 3 times.
    let mut attempt = 0;
    let (g, used) = loop {
        attempt += 1;
        let a = ai.generate(model, &req, cancel).await?;
        usage.add(&a.usage);
        if a.finish == "MAX_TOKENS" {
            bail!("la risposta del modello è stata troncata: scegli nelle Impostazioni un modello con più spazio di risposta");
        }
        let g: Generated = serde_json::from_str(&a.text).map_err(|e| anyhow!("risposta del modello non valida: {e}"))?;
        if !g.sintesi.trim().is_empty() {
            break (g, if a.model.is_empty() { model.to_string() } else { a.model });
        }
        if attempt == 3 {
            bail!("il modello ha risposto tre volte con una sintesi vuota: riprova, o scegli un altro modello per la sintesi nelle Impostazioni");
        }
        log::warn!("sintesi del corso vuota, nuovo tentativo ({attempt})");
    };
    let refs: BTreeMap<String, String> = ls.iter().map(|(c, l, _)| (c.clone(), l.meta.id.clone())).collect();
    let questions: Vec<CourseQuestion> = g.domande.into_iter().map(|mut q| {
        q.lezioni.retain(|c| refs.contains_key(c));
        q
    }).collect();
    let info = CourseInfo { generated: Utc::now(), model: used.clone(), usage: usage.clone(), refs, lessons: stamps(&ls) };
    let _g = lock.lock().unwrap_or_else(|e| e.into_inner());
    // Found again: the course folder may have been renamed meanwhile.
    let dir = lib.course(course_id)?.path.join(GENERATED_DIR);
    crate::pipeline::archive_generated(&dir)?;
    crate::pipeline::write_atomic(&dir, Path::new("sintesi.md"), g.sintesi.trim().as_bytes())?;
    crate::pipeline::write_atomic(&dir, Path::new("domande.json"), &serde_json::to_vec_pretty(&questions)?)?;
    crate::pipeline::write_atomic(&dir, Path::new("info.json"), &serde_json::to_vec_pretty(&info)?)?;
    Ok((usage, used))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{Answer, RemoteFile};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct Fake {
        prompts: Mutex<Vec<String>>,
        empty: AtomicUsize,
    }

    impl Ai for Fake {
        async fn generate(&self, _m: &str, req: &Request, _c: &CancellationToken) -> Result<Answer> {
            let Part::Text(p) = &req.parts[0] else { unreachable!() };
            self.prompts.lock().unwrap().push(p.clone());
            let text = if self.empty.load(Ordering::SeqCst) > 0 {
                self.empty.fetch_sub(1, Ordering::SeqCst);
                r#"{"sintesi":"","domande":[]}"#.to_string()
            } else {
                serde_json::json!({ "sintesi": "## Reti\nDal moto vario [L1] al monopolio [L2].", "domande": [{ "domanda": "Il colpo d'ariete", "traccia": "…", "lezioni": ["L1", "L9"] }] }).to_string()
            };
            Ok(Answer { text, usage: Usage { prompt_token_count: 1000, candidates_token_count: 100, thoughts_token_count: 0, cost: Some(0.01) }, finish: "STOP".into(), model: String::new() })
        }
        async fn count_tokens(&self, _m: &str, _p: &[Part], _c: &CancellationToken) -> Result<u64> {
            Ok(0)
        }
        async fn upload(&self, _p: &Path, _m: &str, _c: &CancellationToken) -> Result<RemoteFile> {
            bail!("no")
        }
        async fn delete_file(&self, _n: &str) {}
    }

    fn summarized(lib: &Library, course: &str, day: u32, title: &str) -> Lesson {
        let l = lib.create_lesson(course, title, chrono::NaiveDate::from_ymd_opt(2026, 11, day).unwrap()).unwrap();
        let g = l.path.join(GENERATED_DIR);
        fs::create_dir_all(&g).unwrap();
        fs::write(g.join("sintesi.md"), format!("Sintesi di {title} [F1 00:12:30; F2].")).unwrap();
        let study = crate::pipeline::Study { domande_esame: vec![crate::pipeline::ExamQuestion { domanda: format!("Domanda su {title}"), traccia: String::new() }], ..Default::default() };
        fs::write(g.join("studio.json"), serde_json::to_vec(&study).unwrap()).unwrap();
        let info = crate::pipeline::Info { generated: Utc::now(), model: "m".into(), usage: Usage::default(), suggested_title: String::new(), sources: Default::default(), edited: None };
        fs::write(g.join("info.json"), serde_json::to_vec(&info).unwrap()).unwrap();
        l
    }

    #[test]
    fn glossaries_of_the_lessons_merged() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = Library::open(tmp.path()).unwrap();
        let c = lib.create_course("Idraulica", "", "").unwrap();
        let term = |t: &str, d: &str| crate::pipeline::Term { termine: t.into(), definizione: d.into() };
        for (day, title, terms) in [(3, "Moto vario", vec![term("Colpo d'ariete", "sovrapressione"), term("ARERA", "autorità")]), (10, "Reti", vec![term("colpo d'ariete", "altra"), term("Bacino", "ente")])] {
            let l = lib.create_lesson(&c.meta.id, title, chrono::NaiveDate::from_ymd_opt(2026, 11, day).unwrap()).unwrap();
            fs::create_dir_all(l.path.join(GENERATED_DIR)).unwrap();
            let study = crate::pipeline::Study { glossario: terms, ..Default::default() };
            fs::write(l.path.join(GENERATED_DIR).join("studio.json"), serde_json::to_vec(&study).unwrap()).unwrap();
        }
        let g = glossary(&lib, &c.meta.id).unwrap();
        assert_eq!(g.iter().map(|t| t.termine.as_str()).collect::<Vec<_>>(), ["ARERA", "Bacino", "Colpo d'ariete"]);
        assert_eq!(g[2].definizione, "sovrapressione", "the first lesson's definition");
        assert_eq!(g[2].lessons.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(), ["Moto vario", "Reti"]);
    }

    #[tokio::test]
    async fn summary_of_the_lessons_saved_with_its_references() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = Library::open(tmp.path()).unwrap();
        let c = lib.create_course("Idraulica", "", "ARERA").unwrap();
        assert!(summarize(&Fake::default(), "m", &lib, &c.meta.id, &Mutex::new(()), &CancellationToken::new()).await.is_err(), "no lesson summary yet");
        let late = summarized(&lib, &c.meta.id, 10, "Monopolio");
        let early = summarized(&lib, &c.meta.id, 3, "Moto vario");
        lib.create_lesson(&c.meta.id, "Senza sintesi", chrono::NaiveDate::from_ymd_opt(2026, 11, 5).unwrap()).unwrap();
        let ai = Fake::default();
        ai.empty.store(1, Ordering::SeqCst);
        let (usage, model) = summarize(&ai, "m", &lib, &c.meta.id, &Mutex::new(()), &CancellationToken::new()).await.unwrap();
        assert_eq!((usage.cost, model.as_str()), (Some(0.02), "m"), "asked twice, both paid");
        let p = ai.prompts.lock().unwrap()[0].clone();
        let (a, b) = (p.find("[L1] Lezione del 2026-11-03: Moto vario").unwrap(), p.find("[L2] Lezione del 2026-11-10: Monopolio").unwrap());
        assert!(a < b && p.contains("- Domanda su Monopolio") && p.contains("ARERA") && !p.contains("Senza sintesi"), "{p}");
        assert!(p.contains("Sintesi di Monopolio.") && !p.contains("[F1"), "{p}");
        let s = read(&lib, &c.meta.id).unwrap();
        assert_eq!(s.summary.as_deref(), Some("## Reti\nDal moto vario [L1] al monopolio [L2]."));
        assert_eq!(s.questions[0].lezioni, ["L1"], "unknown codes dropped");
        let info = s.info.unwrap();
        assert_eq!((info.refs["L1"].as_str(), info.refs["L2"].as_str()), (early.meta.id.as_str(), late.meta.id.as_str()));
        assert_eq!((s.ready, s.changed), (2, 0));
        // A new lesson summarized: out of date. Made again: the old one becomes a version.
        summarized(&lib, &c.meta.id, 17, "Reti a maglia");
        assert_eq!(read(&lib, &c.meta.id).unwrap().changed, 1);
        summarize(&ai, "m", &lib, &c.meta.id, &Mutex::new(()), &CancellationToken::new()).await.unwrap();
        assert_eq!(read(&lib, &c.meta.id).unwrap().changed, 0);
        assert_eq!(fs::read_dir(lib.course(&c.meta.id).unwrap().path.join("generati/versioni")).unwrap().count(), 1);
        // The course folder still lists only its lessons.
        assert_eq!(lib.lessons(&c.meta.id).unwrap().len(), 4);
        // Three empty answers: an error, the saved summary untouched.
        ai.empty.store(3, Ordering::SeqCst);
        assert!(summarize(&ai, "m", &lib, &c.meta.id, &Mutex::new(()), &CancellationToken::new()).await.is_err());
        assert!(read(&lib, &c.meta.id).unwrap().summary.is_some());
    }
}
