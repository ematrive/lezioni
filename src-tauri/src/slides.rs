//! The teacher's slides linked to the recording: when the teacher moves to each page.
//! `allineamento.json` in the lesson folder; made by the model from the transcript (with its
//! times) and an outline of the pages, again when one of the two texts changes.
use std::fs;
use std::path::Path;

use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use crate::ai::{Ai, Part, Request, Usage, prompts};
use crate::library::Library;

pub const ALIGN_FILE: &str = "allineamento.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// Seconds from the start of the recording.
    pub at: u64,
    pub page: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Alignment {
    /// Source ids.
    pub recording: String,
    pub pdf: String,
    pub pages: u32,
    pub steps: Vec<Step>,
    /// Hash of the two texts it was made from.
    pub inputs: String,
}

pub fn read(lesson_dir: &Path) -> Vec<Alignment> {
    fs::read(lesson_dir.join(ALIGN_FILE)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn write(lesson_dir: &Path, v: &[Alignment]) -> Result<()> {
    if v.is_empty() {
        let p = lesson_dir.join(ALIGN_FILE);
        if p.exists() {
            fs::remove_file(p)?;
        }
        return Ok(());
    }
    crate::pipeline::write_atomic(lesson_dir, Path::new(ALIGN_FILE), &serde_json::to_vec_pretty(v)?)
}

/// Saves an alignment in place of the one of the same pair. The caller holds the write lock.
pub fn save(lesson_dir: &Path, a: Alignment) -> Result<()> {
    let mut v = read(lesson_dir);
    v.retain(|x| !(x.recording == a.recording && x.pdf == a.pdf));
    v.push(a);
    write(lesson_dir, &v)
}

/// A source removed: its links go too.
pub fn remove_source(lesson_dir: &Path, source_id: &str) -> Result<()> {
    let mut v = read(lesson_dir);
    let n = v.len();
    v.retain(|a| a.recording != source_id && a.pdf != source_id);
    if v.len() != n {
        write(lesson_dir, &v)?;
    }
    Ok(())
}

/// The pages of a PDF read by the model ("## Pagina N" headings): (number, text).
pub fn pages(text: &str) -> Vec<(u32, String)> {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"(?m)^#{1,3}\s*Pagina\s+(\d+)\s*$").unwrap());
    let heads: Vec<(usize, usize, u32)> = RE.captures_iter(text).filter_map(|c| Some((c.get(0)?.start(), c.get(0)?.end(), c[1].parse().ok()?))).collect();
    heads.iter().enumerate().map(|(i, &(_, end, n))| (n, text[end..heads.get(i + 1).map(|h| h.0).unwrap_or(text.len())].trim().to_string())).collect()
}

/// One line per page: number and the start of its text, for the prompt.
fn outline(pages: &[(u32, String)]) -> String {
    pages
        .iter()
        .map(|(n, t)| {
            let flat = t.split_whitespace().collect::<Vec<_>>().join(" ");
            let short: String = flat.chars().take(250).collect();
            format!("Pagina {n}: {short}\n")
        })
        .collect()
}

pub fn inputs_hash(recording: &str, pdf: &str) -> String {
    let mut h = Sha256::new();
    h.update(recording.as_bytes());
    h.update([0]);
    h.update(pdf.as_bytes());
    h.finalize().iter().take(12).map(|b| format!("{b:02x}")).collect()
}

fn secs(t: &str) -> Option<u64> {
    let parts: Vec<u64> = t.trim().trim_matches(['[', ']']).split(':').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    Some(match parts[..] {
        [h, m, s] => h * 3600 + m * 60 + s,
        [m, s] => m * 60 + s,
        _ => return None,
    })
}

#[derive(Deserialize)]
struct Answer {
    passaggi: Vec<Raw>,
}
#[derive(Deserialize)]
struct Raw {
    inizio: String,
    pagina: u32,
}

/// The model's steps, checked: times inside the recording, pages that exist, in order, no two
/// steps in a row on the same page.
fn checked(answer: &str, last_time: u64, pages: u32) -> Result<Vec<Step>> {
    let a: Answer = serde_json::from_str(answer).map_err(|e| anyhow!("risposta del modello non valida: {e}"))?;
    let mut steps: Vec<Step> = a.passaggi.iter().filter_map(|r| Some(Step { at: secs(&r.inizio)?, page: r.pagina })).filter(|s| s.at <= last_time + 10 && (1..=pages).contains(&s.page)).collect();
    steps.sort_by_key(|s| s.at);
    steps.dedup_by(|b, a| a.page == b.page);
    if steps.len() < 2 {
        bail!("il modello non ha trovato quando il docente passa da una slide all'altra: forse il PDF non sono le slide di questa lezione");
    }
    Ok(steps)
}

/// Links a PDF of slides to a recording, from their texts. Nothing is saved (see `save`).
#[allow(clippy::too_many_arguments)]
pub async fn align<A: Ai>(ai: &A, model: &str, lib: &Library, lesson_id: &str, recording: &str, pdf: &str, cancel: &CancellationToken) -> Result<(Alignment, Usage)> {
    let lesson = lib.lesson(lesson_id)?;
    let out = crate::pipeline::outputs(&lesson);
    let rec_text = out.extracts.get(recording).ok_or_else(|| anyhow!("la registrazione non è ancora trascritta"))?;
    let pdf_text = out.extracts.get(pdf).ok_or_else(|| anyhow!("il PDF non è ancora stato letto"))?;
    let pgs = pages(pdf_text);
    if pgs.len() < 2 {
        bail!("nel testo del PDF non trovo le pagine: rileggilo con «Rielabora anche le fonti»");
    }
    let transcript: String = rec_text.split("\n\n").filter(|p| !crate::pauses::is_pause(p.split_once(']').map(|x| x.1).unwrap_or(""))).collect::<Vec<_>>().join("\n\n");
    let last_time = transcript.lines().filter_map(|l| l.trim_start().strip_prefix('[')?.split_once(']').and_then(|(t, _)| secs(t))).max().unwrap_or(0);
    let req = Request {
        system: Some(prompts::SYSTEM.into()),
        parts: vec![Part::Text(prompts::align_slides(&outline(&pgs), &transcript))],
        schema: Some(prompts::align_slides_schema()),
        max_output_tokens: Some(16_000),
        thinking: Some("LOW"),
        ..Default::default()
    };
    let a = ai.generate(model, &req, cancel).await?;
    if a.finish == "MAX_TOKENS" {
        bail!("la risposta del modello è stata troncata");
    }
    let pages_n = pgs.iter().map(|(n, _)| *n).max().unwrap_or(0);
    let steps = checked(&a.text, last_time, pages_n)?;
    Ok((Alignment { recording: recording.into(), pdf: pdf.into(), pages: pages_n, steps, inputs: inputs_hash(rec_text, pdf_text) }, a.usage))
}

/// Lines for the summary prompt: "- F2 pagina 3 = F1 00:05:10".
pub fn prompt_lines(v: &[Alignment], code_of: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    for a in v {
        let (Some(r), Some(p)) = (code_of(&a.recording), code_of(&a.pdf)) else { continue };
        for s in &a.steps {
            out.push_str(&format!("- {p} pagina {} = {r} {}\n", s.page, crate::pipeline::hms(s.at)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_and_checks() {
        let t = "## Pagina 1\nDighe\n\n## Pagina 2\nSperoni e gravità\n\n## Pagina 3\nFranco netto";
        let p = pages(t);
        assert_eq!(p.iter().map(|(n, _)| *n).collect::<Vec<_>>(), [1, 2, 3]);
        assert_eq!(p[1].1, "Speroni e gravità");
        assert!(outline(&p).contains("Pagina 2: Speroni e gravità"));
        let s = checked(r#"{"passaggi":[{"inizio":"00:05:00","pagina":2},{"inizio":"00:00:10","pagina":1},{"inizio":"00:06:00","pagina":2},{"inizio":"02:00:00","pagina":3},{"inizio":"00:07:00","pagina":9}]}"#, 600, 3).unwrap();
        assert_eq!(s, [Step { at: 10, page: 1 }, Step { at: 300, page: 2 }], "sorted, repeats joined, out of range dropped");
        assert!(checked(r#"{"passaggi":[{"inizio":"00:00:10","pagina":1}]}"#, 600, 3).is_err());
        assert_ne!(inputs_hash("a", "b"), inputs_hash("a", "c"));
    }
}
