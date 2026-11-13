//! Processing of a lesson with the AI:
//! 1. each source → `estratti/<file>.md` (recordings transcribed in 20-minute windows of one
//!    uploaded file, photos and PDFs read by the model, text copied);
//! 2. all extracts together → `generati/` (summary, concept map, glossary, flashcards, exam questions).
//!
//! Extracts already there are reused (unless `force`), so a second run only pays for what is
//! new. Previous outputs move to `generati/versioni/<data-ora>/`. Every write re-reads the lesson
//! by id under the archive lock: the folder may be renamed while a long job runs.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::ai::gemini::mime_for;
use crate::ai::{Ai, Part, Request, Usage, prompts};
use crate::asr::{self, Asr};
use crate::audio;
use crate::lang::Lang;
use crate::library::{EXTRACTS_DIR, GENERATED_DIR, Lesson, Library, SOURCES_DIR, Source, SourceKind};

/// Length of a transcription window: keeps each answer well below the output limit.
pub const WINDOW_SECS: u64 = 20 * 60;
const AUDIO_TOKENS_PER_SEC: u64 = 32;
/// Video: ~258 tokens per frame at 1 fps plus the audio track.
const VIDEO_TOKENS_PER_SEC: u64 = 290;

/// Who turns recordings into text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    /// Whisper on the computer, then Gemini cleans the text.
    #[default]
    Local,
    /// The audio goes to Gemini.
    Gemini,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Models {
    #[serde(default)]
    pub engine: Engine,
    pub transcribe: String,
    pub generate: String,
    /// Model used when the asked one is overloaded (503); empty = none.
    #[serde(default = "default_fallback")]
    pub fallback: String,
}

fn default_fallback() -> String {
    "gemini-3.5-flash-lite".into()
}

impl Default for Models {
    fn default() -> Self {
        // Cleanup and PDFs with the light model (fast, and available when Flash is overloaded).
        Self { engine: Engine::Local, transcribe: "gemini-3.5-flash-lite".into(), generate: "gemini-3.8-flash".into(), fallback: default_fallback() }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    /// Extract again even the sources that already have an extract.
    #[serde(default)]
    pub force: bool,
    /// Only the extracts, no summary.
    #[serde(default)]
    pub extract_only: bool,
}

/// The archive as seen by a long job: opened again at each step, writes under the lock.
#[derive(Clone)]
pub struct Archive {
    pub dir: PathBuf,
    pub lock: Arc<Mutex<()>>,
}

impl Archive {
    fn lib(&self) -> Result<Library> {
        Library::open(&self.dir)
    }

    pub fn lesson(&self, id: &str) -> Result<Lesson> {
        self.lib()?.lesson(id)
    }

    /// (spoken, materials) languages of a lesson.
    fn langs(&self, lesson_id: &str) -> Result<(Lang, Lang)> {
        let lib = self.lib()?;
        Ok(lib.langs(&lib.lesson(lesson_id)?))
    }

    /// Writes `<lesson>/<rel>` atomically, finding the lesson folder again first.
    fn write(&self, lesson_id: &str, rel: &Path, content: &[u8]) -> Result<PathBuf> {
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let l = self.lesson(lesson_id)?;
        let p = l.path.join(rel);
        if let Some(d) = p.parent() {
            fs::create_dir_all(d)?;
        }
        let tmp = p.with_extension("tmp");
        fs::write(&tmp, content).with_context(|| crate::tr!("scrittura di {}", "writing {}", p.display()))?;
        fs::rename(&tmp, &p)?;
        Ok(p)
    }

    /// Moves the current outputs to `generati/versioni/<stamp>/`.
    fn archive_outputs(&self, lesson_id: &str) -> Result<()> {
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        archive_generated(&self.lesson(lesson_id)?.path.join(GENERATED_DIR))
    }
}

/// A new folder `<dir>/versioni/<now>` (UTC, "2026-11-11 14-03-22"; " 2" etc. when taken).
pub(crate) fn new_version_dir(dir: &Path) -> PathBuf {
    let stamp = Utc::now().format("%Y-%m-%d %H-%M-%S").to_string();
    let base = dir.join("versioni");
    let mut dest = base.join(&stamp);
    let mut n = 2;
    while dest.exists() {
        dest = base.join(format!("{stamp} {n}"));
        n += 1;
    }
    dest
}

/// Moves the files of a `generati/` folder (lesson or course) to `versioni/<stamp>/`.
pub(crate) fn archive_generated(gdir: &Path) -> Result<()> {
    let files: Vec<PathBuf> = match fs::read_dir(gdir) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_file() && p.extension().is_none_or(|e| e != "tmp")).collect(),
        Err(_) => return Ok(()),
    };
    if files.is_empty() {
        return Ok(());
    }
    let dest = new_version_dir(gdir);
    fs::create_dir_all(&dest)?;
    for f in files {
        fs::rename(&f, dest.join(f.file_name().unwrap()))?;
    }
    Ok(())
}

/// Parts of a recording transcribed so far (removed when the transcript is complete).
pub fn parts_rel(s: &Source) -> PathBuf {
    Path::new(EXTRACTS_DIR).join(".parti").join(&s.file)
}

/// Adds `offset` seconds to the `[mm:ss]` / `[hh:mm:ss]` times of a part's transcript.
pub fn shift_times(text: &str, offset: u64) -> String {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"\[(\d{1,2}):(\d{2})(?::(\d{2}))?\]").unwrap());
    RE.replace_all(text, |c: &regex::Captures| {
        let n = |i: usize| c.get(i).map(|m| m.as_str().parse::<u64>().unwrap_or(0));
        let secs = match n(3) {
            Some(sec) => n(1).unwrap() * 3600 + n(2).unwrap() * 60 + sec,
            None => n(1).unwrap() * 60 + n(2).unwrap(),
        };
        format!("[{}]", hms(secs + offset))
    })
    .into_owned()
}

pub fn extract_rel(s: &Source) -> PathBuf {
    Path::new(EXTRACTS_DIR).join(format!("{}.md", s.file))
}

/// Whisper's text of a whole recording, before the cleanup (kept to be compared and corrected).
pub fn raw_rel(s: &Source) -> PathBuf {
    Path::new(EXTRACTS_DIR).join(format!("{}.whisper.md", s.file))
}

/// Whisper's text of the parts still in `.parti/` (None when there are none).
fn whisper_parts(lesson_dir: &Path, s: &Source) -> Option<String> {
    let dir = lesson_dir.join(parts_rel(s));
    let mut names: Vec<String> = fs::read_dir(&dir).ok()?.flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.ends_with(".whisper.md")).collect();
    names.sort();
    let texts: Vec<String> = names.iter().filter_map(|n| fs::read_to_string(dir.join(n)).ok()).filter(|t| !t.trim().is_empty()).collect();
    // The parts' pauses are already marked, in the lesson's language: here they are only joined,
    // and a joined pause keeps its language.
    if texts.is_empty() { None } else { Some(crate::pauses::mark(&texts.join("\n\n"), crate::lang::Lang::It).0) }
}

/// Writes `<dir>/<rel>` through a temporary file, so a crash never leaves half a file.
pub(crate) fn write_atomic(dir: &Path, rel: &Path, content: &[u8]) -> Result<()> {
    let p = dir.join(rel);
    if let Some(d) = p.parent() {
        fs::create_dir_all(d)?;
    }
    let tmp = p.with_extension("tmp");
    fs::write(&tmp, content).with_context(|| crate::tr!("scrittura di {}", "writing {}", p.display()))?;
    fs::rename(&tmp, &p)?;
    Ok(())
}

/// Saves the parts' Whisper text as `<file>.whisper.md` and removes `.parti/`: the transcript is
/// final (complete, or corrected by hand).
fn close_parts(lesson_dir: &Path, s: &Source) -> Result<()> {
    if let Some(raw) = whisper_parts(lesson_dir, s) {
        write_atomic(lesson_dir, &raw_rel(s), raw.as_bytes())?;
    }
    let _ = fs::remove_dir_all(lesson_dir.join(parts_rel(s)));
    Ok(())
}

// ---------- corrections by hand (the caller holds the archive's write lock) ----------

/// A source's extract corrected by hand. The parts waiting for a cleanup are closed, otherwise
/// the next processing would clean them again over the correction.
pub fn save_extract(lib: &Library, lesson_id: &str, source_id: &str, text: &str) -> Result<Lesson> {
    let l = lib.lesson(lesson_id)?;
    let s = l.meta.sources.iter().find(|s| s.id == source_id).ok_or_else(|| anyhow!(crate::tr!("fonte non trovata", "source not found")))?;
    close_parts(&l.path, s)?;
    write_atomic(&l.path, &extract_rel(s), text.as_bytes())?;
    lib.set_source_edited(lesson_id, source_id, Some(Utc::now()))
}

/// Marks the pauses Whisper filled with invented words in a transcript already made (its text and
/// Whisper's). Counts as a correction by hand. Returns how many pauses.
pub fn mark_pauses(lib: &Library, lesson_id: &str, source_id: &str) -> Result<usize> {
    let l = lib.lesson(lesson_id)?;
    let s = l.meta.sources.iter().find(|s| s.id == source_id).ok_or_else(|| anyhow!(crate::tr!("fonte non trovata", "source not found")))?;
    let spoken = lib.langs(&l).0;
    let (text, n) = crate::pauses::mark(&fs::read_to_string(l.path.join(extract_rel(s)))?, spoken);
    if n == 0 {
        return Ok(0);
    }
    if let Ok(raw) = fs::read_to_string(l.path.join(raw_rel(s))) {
        write_atomic(&l.path, &raw_rel(s), crate::pauses::mark(&raw, spoken).0.as_bytes())?;
    }
    save_extract(lib, lesson_id, source_id, &text)?;
    Ok(n)
}

fn mark_info_edited(l: &Lesson) -> Result<()> {
    let p = l.path.join(GENERATED_DIR).join("info.json");
    if let Some(mut info) = fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<Info>(&t).ok()) {
        info.edited = Some(Utc::now());
        write_atomic(&l.path, &Path::new(GENERATED_DIR).join("info.json"), &serde_json::to_vec_pretty(&info)?)?;
    }
    Ok(())
}

pub fn save_summary(lib: &Library, lesson_id: &str, text: &str) -> Result<()> {
    let l = lib.lesson(lesson_id)?;
    write_atomic(&l.path, &Path::new(GENERATED_DIR).join("sintesi.md"), text.as_bytes())?;
    mark_info_edited(&l)
}

pub fn save_study(lib: &Library, lesson_id: &str, study: &Study) -> Result<()> {
    let l = lib.lesson(lesson_id)?;
    write_atomic(&l.path, &Path::new(GENERATED_DIR).join("studio.json"), &serde_json::to_vec_pretty(study)?)?;
    mark_info_edited(&l)
}

/// `[hh:mm:ss]`
pub fn hms(secs: u64) -> String {
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MapNode {
    pub etichetta: String,
    /// Where the sources talk about it, as in the summary: "F1 00:12:30" or "F3".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rif: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub figli: Vec<MapNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Term {
    pub termine: String,
    pub definizione: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Card {
    pub domanda: String,
    pub risposta: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExamQuestion {
    pub domanda: String,
    pub traccia: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Generated {
    pub titolo: String,
    pub sintesi: String,
    pub mappa: MapNode,
    #[serde(default)]
    pub glossario: Vec<Term>,
    #[serde(default)]
    pub flashcard: Vec<Card>,
    #[serde(default)]
    pub domande_esame: Vec<ExamQuestion>,
}

/// `generati/studio.json`
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Study {
    pub glossario: Vec<Term>,
    pub flashcard: Vec<Card>,
    pub domande_esame: Vec<ExamQuestion>,
}

/// `generati/info.json`: how and when the outputs were made, and which source each [Fn] is.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub generated: chrono::DateTime<Utc>,
    pub model: String,
    pub usage: Usage,
    pub suggested_title: String,
    /// "F1" → source id
    pub sources: BTreeMap<String, String>,
    /// Last correction by hand of the summary or of the study material.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited: Option<chrono::DateTime<Utc>>,
}

/// Mermaid mindmap of the tree (for Obsidian and Markdown exports). Shapes are not used, so
/// only brackets need to go: they would start a node shape.
pub fn mermaid(root: &MapNode) -> String {
    fn clean(s: &str) -> String {
        s.chars().map(|c| if "()[]{}".contains(c) { ' ' } else { c }).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
    }
    fn walk(n: &MapNode, depth: usize, out: &mut String) {
        out.push_str(&"  ".repeat(depth + 1));
        out.push_str(&if depth == 0 { format!("root(({}))", clean(&n.etichetta)) } else { clean(&n.etichetta) });
        out.push('\n');
        for c in &n.figli {
            walk(c, depth + 1, out);
        }
    }
    let mut out = String::from("mindmap\n");
    walk(root, 0, &mut out);
    out
}

pub struct Ctx<'a, A: Ai> {
    pub ai: &'a A,
    pub archive: &'a Archive,
    pub models: &'a Models,
    pub cancel: &'a CancellationToken,
    /// (steps done, fractional within a long step; total steps; message)
    pub progress: &'a (dyn Fn(f64, u32, String) + Send + Sync),
    /// A second line under the step: what the current call is waiting for.
    pub detail: &'a (dyn Fn(String) + Send + Sync),
    /// Length of a transcription part (`WINDOW_SECS`; shorter in tests).
    pub window: u64,
    /// Speech recognition on the computer; None = recordings go to Gemini.
    pub asr: Option<Arc<dyn Asr>>,
    /// Requests answered by the fallback model: (count, model).
    pub fallbacks: Mutex<(u32, String)>,
}

#[derive(Debug, Default)]
pub struct Report {
    pub usage: Usage,
    pub warnings: Vec<String>,
}

fn kind_label(k: SourceKind, l: Lang) -> &'static str {
    match k {
        SourceKind::Audio => l.pick("registrazione", "recording"),
        SourceKind::Video => "video",
        SourceKind::Pdf => "PDF",
        SourceKind::Image => l.pick("foto/appunti", "photo/notes"),
        SourceKind::Text => l.pick("testo", "text"),
        SourceKind::Document => l.pick("documento", "document"),
        SourceKind::Other => "file",
    }
}

/// Whisper's initial prompt: context that steers spelling of names and terms (short: Whisper
/// keeps only the last ~220 tokens).
/// Whisper's initial prompt: it steers spelling and vocabulary, and only the last ~220 tokens
/// count, so the course's terms (the most useful part) are kept short and come first.
fn asr_prompt(l: Lang, course: &str, title: &str, terms: &str, notes: &str) -> String {
    let notes: String = notes.split_whitespace().take(40).collect::<Vec<_>>().join(" ");
    let terms: String = terms.split([',', ';', '\n']).map(str::trim).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(", ");
    let terms: String = terms.split_whitespace().take(40).collect::<Vec<_>>().join(" ");
    let mut p = l.pick(format!("Lezione universitaria di {course}: {title}."), format!("University lecture of {course}: {title}."));
    if !terms.is_empty() {
        p.push_str(&l.pick(format!(" Termini: {terms}."), format!(" Terms: {terms}.")));
    }
    if !notes.is_empty() {
        p.push(' ');
        p.push_str(&notes);
    }
    p
}

/// True when processing will transcribe at least one recording (to get Whisper ready first).
/// Whisper is needed: a recording without its transcript, or with parts whose cleanup failed
/// last time (`run` takes those up again through Whisper's saved text, `raw_part`, so nothing is
/// transcribed twice; without Whisper loaded they would go to the model as audio).
pub fn needs_transcription(lesson: &Lesson, force: bool) -> bool {
    lesson.meta.sources.iter().any(|s| matches!(s.kind, SourceKind::Audio | SourceKind::Video) && !s.omitted && (force || !lesson.path.join(extract_rel(s)).is_file() || cleanup_pending(&lesson.path, s)))
}

/// A recording with parts still in Whisper's text (cleanup failed last time): processing must
/// try the cleanup again even though the extract exists.
pub fn cleanup_pending(lesson_dir: &Path, s: &Source) -> bool {
    let dir = lesson_dir.join(parts_rel(s));
    fs::read_dir(&dir).into_iter().flatten().flatten().any(|e| {
        let name = e.file_name().to_string_lossy().to_string();
        name.strip_suffix(".whisper.md").is_some_and(|n| !dir.join(format!("{n}.md")).is_file())
    })
}

/// The "TIPO: slide | documento" line the PDF reading starts with, taken off the text.
pub(crate) fn pdf_kind(text: &str) -> (Option<crate::library::SourceRole>, String) {
    let t = text.trim_start();
    let (first, rest) = t.split_once('\n').unwrap_or((t, ""));
    let f = first.trim().trim_matches(['*', '_']).to_lowercase();
    let Some(kind) = f.strip_prefix("tipo:").map(str::trim) else { return (None, text.to_string()) };
    let role = if kind.starts_with("slide") { crate::library::SourceRole::Slide } else { crate::library::SourceRole::Materiale };
    (Some(role), rest.trim_start_matches('\n').to_string())
}

/// Times `[hh:mm:ss]` in a text, in seconds.
fn times(text: &str) -> Vec<u64> {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"\[(\d{2}):(\d{2}):(\d{2})\]").unwrap());
    RE.captures_iter(text).map(|c| c[1].parse::<u64>().unwrap_or(0) * 3600 + c[2].parse::<u64>().unwrap_or(0) * 60 + c[3].parse::<u64>().unwrap_or(0)).collect()
}

/// Some models write the heading after the time ("[00:20:41] ### Titolo"): put it on its own
/// line before the paragraph, where Markdown sees it.
fn fix_headings(text: &str) -> String {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"(?m)^(\[\d{2}:\d{2}:\d{2}\]) *(#{1,6} [^\n]+)\n+").unwrap());
    RE.replace_all(text, "$2\n\n$1 ").into_owned()
}

/// Some models escape part of a JSON string twice: the text then holds a literal `\n` where a
/// line break should be and `\\mu` (a KaTeX line break, then "mu") for `\mu`. Seen on a summary
/// in Nov 2026, from the middle of the text on. Undone only with proof: a `\n` followed by
/// something no LaTeX command can be (`\n\n`, `\n#`, `\n- `…). Inside formulas `\n` + letter
/// stays (`\nu`, `\neq`); `\\` stays unless a letter or `,;:!{}|` follows it.
pub fn fix_double_escapes(text: &str) -> String {
    // Code may hold a real `\n\n` ("print(\"a\\n\\nb\")"): no proof there.
    static CODE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"(?s)```.*?```|`[^`\n]*`").unwrap());
    let prose = CODE.replace_all(text, "");
    let b = prose.as_bytes();
    let proof = b.windows(3).any(|w| w[0] == b'\\' && w[1] == b'n' && matches!(w[2], b'\\' | b'#' | b'-' | b'*' | b'>' | b'|' | b'$' | b' ' | b'0'..=b'9'))
        || prose.ends_with("\\n");
    if !proof {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut math = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '$' => {
                if chars.peek() == Some(&'$') {
                    chars.next();
                    out.push('$');
                }
                out.push('$');
                math = !math;
            }
            '\\' => match chars.peek().copied() {
                Some('\\') => {
                    chars.next();
                    match chars.peek().copied() {
                        Some(n) if n.is_ascii_alphabetic() || ",;:!{}|".contains(n) => out.push('\\'),
                        // `\\\\` is a LaTeX line break escaped twice.
                        Some('\\') if chars.clone().nth(1) == Some('\\') => {
                            chars.next();
                            chars.next();
                            out.push_str("\\\\");
                        }
                        _ => out.push_str("\\\\"),
                    }
                }
                Some('n') => {
                    chars.next();
                    let word: String = chars.clone().take_while(|c| c.is_ascii_alphabetic()).collect();
                    if math && is_n_command(&format!("n{word}")) {
                        out.push_str("\\n");
                    } else {
                        out.push('\n');
                    }
                }
                Some('"') => {
                    chars.next();
                    out.push('"');
                }
                Some('$') => {
                    chars.next();
                    out.push_str("\\$");
                }
                _ => out.push('\\'),
            },
            _ => out.push(c),
        }
    }
    out
}

/// LaTeX commands starting with "n", told apart from a line break before a word in a formula.
fn is_n_command(w: &str) -> bool {
    matches!(
        w,
        "nu" | "ne" | "neq" | "nabla" | "neg" | "not" | "notin" | "ni" | "nmid" | "newline" | "nonumber" | "nleq" | "ngeq" | "nless" | "ngtr" | "nsubseteq" | "nsupseteq" | "nexists" | "nearrow" | "nwarrow" | "natural" | "napprox" | "nsim" | "ncong" | "nparallel" | "nRightarrow" | "nLeftarrow" | "nrightarrow" | "nleftarrow" | "nolimits" | "normalsize"
    )
}

fn fix_study(mut s: Study) -> Study {
    for t in &mut s.glossario {
        t.termine = fix_double_escapes(&t.termine);
        t.definizione = fix_double_escapes(&t.definizione);
    }
    for c in &mut s.flashcard {
        c.domanda = fix_double_escapes(&c.domanda);
        c.risposta = fix_double_escapes(&c.risposta);
    }
    for q in &mut s.domande_esame {
        q.domanda = fix_double_escapes(&q.domanda);
        q.traccia = fix_double_escapes(&q.traccia);
    }
    s
}

/// A cleaned transcript must keep the content and the times of the raw one: at least half of
/// the words and of the times, all times inside the part (a few seconds of slack).
fn plausible_cleanup(raw: &str, cleaned: &str, start: u64, end: u64) -> bool {
    let words = |t: &str| t.split_whitespace().count();
    let (rt, ct) = (times(raw), times(cleaned));
    words(cleaned) * 2 >= words(raw) && ct.len() * 2 >= rt.len() && ct.iter().all(|&t| t + 5 >= start && t <= end + 5)
}

/// A piece cleaned again keeps at least half the words and one time, all inside the piece.
/// Looser than `plausible_cleanup`: merging 3 short paragraphs into 1 is fine for a piece.
fn plausible_fragment(raw: &str, cleaned: &str, start: u64, end: u64) -> bool {
    let words = |t: &str| t.split_whitespace().count();
    let ct = times(cleaned);
    words(cleaned) * 2 >= words(raw) && !ct.is_empty() && ct.iter().all(|&t| t + 5 >= start && t <= end + 5)
}

/// The paragraphs of a timed text (each starts with `[hh:mm:ss]`) from `from` up to `to`
/// (excluded; None = to the end).
fn paragraphs_between(text: &str, from: u64, to: Option<u64>) -> String {
    text.split("\n\n")
        .map(str::trim)
        .filter(|p| match times(p.lines().next().unwrap_or("")).first() {
            Some(&t) => t >= from && to.is_none_or(|to| t < to),
            None => false,
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Cleans again, with `model`, the piece of Whisper's text of a recording from `from` up to
/// `to`. Nothing is saved: the user sees the result and decides. Returns (text, usage, model).
#[allow(clippy::too_many_arguments)]
pub async fn reclean<A: Ai>(ai: &A, model: &str, lib: &crate::library::Library, lesson_id: &str, source_id: &str, from: u64, to: Option<u64>, cancel: &CancellationToken) -> Result<(String, Usage, String)> {
    let lesson = lib.lesson(lesson_id)?;
    let s = lesson.meta.sources.iter().find(|s| s.id == source_id).ok_or_else(|| anyhow!(crate::tr!("fonte non trovata", "source not found")))?;
    let raw = fs::read_to_string(lesson.path.join(raw_rel(s)))
        .ok()
        .or_else(|| whisper_parts(&lesson.path, s))
        .ok_or_else(|| anyhow!(crate::tr!("questa trascrizione non ha il testo di Whisper (le lezioni elaborate prima della versione 0.8 non lo conservano): rifalla con «Rielabora anche le fonti»", "this transcript has no Whisper text (lectures processed before version 0.8 do not keep it): redo it with “Process the sources again too”")))?;
    let piece = paragraphs_between(&raw, from, to);
    if piece.trim().is_empty() {
        bail!(crate::tr!("nel testo di Whisper non c'è niente fra {} e {}", "there is nothing in Whisper's text between {} and {}", hms(from), to.map(hms).unwrap_or_else(|| crate::tr!("la fine", "the end"))));
    }
    let course = lib.course(&lesson.course_id)?;
    let (spoken, _) = lib.langs(&lesson);
    let req = Request {
        system: Some(prompts::system(spoken)),
        parts: vec![Part::Text(prompts::clean_fragment(spoken, &course.meta.name, &lesson.meta.title, &lib.terms_for(&course.meta), &piece))],
        max_output_tokens: Some(32_000),
        thinking: Some("LOW"),
        retries: Some(3),
        ..Default::default()
    };
    let a = ai.generate(model, &req, cancel).await?;
    let used = if a.model.is_empty() { model.to_string() } else { a.model.clone() };
    if a.finish == "MAX_TOKENS" {
        bail!(crate::tr!("la risposta del modello è stata troncata: scegli un pezzo più corto", "the model's answer was truncated: choose a shorter piece"));
    }
    let t = fix_headings(a.text.trim());
    let end = to.unwrap_or_else(|| times(&piece).last().copied().unwrap_or(from) + 60);
    if !plausible_fragment(&piece, &t, from, end) {
        bail!(crate::tr!("la nuova ripulitura ha perso parte del testo o dei minuti: riprova, o scegli un altro modello", "the new cleanup lost part of the text or of the timestamps: try again, or choose another model"));
    }
    Ok((t, a.usage, used))
}

/// Result of the cleanup of one part (its text is Whisper's when it failed).
struct Cleaned {
    text: String,
    usage: Usage,
    warning: Option<String>,
    /// Gemini did not answer at all (overloaded, offline): skip the cleanup of the next parts.
    unreachable: bool,
}

/// What a part needs: nothing (already cleaned) or the cleanup of Whisper's text.
enum RawPart {
    Done(String),
    Raw(String),
}

impl<A: Ai> Ctx<'_, A> {
    fn note_fallback(&self, a: &crate::ai::Answer) {
        if !a.model.is_empty() {
            let mut f = self.fallbacks.lock().unwrap();
            f.0 += 1;
            f.1 = a.model.clone();
        }
    }

    fn check(&self) -> Result<()> {
        if self.cancel.is_cancelled() { bail!("annullato") } else { Ok(()) }
    }

    /// A file as a prompt part: inline when small, uploaded otherwise. Returns the remote name
    /// to delete afterwards.
    async fn file_part(&self, path: &Path, mime: &str) -> Result<(Part, Option<String>)> {
        let size = fs::metadata(path)?.len();
        if size <= self.ai.inline_max() {
            return Ok((Part::Inline { mime: mime.into(), data: fs::read(path)? }, None));
        }
        let f = self.ai.upload(path, mime, self.cancel).await?;
        Ok((Part::File { mime: f.mime.clone().unwrap_or_else(|| mime.into()), uri: f.uri }, Some(f.name)))
    }

    /// One request on a media part: (text, complete). `[SILENZIO]` becomes empty.
    async fn ask_media(&self, part: Part, l: Lang, prompt: String, usage: &mut Usage) -> Result<(String, bool)> {
        let req = Request { system: Some(prompts::system(l)), parts: vec![part, Part::Text(prompt)], max_output_tokens: Some(32_000), thinking: Some("LOW"), ..Default::default() };
        (self.detail)(crate::tr!("In attesa della risposta del modello…", "Waiting for the model's answer…"));
        let a = self.ai.generate(&self.models.transcribe, &req, self.cancel).await?;
        self.note_fallback(&a);
        usage.add(&a.usage);
        let t = a.text.trim();
        Ok((if t == "[SILENZIO]" { String::new() } else { t.to_string() }, a.finish != "MAX_TOKENS"))
    }

    /// Recording → transcript. Split here in `window`-second parts (each minute billed once);
    /// every part is saved as soon as it is done (`estratti/.parti/<file>/NN.md`), so after a
    /// failure (often the daily quota) the next run resumes from there. A format we cannot
    /// decode goes whole, with time windows in the prompt (costs more).
    #[allow(clippy::too_many_arguments)]
    async fn transcribe(&self, lesson_id: &str, s: &Source, path: &Path, mime: &str, video: bool, step: (u32, u32), usage: &mut Usage, warnings: &mut Vec<String>) -> Result<(String, bool)> {
        let label = &s.original_name;
        (self.progress)(step.0 as f64, step.1, crate::tr!("Preparazione di {label}…", "Preparing {label}…"));
        let tmp = std::env::temp_dir().join(format!("lezioni-{}", uuid::Uuid::new_v4()));
        let (p, d, c, w) = (path.to_path_buf(), tmp.clone(), self.cancel.clone(), self.window);
        let split = tokio::task::spawn_blocking(move || audio::split(&p, &d, w, &c)).await?;
        // Transcription on the computer (always with OpenRouter): the audio must never go to the
        // model, which with OpenRouter refuses it anyway ("troppo grande").
        if self.models.engine == Engine::Local && self.asr.is_none() {
            let _ = fs::remove_dir_all(&tmp);
            log::warn!("{label}: trascrizione locale senza Whisper caricato");
            bail!(crate::tr!("la trascrizione sul computer non era pronta: rilancia l'elaborazione", "the transcription on the computer was not ready: run the processing again"));
        }
        let result = match split {
            Ok(chunks) => match &self.asr {
                Some(asr) => self.transcribe_local(asr, lesson_id, s, &chunks, step, usage, warnings).await,
                None => self.transcribe_chunks(lesson_id, s, &chunks, step, usage, warnings).await,
            },
            Err(e) if e.to_string() == "annullato" => Err(e),
            Err(e) if self.asr.is_some() => Err(anyhow!(crate::tr!("non riesco a leggere l'audio ({e:#}): convertila in m4a o mp3 e aggiungila di nuovo, oppure scegli la trascrizione con Gemini nelle Impostazioni", "cannot read the audio ({e:#}): convert it to m4a or mp3 and add it again, or choose the transcription with Gemini in Settings"))),
            Err(e) => {
                log::warn!("{label}: {e:#}");
                warnings.push(crate::tr!("{label}: non riesco a dividere la registrazione in parti ({e:#}), la invio intera e costa di più; convertila in m4a o mp3 per risparmiare", "{label}: cannot split the recording into parts ({e:#}), sending it whole, which costs more; convert it to m4a or mp3 to save"));
                self.transcribe_whole(path, mime, video, label, self.archive.langs(lesson_id)?.0, step, usage).await
            }
        };
        let _ = fs::remove_dir_all(&tmp);
        result
    }

    /// Whisper on one part, in a blocking thread; its percentage goes to the detail line.
    async fn whisper(&self, asr: &Arc<dyn Asr>, wav: &Path, l: Lang, prompt: &str, cleaning: &Mutex<Option<usize>>) -> Result<Vec<asr::Segment>> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<i32>();
        let (a, w, p, c) = (asr.clone(), wav.to_path_buf(), prompt.to_string(), self.cancel.clone());
        let mut job = tokio::task::spawn_blocking(move || a.transcribe(&w, l, &p, &c, &move |v| {
            let _ = tx.send(v);
        }));
        loop {
            tokio::select! {
                r = &mut job => return r?,
                Some(v) = rx.recv() => {
                    let other = cleaning.lock().unwrap().map(|j| crate::tr!(" · ripulitura della parte {}", " · cleaning up part {}", j + 1)).unwrap_or_default();
                    (self.detail)(format!("Whisper: {v}%{other}"));
                }
            }
        }
    }

    /// Part `i` from Whisper, or what was saved by an earlier run.
    #[allow(clippy::too_many_arguments)]
    async fn raw_part(&self, asr: &Arc<dyn Asr>, lesson_id: &str, s: &Source, chunks: &[audio::Chunk], i: usize, step: (u32, u32), l: Lang, prompt: &str, cleaning: &Mutex<Option<usize>>) -> Result<RawPart> {
        let dir = self.archive.lesson(lesson_id)?.path.join(parts_rel(s));
        if let Ok(t) = fs::read_to_string(dir.join(format!("{:02}.md", i + 1))) {
            return Ok(RawPart::Done(t));
        }
        let raw_rel = parts_rel(s).join(format!("{:02}.whisper.md", i + 1));
        let c = &chunks[i];
        let total = chunks.last().map(|c| c.start + c.secs.round() as u64).unwrap_or(0);
        let frac = step.0 as f64 + i as f64 / chunks.len() as f64;
        if let Ok(t) = fs::read_to_string(dir.join(format!("{:02}.whisper.md", i + 1))) {
            (self.progress)(frac, step.1, crate::tr!("Ripulitura di {}, parte {} di {} (già trascritta sul computer)", "Cleaning up {}, part {} of {} (already transcribed on the computer)", s.original_name, i + 1, chunks.len()));
            return Ok(RawPart::Raw(t));
        }
        (self.progress)(frac, step.1, crate::tr!("Trascrizione sul computer di {}, parte {} di {}: {}–{} di {}", "Transcribing {} on the computer, part {} of {}: {}–{} of {}", s.original_name, i + 1, chunks.len(), hms(c.start), hms(c.start + c.secs.round() as u64), hms(total)));
        let segs = self.whisper(asr, &c.path, l, prompt, cleaning).await?;
        // What Whisper invents on silence (a break) becomes one pause line before the cleanup.
        let text = crate::pauses::mark_until(&asr::paragraphs(&segs, c.start), Some(c.start + c.secs.round() as u64), l).0;
        if text.trim().is_empty() {
            // Silence: nothing to clean.
            self.archive.write(lesson_id, &parts_rel(s).join(format!("{:02}.md", i + 1)), b"")?;
            return Ok(RawPart::Done(String::new()));
        }
        self.archive.write(lesson_id, &raw_rel, text.as_bytes())?;
        Ok(RawPart::Raw(text))
    }

    /// Gemini cleans Whisper's text of part `i`. On any failure the raw text is used, and the
    /// next run tries the cleanup again.
    #[allow(clippy::too_many_arguments)]
    async fn clean_part(&self, lesson_id: &str, s: &Source, chunks: &[audio::Chunk], i: usize, raw: String, l: Lang, context: &(String, String, String)) -> Result<Cleaned> {
        let c = &chunks[i];
        let (start, end) = (c.start, c.start + c.secs.round() as u64);
        let req = Request {
            system: Some(prompts::system(l)),
            parts: vec![Part::Text(prompts::clean_transcript(l, i + 1, chunks.len(), &context.0, &context.1, &context.2, &raw))],
            max_output_tokens: Some(32_000),
            thinking: Some("LOW"),
            // Best effort (Whisper's text is already there): do not wait minutes on an overloaded
            // Gemini; two 503 still switch to the fallback model.
            retries: Some(3),
            ..Default::default()
        };
        let not_clean = |why: String| crate::tr!("{}: parte da {} non ripulita ({why}); c'è il testo di Whisper, verrà ripulita alla prossima elaborazione", "{}: part from {} not cleaned up ({why}); Whisper's text is there, it will be cleaned up at the next processing", s.original_name, hms(start));
        match self.ai.generate(&self.models.transcribe, &req, self.cancel).await {
            Err(e) if e.to_string() == "annullato" => Err(e),
            Err(e) => Ok(Cleaned { text: raw, usage: Usage::default(), warning: Some(not_clean(format!("{e:#}"))), unreachable: true }),
            Ok(a) => {
                self.note_fallback(&a);
                let t = fix_headings(a.text.trim());
                if a.finish == "MAX_TOKENS" {
                    Ok(Cleaned { text: raw, usage: a.usage, warning: Some(not_clean(crate::tr!("risposta troncata", "answer truncated"))), unreachable: false })
                } else if !plausible_cleanup(&raw, &t, start, end) {
                    log::warn!("ripulitura scartata per {} parte {}: {} parole e {} tempi contro {} e {}", s.original_name, i + 1, t.split_whitespace().count(), times(&t).len(), raw.split_whitespace().count(), times(&raw).len());
                    Ok(Cleaned { text: raw, usage: a.usage, warning: Some(not_clean(crate::tr!("la ripulitura aveva perso parti del testo o dei minuti", "the cleanup had lost parts of the text or of the timestamps"))), unreachable: false })
                } else {
                    self.archive.write(lesson_id, &parts_rel(s).join(format!("{:02}.md", i + 1)), t.as_bytes())?;
                    Ok(Cleaned { text: t, usage: a.usage, warning: None, unreachable: false })
                }
            }
        }
    }

    /// Recording → transcript with Whisper, part by part; Gemini cleans part N while Whisper
    /// does part N+1. Raw parts are kept (`NN.whisper.md`) until cleaned (`NN.md`).
    #[allow(clippy::too_many_arguments)]
    async fn transcribe_local(&self, asr: &Arc<dyn Asr>, lesson_id: &str, s: &Source, chunks: &[audio::Chunk], step: (u32, u32), usage: &mut Usage, warnings: &mut Vec<String>) -> Result<(String, bool)> {
        let lesson = self.archive.lesson(lesson_id)?;
        let lib = self.archive.lib()?;
        let course = lib.courses()?.into_iter().find(|c| c.meta.id == lesson.course_id).map(|c| (lib.terms_for(&c.meta), c.meta.name)).map(|(t, n)| (n, t)).unwrap_or_else(|| (String::new(), lib.global_terms()));
        let context = (course.0, lesson.meta.title.clone(), course.1);
        let spoken = lib.langs(&lesson).0;
        let prompt = asr_prompt(spoken, &context.0, &context.1, &context.2, &lesson.meta.notes);
        let n = chunks.len();
        let mut texts: Vec<String> = vec![String::new(); n];
        let mut skipped = 0;
        let cleaning = Mutex::new(None);
        let mut pending: Option<(usize, String)> = None;
        // After a cleanup that Gemini could not answer, the other parts keep Whisper's text.
        let mut gave_up = false;
        for i in 0..=n {
            self.check()?;
            let p = pending.take();
            let next = async {
                if i < n { self.raw_part(asr, lesson_id, s, chunks, i, step, spoken, &prompt, &cleaning).await.map(Some) } else { Ok(None) }
            };
            let skip = gave_up;
            let clean = async {
                let (j, raw) = p?;
                if skip {
                    return Some((j, Ok(Cleaned { text: raw, usage: Usage::default(), warning: None, unreachable: false })));
                }
                *cleaning.lock().unwrap() = Some(j);
                if i == n {
                    (self.detail)(crate::tr!("Ripulitura della parte {} di {n}…", "Cleaning up part {} of {n}…", j + 1));
                }
                let r = self.clean_part(lesson_id, s, chunks, j, raw, spoken, &context).await;
                *cleaning.lock().unwrap() = None;
                Some((j, r))
            };
            let (next, cleaned) = futures::join!(next, clean);
            if let Some((j, r)) = cleaned {
                let c = r?;
                usage.add(&c.usage);
                gave_up |= c.unreachable;
                if skip {
                    skipped += 1;
                }
                warnings.extend(c.warning);
                texts[j] = c.text;
            }
            match next? {
                Some(RawPart::Done(t)) => texts[i] = t,
                Some(RawPart::Raw(t)) => pending = Some((i, t)),
                None => {}
            }
        }
        if skipped > 0 {
            warnings.push(crate::tr!("{}: il servizio AI non risponde, {skipped} parti restano con il testo di Whisper; verranno ripulite alla prossima elaborazione", "{}: the AI service is not answering, {skipped} parts keep Whisper's text; they will be cleaned up at the next processing", s.original_name));
        }
        // Complete even with parts not cleaned: the text is saved and shown, and the parts left
        // in Whisper's text are found again by `cleanup_pending`.
        // A pause across two parts becomes one.
        Ok((crate::pauses::mark(&texts.into_iter().filter(|t| !t.trim().is_empty()).collect::<Vec<_>>().join("\n\n"), spoken).0, true))
    }

    async fn transcribe_chunks(&self, lesson_id: &str, s: &Source, chunks: &[audio::Chunk], step: (u32, u32), usage: &mut Usage, warnings: &mut Vec<String>) -> Result<(String, bool)> {
        let label = &s.original_name;
        let spoken = self.archive.langs(lesson_id)?.0;
        let total = chunks.last().map(|c| c.start + c.secs.round() as u64).unwrap_or(0);
        let mut texts = Vec::new();
        let mut complete = true;
        for (i, c) in chunks.iter().enumerate() {
            self.check()?;
            let rel = parts_rel(s).join(format!("{:02}.md", i + 1));
            if let Ok(t) = fs::read_to_string(self.archive.lesson(lesson_id)?.path.join(&rel)) {
                texts.push(t);
                continue;
            }
            let end = c.start + c.secs.round() as u64;
            let frac = step.0 as f64 + i as f64 / chunks.len() as f64;
            (self.progress)(frac, step.1, crate::tr!("Trascrizione di {label}, parte {} di {}: {}–{} di {}", "Transcribing {label}, part {} of {}: {}–{} of {}", i + 1, chunks.len(), hms(c.start), hms(end), hms(total)));
            let (part, remote) = self.file_part(&c.path, "audio/wav").await?;
            let r = self.ask_media(part, spoken, prompts::transcribe_clip(spoken, i + 1, chunks.len(), &hms(c.start)), usage).await;
            if let Some(n) = remote {
                self.ai.delete_file(&n).await;
            }
            let (t, ok) = r?;
            let t = shift_times(&t, c.start);
            if ok {
                self.archive.write(lesson_id, &rel, t.as_bytes())?;
            } else {
                complete = false;
                warnings.push(crate::tr!("{label}: la parte da {} è stata troncata da Gemini", "{label}: the part from {} was truncated by Gemini", hms(c.start)));
            }
            texts.push(t);
        }
        Ok((texts.into_iter().filter(|t| !t.trim().is_empty()).collect::<Vec<_>>().join("\n\n"), complete))
    }

    /// Fallback: the whole file uploaded once, transcribed in time windows. Each window bills
    /// the whole file again.
    #[allow(clippy::too_many_arguments)]
    async fn transcribe_whole(&self, path: &Path, mime: &str, video: bool, label: &str, l: Lang, step: (u32, u32), usage: &mut Usage) -> Result<(String, bool)> {
        (self.progress)(step.0 as f64, step.1, crate::tr!("Caricamento di {label} su Gemini…", "Uploading {label} to Gemini…"));
        let f = self.ai.upload(path, mime, self.cancel).await?;
        let file = Part::File { mime: f.mime.clone().unwrap_or_else(|| mime.into()), uri: f.uri.clone() };
        let result = async {
            let per_sec = if video { VIDEO_TOKENS_PER_SEC } else { AUDIO_TOKENS_PER_SEC };
            let secs = self.ai.count_tokens(&self.models.transcribe, std::slice::from_ref(&file), self.cancel).await.map_err(|e| e.context(crate::tr!("durata della registrazione", "length of the recording")))? / per_sec;
            let windows = secs.div_ceil(self.window).max(1);
            let (mut out, mut complete) = (Vec::new(), true);
            for i in 0..windows {
                self.check()?;
                let (from, to) = (hms(i * self.window), hms((i + 1) * self.window));
                (self.progress)(step.0 as f64 + i as f64 / windows as f64, step.1, crate::tr!("Trascrizione di {label}: {from}–{} di {}", "Transcribing {label}: {from}–{} of {}", hms(((i + 1) * self.window).min(secs)), hms(secs)));
                let (t, ok) = self.ask_media(file.clone(), l, prompts::transcribe_window(l, &from, &to, i == 0, i + 1 == windows), usage).await?;
                complete &= ok;
                if !t.is_empty() {
                    out.push(t);
                }
            }
            Ok::<_, anyhow::Error>((out.join("\n\n"), complete))
        }
        .await;
        self.ai.delete_file(&f.name).await;
        result
    }

    async fn read_with_model(&self, path: &Path, mime: &str, l: Lang, prompt: String, usage: &mut Usage) -> Result<(String, bool)> {
        let (part, remote) = self.file_part(path, mime).await?;
        let r = self.ask_media(part, l, prompt, usage).await;
        if let Some(n) = remote {
            self.ai.delete_file(&n).await;
        }
        r
    }

    /// (text, complete) of one source, or None when it cannot be read (with a warning).
    async fn extract(&self, lesson: &Lesson, s: &Source, step: (u32, u32), usage: &mut Usage, warnings: &mut Vec<String>) -> Result<Option<(String, bool)>> {
        let path = lesson.path.join(SOURCES_DIR).join(&s.file);
        let label = &s.original_name;
        let mime = mime_for(&path);
        Ok(Some(match (s.kind, mime) {
            (SourceKind::Text, _) => (fs::read_to_string(&path).with_context(|| crate::tr!("lettura di {label}", "reading {label}"))?, true),
            (SourceKind::Audio, Some(m)) => self.transcribe(&lesson.meta.id, s, &path, m, false, step, usage, warnings).await?,
            (SourceKind::Video, Some(m)) => self.transcribe(&lesson.meta.id, s, &path, m, true, step, usage, warnings).await?,
            (SourceKind::Image, Some(m)) => {
                (self.progress)(step.0 as f64, step.1, crate::tr!("Lettura di {label}…", "Reading {label}…"));
                let l = self.archive.langs(&lesson.meta.id)?.0;
                self.read_with_model(&path, m, l, prompts::image(l), usage).await?
            }
            (SourceKind::Pdf, Some(m)) => {
                (self.progress)(step.0 as f64, step.1, crate::tr!("Lettura di {label}…", "Reading {label}…"));
                let l = self.archive.langs(&lesson.meta.id)?.0;
                let (text, complete) = self.read_with_model(&path, m, l, prompts::pdf(l), usage).await?;
                // "TIPO: slide" on the first line: the model's guess of what the PDF is.
                let (kind, text) = pdf_kind(&text);
                if let Some(role) = kind {
                    let _g = self.archive.lock.lock().unwrap_or_else(|e| e.into_inner());
                    self.archive.lib()?.set_source_role(&lesson.meta.id, &s.id, role, true)?;
                }
                (text, complete)
            }
            (SourceKind::Document, _) => {
                warnings.push(crate::tr!("{label}: i file di Office non si possono leggere, esportalo in PDF e aggiungi il PDF", "{label}: Office files cannot be read, export it to PDF and add the PDF"));
                return Ok(None);
            }
            _ => {
                warnings.push(crate::tr!("{label}: tipo di file non supportato, saltato", "{label}: file type not supported, skipped"));
                return Ok(None);
            }
        }))
    }

    fn fallback_warning(&self, report: &mut Report) {
        let f = self.fallbacks.lock().unwrap();
        if f.0 > 0 {
            report.warnings.push(crate::tr!("Il modello era sovraccarico: {} richieste fatte con il modello di riserva {}", "The model was overloaded: {} requests made with the fallback model {}", f.0, f.1));
        }
    }

    /// Links each PDF marked as slides to each recording, when the link is missing or its texts
    /// changed. A failure is a warning: the summary is made anyway.
    async fn link_slides(&self, lesson_id: &str, report: &mut Report) -> Result<()> {
        let lesson = self.archive.lesson(lesson_id)?;
        let out = outputs(&lesson);
        let recs: Vec<&Source> = lesson.meta.sources.iter().filter(|s| matches!(s.kind, SourceKind::Audio | SourceKind::Video) && out.extracts.contains_key(&s.id)).collect();
        let pdfs: Vec<&Source> = lesson.meta.sources.iter().filter(|s| s.kind == SourceKind::Pdf && s.role == Some(crate::library::SourceRole::Slide) && out.extracts.contains_key(&s.id)).collect();
        for p in &pdfs {
            for r in &recs {
                let hash = crate::slides::inputs_hash(&out.extracts[&r.id], &out.extracts[&p.id]);
                if out.slides.iter().any(|a| a.recording == r.id && a.pdf == p.id && a.inputs == hash) {
                    continue;
                }
                self.check()?;
                (self.detail)(crate::tr!("Collego {} a {}…", "Linking {} to {}…", p.original_name, r.original_name));
                let lib = self.archive.lib()?;
                match crate::slides::align(self.ai, &self.models.transcribe, &lib, lesson_id, &r.id, &p.id, self.cancel).await {
                    Ok((a, usage)) => {
                        report.usage.add(&usage);
                        let _g = self.archive.lock.lock().unwrap_or_else(|e| e.into_inner());
                        crate::slides::save(&self.archive.lesson(lesson_id)?.path, a)?;
                    }
                    Err(e) if e.to_string() == "annullato" => return Err(e),
                    Err(e) => report.warnings.push(crate::tr!("{}: slide non collegate alla registrazione ({e:#})", "{}: slides not linked to the recording ({e:#})", p.original_name)),
                }
            }
        }
        Ok(())
    }

    /// Links the photos to the recordings (see `photos::link_lesson`).
    async fn link_photos(&self, lesson_id: &str, report: &mut Report) -> Result<()> {
        let lesson = self.archive.lesson(lesson_id)?;
        if !lesson.meta.sources.iter().any(|s| s.kind == SourceKind::Image) {
            return Ok(());
        }
        self.check()?;
        (self.detail)(crate::tr!("Aggancio le foto alla registrazione…", "Attaching the photos to the recording…"));
        let found = crate::photos::link_lesson(self.ai, &self.models.transcribe, &lesson, false, self.cancel).await?;
        report.usage.add(&found.usage);
        report.warnings.extend(found.warnings);
        if !found.links.is_empty() {
            let _g = self.archive.lock.lock().unwrap_or_else(|e| e.into_inner());
            crate::photos::save_found(&self.archive.lesson(lesson_id)?.path, found.links)?;
        }
        Ok(())
    }

    pub async fn run(&self, lesson_id: &str, opts: &Options) -> Result<Report> {
        let mut report = Report::default();
        let lesson = self.archive.lesson(lesson_id)?;
        if lesson.meta.sources.is_empty() {
            bail!(crate::tr!("la lezione non ha fonti: aggiungi registrazioni, appunti o PDF", "the lecture has no sources: add recordings, notes or PDFs"));
        }
        let total = lesson.meta.sources.len() as u32 + if opts.extract_only { 0 } else { 1 };
        let mut extracts: Vec<(String, Source, String)> = Vec::new();
        // Recordings that could not be transcribed: without them the summary would be wrong.
        let mut media_failed = Vec::new();
        // Folder name of the copies of the transcripts corrected by hand, if any is redone.
        let stamp = Utc::now().format("%Y-%m-%d %H-%M-%S").to_string();
        // Recordings first (the longest step, and the transcript is what the rest is read
        // against), then the other sources in the order they were added. The codes F1, F2…
        // follow this order: the recording is usually F1.
        let mut order: Vec<&Source> = lesson.meta.sources.iter().collect();
        order.sort_by_key(|s| !matches!(s.kind, SourceKind::Audio | SourceKind::Video));
        for (i, s) in order.into_iter().enumerate() {
            self.check()?;
            let step = (i as u32, total);
            // The folder may have been renamed since the job started.
            let current = self.archive.lesson(lesson_id)?;
            let existing = current.path.join(extract_rel(s));
            if opts.force {
                let _ = fs::remove_dir_all(current.path.join(parts_rel(s)));
                // A transcript corrected by hand is about to be redone: keep a copy.
                if s.edited.is_some() && existing.is_file() {
                    let _g = self.archive.lock.lock().unwrap_or_else(|e| e.into_inner());
                    let dest = current.path.join(EXTRACTS_DIR).join("versioni").join(&stamp);
                    fs::create_dir_all(&dest)?;
                    fs::copy(&existing, dest.join(format!("{}.md", s.file))).with_context(|| crate::tr!("copia della trascrizione corretta", "copying the corrected transcript"))?;
                }
            }
            // From a package without its file (the recording): the extract is all there is.
            if s.omitted {
                match fs::read_to_string(&existing) {
                    Ok(t) if !t.trim().is_empty() => extracts.push((format!("F{}", extracts.len() + 1), s.clone(), t)),
                    _ => report.warnings.push(crate::tr!("{}: il file non era nel pacchetto ricevuto e non c'è un testo estratto, saltato", "{}: the file was not in the received package and there is no extracted text, skipped", s.original_name)),
                }
                continue;
            }
            let text = if existing.is_file() && !opts.force && !cleanup_pending(&current.path, s) {
                Some(fs::read_to_string(&existing)?)
            } else {
                let before = report.warnings.len();
                match self.extract(&current, s, step, &mut report.usage, &mut report.warnings).await {
                    Ok(Some((t, true))) => {
                        self.archive.write(lesson_id, &extract_rel(s), t.as_bytes())?;
                        let _g = self.archive.lock.lock().unwrap_or_else(|e| e.into_inner());
                        let lib = self.archive.lib()?;
                        let dir = lib.lesson(lesson_id)?.path;
                        // Parts still waiting for the cleanup stay for the next run.
                        if !cleanup_pending(&dir, s) {
                            close_parts(&dir, s)?;
                        }
                        if s.edited.is_some() {
                            lib.set_source_edited(lesson_id, &s.id, None)?;
                        }
                        Some(t)
                    }
                    // Truncated: used for this summary, not kept as if it were complete.
                    Ok(Some((t, false))) => {
                        if report.warnings.len() == before {
                            report.warnings.push(crate::tr!("{}: testo incompleto (risposta troppo lunga), non salvato; verrà rifatto alla prossima elaborazione", "{}: incomplete text (answer too long), not saved; it will be redone at the next processing", s.original_name));
                        }
                        Some(t)
                    }
                    Ok(None) => None,
                    Err(e) if e.to_string() == "annullato" => return Err(e),
                    // One unreadable source does not stop the others.
                    Err(e) => {
                        if matches!(s.kind, SourceKind::Audio | SourceKind::Video) {
                            media_failed.push(format!("{}: {e:#}", s.original_name));
                        } else {
                            report.warnings.push(format!("{}: {e:#}", s.original_name));
                        }
                        None
                    }
                }
            };
            if let Some(t) = text.filter(|t| !t.trim().is_empty()) {
                extracts.push((format!("F{}", extracts.len() + 1), s.clone(), t));
            }
        }
        if !media_failed.is_empty() {
            bail!(crate::tr!(
                "trascrizione non riuscita, sintesi non generata:\n{}\nLe parti già trascritte sono salvate: rilancia l'elaborazione per riprendere da lì.",
                "transcription failed, summary not generated:\n{}\nThe parts already transcribed are saved: run the processing again to resume from there.",
                media_failed.join("\n")
            ));
        }
        self.link_slides(lesson_id, &mut report).await?;
        self.link_photos(lesson_id, &mut report).await?;
        if opts.extract_only {
            self.fallback_warning(&mut report);
            return Ok(report);
        }
        if extracts.is_empty() {
            bail!(crate::tr!("nessuna fonte leggibile: {}", "no readable source: {}", report.warnings.join("; ")));
        }
        self.check()?;
        (self.progress)((total - 1) as f64, total, crate::tr!("Sintesi, mappa e materiale di studio…", "Summary, map and study material…"));
        let lesson = self.archive.lesson(lesson_id)?;
        let lib = self.archive.lib()?;
        let course = lib.courses()?.into_iter().find(|c| c.meta.id == lesson.course_id).map(|c| (lib.terms_for(&c.meta), c.meta.name)).map(|(t, n)| (n, t)).unwrap_or_else(|| (String::new(), lib.global_terms()));
        let (spoken, ml) = lib.langs(&lesson);
        let sources: String = extracts.iter().map(|(code, s, t)| format!("[{code}] {} ({})\n{t}\n\n", s.original_name, kind_label(s.kind, ml))).collect();
        let code_of = |sid: &str| extracts.iter().find(|(_, s, _)| s.id == sid).map(|(c, _, _)| c.to_string());
        let marks = crate::bookmarks::prompt_lines(&crate::bookmarks::read(&lesson.path), ml, code_of);
        let slides = crate::slides::prompt_lines(&crate::slides::read(&lesson.path), ml, code_of);
        let photos = crate::photos::prompt_lines(&crate::photos::read(&lesson.path), &lesson.meta.sources, ml, code_of);
        let req = Request {
            system: Some(prompts::system(ml)),
            parts: vec![Part::Text(prompts::generate(ml, spoken, &course.0, &lesson.meta.title, &lesson.meta.date.to_string(), &lesson.meta.notes, &course.1, &marks, &slides, &photos, &sources))],
            schema: Some(prompts::generate_schema()),
            max_output_tokens: Some(48_000),
            ..Default::default()
        };
        (self.detail)(crate::tr!("In attesa della risposta del modello…", "Waiting for the model's answer…"));
        // Some models now and then answer with the schema's skeleton (every field empty): Haiku 5.5
        // about once in 7 with reasoning on (measured on OpenRouter, Nov 2026). Ask again, at most 3
        // times in all, and never replace the saved outputs with an empty summary.
        // After two empty answers, or one cut off (Haiku once wrote 39k tokens on a long lesson
        // and was truncated), the last try goes to the fallback model, when there is one.
        let fallback = Some(self.models.fallback.trim()).filter(|f| !f.is_empty() && *f != self.models.generate);
        let mut model = self.models.generate.clone();
        let mut attempt = 0;
        let (a, g) = loop {
            attempt += 1;
            let a = self.ai.generate(&model, &req, self.cancel).await?;
            self.note_fallback(&a);
            report.usage.add(&a.usage);
            let last = attempt == 3 || Some(model.as_str()) == fallback;
            if a.finish == "MAX_TOKENS" {
                let tail: String = a.text.chars().rev().take(600).collect::<Vec<_>>().into_iter().rev().collect();
                log::warn!("sintesi troncata ({model}, {} caratteri); fine della risposta: …{tail}", a.text.chars().count());
                match fallback {
                    Some(f) if !last => {
                        report.warnings.push(crate::tr!("La risposta di {model} era troppo lunga ed è stata troncata: sintesi fatta con il modello di riserva {f}", "The answer of {model} was too long and was truncated: summary made with the fallback model {f}"));
                        model = f.to_string();
                        attempt = 2;
                        (self.detail)(crate::tr!("Risposta troncata, nuovo tentativo con {f}…", "Answer truncated, trying again with {f}…"));
                        continue;
                    }
                    _ => bail!(crate::tr!("la risposta del modello è stata troncata: prova con meno fonti o scegli un altro modello per la sintesi nelle Impostazioni", "the model's answer was truncated: try with fewer sources or choose another model for the summary in Settings")),
                }
            }
            let g: Generated = serde_json::from_str(&a.text).map_err(|e| anyhow!(crate::tr!("risposta del modello non valida: {e}", "invalid answer from the model: {e}")))?;
            if !g.sintesi.trim().is_empty() {
                break (a, g);
            }
            log::warn!("sintesi vuota ({model}, tentativo {attempt})");
            if last {
                bail!(crate::tr!("il modello ha risposto tre volte con una sintesi vuota: riprova, o scegli un altro modello per la sintesi nelle Impostazioni", "the model answered three times with an empty summary: try again, or choose another model for the summary in Settings"));
            }
            if attempt == 2
                && let Some(f) = fallback
            {
                report.warnings.push(crate::tr!("{model} ha risposto due volte con una sintesi vuota: sintesi fatta con il modello di riserva {f}", "{model} answered twice with an empty summary: summary made with the fallback model {f}"));
                model = f.to_string();
                (self.detail)(crate::tr!("Sintesi vuota, nuovo tentativo con {f}…", "Empty summary, trying again with {f}…"));
            } else {
                (self.detail)(crate::tr!("Sintesi vuota, nuovo tentativo…", "Empty summary, trying again…"));
            }
        };

        self.archive.archive_outputs(lesson_id)?;
        let gdir = Path::new(GENERATED_DIR);
        self.archive.write(lesson_id, &gdir.join("sintesi.md"), fix_double_escapes(&g.sintesi).as_bytes())?;
        self.archive.write(lesson_id, &gdir.join("mappa.json"), &serde_json::to_vec_pretty(&g.mappa)?)?;
        self.archive.write(lesson_id, &gdir.join("mappa.mmd"), mermaid(&g.mappa).as_bytes())?;
        let study = fix_study(Study { glossario: g.glossario, flashcard: g.flashcard, domande_esame: g.domande_esame });
        self.archive.write(lesson_id, &gdir.join("studio.json"), &serde_json::to_vec_pretty(&study)?)?;
        let info = Info {
            generated: Utc::now(),
            model: if a.model.is_empty() { model.clone() } else { a.model.clone() },
            usage: report.usage.clone(),
            suggested_title: g.titolo,
            sources: extracts.iter().map(|(code, s, _)| (code.clone(), s.id.clone())).collect(),
            edited: None,
        };
        self.archive.write(lesson_id, &gdir.join("info.json"), &serde_json::to_vec_pretty(&info)?)?;
        self.fallback_warning(&mut report);
        (self.progress)(total as f64, total, crate::tr!("Fatto", "Done"));
        Ok(report)
    }
}

/// Everything the UI shows in the Trascrizione / Sintesi / Mappa / Studio tabs.
#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Outputs {
    /// source id → extracted text
    pub extracts: BTreeMap<String, String>,
    /// source id → Whisper's text before the cleanup (recordings transcribed on the computer)
    pub raw: BTreeMap<String, String>,
    pub summary: Option<String>,
    pub map: Option<MapNode>,
    pub study: Option<Study>,
    pub info: Option<Info>,
    /// The student's bookmarks (`segnaposti.json`).
    pub bookmarks: Vec<crate::bookmarks::Bookmark>,
    /// Earlier versions of the outputs and of the transcripts, newest first.
    pub versions: Vec<crate::versions::Version>,
    /// source id → pauses Whisper filled with invented words, still to be marked.
    pub pauses: BTreeMap<String, usize>,
    /// Slides linked to recordings (`allineamento.json`).
    pub slides: Vec<crate::slides::Alignment>,
    /// Photos linked to a minute of a recording (`foto.json`).
    pub photos: Vec<crate::photos::ImageLink>,
}

/// Just the study material (`generati/studio.json`), for the review.
pub fn outputs_study(lesson: &Lesson) -> Option<Study> {
    serde_json::from_slice(&fs::read(lesson.path.join(GENERATED_DIR).join("studio.json")).ok()?).ok().map(fix_study)
}

pub fn outputs(lesson: &Lesson) -> Outputs {
    let read = |rel: &Path| fs::read_to_string(lesson.path.join(rel)).ok();
    let gdir = Path::new(GENERATED_DIR);
    let extracts: BTreeMap<String, String> = lesson.meta.sources.iter().filter_map(|s| read(&extract_rel(s)).map(|t| (s.id.clone(), t))).collect();
    let pauses = lesson
        .meta
        .sources
        .iter()
        .filter(|s| matches!(s.kind, SourceKind::Audio | SourceKind::Video))
        .filter_map(|s| Some((s.id.clone(), crate::pauses::mark(extracts.get(&s.id)?, crate::lang::Lang::It).1)))
        .filter(|(_, n)| *n > 0)
        .collect();
    Outputs {
        pauses,
        extracts,
        // While parts wait for the cleanup, their Whisper text is still in `.parti/`.
        raw: lesson.meta.sources.iter().filter_map(|s| read(&raw_rel(s)).or_else(|| whisper_parts(&lesson.path, s)).map(|t| (s.id.clone(), t))).collect(),
        // Repaired when read too: summaries saved before the fix need no new (paid) run.
        summary: read(&gdir.join("sintesi.md")).map(|t| fix_double_escapes(&t)),
        map: read(&gdir.join("mappa.json")).and_then(|t| serde_json::from_str(&t).ok()),
        study: read(&gdir.join("studio.json")).and_then(|t| serde_json::from_str(&t).ok()).map(fix_study),
        info: read(&gdir.join("info.json")).and_then(|t| serde_json::from_str(&t).ok()),
        bookmarks: crate::bookmarks::read(&lesson.path),
        versions: crate::versions::list(lesson),
        slides: crate::slides::read(&lesson.path),
        photos: crate::photos::read(&lesson.path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{Answer, RemoteFile};
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

    /// Fake model with canned answers; counts calls, can fail one call or cut the photo reading.
    #[derive(Default)]
    struct Fake {
        uploads: AtomicU32,
        deletes: AtomicU32,
        calls: AtomicUsize,
        prompts: Mutex<Vec<String>>,
        /// 1-based index of the generate call that fails with a quota error.
        fail_call: Mutex<Option<usize>>,
        truncate_images: bool,
        /// Seconds the fallback (whole file) path believes the recording lasts.
        whole_secs: u64,
        /// The cleanup of Whisper's text fails (Gemini overloaded).
        fail_cleanup: std::sync::atomic::AtomicBool,
        /// How many summary answers come back as the empty skeleton (seen with Haiku).
        empty_summaries: AtomicUsize,
        /// The next N summaries are cut off (MAX_TOKENS).
        truncate_summaries: AtomicUsize,
        /// Model asked for each summary.
        summary_models: Mutex<Vec<String>>,
    }

    /// Fake Whisper: one sentence per part, 1 s after its start.
    #[derive(Default)]
    struct FakeAsr {
        calls: AtomicUsize,
        prompt: Mutex<String>,
        lang: Mutex<Option<Lang>>,
        /// The second part is a break: Whisper fills it with "grazie".
        thanks: std::sync::atomic::AtomicBool,
    }

    impl Asr for FakeAsr {
        fn transcribe(&self, wav: &Path, lang: Lang, prompt: &str, _c: &CancellationToken, progress: &(dyn Fn(i32) + Send + Sync)) -> Result<Vec<asr::Segment>> {
            assert!(wav.is_file());
            *self.lang.lock().unwrap() = Some(lang);
            let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            *self.prompt.lock().unwrap() = prompt.to_string();
            progress(100);
            if n == 2 && self.thanks.load(Ordering::SeqCst) {
                return Ok(vec![asr::Segment { start: 1.0, end: 2.0, text: "Grazie.".into() }, asr::Segment { start: 5.0, end: 9.0, text: "grazie grazie grazie".into() }]);
            }
            Ok(vec![asr::Segment { start: 1.0, end: 4.0, text: "ehm allora il simplesso".into() }])
        }
    }

    impl Ai for Fake {
        async fn generate(&self, model: &str, req: &Request, _c: &CancellationToken) -> Result<Answer> {
            let photos_req = req.parts.iter().any(|p| matches!(p, Part::Text(t) if t.contains("FOTO:\n")));
            if req.schema.is_some() && !photos_req {
                self.summary_models.lock().unwrap().push(model.to_string());
                if self.truncate_summaries.load(Ordering::SeqCst) > 0 {
                    self.truncate_summaries.fetch_sub(1, Ordering::SeqCst);
                    let usage = Usage { prompt_token_count: 100, candidates_token_count: 10, thoughts_token_count: 0, cost: None };
                    return Ok(Answer { text: "{\"sintesi\":\"## Dighe dighe dighe".into(), usage, finish: "MAX_TOKENS".into(), model: String::new() });
                }
            }
            let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            if *self.fail_call.lock().unwrap() == Some(n) {
                bail!("quota esaurita (HTTP 429)");
            }
            let prompt = req.parts.iter().find_map(|p| if let Part::Text(t) = p { Some(t.clone()) } else { None }).unwrap();
            self.prompts.lock().unwrap().push(prompt.clone());
            let usage = Usage { prompt_token_count: 100, candidates_token_count: 10, thoughts_token_count: 0, cost: None };
            let mut finish = "STOP";
            let text = if let Some(raw) = prompt.split("Trascrizione:\n").nth(1).filter(|_| prompt.contains("Ripuliscila")) {
                if self.fail_cleanup.load(Ordering::SeqCst) {
                    bail!("errore di Gemini (HTTP 503): high demand");
                }
                raw.replace("ehm allora ", "")
            } else if let Some(raw) = prompt.split("Pezzo:\n").nth(1) {
                raw.replace("ehm allora ", "")
            } else if photos_req {
                r#"{"foto":[{"codice":"FOTO1","registrazione":"REG1","inizio":"00:00:15","motivo":""}]}"#.into()
            } else if prompt.contains("SLIDE:\n") {
                r#"{"passaggi":[{"inizio":"00:00:01","pagina":1},{"inizio":"00:00:11","pagina":2}]}"#.into()
            } else if req.schema.is_some() && self.empty_summaries.load(Ordering::SeqCst) > 0 {
                self.empty_summaries.fetch_sub(1, Ordering::SeqCst);
                r#"{"titolo":"","sintesi":"","mappa":{"etichetta":""},"glossario":[],"flashcard":[],"domande_esame":[]}"#.into()
            } else if req.schema.is_some() {
                serde_json::json!({
                    "titolo": "Il simplesso",
                    "sintesi": "## Simplesso\nSi parte da una base ammissibile. [F1 00:00:10]",
                    "mappa": { "etichetta": "Simplesso", "figli": [{ "etichetta": "Base (ammissibile)" }] },
                    "glossario": [{ "termine": "Base", "definizione": "insieme di variabili" }],
                    "flashcard": [{ "domanda": "Cos'è una base?", "risposta": "…" }],
                    "domande_esame": [{ "domanda": "Descrivi il simplesso", "traccia": "…" }]
                })
                .to_string()
            } else if prompt.starts_with("Questo è un PDF") {
                "TIPO: slide\n## Pagina 1\nIl simplesso\n\n## Pagina 2\nIl pivot".into()
            } else if prompt.contains("Questa è la parte") || prompt.contains("È la registrazione") {
                "[00:05] testo".into()
            } else if prompt.contains("Trascrivi SOLO") || prompt.contains("fino alla fine") || prompt.contains("tutta la registrazione") {
                "[00:00:01] finestra".into()
            } else {
                if self.truncate_images {
                    finish = "MAX_TOKENS";
                }
                "# Appunti\n$x_1 + x_2 \\le 4$".into()
            };
            Ok(Answer { text, usage, finish: finish.into(), model: String::new() })
        }
        async fn count_tokens(&self, _m: &str, _p: &[Part], _c: &CancellationToken) -> Result<u64> {
            Ok(self.whole_secs * 32)
        }
        async fn upload(&self, _p: &Path, mime: &str, _c: &CancellationToken) -> Result<RemoteFile> {
            self.uploads.fetch_add(1, Ordering::SeqCst);
            Ok(RemoteFile { name: "files/x".into(), uri: "https://x/files/x".into(), state: Some("ACTIVE".into()), mime: Some(mime.into()) })
        }
        async fn delete_file(&self, _n: &str) {
            self.deletes.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// 44.1 kHz mono tone of `secs` seconds.
    fn tone(path: &Path, secs: u32) {
        let spec = hound::WavSpec { channels: 1, sample_rate: 44_100, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..44_100 * secs {
            w.write_sample(((i as f32 * 0.06).sin() * 6000.0) as i16).unwrap();
        }
        w.finalize().unwrap();
    }

    struct Setup {
        _t: tempfile::TempDir,
        lib: Library,
        archive: Archive,
        lesson: String,
    }

    /// A lesson with the given (name, content) sources; `None` content = a 25 s tone.
    fn setup(files: &[(&str, Option<&[u8]>)]) -> Setup {
        let t = tempfile::tempdir().unwrap();
        let lib = Library::open(t.path().join("archivio")).unwrap();
        let c = lib.create_course("Ricerca operativa", "", "").unwrap();
        let l = lib.create_lesson(&c.meta.id, "Lezione 5", "2026-11-10".parse().unwrap()).unwrap();
        let mut paths = Vec::new();
        for (n, b) in files {
            let p = t.path().join(n);
            match b {
                Some(b) => fs::write(&p, b).unwrap(),
                None => tone(&p, 25),
            }
            paths.push(p);
        }
        lib.add_files(&l.meta.id, &paths).unwrap();
        let archive = Archive { dir: t.path().join("archivio"), lock: Default::default() };
        Setup { _t: t, lib, archive, lesson: l.meta.id }
    }

    async fn run(st: &Setup, ai: &Fake, opts: Options) -> Result<Report> {
        run_with(st, ai, None, opts).await
    }

    async fn run_with(st: &Setup, ai: &Fake, asr: Option<Arc<dyn Asr>>, opts: Options) -> Result<Report> {
        // Without Whisper the recordings go to the model ("Con Gemini"), as the app does.
        let models = Models { engine: if asr.is_some() { Engine::Local } else { Engine::Gemini }, ..Models::default() };
        run_models(st, ai, asr, opts, models).await
    }

    async fn run_models(st: &Setup, ai: &Fake, asr: Option<Arc<dyn Asr>>, opts: Options, models: Models) -> Result<Report> {
        let cancel = CancellationToken::new();
        let progress = |_: f64, _: u32, _: String| {};
        let detail = |_: String| {};
        let ctx = Ctx { ai, archive: &st.archive, models: &models, cancel: &cancel, progress: &progress, detail: &detail, window: 10, asr, fallbacks: Default::default() };
        ctx.run(&st.lesson, &opts).await
    }

    #[tokio::test]
    async fn local_transcription_then_cleanup() {
        let st = setup(&[("reg.wav", None)]);
        let ai = Fake::default();
        let asr = Arc::new(FakeAsr::default());
        let r = run_with(&st, &ai, Some(asr.clone()), Options::default()).await.unwrap();
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        assert_eq!(asr.calls.load(Ordering::SeqCst), 3);
        assert_eq!(count(&ai, "Ripuliscila"), 3);
        assert_eq!(count(&ai, "Trascrivila tutta"), 0, "no audio to Gemini");
        let l = st.lib.lesson(&st.lesson).unwrap();
        let out = outputs(&l);
        assert_eq!(out.extracts[&l.meta.sources[0].id], "[00:00:01] il simplesso\n\n[00:00:11] il simplesso\n\n[00:00:21] il simplesso");
        assert!(!l.path.join(parts_rel(&l.meta.sources[0])).exists());
        assert!(out.summary.is_some());
        // A summary saved broken before the fix is shown repaired.
        fs::write(l.path.join(GENERATED_DIR).join("sintesi.md"), r"## A\n\n## B\n$\\mu$").unwrap();
        assert_eq!(outputs(&l).summary.unwrap(), "## A\n\n## B\n$\\mu$");
        // Whisper's text stays, to be compared with the cleaned one.
        let raw = &out.raw[&l.meta.sources[0].id];
        assert_eq!(raw, "[00:00:01] ehm allora il simplesso\n\n[00:00:11] ehm allora il simplesso\n\n[00:00:21] ehm allora il simplesso");
        assert!(l.path.join(raw_rel(&l.meta.sources[0])).is_file());
        // Removing the recording removes its Whisper text too.
        st.lib.remove_source(&l.meta.id, &l.meta.sources[0].id).unwrap();
        assert!(!l.path.join(raw_rel(&l.meta.sources[0])).exists());
    }

    #[tokio::test]
    async fn corrected_transcript_is_used_and_copied_before_being_redone() {
        let st = setup(&[("reg.wav", None)]);
        let ai = Fake::default();
        let asr = Arc::new(FakeAsr::default());
        run_with(&st, &ai, Some(asr.clone()), Options::default()).await.unwrap();
        let sid = st.lib.lesson(&st.lesson).unwrap().meta.sources[0].id.clone();
        let l = save_extract(&st.lib, &st.lesson, &sid, "[00:00:01] il metodo spiegato da TIM").unwrap();
        assert!(l.meta.sources[0].edited.is_some());

        // Rigenera: the correction goes into the summary, nothing is transcribed again.
        run_with(&st, &ai, Some(asr.clone()), Options::default()).await.unwrap();
        assert_eq!(asr.calls.load(Ordering::SeqCst), 3);
        assert!(ai.prompts.lock().unwrap().last().unwrap().contains("spiegato da TIM"));

        // Rielabora anche le fonti: transcribed again, the correction kept in estratti/versioni.
        run_with(&st, &ai, Some(asr.clone()), Options { force: true, ..Default::default() }).await.unwrap();
        let l = st.lib.lesson(&st.lesson).unwrap();
        assert!(l.meta.sources[0].edited.is_none());
        assert!(!outputs(&l).extracts[&sid].contains("TIM"));
        let versions: Vec<_> = fs::read_dir(l.path.join(EXTRACTS_DIR).join("versioni")).unwrap().flatten().collect();
        assert_eq!(versions.len(), 1);
        let copy = fs::read_to_string(versions[0].path().join(format!("{}.md", l.meta.sources[0].file))).unwrap();
        assert_eq!(copy, "[00:00:01] il metodo spiegato da TIM");
    }

    #[tokio::test]
    async fn course_terms_reach_whisper_cleanup_and_summary() {
        let st = setup(&[("reg.wav", None)]);
        let l = st.lib.lesson(&st.lesson).unwrap();
        st.lib.update_course(&l.course_id, "Ricerca operativa", "", "ARERA, colpo d'ariete\nGauckler-Strickler", None).unwrap();
        // Terms for all the courses come after the course's own, without repeats.
        st.lib.set_global_terms("arera\nTIM, Wind").unwrap();
        let ai = Fake::default();
        let asr = Arc::new(FakeAsr::default());
        run_with(&st, &ai, Some(asr.clone()), Options::default()).await.unwrap();
        assert!(asr.prompt.lock().unwrap().contains("Termini: ARERA, colpo d'ariete, Gauckler-Strickler, TIM, Wind."), "{}", asr.prompt.lock().unwrap());
        let line = "Termini e nomi ricorrenti del corso, scritti correttamente: ARERA, colpo d'ariete, Gauckler-Strickler, TIM, Wind.";
        let prompts = ai.prompts.lock().unwrap();
        assert!(prompts.iter().filter(|p| p.contains("Ripuliscila")).all(|p| p.contains(line)));
        assert!(prompts.last().unwrap().contains(line), "summary");
    }

    #[tokio::test]
    async fn an_english_lesson_in_an_italian_course() {
        let st = setup(&[("reg.wav", None)]);
        let l = st.lib.update_lesson(&st.lesson, crate::library::LessonPatch { language: Some(Some(Lang::En)), ..Default::default() }).unwrap();
        assert_eq!(st.lib.langs(&l), (Lang::En, Lang::It));
        let ai = Fake::default();
        let asr = Arc::new(FakeAsr::default());
        run_with(&st, &ai, Some(asr.clone()), Options::default()).await.unwrap();
        assert_eq!(*asr.lang.lock().unwrap(), Some(Lang::En));
        assert!(asr.prompt.lock().unwrap().starts_with("University lecture of"), "{}", asr.prompt.lock().unwrap());
        {
            let prompts = ai.prompts.lock().unwrap();
            let cleanups: Vec<_> = prompts.iter().filter(|p| p.contains("Clean it up")).collect();
            assert!(!cleanups.is_empty() && cleanups.iter().all(|p| p.contains("do not translate")));
            let summary = prompts.last().unwrap();
            assert!(summary.contains("Produci, in italiano") && summary.contains("La lezione è in inglese"), "{summary}");
        }

        // Materials in English too: the whole summary prompt is English, without the note.
        st.lib.update_course(&l.course_id, "Ricerca operativa", "", "", Some(crate::library::CourseLangs { language: Lang::It, materials_language: Lang::En })).unwrap();
        run_with(&st, &ai, Some(asr.clone()), Options::default()).await.unwrap();
        let summary = ai.prompts.lock().unwrap().last().unwrap().clone();
        assert!(summary.contains("Produce, in English") && !summary.contains("The lecture is in"), "{summary}");

        // Back to the course's language.
        let l = st.lib.update_lesson(&st.lesson, serde_json::from_str(r#"{"language":null}"#).unwrap()).unwrap();
        assert_eq!(l.meta.language, None);
        assert_eq!(st.lib.langs(&l), (Lang::It, Lang::En));
    }

    #[tokio::test]
    async fn a_piece_is_cleaned_again_from_whispers_text() {
        let st = setup(&[("reg.wav", None)]);
        let ai = Fake::default();
        let sid = st.lib.lesson(&st.lesson).unwrap().meta.sources[0].id.clone();
        // No Whisper text yet.
        assert!(reclean(&ai, "m", &st.lib, &st.lesson, &sid, 0, None, &CancellationToken::new()).await.unwrap_err().to_string().contains("Rielabora anche le fonti"));
        run_with(&st, &ai, Some(Arc::new(FakeAsr::default())), Options::default()).await.unwrap();
        let raw = outputs(&st.lib.lesson(&st.lesson).unwrap()).raw[&sid].clone();
        let ts = super::times(&raw);
        assert!(ts.len() >= 2, "{raw}");
        let n = ai.prompts.lock().unwrap().len();
        let (t, _, model) = reclean(&ai, "altro-modello", &st.lib, &st.lesson, &sid, ts[1], None, &CancellationToken::new()).await.unwrap();
        assert_eq!(model, "altro-modello");
        assert!(!t.is_empty());
        let prompts = ai.prompts.lock().unwrap();
        assert_eq!(prompts.len(), n + 1);
        let p = &prompts[n];
        assert!(p.contains("Pezzo:") && p.contains(&hms(ts[1])) && !p.contains(&format!("[{}]", hms(ts[0]))), "{p}");
    }

    #[test]
    fn double_escaped_text_is_repaired() {
        // As in the bug report: a healthy start, then everything escaped twice.
        let broken = r#"## Dighe
Con $\nu$ e $\frac{a}{b}$ [F2 00:01:00]\n\n## Scarichi\n### Legge di scarico\n$Q = \\mu L_e \\sqrt{2g}\\, (h-h_0)^{3/2}$\ndove $h$ è la quota.\n- Bazin: $\\mu = 0.4$\n- Creager: $\\mu = 0.48 \\div 0.55$; si dice \"profilo\""#;
        let fixed = fix_double_escapes(broken);
        assert_eq!(
            fixed,
            "## Dighe\nCon $\\nu$ e $\\frac{a}{b}$ [F2 00:01:00]\n\n## Scarichi\n### Legge di scarico\n$Q = \\mu L_e \\sqrt{2g}\\, (h-h_0)^{3/2}$\ndove $h$ è la quota.\n- Bazin: $\\mu = 0.4$\n- Creager: $\\mu = 0.48 \\div 0.55$; si dice \"profilo\""
        );
        assert_eq!(fix_double_escapes(&fixed), fixed, "nothing left to undo");
        // Healthy texts stay as they are, also one-liners and LaTeX line breaks.
        for ok in [
            r"$\nu \neq \nabla f$",
            r"La viscosità $\nu$ e $\theta$",
            "## Titolo\nTesto con $\\frac{1}{2}$ e $$\\begin{aligned} a &= b \\\\ c &= d \\end{aligned}$$",
            r"Costo: 5\$ e $\neg p$",
            "## Codice\n`print(\"a\\n\\nb\")` e `\\d+`\n```\ns = \"x\\n- y\"\n```",
        ] {
            assert_eq!(fix_double_escapes(ok), ok);
        }
        // A display formula on its own lines, a LaTeX line break: both escaped twice.
        assert_eq!(
            fix_double_escapes(r"Legge:\n$$\nQ = \\nu L\n$$\n- e $a \\\\ b$"),
            "Legge:\n$$\nQ = \\nu L\n$$\n- e $a \\\\ b$"
        );
    }

    #[test]
    fn map_nodes_with_and_without_references() {
        let old: MapNode = serde_json::from_str(r#"{"etichetta":"Dighe","figli":[{"etichetta":"Speroni"}]}"#).unwrap();
        assert_eq!(old.figli[0].rif, "");
        let new: MapNode = serde_json::from_str(r#"{"etichetta":"Dighe","rif":"F1 00:12:30"}"#).unwrap();
        assert_eq!(new.rif, "F1 00:12:30");
        assert!(!serde_json::to_string(&old).unwrap().contains("rif"), "nothing written when there is none");
        let schema = prompts::generate_schema();
        assert_eq!(schema["properties"]["mappa"]["properties"]["rif"]["type"], "string");
        assert_eq!(schema["properties"]["mappa"]["required"], serde_json::json!(["etichetta", "figli"]));
        let leaf = &schema["properties"]["mappa"]["properties"]["figli"]["items"]["properties"]["figli"]["items"]["properties"]["figli"]["items"];
        assert_eq!(leaf["required"], serde_json::json!(["etichetta"]));
        assert!(leaf["properties"].get("figli").is_none());
    }

    #[test]
    fn pieces_and_their_check() {
        let raw = "[00:00:05] a b\n\n[00:01:00] c d e\n\n[00:02:00] f";
        assert_eq!(paragraphs_between(raw, 60, Some(120)), "[00:01:00] c d e");
        assert_eq!(paragraphs_between(raw, 60, None), "[00:01:00] c d e\n\n[00:02:00] f");
        // Three paragraphs merged into one: fine for a piece.
        assert!(plausible_fragment(raw, "[00:00:05] a b c d e f", 0, 120));
        assert!(!plausible_fragment(raw, "a b c d e f", 0, 120), "no time");
        assert!(!plausible_fragment(raw, "[00:09:00] a b c d e f", 0, 120), "time outside");
        assert!(!plausible_fragment(raw, "[00:00:05] a", 0, 120), "words lost");
    }

    #[tokio::test]
    async fn slides_are_recognised_and_linked_to_the_recording() {
        let st = setup(&[("reg.wav", None), ("slide.pdf", Some(b"%PDF-1.4"))]);
        let ai = Fake::default();
        let asr: Arc<dyn Asr> = Arc::new(FakeAsr::default());
        run_with(&st, &ai, Some(asr.clone()), Options::default()).await.unwrap();
        let l = st.lib.lesson(&st.lesson).unwrap();
        let pdf = l.meta.sources.iter().find(|s| s.kind == SourceKind::Pdf).unwrap();
        assert_eq!((pdf.role, pdf.role_guessed), (Some(crate::library::SourceRole::Slide), true), "proposed by the model");
        let out = outputs(&l);
        assert!(out.extracts[&pdf.id].starts_with("## Pagina 1"), "the TIPO line is not kept");
        assert_eq!(out.slides.len(), 1);
        assert_eq!(out.slides[0].steps.iter().map(|s| (s.at, s.page)).collect::<Vec<_>>(), [(1, 1), (11, 2)]);
        let prompts = ai.prompts.lock().unwrap().clone();
        assert!(prompts.last().unwrap().contains("- F2 pagina 2 = F1 00:00:11"), "{}", prompts.last().unwrap());
        // Nothing changed: not linked again. Marked as other material by hand: no more links made.
        let before = ai.prompts.lock().unwrap().iter().filter(|p| p.contains("SLIDE:\n")).count();
        run_with(&st, &ai, Some(asr.clone()), Options::default()).await.unwrap();
        assert_eq!(ai.prompts.lock().unwrap().iter().filter(|p| p.contains("SLIDE:\n")).count(), before);
        st.lib.set_source_role(&st.lesson, &pdf.id, crate::library::SourceRole::Materiale, false).unwrap();
        st.lib.set_source_role(&st.lesson, &pdf.id, crate::library::SourceRole::Slide, true).unwrap();
        assert_eq!(st.lib.lesson(&st.lesson).unwrap().meta.sources.iter().find(|s| s.id == pdf.id).unwrap().role, Some(crate::library::SourceRole::Materiale), "a guess never overrides the user");
        st.lib.remove_source(&st.lesson, &pdf.id).unwrap();
        assert!(outputs(&st.lib.lesson(&st.lesson).unwrap()).slides.is_empty());
    }

    #[tokio::test]
    async fn bookmarks_reach_the_summary() {
        let st = setup(&[("reg.wav", None)]);
        let sid = st.lib.lesson(&st.lesson).unwrap().meta.sources[0].id.clone();
        crate::bookmarks::add(&st.lib, &st.lesson, &sid, 750.0, crate::bookmarks::Kind::Dubbio, "perché il rapporto minimo?").unwrap();
        let ai = Fake::default();
        run_with(&st, &ai, Some(Arc::new(FakeAsr::default())), Options::default()).await.unwrap();
        let prompts = ai.prompts.lock().unwrap();
        assert!(prompts.last().unwrap().contains("- [F1 00:12:30] non capito: perché il rapporto minimo?"), "{}", prompts.last().unwrap());
    }

    #[tokio::test]
    async fn summary_and_study_corrected_by_hand() {
        let st = setup(&[("lavagna.jpg", Some(b"img"))]);
        run(&st, &Fake::default(), Options::default()).await.unwrap();
        save_summary(&st.lib, &st.lesson, "## Simplesso\nCorretta.").unwrap();
        let mut study = outputs(&st.lib.lesson(&st.lesson).unwrap()).study.unwrap();
        study.flashcard.remove(0);
        save_study(&st.lib, &st.lesson, &study).unwrap();
        let out = outputs(&st.lib.lesson(&st.lesson).unwrap());
        assert_eq!(out.summary.unwrap(), "## Simplesso\nCorretta.");
        assert!(out.study.unwrap().flashcard.is_empty());
        assert!(out.info.unwrap().edited.is_some());
    }

    #[tokio::test]
    async fn a_break_becomes_a_pause_before_the_cleanup() {
        let st = setup(&[("reg.wav", None)]);
        let ai = Fake::default();
        let asr = Arc::new(FakeAsr::default());
        asr.thanks.store(true, Ordering::SeqCst);
        run_with(&st, &ai, Some(asr), Options::default()).await.unwrap();
        let l = st.lib.lesson(&st.lesson).unwrap();
        let out = outputs(&l);
        let sid = &l.meta.sources[0].id;
        assert_eq!(out.extracts[sid], "[00:00:01] il simplesso\n\n[00:00:11] *(pausa fino a 00:00:20)*\n\n[00:00:21] il simplesso");
        assert!(out.raw[sid].contains("*(pausa fino a 00:00:20)*") && !out.raw[sid].contains("grazie"), "{}", out.raw[sid]);
        assert!(!ai.prompts.lock().unwrap().iter().any(|p| p.contains("grazie")), "the invented words never reach the model");
        assert!(out.pauses.is_empty());
    }

    #[tokio::test]
    async fn pauses_marked_in_a_lesson_made_before() {
        let st = setup(&[("reg.wav", None)]);
        run_with(&st, &Fake::default(), Some(Arc::new(FakeAsr::default())), Options::default()).await.unwrap();
        let l = st.lib.lesson(&st.lesson).unwrap();
        let s = &l.meta.sources[0];
        let old = "[00:00:01] il simplesso\n\n[00:00:11] grazie\n\n[00:00:15] grazie grazie grazie\n\n[00:00:21] il simplesso";
        fs::write(l.path.join(extract_rel(s)), old).unwrap();
        fs::write(l.path.join(raw_rel(s)), old).unwrap();
        assert_eq!(outputs(&l).pauses[&s.id], 1);
        assert_eq!(mark_pauses(&st.lib, &st.lesson, &s.id).unwrap(), 1);
        let l = st.lib.lesson(&st.lesson).unwrap();
        let out = outputs(&l);
        assert!(out.extracts[&s.id].contains("[00:00:11] *(pausa fino a 00:00:21)*"));
        assert!(out.raw[&s.id].contains("[00:00:11] *(pausa fino a 00:00:21)*"));
        assert!(l.meta.sources[0].edited.is_some());
        assert!(out.pauses.is_empty());
    }

    /// Transcription on the computer without Whisper loaded: an error, never the audio to the model.
    #[tokio::test]
    async fn local_transcription_never_sends_the_audio() {
        let st = setup(&[("reg.wav", None)]);
        let ai = Fake::default();
        let err = run_models(&st, &ai, None, Options::default(), Models::default()).await.unwrap_err().to_string();
        assert!(err.contains("non era pronta"), "{err}");
        assert_eq!(ai.uploads.load(Ordering::SeqCst), 0);
        assert_eq!(ai.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn failed_cleanup_keeps_whisper_text_and_retries_only_the_cleanup() {
        let st = setup(&[("reg.wav", None)]);
        let ai = Fake::default();
        ai.fail_cleanup.store(true, Ordering::SeqCst);
        let asr = Arc::new(FakeAsr::default());
        let r = run_with(&st, &ai, Some(asr.clone()), Options::default()).await.unwrap();
        // The first failure stops the cleanups of this run (Gemini is not answering).
        assert_eq!(r.warnings.len(), 2, "{:?}", r.warnings);
        assert!(r.warnings[0].contains("parte da 00:00:00 non ripulita") && r.warnings[1].contains("2 parti restano con il testo di Whisper"), "{:?}", r.warnings);
        let l = st.lib.lesson(&st.lesson).unwrap();
        let src = &l.meta.sources[0];
        // Whisper's text is what the user sees (transcript tab, search, chat) until cleaned.
        assert_eq!(outputs(&l).extracts[&src.id], "[00:00:01] ehm allora il simplesso\n\n[00:00:11] ehm allora il simplesso\n\n[00:00:21] ehm allora il simplesso");
        assert!(cleanup_pending(&l.path, src));
        // So the app loads Whisper again for the next run (it once sent the audio to OpenRouter).
        assert!(needs_transcription(&l, false));
        assert!(outputs(&l).summary.unwrap().contains("Simplesso"), "summary made from Whisper's text");

        ai.fail_cleanup.store(false, Ordering::SeqCst);
        let r = run_with(&st, &ai, Some(asr.clone()), Options::default()).await.unwrap();
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        assert_eq!(asr.calls.load(Ordering::SeqCst), 3, "Whisper not run again");
        let l = st.lib.lesson(&st.lesson).unwrap();
        assert_eq!(outputs(&l).extracts[&l.meta.sources[0].id], "[00:00:01] il simplesso\n\n[00:00:11] il simplesso\n\n[00:00:21] il simplesso");
        assert!(!l.path.join(parts_rel(&l.meta.sources[0])).exists(), "parts removed once all cleaned");
        assert!(!needs_transcription(&l, false));

        // Nothing pending: a third run only redoes the summary.
        let before = count(&ai, "Ripuliscila");
        run_with(&st, &ai, Some(asr.clone()), Options::default()).await.unwrap();
        assert_eq!(count(&ai, "Ripuliscila"), before);
    }

    #[test]
    fn headings_after_the_time() {
        assert_eq!(fix_headings("[00:20:00] ### Forma standard\nQuindi abbiamo\n\n[00:20:41] testo"), "### Forma standard\n\n[00:20:00] Quindi abbiamo\n\n[00:20:41] testo");
        assert_eq!(fix_headings("### Titolo\n\n[00:20:00] ok"), "### Titolo\n\n[00:20:00] ok");
    }

    #[test]
    fn cleanup_checks() {
        let raw = "[00:20:01] uno due tre quattro\n\n[00:20:30] cinque sei sette otto";
        assert!(plausible_cleanup(raw, "### Titolo\n[00:20:01] uno due tre quattro cinque sei sette", 1200, 1800));
        assert!(!plausible_cleanup(raw, "[00:20:01] uno", 1200, 1800), "text lost");
        assert!(!plausible_cleanup(raw, "uno due tre quattro cinque sei sette otto", 1200, 1800), "times lost");
        assert!(!plausible_cleanup(raw, "[00:45:00] uno due tre quattro cinque sei sette otto", 1200, 1800), "time outside the part");
    }

    fn count(ai: &Fake, needle: &str) -> usize {
        ai.prompts.lock().unwrap().iter().filter(|p| p.contains(needle)).count()
    }

    #[tokio::test]
    async fn recordings_go_first() {
        // The photo was added before the recording: still read after it.
        let st = setup(&[("lavagna.jpg", Some(b"img")), ("reg.wav", None)]);
        let ai = Fake::default();
        run(&st, &ai, Options::default()).await.unwrap();
        let prompts = ai.prompts.lock().unwrap();
        let first = |needle: &str| prompts.iter().position(|p| p.contains(needle)).unwrap();
        assert!(first("Questa è la parte") < first("Questa è una foto"), "{prompts:#?}");
        let l = st.lib.lesson(&st.lesson).unwrap();
        assert_eq!(outputs(&l).info.unwrap().sources["F1"], l.meta.sources[1].id, "the recording is F1");
    }

    #[tokio::test]
    async fn full_run() {
        let st = setup(&[("reg.wav", None), ("lavagna.jpg", Some(b"img")), ("slide.docx", Some(b"doc"))]);
        st.lib.add_text(&st.lesson, "Domande", "Che cos'è la degenerazione?").unwrap();
        let ai = Fake::default();
        let r = run(&st, &ai, Options::default()).await.unwrap();

        assert_eq!(r.warnings.len(), 1, "docx skipped: {:?}", r.warnings);
        // 25 s in parts of 10 s: 3 parts, small enough to go inline (no upload).
        assert_eq!(count(&ai, "Questa è la parte"), 3);
        assert_eq!(ai.uploads.load(Ordering::SeqCst), 0);
        // 3 parts + photo + its link to the recording + summary, each billed once.
        assert_eq!(r.usage.prompt_token_count, 600);

        let l = st.lib.lesson(&st.lesson).unwrap();
        let out = outputs(&l);
        assert_eq!(out.extracts.len(), 3, "audio, photo, text");
        assert_eq!(out.extracts[&l.meta.sources[0].id], "[00:00:05] testo\n\n[00:00:15] testo\n\n[00:00:25] testo", "times shifted by part");
        assert!(!l.path.join(parts_rel(&l.meta.sources[0])).exists(), "parts removed once complete");
        assert!(out.summary.unwrap().contains("[F1 00:00:10]"));
        assert_eq!(out.info.as_ref().unwrap().sources["F1"], l.meta.sources[0].id);
        assert_eq!(out.study.unwrap().flashcard.len(), 1);
        let mmd = fs::read_to_string(l.path.join("generati/mappa.mmd")).unwrap();
        assert!(mmd.contains("root((Simplesso))") && mmd.contains("Base ammissibile"), "{mmd}");
        // The photo linked by the model, and told to the summary.
        let photo = &l.meta.sources[1];
        assert_eq!(out.photos.iter().map(|p| (p.image.as_str(), p.at, p.by)).collect::<Vec<_>>(), [(photo.id.as_str(), Some(15), crate::photos::By::Modello)]);
        assert!(ai.prompts.lock().unwrap().last().unwrap().contains("foto «lavagna.jpg» = F1 00:00:15"));

        // Second run: extracts reused, only the summary is asked again; old outputs kept.
        let before = ai.prompts.lock().unwrap().len();
        run(&st, &ai, Options::default()).await.unwrap();
        assert_eq!(ai.prompts.lock().unwrap().len(), before + 1);
        assert_eq!(fs::read_dir(l.path.join("generati/versioni")).unwrap().count(), 1);

        // A new title asks again; a link by hand never.
        st.lib.set_source_info(&st.lesson, &photo.id, "Bazin", "").unwrap();
        run(&st, &ai, Options::default()).await.unwrap();
        assert_eq!(count(&ai, "Titolo dato dallo studente: Bazin"), 1);
        crate::photos::save(&l.path, vec![crate::photos::ImageLink { image: photo.id.clone(), recording: l.meta.sources[0].id.clone(), at: Some(25), by: crate::photos::By::Mano, why: String::new(), inputs: String::new() }]).unwrap();
        st.lib.set_source_info(&st.lesson, &photo.id, "Bazin", "stramazzo").unwrap();
        run(&st, &ai, Options::default()).await.unwrap();
        assert_eq!(count(&ai, "FOTO:\n"), 2);
        assert_eq!(outputs(&l).photos[0].at, Some(25));
        assert!(ai.prompts.lock().unwrap().last().unwrap().contains("foto «Bazin» (stramazzo) = F1 00:00:25"));
    }

    #[tokio::test]
    async fn photo_linked_from_the_clocks() {
        // Recorded until now (the file's date), 25 s long; the photo taken 10 s ago.
        let taken = (Utc::now() - chrono::Duration::seconds(10)).format("%Y:%m:%d %H:%M:%S").to_string();
        let jpeg = crate::photos::tests::jpeg_with_exif(&taken, Some("+00:00"));
        let st = setup(&[("reg.wav", None), ("lavagna.jpg", Some(&jpeg))]);
        let l = st.lib.lesson(&st.lesson).unwrap();
        assert!(l.meta.sources[0].started.is_some() && !l.meta.sources[0].started_mp4);
        assert!(l.meta.sources[1].taken.is_some());
        let ai = Fake::default();
        let r = run(&st, &ai, Options::default()).await.unwrap();
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        assert_eq!(count(&ai, "FOTO:\n"), 0, "no question to the model");
        let link = &outputs(&l).photos[0];
        assert_eq!(link.by, crate::photos::By::Orario);
        assert!((13..=17).contains(&link.at.unwrap()), "{link:?}");
    }

    #[tokio::test]
    async fn resumes_after_a_failed_part() {
        let st = setup(&[("reg.wav", None)]);
        let ai = Fake::default();
        *ai.fail_call.lock().unwrap() = Some(2);
        let e = run(&st, &ai, Options::default()).await.unwrap_err().to_string();
        assert!(e.contains("sintesi non generata") && e.contains("quota"), "{e}");
        let l = st.lib.lesson(&st.lesson).unwrap();
        assert!(l.path.join(parts_rel(&l.meta.sources[0])).join("01.md").is_file(), "first part kept");
        assert!(outputs(&l).summary.is_none());

        *ai.fail_call.lock().unwrap() = None;
        ai.prompts.lock().unwrap().clear();
        run(&st, &ai, Options::default()).await.unwrap();
        assert_eq!(count(&ai, "Questa è la parte 1 "), 0, "part 1 not asked again");
        assert_eq!(count(&ai, "Questa è la parte"), 2);
        assert!(outputs(&st.lib.lesson(&st.lesson).unwrap()).summary.is_some());
    }

    #[tokio::test]
    async fn undecodable_recording_goes_whole() {
        let st = setup(&[("reg.m4a", Some(b"non decodificabile"))]);
        let ai = Fake { whole_secs: 25, ..Default::default() };
        let r = run(&st, &ai, Options::default()).await.unwrap();
        assert!(r.warnings[0].contains("non riesco a dividere"), "{:?}", r.warnings);
        assert_eq!(ai.uploads.load(Ordering::SeqCst), 1);
        assert_eq!(ai.deletes.load(Ordering::SeqCst), 1);
        assert_eq!(count(&ai, "Trascrivi SOLO"), 2);
        assert_eq!(count(&ai, "fino alla fine"), 1);
    }

    #[tokio::test]
    async fn truncated_extract_is_not_kept() {
        let st = setup(&[("lavagna.jpg", Some(b"img"))]);
        let ai = Fake { truncate_images: true, ..Default::default() };
        let r = run(&st, &ai, Options::default()).await.unwrap();
        assert!(r.warnings.iter().any(|w| w.contains("incompleto")), "{:?}", r.warnings);
        let l = st.lib.lesson(&st.lesson).unwrap();
        assert!(outputs(&l).extracts.is_empty(), "not saved");
        assert!(outputs(&l).summary.is_some(), "but used for this summary");
    }

    #[tokio::test]
    async fn empty_summary_is_asked_again_and_never_saved() {
        let st = setup(&[("lavagna.jpg", Some(b"img"))]);
        let ai = Fake { empty_summaries: AtomicUsize::new(2), ..Default::default() };
        run(&st, &ai, Options::default()).await.unwrap();
        let out = outputs(&st.lib.lesson(&st.lesson).unwrap());
        let first = out.summary.unwrap();
        assert!(first.contains("Simplesso"), "third answer used: {first}");
        // The third try went to the fallback model.
        assert_eq!(*ai.summary_models.lock().unwrap(), ["gemini-3.8-flash", "gemini-3.8-flash", "gemini-3.5-flash-lite"]);
        assert_eq!(out.info.unwrap().model, "gemini-3.5-flash-lite");

        ai.empty_summaries.store(3, Ordering::SeqCst);
        let e = run(&st, &ai, Options::default()).await.unwrap_err();
        assert!(e.to_string().contains("sintesi vuota"), "{e}");
        assert_eq!(outputs(&st.lib.lesson(&st.lesson).unwrap()).summary.unwrap(), first, "saved summary kept");
    }

    #[tokio::test]
    async fn a_summary_cut_off_is_made_again_with_the_fallback_model() {
        let st = setup(&[("lavagna.jpg", Some(b"img"))]);
        let ai = Fake { truncate_summaries: AtomicUsize::new(1), ..Default::default() };
        let r = run(&st, &ai, Options::default()).await.unwrap();
        assert_eq!(*ai.summary_models.lock().unwrap(), ["gemini-3.8-flash", "gemini-3.5-flash-lite"]);
        assert!(r.warnings.iter().any(|w| w.contains("troncata") && w.contains("gemini-3.5-flash-lite")), "{:?}", r.warnings);
        assert!(outputs(&st.lib.lesson(&st.lesson).unwrap()).summary.unwrap().contains("Simplesso"));
        // Cut off by the fallback model too: an error, nothing saved over.
        ai.truncate_summaries.store(2, Ordering::SeqCst);
        assert!(run(&st, &ai, Options::default()).await.unwrap_err().to_string().contains("troncata"));
    }

    #[tokio::test]
    async fn omitted_sources_keep_their_extract() {
        let st = setup(&[("reg.wav", None)]);
        let ai = Fake::default();
        run(&st, &ai, Options::default()).await.unwrap();
        // As if imported from a package without the recording.
        let mut l = st.lib.lesson(&st.lesson).unwrap();
        fs::remove_file(l.path.join("fonti/reg.wav")).unwrap();
        l.meta.sources[0].omitted = true;
        fs::write(l.path.join("lezione.json"), serde_json::to_vec(&l.meta).unwrap()).unwrap();
        ai.prompts.lock().unwrap().clear();
        let r = run(&st, &ai, Options { force: true, ..Default::default() }).await.unwrap();
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        assert_eq!(count(&ai, "Questa è la parte"), 0, "nothing to transcribe");
        assert!(ai.prompts.lock().unwrap()[0].contains("[00:00:15] testo"), "the old transcript is in the summary prompt");
    }

    #[test]
    fn times() {
        assert_eq!(hms(3725), "01:02:05");
        assert_eq!(shift_times("[00:05] a\n[12:30] b [01:00:00] c [F1 00:10]", 1200), "[00:20:05] a\n[00:32:30] b [01:20:00] c [F1 00:10]");
    }
}
