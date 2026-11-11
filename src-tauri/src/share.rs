//! Sharing lessons with other students:
//! - **`.lezione` package**: a zip with one or more lesson folders, imported by the app as they
//!   are (no AI work again). Recordings are left out by default (size, and the teacher's
//!   consent): the source stays listed as `omitted`, with its transcript.
//! - **Markdown folder** for Obsidian and NotebookLM (which takes `.md` files).
//! - **Anki** text file of flashcards and exam questions (File → Importa in Anki).
//!
//! PDF and the single-file HTML page are built by the UI (it already renders Markdown and
//! formulas); the backend only writes or prints them (`commands.rs`).
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::library::{EXTRACTS_DIR, GENERATED_DIR, LESSON_FILE, Lesson, LessonMeta, Library, SOURCES_DIR, SourceKind, is_plain_file_name, safe_name};
use crate::pipeline::{self, MapNode, mermaid};

pub const PACKAGE_FILE: &str = "pacchetto.json";
const FORMAT: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageCourse {
    pub name: String,
    #[serde(default)]
    pub teacher: String,
    /// The course's terms travel with the package (they help whoever processes it again).
    #[serde(default)]
    pub terms: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: u32,
    pub app: String,
    pub exported: DateTime<Utc>,
    pub course: PackageCourse,
    /// Folder of each lesson inside the zip (`lezioni/01`, …).
    pub lessons: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Include {
    /// Recordings and videos (large; ask the teacher before sharing them).
    #[serde(default)]
    pub media: bool,
    /// Photos, PDFs, texts and other files.
    #[serde(default = "yes")]
    pub files: bool,
    /// The student's own notes on the lesson.
    #[serde(default)]
    pub notes: bool,
}

fn yes() -> bool {
    true
}

fn zip_options() -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(true)
}

/// Adds every file under `dir` as `prefix/<relative path>`, skipping `skip(rel)`.
fn add_dir<W: Write + std::io::Seek>(zw: &mut zip::ZipWriter<W>, dir: &Path, prefix: &str, skip: &dyn Fn(&Path) -> bool) -> Result<()> {
    fn walk<W: Write + std::io::Seek>(zw: &mut zip::ZipWriter<W>, root: &Path, dir: &Path, prefix: &str, skip: &dyn Fn(&Path) -> bool) -> Result<()> {
        let Ok(rd) = fs::read_dir(dir) else { return Ok(()) };
        for e in rd {
            let p = e?.path();
            let rel = p.strip_prefix(root)?;
            if skip(rel) {
                continue;
            }
            if p.is_dir() {
                walk(zw, root, &p, prefix, skip)?;
            } else {
                // Zip paths always use "/", also on Windows.
                let name = format!("{prefix}/{}", rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"));
                zw.start_file(name, zip_options())?;
                std::io::copy(&mut fs::File::open(&p)?, zw)?;
            }
        }
        Ok(())
    }
    walk(zw, dir, dir, prefix, skip)
}

/// Writes a `.lezione` package with the given lessons (all from the same course).
pub fn export_package(lib: &Library, lesson_ids: &[String], include: Include, dest: &Path) -> Result<()> {
    if lesson_ids.is_empty() {
        bail!("nessuna lezione da esportare");
    }
    let lessons: Vec<Lesson> = lesson_ids.iter().map(|id| lib.lesson(id)).collect::<Result<_>>()?;
    let course = lib.course(&lessons[0].course_id)?;
    let tmp = dest.with_extension("lezione.tmp");
    let mut zw = zip::ZipWriter::new(fs::File::create(&tmp).with_context(|| format!("creazione di {}", dest.display()))?);
    let mut folders = Vec::new();
    for (i, l) in lessons.iter().enumerate() {
        let folder = format!("lezioni/{:02}", i + 1);
        let mut meta = l.meta.clone();
        if !include.notes {
            meta.notes.clear();
        }
        let mut left_out = Vec::new();
        for s in &mut meta.sources {
            let media = matches!(s.kind, SourceKind::Audio | SourceKind::Video);
            if s.omitted || (media && !include.media) || (!media && !include.files) {
                s.omitted = true;
                left_out.push(Path::new(SOURCES_DIR).join(&s.file));
            }
        }
        zw.start_file(format!("{folder}/{LESSON_FILE}"), zip_options())?;
        zw.write_all(&serde_json::to_vec_pretty(&meta)?)?;
        let skip = |rel: &Path| {
            rel == Path::new(LESSON_FILE)
                // Private: the conversations with the assistant stay on this computer.
                || rel == Path::new(crate::chat::CHAT_FILE)
                // Private too: what this person spent.
                || rel == Path::new(crate::costs::COSTS_FILE)
                || left_out.iter().any(|o| o == rel)
                || rel.starts_with(Path::new(EXTRACTS_DIR).join(".parti"))
                || rel.starts_with(Path::new(GENERATED_DIR).join("versioni"))
                || rel.starts_with(Path::new(EXTRACTS_DIR).join("versioni"))
                || rel.extension().is_some_and(|e| e == "tmp")
        };
        add_dir(&mut zw, &l.path, &folder, &skip)?;
        folders.push(folder);
    }
    let manifest = Manifest {
        format: FORMAT,
        app: format!("Lezioni {}", env!("CARGO_PKG_VERSION")),
        exported: Utc::now(),
        course: PackageCourse { name: course.meta.name.clone(), teacher: course.meta.teacher.clone(), terms: course.meta.terms.clone() },
        lessons: folders,
    };
    zw.start_file(PACKAGE_FILE, zip_options())?;
    zw.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
    zw.finish()?;
    fs::rename(&tmp, dest)?;
    Ok(())
}

/// What a package contains, shown before importing.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageInfo {
    pub course: PackageCourse,
    pub exported: DateTime<Utc>,
    pub lessons: Vec<PackageLesson>,
    /// Id of a course of the archive with the same name, if any.
    pub matching_course: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageLesson {
    pub id: String,
    pub title: String,
    pub date: String,
    pub sources: usize,
    pub omitted: usize,
    /// Already in the archive (same id).
    pub exists: bool,
}

fn open_zip(path: &Path) -> Result<zip::ZipArchive<fs::File>> {
    zip::ZipArchive::new(fs::File::open(path).with_context(|| format!("apertura di {}", path.display()))?).map_err(|_| anyhow!("{} non è un pacchetto di lezioni valido", path.display()))
}

fn read_entry(z: &mut zip::ZipArchive<fs::File>, name: &str) -> Result<Vec<u8>> {
    let mut f = z.by_name(name).map_err(|_| anyhow!("pacchetto incompleto: manca {name}"))?;
    let mut v = Vec::new();
    f.read_to_end(&mut v)?;
    Ok(v)
}

fn manifest(z: &mut zip::ZipArchive<fs::File>) -> Result<Manifest> {
    let m: Manifest = serde_json::from_slice(&read_entry(z, PACKAGE_FILE)?).context("pacchetto non valido")?;
    if m.format > FORMAT {
        bail!("pacchetto creato con una versione più recente di Lezioni: aggiorna l'app");
    }
    Ok(m)
}

pub fn inspect(lib: &Library, path: &Path) -> Result<PackageInfo> {
    let mut z = open_zip(path)?;
    let m = manifest(&mut z)?;
    let mut lessons = Vec::new();
    for f in &m.lessons {
        let meta: LessonMeta = serde_json::from_slice(&read_entry(&mut z, &format!("{f}/{LESSON_FILE}"))?)?;
        lessons.push(PackageLesson {
            exists: lib.lesson(&meta.id).is_ok(),
            sources: meta.sources.len(),
            omitted: meta.sources.iter().filter(|s| s.omitted).count(),
            id: meta.id,
            title: meta.title,
            date: meta.date.to_string(),
        });
    }
    let matching_course = lib.courses()?.into_iter().find(|c| c.meta.name.to_lowercase() == m.course.name.to_lowercase()).map(|c| c.meta.id);
    Ok(PackageInfo { course: m.course, exported: m.exported, lessons, matching_course })
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum OnExisting {
    /// Leave the lesson already in the archive.
    Skip,
    /// Import as a new lesson next to it.
    Copy,
    /// The archive's lesson goes to the trash, the package's one takes its place.
    Replace,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub course_id: String,
    pub imported: Vec<String>,
    pub skipped: Vec<String>,
}

/// Imports a package into `course_id` (None = the course with the package's name, created if
/// missing).
pub fn import(lib: &Library, path: &Path, course_id: Option<&str>, on_existing: OnExisting) -> Result<ImportReport> {
    let mut z = open_zip(path)?;
    let m = manifest(&mut z)?;
    let course_id = match course_id {
        Some(id) => lib.course(id)?.meta.id,
        None => match lib.courses()?.into_iter().find(|c| c.meta.name.to_lowercase() == m.course.name.to_lowercase()) {
            Some(c) => c.meta.id,
            None => lib.create_course(&m.course.name, &m.course.teacher, &m.course.terms)?.meta.id,
        },
    };
    // Staged inside the archive, so the final move is a rename on the same disk.
    let stage = lib.root().join(format!(".importazione-{}", uuid::Uuid::new_v4()));
    let r = (|| {
        let mut report = ImportReport { course_id: course_id.clone(), imported: Vec::new(), skipped: Vec::new() };
        for (i, folder) in m.lessons.iter().enumerate() {
            let mut meta: LessonMeta = serde_json::from_slice(&read_entry(&mut z, &format!("{folder}/{LESSON_FILE}"))?)?;
            // The package comes from someone else: a source "file" must not be a path.
            if let Some(s) = meta.sources.iter().find(|s| !is_plain_file_name(&s.file)) {
                bail!("pacchetto non valido: nome di file non ammesso ({})", s.file);
            }
            let mut replaced = None;
            if let Ok(existing) = lib.lesson(&meta.id) {
                match on_existing {
                    OnExisting::Skip => {
                        report.skipped.push(meta.title.clone());
                        continue;
                    }
                    OnExisting::Copy => meta.id = uuid::Uuid::new_v4().to_string(),
                    // Discarded only once the new copy is fully extracted (below).
                    OnExisting::Replace => replaced = Some(existing.path),
                }
            }
            let dir = stage.join(format!("{i:02}"));
            fs::create_dir_all(&dir)?;
            let prefix = format!("{folder}/");
            for k in 0..z.len() {
                let mut f = z.by_index(k)?;
                let Some(name) = f.enclosed_name() else { continue };
                let name = name.to_string_lossy().replace('\\', "/");
                let Some(rel) = name.strip_prefix(&prefix) else { continue };
                if rel == LESSON_FILE || f.is_dir() {
                    continue;
                }
                // `enclosed_name` already refused `..` and absolute paths.
                let out = dir.join(rel);
                fs::create_dir_all(out.parent().unwrap())?;
                std::io::copy(&mut f, &mut fs::File::create(&out)?)?;
            }
            // Sources whose file is not in the package count as omitted.
            for s in &mut meta.sources {
                if !dir.join(SOURCES_DIR).join(&s.file).is_file() {
                    s.omitted = true;
                }
            }
            if let Some(old) = replaced {
                crate::library::discard(&old)?;
            }
            let l = lib.adopt_lesson(&course_id, &dir, meta)?;
            report.imported.push(l.meta.id);
        }
        Ok(report)
    })();
    let _ = fs::remove_dir_all(&stage);
    r
}

// ---------- Markdown (Obsidian, NotebookLM) ----------

fn map_outline(n: &MapNode, depth: usize, out: &mut String) {
    out.push_str(&format!("{}- {}\n", "  ".repeat(depth), n.etichetta));
    for c in &n.figli {
        map_outline(c, depth + 1, out);
    }
}

/// Writes one folder per lesson into `dest`: summary, map, study, and one file per transcript.
/// Returns the folders written.
pub fn export_markdown(lib: &Library, lesson_ids: &[String], dest: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for id in lesson_ids {
        let l = lib.lesson(id)?;
        let course = lib.course(&l.course_id)?.meta.name;
        let o = pipeline::outputs(&l);
        let dir = dest.join(safe_name(&format!("{} {}", l.meta.date, l.meta.title), 100));
        fs::create_dir_all(&dir)?;
        let front = format!("---\ncorso: \"{}\"\nlezione: \"{}\"\ndata: {}\n---\n\n", course.replace('"', "'"), l.meta.title.replace('"', "'"), l.meta.date);
        let codes: Vec<(String, String)> = o.info.as_ref().map(|i| i.sources.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default();
        let legend: String = codes
            .iter()
            .filter_map(|(code, sid)| l.meta.sources.iter().find(|s| &s.id == sid).map(|s| format!("- **{code}**: {}\n", s.original_name)))
            .collect();
        if let Some(s) = &o.summary {
            let legend = if legend.is_empty() { String::new() } else { format!("\n\n---\n\n**Fonti**\n\n{legend}") };
            fs::write(dir.join("Sintesi.md"), format!("{front}# {}\n\n{s}{legend}\n", l.meta.title))?;
        }
        if let Some(m) = &o.map {
            let mut outline = String::new();
            map_outline(m, 0, &mut outline);
            fs::write(dir.join("Mappa.md"), format!("{front}# Mappa · {}\n\n```mermaid\n{}```\n\n## Schema\n\n{outline}", l.meta.title, mermaid(m)))?;
        }
        if let Some(st) = &o.study {
            let mut t = format!("{front}# Studio · {}\n\n## Flashcard\n\n", l.meta.title);
            for c in &st.flashcard {
                t.push_str(&format!("**D:** {}\n\n**R:** {}\n\n", c.domanda, c.risposta));
            }
            t.push_str("## Domande d'esame\n\n");
            for (i, q) in st.domande_esame.iter().enumerate() {
                t.push_str(&format!("{}. **{}**\n\n   {}\n\n", i + 1, q.domanda, q.traccia.replace('\n', "\n   ")));
            }
            t.push_str("## Glossario\n\n");
            for g in &st.glossario {
                t.push_str(&format!("- **{}**: {}\n", g.termine, g.definizione));
            }
            fs::write(dir.join("Studio.md"), t)?;
        }
        for s in &l.meta.sources {
            if let Some(text) = o.extracts.get(&s.id) {
                let what = if matches!(s.kind, SourceKind::Audio | SourceKind::Video) { "Trascrizione" } else { "Testo" };
                let name = safe_name(&format!("{what} - {}", s.original_name), 110);
                fs::write(dir.join(format!("{name}.md")), format!("{front}# {what}: {}\n\n{text}\n", s.original_name))?;
            }
        }
        if !l.meta.notes.trim().is_empty() {
            fs::write(dir.join("Note.md"), format!("{front}# Note\n\n{}\n", l.meta.notes))?;
        }
        out.push(dir);
    }
    Ok(out)
}

// ---------- Anki ----------

fn anki_field(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('\t', " ").replace("\r\n", "\n").replace('\n', "<br>")
}

/// Anki text import (Anki 2.1.54+ reads the `#` headers): front, back, tags. Returns the cards.
pub fn export_anki(lib: &Library, lesson_ids: &[String], dest: &Path) -> Result<usize> {
    let mut t = String::from("#separator:tab\n#html:true\n#tags column:3\n");
    let mut n = 0;
    for id in lesson_ids {
        let l = lib.lesson(id)?;
        let course = lib.course(&l.course_id)?.meta.name;
        let tag = |s: &str| s.split_whitespace().collect::<Vec<_>>().join("_");
        let tags = format!("{} {}", tag(&course), tag(&format!("{} {}", l.meta.date, l.meta.title)));
        let Some(st) = pipeline::outputs(&l).study else { continue };
        for c in &st.flashcard {
            t.push_str(&format!("{}\t{}\t{tags}\n", anki_field(&c.domanda), anki_field(&c.risposta)));
            n += 1;
        }
        for q in &st.domande_esame {
            t.push_str(&format!("{}\t{}\t{tags} domanda_esame\n", anki_field(&q.domanda), anki_field(&q.traccia)));
            n += 1;
        }
    }
    if n == 0 {
        bail!("nessuna flashcard: elabora prima le lezioni con Gemini");
    }
    fs::write(dest, t)?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{Card, ExamQuestion, Study, Term, extract_rel};

    fn lesson_with_outputs(lib: &Library, course: &str) -> Lesson {
        let c = match lib.courses().unwrap().into_iter().find(|c| c.meta.name == course) {
            Some(c) => c,
            None => lib.create_course(course, "Bianchi", "").unwrap(),
        };
        let l = lib.create_lesson(&c.meta.id, "Simplesso", "2026-11-10".parse().unwrap()).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let (a, img) = (tmp.path().join("reg.m4a"), tmp.path().join("lavagna.jpg"));
        fs::write(&a, vec![7u8; 1000]).unwrap();
        fs::write(&img, b"jpg").unwrap();
        let (l, _) = lib.add_files(&l.meta.id, &[a, img]).unwrap();
        fs::write(l.path.join(extract_rel(&l.meta.sources[0])), "[00:00:05] Oggi il simplesso.").unwrap();
        fs::write(l.path.join("generati/sintesi.md"), "## Simplesso\nVertici. [F1 00:00:05]").unwrap();
        fs::write(l.path.join("generati/mappa.json"), r#"{"etichetta":"Simplesso","figli":[{"etichetta":"Pivot"}]}"#).unwrap();
        let st = Study {
            glossario: vec![Term { termine: "Base".into(), definizione: "variabili".into() }],
            flashcard: vec![Card { domanda: "Cos'è\nil pivot?".into(), risposta: "Un <cambio> di base".into() }],
            domande_esame: vec![ExamQuestion { domanda: "Spiega il simplesso".into(), traccia: "…".into() }],
        };
        fs::write(l.path.join("generati/studio.json"), serde_json::to_vec(&st).unwrap()).unwrap();
        fs::write(l.path.join("generati/info.json"), format!(r#"{{"generated":"2026-11-10T10:00:00Z","model":"m","usage":{{}},"suggestedTitle":"x","sources":{{"F1":"{}"}}}}"#, l.meta.sources[0].id)).unwrap();
        fs::create_dir_all(l.path.join("generati/versioni/vecchia")).unwrap();
        fs::write(l.path.join("generati/versioni/vecchia/sintesi.md"), "old").unwrap();
        fs::create_dir_all(l.path.join("estratti/versioni/vecchia")).unwrap();
        fs::write(l.path.join("estratti/versioni/vecchia/reg.m4a.md"), "old").unwrap();
        lib.lesson(&l.meta.id).unwrap()
    }

    #[test]
    fn package_round_trip() {
        let a = tempfile::tempdir().unwrap();
        let lib_a = Library::open(a.path()).unwrap();
        let l = lesson_with_outputs(&lib_a, "Ricerca operativa");
        let pkg = a.path().join("simplesso.lezione");
        export_package(&lib_a, &[l.meta.id.clone()], Include { media: false, files: true, notes: false }, &pkg).unwrap();

        // Another student's archive.
        let b = tempfile::tempdir().unwrap();
        let lib_b = Library::open(b.path()).unwrap();
        let info = inspect(&lib_b, &pkg).unwrap();
        assert_eq!(info.course.name, "Ricerca operativa");
        assert_eq!((info.lessons.len(), info.lessons[0].omitted, info.lessons[0].exists), (1, 1, false));
        let r = import(&lib_b, &pkg, None, OnExisting::Skip).unwrap();
        assert_eq!(r.imported, [l.meta.id.clone()]);
        let course = lib_b.course(&r.course_id).unwrap();
        assert_eq!((course.meta.name.as_str(), course.meta.teacher.as_str()), ("Ricerca operativa", "Bianchi"));
        let got = lib_b.lesson(&l.meta.id).unwrap();
        assert!(got.meta.sources[0].omitted, "recording left out");
        assert!(!got.path.join("fonti/reg.m4a").exists());
        assert!(got.path.join("fonti/lavagna.jpg").exists());
        let o = pipeline::outputs(&got);
        assert_eq!(o.extracts[&got.meta.sources[0].id], "[00:00:05] Oggi il simplesso.", "transcript travels without the audio");
        assert!(o.summary.is_some() && o.study.is_some() && o.map.is_some());
        assert!(!got.path.join("generati/versioni").exists(), "old versions not shared");
        assert!(!got.path.join("estratti/versioni").exists(), "copies of corrected transcripts not shared");
        assert!(fs::read_dir(b.path()).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().starts_with(".importazione")));

        // Same package again.
        assert_eq!(inspect(&lib_b, &pkg).unwrap().lessons[0].exists, true);
        assert_eq!(import(&lib_b, &pkg, None, OnExisting::Skip).unwrap().skipped.len(), 1);
        let copy = import(&lib_b, &pkg, None, OnExisting::Copy).unwrap();
        assert_ne!(copy.imported[0], l.meta.id);
        assert_eq!(lib_b.lessons(&r.course_id).unwrap().len(), 2);
        import(&lib_b, &pkg, None, OnExisting::Replace).unwrap();
        assert_eq!(lib_b.lessons(&r.course_id).unwrap().len(), 2, "replaced, not added");

        // With the recording.
        export_package(&lib_a, &[l.meta.id.clone()], Include { media: true, files: true, notes: true }, &pkg).unwrap();
        let c = tempfile::tempdir().unwrap();
        let lib_c = Library::open(c.path()).unwrap();
        import(&lib_c, &pkg, None, OnExisting::Skip).unwrap();
        let got = lib_c.lesson(&l.meta.id).unwrap();
        assert!(!got.meta.sources[0].omitted);
        assert_eq!(fs::read(got.path.join("fonti/reg.m4a")).unwrap().len(), 1000);
    }

    #[test]
    fn private_things_stay_home() {
        let a = tempfile::tempdir().unwrap();
        let lib = Library::open(a.path().join("a")).unwrap();
        let l = lesson_with_outputs(&lib, "RO");
        lib.update_lesson(&l.meta.id, crate::library::LessonPatch { notes: Some("non capisco niente".into()), ..Default::default() }).unwrap();
        fs::write(l.path.join(crate::chat::CHAT_FILE), "[]").unwrap();
        fs::write(l.path.join(crate::costs::COSTS_FILE), "[]").unwrap();
        let pkg = a.path().join("p.lezione");
        export_package(&lib, std::slice::from_ref(&l.meta.id), Include { media: false, files: true, notes: false }, &pkg).unwrap();
        let mut z = open_zip(&pkg).unwrap();
        let names: Vec<String> = z.file_names().map(str::to_string).collect();
        assert!(!names.iter().any(|n| n.ends_with("chat.json") || n.ends_with("costi.json")), "{names:?}");
        let meta: LessonMeta = serde_json::from_slice(&read_entry(&mut z, "lezioni/01/lezione.json").unwrap()).unwrap();
        assert!(meta.notes.is_empty());
    }

    /// A package written by hand (or by someone malicious).
    fn handmade(path: &Path, meta: &serde_json::Value, extra: &[(&str, &[u8])]) {
        let mut zw = zip::ZipWriter::new(fs::File::create(path).unwrap());
        let m = Manifest { format: 1, app: "x".into(), exported: Utc::now(), course: PackageCourse { name: "RO".into(), teacher: String::new(), terms: String::new() }, lessons: vec!["lezioni/01".into()] };
        zw.start_file(PACKAGE_FILE, zip_options()).unwrap();
        zw.write_all(&serde_json::to_vec(&m).unwrap()).unwrap();
        zw.start_file("lezioni/01/lezione.json", zip_options()).unwrap();
        zw.write_all(&serde_json::to_vec(meta).unwrap()).unwrap();
        for (n, b) in extra {
            zw.start_file(*n, zip_options()).unwrap();
            zw.write_all(b).unwrap();
        }
        zw.finish().unwrap();
    }

    #[test]
    fn rejects_paths_in_packages() {
        let t = tempfile::tempdir().unwrap();
        let lib = Library::open(t.path().join("a")).unwrap();
        let victim = t.path().join("importante.txt");
        fs::write(&victim, "non toccare").unwrap();
        let pkg = t.path().join("cattivo.lezione");
        let meta = serde_json::json!({ "id": "x1", "title": "T", "date": "2026-11-10", "created": "2026-11-10T00:00:00Z",
            "sources": [{ "id": "s", "file": "../../../importante.txt", "kind": "text", "originalName": "a", "size": 1, "added": "2026-11-10T00:00:00Z" }] });
        handmade(&pkg, &meta, &[]);
        assert!(import(&lib, &pkg, None, OnExisting::Skip).unwrap_err().to_string().contains("non ammesso"));
        assert!(lib.lesson("x1").is_err());
        assert!(victim.exists());
    }

    #[test]
    fn failed_replace_keeps_the_old_lesson() {
        let t = tempfile::tempdir().unwrap();
        let lib = Library::open(t.path().join("a")).unwrap();
        let l = lesson_with_outputs(&lib, "RO");
        let pkg = t.path().join("p.lezione");
        export_package(&lib, std::slice::from_ref(&l.meta.id), Include::default(), &pkg).unwrap();
        // Cut the zip in half: unreadable entries.
        let bytes = fs::read(&pkg).unwrap();
        fs::write(&pkg, &bytes[..bytes.len() / 2]).unwrap();
        assert!(import(&lib, &pkg, None, OnExisting::Replace).is_err());
        assert!(lib.lesson(&l.meta.id).unwrap().path.join("generati/sintesi.md").is_file(), "old lesson still there");
    }

    #[test]
    fn not_a_package() {
        let t = tempfile::tempdir().unwrap();
        let lib = Library::open(t.path().join("a")).unwrap();
        let p = t.path().join("x.lezione");
        fs::write(&p, b"ciao").unwrap();
        assert!(inspect(&lib, &p).unwrap_err().to_string().contains("non è un pacchetto"));
    }

    #[test]
    fn markdown_and_anki() {
        let t = tempfile::tempdir().unwrap();
        let lib = Library::open(t.path().join("archivio")).unwrap();
        let l = lesson_with_outputs(&lib, "Ricerca operativa");
        let dirs = export_markdown(&lib, &[l.meta.id.clone()], &t.path().join("md")).unwrap();
        let sintesi = fs::read_to_string(dirs[0].join("Sintesi.md")).unwrap();
        assert!(sintesi.starts_with("---\ncorso: \"Ricerca operativa\""));
        assert!(sintesi.contains("**F1**: reg.m4a"));
        assert!(fs::read_to_string(dirs[0].join("Mappa.md")).unwrap().contains("```mermaid\nmindmap"));
        assert!(dirs[0].join("Trascrizione - reg.m4a.md").is_file());

        let anki = t.path().join("carte.txt");
        assert_eq!(export_anki(&lib, &[l.meta.id.clone()], &anki).unwrap(), 2);
        let txt = fs::read_to_string(&anki).unwrap();
        assert!(txt.contains("Cos'è<br>il pivot?\tUn &lt;cambio&gt; di base\tRicerca_operativa 2026-11-10_Simplesso\n"), "{txt}");
    }
}
