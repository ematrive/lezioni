//! Earlier versions of a lesson's texts, kept in `generati/versioni/<stamp>/` (outputs replaced
//! by Rigenera) and `estratti/versioni/<stamp>/` (transcripts corrected by hand, then redone).
//! They can be looked at and brought back; what is replaced becomes a version in turn.
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use crate::library::{EXTRACTS_DIR, GENERATED_DIR, Lesson, Library, is_plain_file_name};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Area {
    Generati,
    Estratti,
}

impl Area {
    fn dir(self) -> &'static str {
        match self {
            Area::Generati => GENERATED_DIR,
            Area::Estratti => EXTRACTS_DIR,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub area: Area,
    /// Folder name: UTC "2026-11-11 14-03-22" (with " 2" when two fell in the same second).
    pub stamp: String,
    pub files: Vec<String>,
}

/// All versions of a lesson, newest first.
pub fn list(lesson: &Lesson) -> Vec<Version> {
    let mut out = Vec::new();
    for area in [Area::Generati, Area::Estratti] {
        let Ok(rd) = fs::read_dir(lesson.path.join(area.dir()).join("versioni")) else { continue };
        for e in rd.flatten().filter(|e| e.path().is_dir()) {
            let stamp = e.file_name().to_string_lossy().to_string();
            let mut files: Vec<String> = fs::read_dir(e.path()).map(|rd| rd.flatten().filter(|f| f.path().is_file()).map(|f| f.file_name().to_string_lossy().to_string()).collect()).unwrap_or_default();
            if files.is_empty() {
                continue;
            }
            files.sort();
            out.push(Version { area, stamp, files });
        }
    }
    out.sort_by(|a, b| b.stamp.cmp(&a.stamp));
    out
}

/// `<lesson>/<area>/versioni/<stamp>`, refusing names that would leave it.
fn version_dir(lesson: &Lesson, area: Area, stamp: &str) -> Result<PathBuf> {
    if !is_plain_file_name(stamp) {
        bail!("versione non valida");
    }
    let d = lesson.path.join(area.dir()).join("versioni").join(stamp);
    if !d.is_dir() {
        bail!("versione non trovata: {stamp}");
    }
    Ok(d)
}

fn file_in(dir: &Path, file: &str) -> Result<PathBuf> {
    if !is_plain_file_name(file) {
        bail!("file non valido");
    }
    let p = dir.join(file);
    if !p.is_file() {
        bail!("file non trovato nella versione: {file}");
    }
    Ok(p)
}

pub fn read(lib: &Library, lesson_id: &str, area: Area, stamp: &str, file: &str) -> Result<String> {
    let l = lib.lesson(lesson_id)?;
    Ok(fs::read_to_string(file_in(&version_dir(&l, area, stamp)?, file)?)?)
}

/// Brings a version back. Outputs: all the files of the version, after moving the current ones
/// to a new version. A transcript: `file` (`<source file>.md`), after copying the current one to
/// a new version; it counts as corrected by hand. The caller holds the write lock and checks
/// that the lesson is not being processed.
pub fn restore(lib: &Library, lesson_id: &str, area: Area, stamp: &str, file: Option<&str>) -> Result<Lesson> {
    let l = lib.lesson(lesson_id)?;
    let from = version_dir(&l, area, stamp)?;
    match area {
        Area::Generati => {
            let gdir = l.path.join(GENERATED_DIR);
            // Copied out first: the current files and the version could be moved under each other.
            let files: Vec<(String, Vec<u8>)> = fs::read_dir(&from)?.flatten().filter(|e| e.path().is_file()).map(|e| Ok((e.file_name().to_string_lossy().to_string(), fs::read(e.path())?))).collect::<Result<_>>()?;
            crate::pipeline::archive_generated(&gdir)?;
            for (name, bytes) in files {
                crate::pipeline::write_atomic(&gdir, Path::new(&name), &bytes)?;
            }
            Ok(l)
        }
        Area::Estratti => {
            let file = file.ok_or_else(|| anyhow!("quale trascrizione?"))?;
            let text = fs::read_to_string(file_in(&from, file)?)?;
            let s = l.meta.sources.iter().find(|s| format!("{}.md", s.file) == file).ok_or_else(|| anyhow!("la fonte di questa versione non c'è più nella lezione"))?;
            let current = l.path.join(EXTRACTS_DIR).join(file);
            if current.is_file() {
                let dest = crate::pipeline::new_version_dir(&l.path.join(EXTRACTS_DIR));
                fs::create_dir_all(&dest)?;
                fs::copy(&current, dest.join(file))?;
            }
            crate::pipeline::save_extract(lib, lesson_id, &s.id.clone(), &text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, Library, Lesson) {
        let tmp = tempfile::tempdir().unwrap();
        let lib = Library::open(tmp.path()).unwrap();
        let c = lib.create_course("RO", "", "").unwrap();
        let l = lib.create_lesson(&c.meta.id, "Simplesso", chrono::NaiveDate::from_ymd_opt(2026, 11, 3).unwrap()).unwrap();
        let l = lib.add_bytes(&l.meta.id, "reg.m4a", b"x").unwrap();
        (tmp, lib, l)
    }

    #[test]
    fn outputs_come_back_and_the_current_ones_become_a_version() {
        let (_t, lib, l) = setup();
        let g = l.path.join(GENERATED_DIR);
        fs::create_dir_all(g.join("versioni/2026-11-01 10-00-00")).unwrap();
        fs::write(g.join("versioni/2026-11-01 10-00-00/sintesi.md"), "vecchia").unwrap();
        fs::write(g.join("sintesi.md"), "nuova").unwrap();
        fs::write(g.join("mappa.json"), "{}").unwrap();
        let v = list(&l);
        assert_eq!(v, [Version { area: Area::Generati, stamp: "2026-11-01 10-00-00".into(), files: vec!["sintesi.md".into()] }]);
        assert_eq!(read(&lib, &l.meta.id, Area::Generati, &v[0].stamp, "sintesi.md").unwrap(), "vecchia");
        restore(&lib, &l.meta.id, Area::Generati, &v[0].stamp, None).unwrap();
        assert_eq!(fs::read_to_string(g.join("sintesi.md")).unwrap(), "vecchia");
        assert!(!g.join("mappa.json").exists(), "the version had no map");
        let v = list(&l);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].files, ["mappa.json", "sintesi.md"], "the replaced outputs, newest first");
        assert_eq!(read(&lib, &l.meta.id, Area::Generati, &v[0].stamp, "sintesi.md").unwrap(), "nuova");
    }

    #[test]
    fn a_transcript_comes_back_corrected_by_hand() {
        let (_t, lib, l) = setup();
        let e = l.path.join(EXTRACTS_DIR);
        fs::create_dir_all(e.join("versioni/2026-11-01 10-00-00")).unwrap();
        fs::write(e.join("versioni/2026-11-01 10-00-00/reg.m4a.md"), "[00:00:01] corretta").unwrap();
        fs::write(e.join("reg.m4a.md"), "[00:00:01] rifatta").unwrap();
        let l2 = restore(&lib, &l.meta.id, Area::Estratti, "2026-11-01 10-00-00", Some("reg.m4a.md")).unwrap();
        assert_eq!(fs::read_to_string(e.join("reg.m4a.md")).unwrap(), "[00:00:01] corretta");
        assert!(l2.meta.sources[0].edited.is_some());
        let v = list(&l);
        assert_eq!(v.len(), 2);
        assert_eq!(read(&lib, &l.meta.id, Area::Estratti, &v[0].stamp, "reg.m4a.md").unwrap(), "[00:00:01] rifatta");
    }

    #[test]
    fn names_that_leave_the_folder_are_refused() {
        let (_t, lib, l) = setup();
        fs::create_dir_all(l.path.join(GENERATED_DIR).join("versioni/a")).unwrap();
        fs::write(l.path.join("lezione.json.bak"), "x").unwrap();
        assert!(read(&lib, &l.meta.id, Area::Generati, "..", "lezione.json").is_err());
        assert!(read(&lib, &l.meta.id, Area::Generati, "a", "../../lezione.json").is_err());
        assert!(restore(&lib, &l.meta.id, Area::Generati, "../..", None).is_err());
    }
}
