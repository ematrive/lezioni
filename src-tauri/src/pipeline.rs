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

use crate::ai::gemini::{INLINE_MAX, mime_for};
use crate::ai::{Ai, Part, Request, Usage, prompts};
use crate::audio;
use crate::library::{EXTRACTS_DIR, GENERATED_DIR, Lesson, Library, SOURCES_DIR, Source, SourceKind};

/// Length of a transcription window: keeps each answer well below the output limit.
pub const WINDOW_SECS: u64 = 20 * 60;
const AUDIO_TOKENS_PER_SEC: u64 = 32;
/// Video: ~258 tokens per frame at 1 fps plus the audio track.
const VIDEO_TOKENS_PER_SEC: u64 = 290;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Models {
    pub transcribe: String,
    pub generate: String,
}

impl Default for Models {
    fn default() -> Self {
        Self { transcribe: "gemini-3.8-flash".into(), generate: "gemini-3.8-flash".into() }
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

    /// Writes `<lesson>/<rel>` atomically, finding the lesson folder again first.
    fn write(&self, lesson_id: &str, rel: &Path, content: &[u8]) -> Result<PathBuf> {
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let l = self.lesson(lesson_id)?;
        let p = l.path.join(rel);
        if let Some(d) = p.parent() {
            fs::create_dir_all(d)?;
        }
        let tmp = p.with_extension("tmp");
        fs::write(&tmp, content).with_context(|| format!("scrittura di {}", p.display()))?;
        fs::rename(&tmp, &p)?;
        Ok(p)
    }

    /// Moves the current outputs to `generati/versioni/<stamp>/`.
    fn archive_outputs(&self, lesson_id: &str) -> Result<()> {
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let gdir = self.lesson(lesson_id)?.path.join(GENERATED_DIR);
        let files: Vec<PathBuf> = match fs::read_dir(&gdir) {
            Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_file()).collect(),
            Err(_) => return Ok(()),
        };
        if files.is_empty() {
            return Ok(());
        }
        let dest = gdir.join("versioni").join(Utc::now().format("%Y-%m-%d %H-%M-%S").to_string());
        fs::create_dir_all(&dest)?;
        for f in files {
            fs::rename(&f, dest.join(f.file_name().unwrap()))?;
        }
        Ok(())
    }
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

/// `[hh:mm:ss]`
pub fn hms(secs: u64) -> String {
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MapNode {
    pub etichetta: String,
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
}

#[derive(Debug, Default)]
pub struct Report {
    pub usage: Usage,
    pub warnings: Vec<String>,
}

fn kind_label(k: SourceKind) -> &'static str {
    match k {
        SourceKind::Audio => "registrazione",
        SourceKind::Video => "video",
        SourceKind::Pdf => "PDF",
        SourceKind::Image => "foto/appunti",
        SourceKind::Text => "testo",
        SourceKind::Document => "documento",
        SourceKind::Other => "file",
    }
}

impl<A: Ai> Ctx<'_, A> {
    fn check(&self) -> Result<()> {
        if self.cancel.is_cancelled() { bail!("annullato") } else { Ok(()) }
    }

    /// A file as a prompt part: inline when small, uploaded otherwise. Returns the remote name
    /// to delete afterwards.
    async fn file_part(&self, path: &Path, mime: &str) -> Result<(Part, Option<String>)> {
        let size = fs::metadata(path)?.len();
        if size <= INLINE_MAX {
            return Ok((Part::Inline { mime: mime.into(), data: fs::read(path)? }, None));
        }
        let f = self.ai.upload(path, mime, self.cancel).await?;
        Ok((Part::File { mime: f.mime.clone().unwrap_or_else(|| mime.into()), uri: f.uri }, Some(f.name)))
    }

    /// One request on a media part: (text, complete). `[SILENZIO]` becomes empty.
    async fn ask_media(&self, part: Part, prompt: String, usage: &mut Usage) -> Result<(String, bool)> {
        let req = Request { system: Some(prompts::SYSTEM.into()), parts: vec![part, Part::Text(prompt)], max_output_tokens: Some(32_000), thinking: Some("LOW"), ..Default::default() };
        (self.detail)("In attesa della risposta di Gemini…".into());
        let a = self.ai.generate(&self.models.transcribe, &req, self.cancel).await?;
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
        (self.progress)(step.0 as f64, step.1, format!("Preparazione di {label}…"));
        let tmp = std::env::temp_dir().join(format!("lezioni-{}", uuid::Uuid::new_v4()));
        let (p, d, c, w) = (path.to_path_buf(), tmp.clone(), self.cancel.clone(), self.window);
        let split = tokio::task::spawn_blocking(move || audio::split(&p, &d, w, &c)).await?;
        let result = match split {
            Ok(chunks) => self.transcribe_chunks(lesson_id, s, &chunks, step, usage, warnings).await,
            Err(e) if e.to_string() == "annullato" => Err(e),
            Err(e) => {
                log::warn!("{label}: {e:#}");
                warnings.push(format!("{label}: non riesco a dividere la registrazione in parti ({e:#}), la invio intera e costa di più; convertila in m4a o mp3 per risparmiare"));
                self.transcribe_whole(path, mime, video, label, step, usage).await
            }
        };
        let _ = fs::remove_dir_all(&tmp);
        result
    }

    async fn transcribe_chunks(&self, lesson_id: &str, s: &Source, chunks: &[audio::Chunk], step: (u32, u32), usage: &mut Usage, warnings: &mut Vec<String>) -> Result<(String, bool)> {
        let label = &s.original_name;
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
            (self.progress)(frac, step.1, format!("Trascrizione di {label}, parte {} di {}: {}–{} di {}", i + 1, chunks.len(), hms(c.start), hms(end), hms(total)));
            let (part, remote) = self.file_part(&c.path, "audio/wav").await?;
            let r = self.ask_media(part, prompts::transcribe_clip(i + 1, chunks.len(), &hms(c.start)), usage).await;
            if let Some(n) = remote {
                self.ai.delete_file(&n).await;
            }
            let (t, ok) = r?;
            let t = shift_times(&t, c.start);
            if ok {
                self.archive.write(lesson_id, &rel, t.as_bytes())?;
            } else {
                complete = false;
                warnings.push(format!("{label}: la parte da {} è stata troncata da Gemini", hms(c.start)));
            }
            texts.push(t);
        }
        Ok((texts.into_iter().filter(|t| !t.trim().is_empty()).collect::<Vec<_>>().join("\n\n"), complete))
    }

    /// Fallback: the whole file uploaded once, transcribed in time windows. Each window bills
    /// the whole file again.
    async fn transcribe_whole(&self, path: &Path, mime: &str, video: bool, label: &str, step: (u32, u32), usage: &mut Usage) -> Result<(String, bool)> {
        (self.progress)(step.0 as f64, step.1, format!("Caricamento di {label} su Gemini…"));
        let f = self.ai.upload(path, mime, self.cancel).await?;
        let file = Part::File { mime: f.mime.clone().unwrap_or_else(|| mime.into()), uri: f.uri.clone() };
        let result = async {
            let per_sec = if video { VIDEO_TOKENS_PER_SEC } else { AUDIO_TOKENS_PER_SEC };
            let secs = self.ai.count_tokens(&self.models.transcribe, std::slice::from_ref(&file), self.cancel).await.map_err(|e| e.context("durata della registrazione"))? / per_sec;
            let windows = secs.div_ceil(self.window).max(1);
            let (mut out, mut complete) = (Vec::new(), true);
            for i in 0..windows {
                self.check()?;
                let (from, to) = (hms(i * self.window), hms((i + 1) * self.window));
                (self.progress)(step.0 as f64 + i as f64 / windows as f64, step.1, format!("Trascrizione di {label}: {from}–{} di {}", hms(((i + 1) * self.window).min(secs)), hms(secs)));
                let (t, ok) = self.ask_media(file.clone(), prompts::transcribe_window(&from, &to, i == 0, i + 1 == windows), usage).await?;
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

    async fn read_with_model(&self, path: &Path, mime: &str, prompt: &str, usage: &mut Usage) -> Result<(String, bool)> {
        let (part, remote) = self.file_part(path, mime).await?;
        let r = self.ask_media(part, prompt.into(), usage).await;
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
            (SourceKind::Text, _) => (fs::read_to_string(&path).with_context(|| format!("lettura di {label}"))?, true),
            (SourceKind::Audio, Some(m)) => self.transcribe(&lesson.meta.id, s, &path, m, false, step, usage, warnings).await?,
            (SourceKind::Video, Some(m)) => self.transcribe(&lesson.meta.id, s, &path, m, true, step, usage, warnings).await?,
            (SourceKind::Image, Some(m)) => {
                (self.progress)(step.0 as f64, step.1, format!("Lettura di {label}…"));
                self.read_with_model(&path, m, prompts::IMAGE, usage).await?
            }
            (SourceKind::Pdf, Some(m)) => {
                (self.progress)(step.0 as f64, step.1, format!("Lettura di {label}…"));
                self.read_with_model(&path, m, prompts::PDF, usage).await?
            }
            (SourceKind::Document, _) => {
                warnings.push(format!("{label}: Gemini non legge i file di Office, esportalo in PDF e aggiungi il PDF"));
                return Ok(None);
            }
            _ => {
                warnings.push(format!("{label}: tipo di file non supportato, saltato"));
                return Ok(None);
            }
        }))
    }

    pub async fn run(&self, lesson_id: &str, opts: &Options) -> Result<Report> {
        let mut report = Report::default();
        let lesson = self.archive.lesson(lesson_id)?;
        if lesson.meta.sources.is_empty() {
            bail!("la lezione non ha fonti: aggiungi registrazioni, appunti o PDF");
        }
        let total = lesson.meta.sources.len() as u32 + if opts.extract_only { 0 } else { 1 };
        let mut extracts: Vec<(String, Source, String)> = Vec::new();
        // Recordings that could not be transcribed: without them the summary would be wrong.
        let mut media_failed = Vec::new();
        for (i, s) in lesson.meta.sources.iter().enumerate() {
            self.check()?;
            let step = (i as u32, total);
            // The folder may have been renamed since the job started.
            let current = self.archive.lesson(lesson_id)?;
            let existing = current.path.join(extract_rel(s));
            if opts.force {
                let _ = fs::remove_dir_all(current.path.join(parts_rel(s)));
            }
            // From a package without its file (the recording): the extract is all there is.
            if s.omitted {
                match fs::read_to_string(&existing) {
                    Ok(t) if !t.trim().is_empty() => extracts.push((format!("F{}", extracts.len() + 1), s.clone(), t)),
                    _ => report.warnings.push(format!("{}: il file non era nel pacchetto ricevuto e non c'è un testo estratto, saltato", s.original_name)),
                }
                continue;
            }
            let text = if existing.is_file() && !opts.force {
                Some(fs::read_to_string(&existing)?)
            } else {
                match self.extract(&current, s, step, &mut report.usage, &mut report.warnings).await {
                    Ok(Some((t, true))) => {
                        self.archive.write(lesson_id, &extract_rel(s), t.as_bytes())?;
                        let _ = fs::remove_dir_all(self.archive.lesson(lesson_id)?.path.join(parts_rel(s)));
                        Some(t)
                    }
                    // Truncated: used for this summary, not kept as if it were complete.
                    Ok(Some((t, false))) => {
                        report.warnings.push(format!("{}: testo incompleto (risposta troppo lunga), non salvato; verrà rifatto alla prossima elaborazione", s.original_name));
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
            bail!(
                "trascrizione non riuscita, sintesi non generata:\n{}\nLe parti già trascritte sono salvate: rilancia l'elaborazione per riprendere da lì.",
                media_failed.join("\n")
            );
        }
        if opts.extract_only {
            return Ok(report);
        }
        if extracts.is_empty() {
            bail!("nessuna fonte leggibile: {}", report.warnings.join("; "));
        }
        self.check()?;
        (self.progress)((total - 1) as f64, total, "Sintesi, mappa e materiale di studio…".into());
        let lesson = self.archive.lesson(lesson_id)?;
        let course = self.archive.lib()?.courses()?.into_iter().find(|c| c.meta.id == lesson.course_id).map(|c| c.meta.name).unwrap_or_default();
        let sources: String = extracts.iter().map(|(code, s, t)| format!("[{code}] {} ({})\n{t}\n\n", s.original_name, kind_label(s.kind))).collect();
        let req = Request {
            system: Some(prompts::SYSTEM.into()),
            parts: vec![Part::Text(prompts::generate(&course, &lesson.meta.title, &lesson.meta.date.to_string(), &lesson.meta.notes, &sources))],
            schema: Some(prompts::generate_schema()),
            max_output_tokens: Some(48_000),
            ..Default::default()
        };
        (self.detail)("In attesa della risposta di Gemini…".into());
        let a = self.ai.generate(&self.models.generate, &req, self.cancel).await?;
        report.usage.add(&a.usage);
        if a.finish == "MAX_TOKENS" {
            bail!("la risposta di Gemini è stata troncata: prova con meno fonti o con un modello con più spazio di risposta");
        }
        let g: Generated = serde_json::from_str(&a.text).map_err(|e| anyhow!("risposta di Gemini non valida: {e}"))?;

        self.archive.archive_outputs(lesson_id)?;
        let gdir = Path::new(GENERATED_DIR);
        self.archive.write(lesson_id, &gdir.join("sintesi.md"), g.sintesi.as_bytes())?;
        self.archive.write(lesson_id, &gdir.join("mappa.json"), &serde_json::to_vec_pretty(&g.mappa)?)?;
        self.archive.write(lesson_id, &gdir.join("mappa.mmd"), mermaid(&g.mappa).as_bytes())?;
        let study = Study { glossario: g.glossario, flashcard: g.flashcard, domande_esame: g.domande_esame };
        self.archive.write(lesson_id, &gdir.join("studio.json"), &serde_json::to_vec_pretty(&study)?)?;
        let info = Info {
            generated: Utc::now(),
            model: self.models.generate.clone(),
            usage: report.usage.clone(),
            suggested_title: g.titolo,
            sources: extracts.iter().map(|(code, s, _)| (code.clone(), s.id.clone())).collect(),
        };
        self.archive.write(lesson_id, &gdir.join("info.json"), &serde_json::to_vec_pretty(&info)?)?;
        (self.progress)(total as f64, total, "Fatto".into());
        Ok(report)
    }
}

/// Everything the UI shows in the Trascrizione / Sintesi / Mappa / Studio tabs.
#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Outputs {
    /// source id → extracted text
    pub extracts: BTreeMap<String, String>,
    pub summary: Option<String>,
    pub map: Option<MapNode>,
    pub study: Option<Study>,
    pub info: Option<Info>,
}

pub fn outputs(lesson: &Lesson) -> Outputs {
    let read = |rel: &Path| fs::read_to_string(lesson.path.join(rel)).ok();
    let gdir = Path::new(GENERATED_DIR);
    Outputs {
        extracts: lesson.meta.sources.iter().filter_map(|s| read(&extract_rel(s)).map(|t| (s.id.clone(), t))).collect(),
        summary: read(&gdir.join("sintesi.md")),
        map: read(&gdir.join("mappa.json")).and_then(|t| serde_json::from_str(&t).ok()),
        study: read(&gdir.join("studio.json")).and_then(|t| serde_json::from_str(&t).ok()),
        info: read(&gdir.join("info.json")).and_then(|t| serde_json::from_str(&t).ok()),
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
    }

    impl Ai for Fake {
        async fn generate(&self, _model: &str, req: &Request, _c: &CancellationToken) -> Result<Answer> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            if *self.fail_call.lock().unwrap() == Some(n) {
                bail!("quota esaurita (HTTP 429)");
            }
            let prompt = req.parts.iter().find_map(|p| if let Part::Text(t) = p { Some(t.clone()) } else { None }).unwrap();
            self.prompts.lock().unwrap().push(prompt.clone());
            let usage = Usage { prompt_token_count: 100, candidates_token_count: 10, thoughts_token_count: 0 };
            let mut finish = "STOP";
            let text = if req.schema.is_some() {
                serde_json::json!({
                    "titolo": "Il simplesso",
                    "sintesi": "## Simplesso\nSi parte da una base ammissibile. [F1 00:00:10]",
                    "mappa": { "etichetta": "Simplesso", "figli": [{ "etichetta": "Base (ammissibile)" }] },
                    "glossario": [{ "termine": "Base", "definizione": "insieme di variabili" }],
                    "flashcard": [{ "domanda": "Cos'è una base?", "risposta": "…" }],
                    "domande_esame": [{ "domanda": "Descrivi il simplesso", "traccia": "…" }]
                })
                .to_string()
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
            Ok(Answer { text, usage, finish: finish.into() })
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
        let c = lib.create_course("Ricerca operativa", "").unwrap();
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
        let models = Models::default();
        let cancel = CancellationToken::new();
        let progress = |_: f64, _: u32, _: String| {};
        let detail = |_: String| {};
        let ctx = Ctx { ai, archive: &st.archive, models: &models, cancel: &cancel, progress: &progress, detail: &detail, window: 10 };
        ctx.run(&st.lesson, &opts).await
    }

    fn count(ai: &Fake, needle: &str) -> usize {
        ai.prompts.lock().unwrap().iter().filter(|p| p.contains(needle)).count()
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
        // 3 parts + photo + summary, each billed once.
        assert_eq!(r.usage.prompt_token_count, 500);

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

        // Second run: extracts reused, only the summary is asked again; old outputs kept.
        let before = ai.prompts.lock().unwrap().len();
        run(&st, &ai, Options::default()).await.unwrap();
        assert_eq!(ai.prompts.lock().unwrap().len(), before + 1);
        assert_eq!(fs::read_dir(l.path.join("generati/versioni")).unwrap().count(), 1);
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
