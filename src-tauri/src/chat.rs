//! Questions about a lesson or a whole course, answered only from the archive.
//! - Lesson: all its extracts (transcripts, photos, PDFs) with the same [F1] codes as the summary.
//! - Course: the summaries of every lesson ([L1], [L2]…): a course of 12 lectures of 2 hours
//!   has ~400k tokens of transcripts, too much to send at each question.
//!
//! The conversation is kept in `<lesson>/chat.json` (not in `generati/`, which is versioned).
use std::collections::BTreeMap;
use std::fs;

use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::ai::{Ai, Part, Request, Usage, prompts};
use crate::lang::Lang;
use crate::library::{Library, SourceKind};
use crate::pipeline;

pub const CHAT_FILE: &str = "chat.json";
/// Earlier turns sent with a question (older ones stay in the file, not in the prompt).
const HISTORY: usize = 10;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    Lesson,
    Course,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    /// "user" or "model".
    pub role: String,
    pub text: String,
    pub scope: Scope,
    pub at: DateTime<Utc>,
    /// Codes used in the answer → id: "F1" → source id (lesson), "L1" → lesson id (course).
    #[serde(default)]
    pub refs: BTreeMap<String, String>,
}

pub fn load(lib: &Library, lesson_id: &str) -> Result<Vec<Message>> {
    let p = lib.lesson(lesson_id)?.path.join(CHAT_FILE);
    Ok(fs::read(&p).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default())
}

pub fn save(lib: &Library, lesson_id: &str, msgs: &[Message]) -> Result<()> {
    let p = lib.lesson(lesson_id)?.path.join(CHAT_FILE);
    if msgs.is_empty() {
        let _ = fs::remove_file(&p);
        return Ok(());
    }
    fs::write(&p, serde_json::to_vec_pretty(msgs)?)?;
    Ok(())
}

/// The sources of the prompt and what each code points to.
fn context(lib: &Library, lesson_id: &str, scope: Scope) -> Result<(String, BTreeMap<String, String>)> {
    let lesson = lib.lesson(lesson_id)?;
    let course = lib.course(&lesson.course_id)?;
    let ml = lib.langs(&lesson).1;
    let mut text = format!("{}: {}\n\n", ml.pick("Corso", "Course"), course.meta.name);
    let mut refs = BTreeMap::new();
    match scope {
        Scope::Lesson => {
            let o = pipeline::outputs(&lesson);
            // Same codes as the summary when there is one, so [F1] means the same everywhere.
            let mut codes: Vec<(String, String)> = o.info.as_ref().map(|i| i.sources.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default();
            for s in &lesson.meta.sources {
                if o.extracts.contains_key(&s.id) && !codes.iter().any(|(_, id)| id == &s.id) {
                    codes.push((format!("F{}", codes.len() + 1), s.id.clone()));
                }
            }
            text.push_str(&format!("{}: {} ({})\n\n", ml.pick("Lezione", "Lecture"), lesson.meta.title, lesson.meta.date));
            for (code, sid) in &codes {
                let (Some(s), Some(t)) = (lesson.meta.sources.iter().find(|s| &s.id == sid), o.extracts.get(sid)) else { continue };
                let kind = if matches!(s.kind, SourceKind::Audio | SourceKind::Video) { ml.pick("trascrizione della registrazione", "transcript of the recording") } else { ml.pick("testo", "text") };
                text.push_str(&format!("[{code}] {} ({kind})\n{t}\n\n", s.original_name));
                refs.insert(code.clone(), sid.clone());
            }
            let marks = crate::bookmarks::prompt_lines(&crate::bookmarks::read(&lesson.path), ml, |sid| refs.iter().find(|(_, v)| v.as_str() == sid).map(|(k, _)| k.clone()));
            if !marks.is_empty() {
                text.push_str(&format!("[SEGNAPOSTI] {}\n{marks}\n", ml.pick("punti segnati dallo studente mentre riascoltava", "points marked by the student while listening again")));
            }
            if let Some(s) = &o.summary {
                text.push_str(&format!("[SINTESI] {}\n{s}\n\n", ml.pick("(già scritta a partire dalle fonti)", "(summary already written from the sources)")));
            }
            if refs.is_empty() && o.summary.is_none() {
                bail!(crate::tr!("la lezione non ha ancora testi: elaborala prima con Gemini", "the lecture has no texts yet: process it with Gemini first"));
            }
        }
        Scope::Course => {
            for (code, l, o) in crate::course::lessons_with_summary(lib, &course.meta.id)? {
                text.push_str(&format!("[{code}] {} {}: {}\n{}\n\n", ml.pick("Lezione del", "Lecture of"), l.meta.date, l.meta.title, o.summary.unwrap_or_default()));
                refs.insert(code, l.meta.id.clone());
            }
            if refs.is_empty() {
                bail!(crate::tr!("nessuna lezione del corso ha una sintesi: elabora prima le lezioni con Gemini", "no lecture of the course has a summary: process the lectures with Gemini first"));
            }
        }
    }
    Ok((text, refs))
}

fn instructions(scope: Scope, l: Lang) -> &'static str {
    match (scope, l) {
        (Scope::Lesson, Lang::It) => {
            "Rispondi alla domanda dello studente usando SOLO le fonti della lezione qui sotto. \
Dopo ogni affermazione indica la fonte tra parentesi quadre, con il minuto se è una registrazione: [F1 00:12:30], [F2]. \
Se le fonti non contengono la risposta, dillo chiaramente e, se utile, aggiungi una spiegazione generale segnalandola come «(non dalle fonti)». \
Rispondi in italiano, in modo chiaro e ordinato, con formule in LaTeX."
        }
        (Scope::Course, Lang::It) => {
            "Rispondi alla domanda dello studente usando SOLO le sintesi delle lezioni del corso qui sotto. \
Indica sempre la lezione da cui viene ogni informazione tra parentesi quadre: [L3], [L5]. \
Se le sintesi non contengono la risposta, dillo chiaramente e, se utile, aggiungi una spiegazione generale segnalandola come «(non dalle lezioni)». \
Rispondi in italiano, in modo chiaro e ordinato, con formule in LaTeX."
        }
        (Scope::Lesson, Lang::En) => {
            "Answer the student's question using ONLY the sources of the lecture below. \
After each statement give the source in square brackets, with the minute if it is a recording: [F1 00:12:30], [F2]. \
If the sources do not contain the answer, say so clearly and, if useful, add a general explanation marked as “(not from the sources)”. \
Answer in English, clearly and in order, with formulas in LaTeX."
        }
        (Scope::Course, Lang::En) => {
            "Answer the student's question using ONLY the summaries of the course's lectures below. \
Always give the lecture each piece of information comes from in square brackets: [L3], [L5]. \
If the summaries do not contain the answer, say so clearly and, if useful, add a general explanation marked as “(not from the lectures)”. \
Answer in English, clearly and in order, with formulas in LaTeX."
        }
    }
}

/// Asks a question; returns the updated conversation and the tokens used.
pub async fn ask<A: Ai>(ai: &A, model: &str, lib: &Library, lesson_id: &str, scope: Scope, question: &str, cancel: &CancellationToken) -> Result<(Vec<Message>, Usage)> {
    let question = question.trim();
    if question.is_empty() {
        bail!(crate::tr!("scrivi una domanda", "write a question"));
    }
    let (sources, refs) = context(lib, lesson_id, scope)?;
    let ml = lib.langs(&lib.lesson(lesson_id)?).1;
    let mut msgs = load(lib, lesson_id)?;
    let earlier: String = msgs
        .iter()
        .filter(|m| m.scope == scope)
        .rev()
        .take(HISTORY)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|m| format!("{}: {}\n\n", if m.role == "user" { ml.pick("STUDENTE", "STUDENT") } else { ml.pick("ASSISTENTE", "ASSISTANT") }, m.text))
        .collect();
    let earlier = if earlier.is_empty() { String::new() } else { format!("{}:\n\n{earlier}", ml.pick("CONVERSAZIONE PRECEDENTE", "EARLIER CONVERSATION")) };
    let (src, q) = ml.pick(("FONTI", "DOMANDA DELLO STUDENTE"), ("SOURCES", "THE STUDENT'S QUESTION"));
    let prompt = format!("{}\n\n{src}:\n\n{sources}\n{earlier}{q}:\n{question}", instructions(scope, ml));
    let req = Request { system: Some(prompts::system(ml)), parts: vec![Part::Text(prompt)], max_output_tokens: Some(8000), thinking: Some("LOW"), ..Default::default() };
    let a = ai.generate(model, &req, cancel).await?;
    let now = Utc::now();
    msgs.push(Message { role: "user".into(), text: question.into(), scope, at: now, refs: BTreeMap::new() });
    msgs.push(Message { role: "model".into(), text: a.text.trim().into(), scope, at: now, refs });
    save(lib, lesson_id, &msgs)?;
    Ok((msgs, a.usage))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{Answer, RemoteFile};
    use std::path::Path;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Echo {
        prompts: Mutex<Vec<String>>,
    }
    impl Ai for Echo {
        async fn generate(&self, _m: &str, req: &Request, _c: &CancellationToken) -> Result<Answer> {
            let Part::Text(t) = &req.parts[0] else { unreachable!() };
            self.prompts.lock().unwrap().push(t.clone());
            Ok(Answer { text: "Risposta [F1 00:00:05]".into(), usage: Usage::default(), finish: "STOP".into(), model: String::new() })
        }
        async fn count_tokens(&self, _m: &str, _p: &[Part], _c: &CancellationToken) -> Result<u64> {
            Ok(0)
        }
        async fn upload(&self, _p: &Path, _m: &str, _c: &CancellationToken) -> Result<RemoteFile> {
            unreachable!()
        }
        async fn delete_file(&self, _n: &str) {}
    }

    #[tokio::test]
    async fn lesson_and_course() {
        let t = tempfile::tempdir().unwrap();
        let lib = Library::open(t.path().join("a")).unwrap();
        let c = lib.create_course("Economia", "", "").unwrap();
        let l1 = lib.create_lesson(&c.meta.id, "VAN", "2026-11-03".parse().unwrap()).unwrap();
        let l2 = lib.create_lesson(&c.meta.id, "TIR", "2026-11-10".parse().unwrap()).unwrap();
        let ai = Echo::default();
        let cancel = CancellationToken::new();
        assert!(ask(&ai, "m", &lib, &l1.meta.id, Scope::Lesson, "Cos'è il VAN?", &cancel).await.unwrap_err().to_string().contains("elaborala"));

        let a = t.path().join("reg.m4a");
        fs::write(&a, b"x").unwrap();
        let (l1, _) = lib.add_files(&l1.meta.id, &[a]).unwrap();
        fs::write(l1.path.join(pipeline::extract_rel(&l1.meta.sources[0])), "[00:00:05] Il VAN somma i flussi attualizzati.").unwrap();
        fs::write(l1.path.join("generati/sintesi.md"), "## VAN").unwrap();
        fs::write(l2.path.join("generati/sintesi.md"), "## TIR").unwrap();

        let (msgs, _) = ask(&ai, "m", &lib, &l1.meta.id, Scope::Lesson, "Cos'è il VAN?", &cancel).await.unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[1].refs["F1"], l1.meta.sources[0].id);
        assert!(ai.prompts.lock().unwrap()[0].contains("[F1] reg.m4a (trascrizione della registrazione)\n[00:00:05] Il VAN"));

        let (msgs, _) = ask(&ai, "m", &lib, &l1.meta.id, Scope::Course, "Differenza tra VAN e TIR?", &cancel).await.unwrap();
        assert_eq!(msgs.len(), 4);
        assert_eq!((msgs[3].refs["L1"].as_str(), msgs[3].refs["L2"].as_str()), (l1.meta.id.as_str(), l2.meta.id.as_str()));
        let p = ai.prompts.lock().unwrap()[1].clone();
        assert!(p.contains("[L2] Lezione del 2026-11-10: TIR") && !p.contains("flussi attualizzati"), "course scope: summaries only");
        assert!(!p.contains("CONVERSAZIONE PRECEDENTE"), "the lesson chat is not mixed in");

        // Kept on disk; a follow-up carries the earlier turns of the same scope.
        assert_eq!(load(&lib, &l1.meta.id).unwrap().len(), 4);
        ask(&ai, "m", &lib, &l1.meta.id, Scope::Lesson, "E il tasso?", &cancel).await.unwrap();
        assert!(ai.prompts.lock().unwrap()[2].contains("STUDENTE: Cos'è il VAN?"));
        save(&lib, &l1.meta.id, &[]).unwrap();
        assert!(load(&lib, &l1.meta.id).unwrap().is_empty());
    }
}
