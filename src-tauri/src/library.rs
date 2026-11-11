//! The archive is a plain folder the user chooses (and can sync with Drive/Dropbox):
//!
//! ```text
//! <root>/<Corso>/corso.json
//! <root>/<Corso>/<YYYY-MM-DD titolo>/lezione.json
//!                                   /fonti/      files as they were added (copied, never moved)
//!                                   /estratti/   text extracted from each source (phase 2)
//!                                   /generati/   summary, map, flashcards (phase 2)
//! ```
//!
//! The JSON files are the source of truth: everything can be rebuilt by scanning the folder, and
//! the files stay readable without the app. Folder names follow the title, cleaned of the
//! characters Windows forbids, and are renamed when the title or date change.
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

pub const COURSE_FILE: &str = "corso.json";
pub const LESSON_FILE: &str = "lezione.json";
pub const SOURCES_DIR: &str = "fonti";
pub const EXTRACTS_DIR: &str = "estratti";
pub const GENERATED_DIR: &str = "generati";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CourseMeta {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub teacher: String,
    /// Terms and names that recur in the course, written correctly: given to Whisper and to the
    /// model as hints.
    #[serde(default)]
    pub terms: String,
    pub created: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Course {
    #[serde(flatten)]
    pub meta: CourseMeta,
    pub path: PathBuf,
    pub lessons: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SourceKind {
    Audio,
    Video,
    Pdf,
    Image,
    Text,
    Document,
    Other,
}

impl SourceKind {
    pub fn from_name(name: &str) -> Self {
        let ext = Path::new(name).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        match ext.as_str() {
            "mp3" | "m4a" | "wav" | "ogg" | "oga" | "opus" | "flac" | "aac" | "wma" | "amr" | "aiff" | "aif" => Self::Audio,
            "mp4" | "mov" | "mkv" | "webm" | "avi" | "m4v" => Self::Video,
            "pdf" => Self::Pdf,
            "png" | "jpg" | "jpeg" | "heic" | "heif" | "webp" | "gif" | "bmp" | "tif" | "tiff" => Self::Image,
            "txt" | "md" | "markdown" => Self::Text,
            "doc" | "docx" | "odt" | "ppt" | "pptx" | "odp" | "xls" | "xlsx" | "rtf" | "key" | "pages" => Self::Document,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    /// File name inside `fonti/`.
    pub file: String,
    pub kind: SourceKind,
    /// Name the file had when it was added (or the title given to pasted text).
    pub original_name: String,
    pub size: u64,
    pub added: DateTime<Utc>,
    /// The file was left out of the package this lesson came from (usually the recording):
    /// its transcript is here, the file is not.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub omitted: bool,
    /// When its extract was corrected by hand (cleared when it is extracted again).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LessonMeta {
    pub id: String,
    pub title: String,
    pub date: NaiveDate,
    #[serde(default)]
    pub notes: String,
    pub created: DateTime<Utc>,
    #[serde(default)]
    pub sources: Vec<Source>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lesson {
    #[serde(flatten)]
    pub meta: LessonMeta,
    pub course_id: String,
    pub path: PathBuf,
}

/// Editable fields of a lesson; `None` = unchanged.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LessonPatch {
    pub title: Option<String>,
    pub date: Option<NaiveDate>,
    pub notes: Option<String>,
}

#[derive(Clone)]
pub struct Library {
    root: PathBuf,
}

const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// A name that is valid as a file or folder on macOS, Windows and Linux: no `/ \ : * ? " < > |`
/// or control characters, no trailing dots or spaces, not a reserved Windows device name,
/// at most `max` characters.
pub fn safe_name(s: &str, max: usize) -> String {
    let cleaned: String = s.chars().map(|c| if c.is_control() || r#"/\:*?"<>|"#.contains(c) { ' ' } else { c }).collect();
    let mut out = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if out.chars().count() > max {
        out = out.chars().take(max).collect();
    }
    let out = out.trim_end_matches(['.', ' ']).trim_start().to_string();
    let stem = out.split('.').next().unwrap_or("").to_ascii_uppercase();
    if out.is_empty() {
        "senza nome".into()
    } else if WINDOWS_RESERVED.contains(&stem.as_str()) {
        format!("_{out}")
    } else {
        out
    }
}

/// Like `safe_name`, but a long name is shortened before the extension, so `.m4a` stays.
pub fn safe_file_name(name: &str, max: usize) -> String {
    let p = Path::new(name);
    match (p.file_stem().and_then(|s| s.to_str()), p.extension().and_then(|s| s.to_str())) {
        (Some(stem), Some(ext)) if !stem.is_empty() && ext.len() <= 10 => format!("{}.{}", safe_name(stem, max.saturating_sub(ext.len() + 1).max(1)), safe_name(ext, 10)),
        _ => safe_name(name, max),
    }
}

/// A source file name as stored in `lezione.json`: a bare name inside `fonti/`, never a path.
/// `lezione.json` can come from someone else's package, so this is checked before any use.
pub fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':', '\0']) && Path::new(name).file_name().and_then(|f| f.to_str()) == Some(name)
}

fn source_file(l: &Lesson, s: &Source) -> Result<PathBuf> {
    if !is_plain_file_name(&s.file) {
        bail!("nome di file non valido nella lezione: {}", s.file);
    }
    Ok(l.path.join(SOURCES_DIR).join(&s.file))
}

/// `dir/name` if free, otherwise `dir/name (2)`, `dir/name (3)`… (before the extension for files).
fn unique_path(dir: &Path, name: &str, except: Option<&Path>) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() || except == Some(p.as_path()) {
        return p;
    }
    let path = Path::new(name);
    let (stem, ext) = match (path.file_stem().and_then(|s| s.to_str()), path.extension().and_then(|s| s.to_str())) {
        (Some(s), Some(e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    for n in 2.. {
        let p = dir.join(format!("{stem} ({n}){ext}"));
        if !p.exists() || except == Some(p.as_path()) {
            return p;
        }
    }
    unreachable!()
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let raw = fs::read(path).with_context(|| format!("lettura di {}", path.display()))?;
    serde_json::from_slice(&raw).with_context(|| format!("{} non è valido", path.display()))
}

/// Writes through a temporary file and a rename, so a crash never leaves half a JSON.
fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(value)?).with_context(|| format!("scrittura di {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("scrittura di {}", path.display()))
}

fn lesson_dir_name(date: NaiveDate, title: &str) -> String {
    safe_name(&format!("{} {}", date.format("%Y-%m-%d"), title), 100)
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)? {
        let e = e?;
        let p = e.path();
        if p.is_dir() { copy_dir(&p, &to.join(e.file_name()))? } else { fs::copy(&p, to.join(e.file_name())).map(|_| ())? }
    }
    Ok(())
}

/// Moves to the system trash (Cestino); where there is none, deletes.
pub fn discard(path: &Path) -> Result<()> {
    // Tests delete for real instead of filling the user's trash.
    if !cfg!(test) && trash::delete(path).is_ok() {
        return Ok(());
    }
    if path.is_dir() { fs::remove_dir_all(path) } else { fs::remove_file(path) }.with_context(|| format!("eliminazione di {}", path.display()))
}

impl Library {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root).with_context(|| format!("creazione della cartella {}", root.display()))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    // ---------- courses ----------

    fn course_dirs(&self) -> Result<Vec<(PathBuf, CourseMeta)>> {
        let mut out = Vec::new();
        for e in fs::read_dir(&self.root).with_context(|| format!("lettura di {}", self.root.display()))? {
            let dir = e?.path();
            let f = dir.join(COURSE_FILE);
            if f.is_file() {
                match read_json::<CourseMeta>(&f) {
                    Ok(m) => out.push((dir, m)),
                    Err(e) => log::warn!("{e:#}"),
                }
            }
        }
        Ok(out)
    }

    fn course_dir(&self, id: &str) -> Result<(PathBuf, CourseMeta)> {
        self.course_dirs()?.into_iter().find(|(_, m)| m.id == id).ok_or_else(|| anyhow!("corso non trovato (forse è stato spostato o eliminato)"))
    }

    pub fn courses(&self) -> Result<Vec<Course>> {
        let mut out: Vec<Course> = self
            .course_dirs()?
            .into_iter()
            .map(|(path, meta)| {
                let lessons = self.lesson_dirs(&path).map(|l| l.len()).unwrap_or(0);
                Course { meta, path, lessons }
            })
            .collect();
        out.sort_by_key(|c| c.meta.name.to_lowercase());
        Ok(out)
    }

    pub fn course(&self, id: &str) -> Result<Course> {
        let (path, meta) = self.course_dir(id)?;
        let lessons = self.lesson_dirs(&path)?.len();
        Ok(Course { meta, path, lessons })
    }

    pub fn create_course(&self, name: &str, teacher: &str, terms: &str) -> Result<Course> {
        let name = name.trim();
        if name.is_empty() {
            bail!("il nome del corso è vuoto");
        }
        let dir = unique_path(&self.root, &safe_name(name, 80), None);
        fs::create_dir_all(&dir)?;
        let meta = CourseMeta { id: new_id(), name: name.to_string(), teacher: teacher.trim().to_string(), terms: terms.trim().to_string(), created: Utc::now() };
        write_json(&dir.join(COURSE_FILE), &meta)?;
        Ok(Course { meta, path: dir, lessons: 0 })
    }

    pub fn update_course(&self, id: &str, name: &str, teacher: &str, terms: &str) -> Result<Course> {
        let (mut dir, mut meta) = self.course_dir(id)?;
        let name = name.trim();
        if name.is_empty() {
            bail!("il nome del corso è vuoto");
        }
        if name != meta.name {
            let to = unique_path(&self.root, &safe_name(name, 80), Some(&dir));
            if to != dir {
                fs::rename(&dir, &to).context("rinomina della cartella del corso")?;
                dir = to;
            }
            meta.name = name.to_string();
        }
        meta.teacher = teacher.trim().to_string();
        meta.terms = terms.trim().to_string();
        write_json(&dir.join(COURSE_FILE), &meta)?;
        let lessons = self.lesson_dirs(&dir)?.len();
        Ok(Course { meta, path: dir, lessons })
    }

    pub fn delete_course(&self, id: &str) -> Result<()> {
        discard(&self.course_dir(id)?.0)
    }

    // ---------- lessons ----------

    fn lesson_dirs(&self, course_dir: &Path) -> Result<Vec<(PathBuf, LessonMeta)>> {
        let mut out = Vec::new();
        for e in fs::read_dir(course_dir)? {
            let dir = e?.path();
            let f = dir.join(LESSON_FILE);
            if f.is_file() {
                match read_json::<LessonMeta>(&f) {
                    Ok(m) => out.push((dir, m)),
                    Err(e) => log::warn!("{e:#}"),
                }
            }
        }
        Ok(out)
    }

    fn find_lesson(&self, id: &str) -> Result<Lesson> {
        for (cdir, course) in self.course_dirs()? {
            if let Some((path, meta)) = self.lesson_dirs(&cdir)?.into_iter().find(|(_, m)| m.id == id) {
                return Ok(Lesson { meta, course_id: course.id, path });
            }
        }
        bail!("lezione non trovata (forse è stata spostata o eliminata)")
    }

    fn save_lesson(&self, l: &Lesson) -> Result<()> {
        write_json(&l.path.join(LESSON_FILE), &l.meta)
    }

    /// Lessons of a course, most recent first.
    pub fn lessons(&self, course_id: &str) -> Result<Vec<Lesson>> {
        let (cdir, _) = self.course_dir(course_id)?;
        let mut out: Vec<Lesson> = self.lesson_dirs(&cdir)?.into_iter().map(|(path, meta)| Lesson { meta, course_id: course_id.to_string(), path }).collect();
        out.sort_by(|a, b| b.meta.date.cmp(&a.meta.date).then(b.meta.created.cmp(&a.meta.created)));
        Ok(out)
    }

    pub fn lesson(&self, id: &str) -> Result<Lesson> {
        self.find_lesson(id)
    }

    pub fn create_lesson(&self, course_id: &str, title: &str, date: NaiveDate) -> Result<Lesson> {
        let (cdir, _) = self.course_dir(course_id)?;
        let title = match title.trim() {
            "" => "Lezione",
            t => t,
        };
        let dir = unique_path(&cdir, &lesson_dir_name(date, title), None);
        for sub in [SOURCES_DIR, EXTRACTS_DIR, GENERATED_DIR] {
            fs::create_dir_all(dir.join(sub))?;
        }
        let meta = LessonMeta { id: new_id(), title: title.to_string(), date, notes: String::new(), created: Utc::now(), sources: Vec::new() };
        let l = Lesson { meta, course_id: course_id.to_string(), path: dir };
        self.save_lesson(&l)?;
        Ok(l)
    }

    pub fn update_lesson(&self, id: &str, patch: LessonPatch) -> Result<Lesson> {
        let mut l = self.find_lesson(id)?;
        if let Some(t) = patch.title {
            let t = t.trim();
            if t.is_empty() {
                bail!("il titolo della lezione è vuoto");
            }
            l.meta.title = t.to_string();
        }
        if let Some(d) = patch.date {
            l.meta.date = d;
        }
        if let Some(n) = patch.notes {
            l.meta.notes = n;
        }
        let parent = l.path.parent().ok_or_else(|| anyhow!("cartella della lezione non valida"))?.to_path_buf();
        let to = unique_path(&parent, &lesson_dir_name(l.meta.date, &l.meta.title), Some(&l.path));
        if to != l.path {
            fs::rename(&l.path, &to).context("rinomina della cartella della lezione")?;
            l.path = to;
        }
        self.save_lesson(&l)?;
        Ok(l)
    }

    /// Moves a lesson folder prepared elsewhere (an imported package) into a course, with the
    /// usual folder name, and writes its `lezione.json`. `staged` must be on the same disk.
    pub fn adopt_lesson(&self, course_id: &str, staged: &Path, meta: LessonMeta) -> Result<Lesson> {
        let (cdir, _) = self.course_dir(course_id)?;
        let dir = unique_path(&cdir, &lesson_dir_name(meta.date, &meta.title), None);
        fs::rename(staged, &dir).or_else(|_| copy_dir(staged, &dir).and_then(|_| fs::remove_dir_all(staged).map_err(Into::into))).context("spostamento della lezione importata")?;
        for sub in [SOURCES_DIR, EXTRACTS_DIR, GENERATED_DIR] {
            fs::create_dir_all(dir.join(sub))?;
        }
        let l = Lesson { meta, course_id: course_id.to_string(), path: dir };
        self.save_lesson(&l)?;
        Ok(l)
    }

    pub fn delete_lesson(&self, id: &str) -> Result<()> {
        discard(&self.find_lesson(id)?.path)
    }

    // ---------- sources ----------

    fn push_source(&self, l: &mut Lesson, dest: &Path, original_name: &str) -> Result<Source> {
        let file = dest.file_name().and_then(|s| s.to_str()).ok_or_else(|| anyhow!("nome di file non valido"))?.to_string();
        let src = Source {
            id: new_id(),
            kind: SourceKind::from_name(&file),
            file,
            original_name: original_name.to_string(),
            size: fs::metadata(dest)?.len(),
            added: Utc::now(),
            omitted: false,
            edited: None,
        };
        l.meta.sources.push(src.clone());
        Ok(src)
    }

    fn sources_dir(l: &Lesson) -> Result<PathBuf> {
        let d = l.path.join(SOURCES_DIR);
        fs::create_dir_all(&d)?;
        Ok(d)
    }

    /// Copies files (drag & drop, file picker) into `fonti/`. Folders are skipped with a warning.
    pub fn add_files(&self, lesson_id: &str, paths: &[PathBuf]) -> Result<(Lesson, Vec<String>)> {
        let mut l = self.find_lesson(lesson_id)?;
        let dir = Self::sources_dir(&l)?;
        let mut warnings = Vec::new();
        for p in paths {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("file").to_string();
            if p.is_dir() {
                warnings.push(format!("{name}: è una cartella, trascina i file che contiene"));
                continue;
            }
            let dest = unique_path(&dir, &safe_file_name(&name, 120), None);
            match fs::copy(p, &dest) {
                Ok(_) => {
                    self.push_source(&mut l, &dest, &name)?;
                }
                Err(e) => warnings.push(format!("{name}: {e}")),
            }
        }
        self.save_lesson(&l)?;
        Ok((l, warnings))
    }

    /// Saves bytes pasted from the clipboard (a screenshot, an image copied from the tablet app).
    pub fn add_bytes(&self, lesson_id: &str, name: &str, bytes: &[u8]) -> Result<Lesson> {
        let mut l = self.find_lesson(lesson_id)?;
        let dest = unique_path(&Self::sources_dir(&l)?, &safe_file_name(name, 120), None);
        fs::write(&dest, bytes)?;
        self.push_source(&mut l, &dest, name)?;
        self.save_lesson(&l)?;
        Ok(l)
    }

    /// Saves pasted or typed text as a Markdown source.
    pub fn add_text(&self, lesson_id: &str, title: &str, text: &str) -> Result<Lesson> {
        let title = match title.trim() {
            "" => "Testo incollato",
            t => t,
        };
        let mut l = self.find_lesson(lesson_id)?;
        let dest = unique_path(&Self::sources_dir(&l)?, &format!("{}.md", safe_name(title, 80)), None);
        fs::write(&dest, text)?;
        self.push_source(&mut l, &dest, title)?;
        self.save_lesson(&l)?;
        Ok(l)
    }

    pub fn remove_source(&self, lesson_id: &str, source_id: &str) -> Result<Lesson> {
        let mut l = self.find_lesson(lesson_id)?;
        let i = l.meta.sources.iter().position(|s| s.id == source_id).ok_or_else(|| anyhow!("fonte non trovata"))?;
        let p = source_file(&l, &l.meta.sources[i])?;
        let src = l.meta.sources.remove(i);
        if p.exists() {
            discard(&p)?;
        }
        // Its extract (and Whisper's text) must not end up in the next summary.
        for name in [format!("{}.md", src.file), format!("{}.whisper.md", src.file)] {
            let e = l.path.join(EXTRACTS_DIR).join(name);
            if e.exists() {
                fs::remove_file(&e)?;
            }
        }
        crate::bookmarks::remove_source(&l.path, source_id)?;
        self.save_lesson(&l)?;
        Ok(l)
    }

    /// Marks a source's extract as corrected by hand (`Some`) or extracted again (`None`).
    pub fn set_source_edited(&self, lesson_id: &str, source_id: &str, when: Option<DateTime<Utc>>) -> Result<Lesson> {
        let mut l = self.find_lesson(lesson_id)?;
        let s = l.meta.sources.iter_mut().find(|s| s.id == source_id).ok_or_else(|| anyhow!("fonte non trovata"))?;
        if s.edited == when {
            return Ok(l);
        }
        s.edited = when;
        self.save_lesson(&l)?;
        Ok(l)
    }

    pub fn source_path(&self, lesson_id: &str, source_id: &str) -> Result<PathBuf> {
        let l = self.find_lesson(lesson_id)?;
        let s = l.meta.sources.iter().find(|s| s.id == source_id).ok_or_else(|| anyhow!("fonte non trovata"))?;
        source_file(&l, s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib() -> (tempfile::TempDir, Library) {
        let t = tempfile::tempdir().unwrap();
        let l = Library::open(t.path().join("Lezioni")).unwrap();
        (t, l)
    }

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    #[test]
    fn safe_names() {
        assert_eq!(safe_name("Analisi: I/II?", 80), "Analisi I II");
        assert_eq!(safe_name("con", 80), "_con");
        assert_eq!(safe_name("  fine...  ", 80), "fine");
        assert_eq!(safe_name("", 80), "senza nome");
        assert_eq!(safe_name("abcdef", 3), "abc");
        assert!(is_plain_file_name("reg (2).m4a"));
        for bad in ["../x", "a/b", "..\\x", "C:x", "", "..", "/etc/passwd"] {
            assert!(!is_plain_file_name(bad), "{bad}");
        }
        let long = format!("{}.m4a", "x".repeat(300));
        let f = safe_file_name(&long, 120);
        assert!(f.ends_with(".m4a") && f.chars().count() == 120, "{f}");
        assert_eq!(safe_file_name("foto: 1.JPG", 120), "foto 1.JPG");
    }

    #[test]
    fn kinds() {
        assert_eq!(SourceKind::from_name("lezione.M4A"), SourceKind::Audio);
        assert_eq!(SourceKind::from_name("slide.pdf"), SourceKind::Pdf);
        assert_eq!(SourceKind::from_name("foto.heic"), SourceKind::Image);
        assert_eq!(SourceKind::from_name("note"), SourceKind::Other);
    }

    #[test]
    fn courses_and_lessons() {
        let (_t, lib) = lib();
        let c = lib.create_course("Ricerca operativa", "Rossi", "").unwrap();
        let c2 = lib.create_course("Ricerca operativa", "", "").unwrap();
        assert_ne!(c.path, c2.path, "same name gets a different folder");
        let l1 = lib.create_lesson(&c.meta.id, "Simplesso", d("2026-11-03")).unwrap();
        let l2 = lib.create_lesson(&c.meta.id, "Dualità", d("2026-11-10")).unwrap();
        assert!(l1.path.join(SOURCES_DIR).is_dir());
        let ls = lib.lessons(&c.meta.id).unwrap();
        assert_eq!(ls.iter().map(|l| l.meta.title.as_str()).collect::<Vec<_>>(), ["Dualità", "Simplesso"]);
        assert_eq!(lib.courses().unwrap().iter().find(|x| x.meta.id == c.meta.id).unwrap().lessons, 2);

        let u = lib.update_lesson(&l2.meta.id, LessonPatch { title: Some("Teoria della dualità".into()), ..Default::default() }).unwrap();
        assert_eq!(u.path.file_name().unwrap(), "2026-11-10 Teoria della dualità");
        assert!(!l2.path.exists());

        let rc = lib.update_course(&c.meta.id, "RO", "Rossi", "").unwrap();
        assert_eq!(rc.path.file_name().unwrap(), "RO");
        // Lessons are still found after the course folder moved.
        assert_eq!(lib.lesson(&l1.meta.id).unwrap().meta.title, "Simplesso");
    }

    #[test]
    fn sources() {
        let (t, lib) = lib();
        let c = lib.create_course("Economia", "", "").unwrap();
        let l = lib.create_lesson(&c.meta.id, "", d("2026-11-10")).unwrap();
        assert_eq!(l.meta.title, "Lezione");
        let a = t.path().join("reg.m4a");
        fs::write(&a, b"audio").unwrap();
        let (l2, w) = lib.add_files(&l.meta.id, &[a.clone(), a.clone(), t.path().to_path_buf()]).unwrap();
        assert_eq!(w.len(), 1, "folder skipped");
        assert_eq!(l2.meta.sources.iter().map(|s| s.file.as_str()).collect::<Vec<_>>(), ["reg.m4a", "reg (2).m4a"]);
        assert!(a.exists(), "original kept");
        let l3 = lib.add_text(&l.meta.id, "Domande", "Cos'è il VAN?").unwrap();
        assert_eq!(l3.meta.sources[2].kind, SourceKind::Text);
        let l4 = lib.add_bytes(&l.meta.id, "incollato.png", &[1, 2, 3]).unwrap();
        assert_eq!(l4.meta.sources[3].size, 3);
        let first = l4.meta.sources[0].id.clone();
        let p = lib.source_path(&l.meta.id, &first).unwrap();
        let extract = l4.path.join(EXTRACTS_DIR).join("reg.m4a.md");
        fs::write(&extract, "trascrizione").unwrap();
        let l5 = lib.remove_source(&l.meta.id, &first).unwrap();
        assert_eq!(l5.meta.sources.len(), 3);
        assert!(!p.exists());
        assert!(!extract.exists(), "extract removed with its source");
    }
}
