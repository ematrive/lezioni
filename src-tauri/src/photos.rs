//! Photos (blackboard, notes) linked to a minute of a recording. `foto.json` in the lesson
//! folder: one entry per photo, made from the time the photo was taken (EXIF) when the start of
//! the recording is known, otherwise by the model from the texts; or chosen by hand, and then
//! never touched by the automatic linking.
use std::fs;
use std::path::Path;

use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use crate::ai::{Ai, Part, Request, Usage, prompts};

pub const PHOTOS_FILE: &str = "foto.json";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum By {
    /// From the time the photo was taken and the start of the recording.
    Orario,
    Modello,
    Mano,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImageLink {
    /// Source ids; `recording` is empty when there is no minute.
    pub image: String,
    #[serde(default)]
    pub recording: String,
    /// Seconds from the start of the recording. None: not found (by the model) or unlinked by hand.
    #[serde(default)]
    pub at: Option<u64>,
    pub by: By,
    /// Why the model found no minute.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub why: String,
    /// Hash of what it was made from (automatic links only).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub inputs: String,
}

pub fn read(lesson_dir: &Path) -> Vec<ImageLink> {
    fs::read(lesson_dir.join(PHOTOS_FILE)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn write(lesson_dir: &Path, v: &[ImageLink]) -> Result<()> {
    if v.is_empty() {
        let p = lesson_dir.join(PHOTOS_FILE);
        if p.exists() {
            fs::remove_file(p)?;
        }
        return Ok(());
    }
    crate::pipeline::write_atomic(lesson_dir, Path::new(PHOTOS_FILE), &serde_json::to_vec_pretty(v)?)
}

/// Saves links in place of those of the same photos. The caller holds the write lock.
pub fn save(lesson_dir: &Path, links: Vec<ImageLink>) -> Result<()> {
    let mut v = read(lesson_dir);
    v.retain(|x| !links.iter().any(|l| l.image == x.image));
    v.extend(links);
    write(lesson_dir, &v)
}

/// Saves automatic links, but never over a link chosen by hand meanwhile (the user can link a
/// photo while the lesson is processed). The caller holds the write lock.
pub fn save_found(lesson_dir: &Path, mut links: Vec<ImageLink>) -> Result<()> {
    let hand: Vec<String> = read(lesson_dir).into_iter().filter(|l| l.by == By::Mano).map(|l| l.image).collect();
    links.retain(|l| !hand.contains(&l.image));
    if links.is_empty() {
        return Ok(());
    }
    save(lesson_dir, links)
}

/// Forgets the link of a photo: the automatic linking will try again.
pub fn forget(lesson_dir: &Path, image: &str) -> Result<()> {
    let mut v = read(lesson_dir);
    v.retain(|x| x.image != image);
    write(lesson_dir, &v)
}

/// A source removed: a photo's link goes, and the links to a recording.
pub fn remove_source(lesson_dir: &Path, source_id: &str) -> Result<()> {
    let mut v = read(lesson_dir);
    let n = v.len();
    v.retain(|l| l.image != source_id && l.recording != source_id);
    if v.len() != n {
        write(lesson_dir, &v)?;
    }
    Ok(())
}

/// When the photo was taken, from its EXIF data (JPEG, HEIC, PNG, WebP). Without a time zone
/// in the file, the one of this computer.
pub fn taken_at(path: &Path) -> Option<DateTime<Utc>> {
    let f = fs::File::open(path).ok()?;
    let exif = exif::Reader::new().read_from_container(&mut std::io::BufReader::new(f)).ok()?;
    let ascii = |tag| match exif.get_field(tag, exif::In::PRIMARY).map(|f| &f.value) {
        Some(exif::Value::Ascii(v)) => v.first().cloned(),
        _ => None,
    };
    let mut dt = exif::DateTime::from_ascii(&ascii(exif::Tag::DateTimeOriginal).or_else(|| ascii(exif::Tag::DateTime))?).ok()?;
    if let Some(o) = ascii(exif::Tag::OffsetTimeOriginal) {
        let _ = dt.parse_offset(&o);
    }
    let naive = chrono::NaiveDate::from_ymd_opt(dt.year.into(), dt.month.into(), dt.day.into())?.and_hms_opt(dt.hour.into(), dt.minute.into(), dt.second.into())?;
    match dt.offset {
        Some(min) => Some(chrono::FixedOffset::east_opt(i32::from(min) * 60)?.from_local_datetime(&naive).single()?.with_timezone(&Utc)),
        None => Some(chrono::Local.from_local_datetime(&naive).earliest()?.with_timezone(&Utc)),
    }
}
use chrono::TimeZone;

/// The minute of a photo from the clocks: taken − start, inside the recording (a minute of slack).
pub fn by_clock(taken: DateTime<Utc>, start: DateTime<Utc>, length: f64) -> Option<u64> {
    let s = (taken - start).num_seconds();
    (s >= -60 && (s as f64) <= length + 60.0).then(|| s.clamp(0, length as i64) as u64)
}

/// When a recording started, for the photos of its lesson. A time from the mp4 header may be the
/// end instead (it depends on the app): the reading under which more of the photos fall inside
/// the recording wins, decided once for all of them; a tie means the clocks can't tell (None).
pub fn clock_start(taken: &[DateTime<Utc>], started: DateTime<Utc>, from_mp4: bool, length: f64) -> Option<DateTime<Utc>> {
    if !from_mp4 {
        return Some(started);
    }
    let as_end = started - chrono::Duration::milliseconds((length * 1000.0) as i64);
    let inside = |start| taken.iter().filter(|t| by_clock(**t, start, length).is_some()).count();
    match inside(started).cmp(&inside(as_end)) {
        std::cmp::Ordering::Greater => Some(started),
        std::cmp::Ordering::Less => Some(as_end),
        std::cmp::Ordering::Equal => None,
    }
}

pub fn inputs_hash(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update([0]);
    }
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

/// The transcript without the pauses, and its last time.
fn transcript(text: &str) -> (String, u64) {
    let t: String = text.split("\n\n").filter(|p| !crate::pauses::is_pause(p.split_once(']').map(|x| x.1).unwrap_or(""))).collect::<Vec<_>>().join("\n\n");
    let last = t.lines().filter_map(|l| l.trim_start().strip_prefix('[')?.split_once(']').and_then(|(t, _)| secs(t))).max().unwrap_or(0);
    (t, last)
}

/// A photo for the model: its id, name, title and description from the user, text read from it.
pub struct Photo<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    pub text: &'a str,
}

/// A recording for the model: id, name and transcript.
pub struct Recording<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub text: &'a str,
}

#[derive(Deserialize)]
struct Answer {
    foto: Vec<Raw>,
}
#[derive(Deserialize)]
struct Raw {
    codice: String,
    #[serde(default)]
    registrazione: String,
    #[serde(default)]
    inizio: String,
    #[serde(default)]
    motivo: String,
}

/// The model's answer, checked: known codes, minutes inside the recording. A photo missing from
/// the answer, or with a minute that does not fit, is "not found".
fn checked(answer: &str, photos: &[Photo], recs: &[(String, u64)]) -> Result<Vec<ImageLink>> {
    let a: Answer = serde_json::from_str(answer).map_err(|e| anyhow!("risposta del modello non valida: {e}"))?;
    Ok(photos
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let code = format!("FOTO{}", i + 1);
            let raw = a.foto.iter().find(|r| r.codice.trim() == code);
            let found = raw.and_then(|r| {
                let n: usize = r.registrazione.trim().strip_prefix("REG")?.parse().ok()?;
                let (rec, last) = recs.get(n.checked_sub(1)?)?;
                let at = secs(&r.inizio)?;
                (at <= last + 10).then(|| (rec.clone(), at))
            });
            match found {
                Some((recording, at)) => ImageLink { image: p.id.into(), recording, at: Some(at), by: By::Modello, why: String::new(), inputs: String::new() },
                None => {
                    let why = raw.map(|r| r.motivo.trim().to_string()).filter(|m| !m.is_empty()).unwrap_or_else(|| "il modello non ha trovato il punto della lezione".into());
                    ImageLink { image: p.id.into(), recording: String::new(), at: None, by: By::Modello, why, inputs: String::new() }
                }
            }
        })
        .collect())
}

/// Asks the model where each photo belongs. Nothing is saved; `inputs` is left to the caller.
pub async fn link<A: Ai>(ai: &A, model: &str, photos: &[Photo<'_>], recordings: &[Recording<'_>], cancel: &CancellationToken) -> Result<(Vec<ImageLink>, Usage)> {
    let mut list = String::new();
    for (i, p) in photos.iter().enumerate() {
        let text: String = p.text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(600).collect();
        list.push_str(&format!("FOTO{} ({})", i + 1, p.name));
        if !p.title.is_empty() {
            list.push_str(&format!("\nTitolo dato dallo studente: {}", p.title));
        }
        if !p.description.is_empty() {
            list.push_str(&format!("\nDescrizione dello studente: {}", p.description));
        }
        list.push_str(&format!("\nTesto letto dalla foto: {text}\n\n"));
    }
    let mut texts = String::new();
    let mut recs = Vec::new();
    for (i, r) in recordings.iter().enumerate() {
        let (t, last) = transcript(r.text);
        texts.push_str(&format!("### REG{} ({})\n{t}\n\n", i + 1, r.name));
        recs.push((r.id.to_string(), last));
    }
    let req = Request {
        system: Some(prompts::SYSTEM.into()),
        parts: vec![Part::Text(prompts::link_photos(&list, &texts))],
        schema: Some(prompts::link_photos_schema()),
        max_output_tokens: Some(8_000),
        thinking: Some("LOW"),
        ..Default::default()
    };
    let a = ai.generate(model, &req, cancel).await?;
    if a.finish == "MAX_TOKENS" {
        anyhow::bail!("la risposta del modello è stata troncata");
    }
    Ok((checked(&a.text, photos, &recs)?, a.usage))
}

/// What the automatic linking found: links to save (under the write lock), warnings, cost.
#[derive(Default)]
pub struct Found {
    pub links: Vec<ImageLink>,
    pub warnings: Vec<String>,
    pub usage: Usage,
}

/// Links the photos of a lesson that have a text and no link by hand: when what they were made
/// from changed, or all of them with `again`. First the clocks, then one question to the model
/// for the rest. A failure of the model is a warning.
pub async fn link_lesson<A: Ai>(ai: &A, model: &str, lesson: &crate::library::Lesson, again: bool, cancel: &CancellationToken) -> Result<Found> {
    use crate::library::SourceKind;
    let out = crate::pipeline::outputs(lesson);
    let srcs = &lesson.meta.sources;
    let recs: Vec<&crate::library::Source> = srcs.iter().filter(|s| matches!(s.kind, SourceKind::Audio | SourceKind::Video) && out.extracts.contains_key(&s.id)).collect();
    let mut found = Found::default();
    if recs.is_empty() {
        return Ok(found);
    }
    let rec_inputs = inputs_hash(&recs.iter().map(|r| out.extracts[&r.id].as_str()).collect::<Vec<_>>());
    // The clocks. Read from the files when the import did not (lessons added before this version).
    let taken: std::collections::HashMap<&str, DateTime<Utc>> = srcs
        .iter()
        .filter(|s| s.kind == SourceKind::Image)
        .filter_map(|s| Some((s.id.as_str(), s.taken.or_else(|| taken_at(&crate::library::source_file(lesson, s).ok()?))?)))
        .collect();
    let all_taken: Vec<DateTime<Utc>> = taken.values().copied().collect();
    let starts: Vec<(&str, DateTime<Utc>, f64)> = recs
        .iter()
        .filter_map(|r| {
            let file = crate::library::source_file(lesson, r).ok();
            let length = file.as_deref().and_then(crate::audio::duration_secs).unwrap_or_else(|| transcript(&out.extracts[&r.id]).1 as f64);
            let (started, mp4) = match r.started {
                Some(at) => (at, r.started_mp4),
                None => (crate::audio::mp4_header(file.as_deref()?)?.0, true),
            };
            Some((r.id.as_str(), clock_start(&all_taken, started, mp4, length)?, length))
        })
        .collect();
    let saved = read(&lesson.path);
    let mut ask = Vec::new();
    for s in srcs.iter().filter(|s| s.kind == SourceKind::Image) {
        let Some(text) = out.extracts.get(&s.id) else { continue };
        let old = saved.iter().find(|l| l.image == s.id);
        if old.is_some_and(|l| l.by == By::Mano) {
            continue;
        }
        let hash = inputs_hash(&[text, &s.title, &s.description, &rec_inputs]);
        if !again && old.is_some_and(|l| l.inputs == hash) {
            continue;
        }
        // The clocks first: free and exact when both times are there.
        let by_time = taken.get(s.id.as_str()).and_then(|t| starts.iter().find_map(|(r, start, length)| Some((r.to_string(), by_clock(*t, *start, *length)?))));
        match by_time {
            Some((recording, at)) => found.links.push(ImageLink { image: s.id.clone(), recording, at: Some(at), by: By::Orario, why: String::new(), inputs: hash }),
            None => ask.push((s, text.as_str(), hash)),
        }
    }
    if ask.is_empty() {
        return Ok(found);
    }
    let photos: Vec<Photo> = ask.iter().map(|(s, text, _)| Photo { id: &s.id, name: &s.original_name, title: &s.title, description: &s.description, text }).collect();
    let recordings: Vec<Recording> = recs.iter().map(|r| Recording { id: &r.id, name: &r.original_name, text: &out.extracts[&r.id] }).collect();
    match link(ai, model, &photos, &recordings, cancel).await {
        Ok((links, usage)) => {
            found.usage.add(&usage);
            for (mut l, (s, _, hash)) in links.into_iter().zip(&ask) {
                if l.at.is_none() {
                    found.warnings.push(format!("{}: foto non agganciata alla registrazione ({}). Puoi agganciarla a mano dalla trascrizione o dalle Fonti", s.original_name, l.why));
                }
                l.inputs = hash.clone();
                found.links.push(l);
            }
        }
        Err(e) if e.to_string() == "annullato" => return Err(e),
        Err(e) => found.warnings.push(format!("foto non agganciate alla registrazione ({e:#})")),
    }
    Ok(found)
}

/// Lines for the summary prompt: "- F3 foto «Titolo» (descrizione) = F1 00:12:30".
pub fn prompt_lines(v: &[ImageLink], sources: &[crate::library::Source], code_of: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    for l in v {
        let (Some(p), Some(at)) = (code_of(&l.image), l.at) else { continue };
        let Some(r) = code_of(&l.recording) else { continue };
        let s = sources.iter().find(|s| s.id == l.image);
        let title = s.map(|s| if s.title.is_empty() { s.original_name.as_str() } else { s.title.as_str() }).unwrap_or("");
        let desc = s.map(|s| s.description.as_str()).filter(|d| !d.is_empty()).map(|d| format!(" ({d})")).unwrap_or_default();
        out.push_str(&format!("- {p} foto «{title}»{desc} = {r} {}\n", crate::pipeline::hms(at)));
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    #[test]
    fn minute_from_the_clocks() {
        let start = t("2026-11-10T10:00:00Z");
        assert_eq!(by_clock(t("2026-11-10T10:12:30Z"), start, 3600.0), Some(750));
        assert_eq!(by_clock(t("2026-11-10T09:59:30Z"), start, 3600.0), Some(0), "a little before the start");
        assert_eq!(by_clock(t("2026-11-10T09:50:00Z"), start, 3600.0), None, "before the lesson");
        assert_eq!(by_clock(t("2026-11-10T12:00:00Z"), start, 3600.0), None, "after the end");
        // Not from mp4: the start as it is.
        assert_eq!(clock_start(&[t("2026-11-10T09:30:00Z")], start, false, 3600.0), Some(start));
        // An mp4 header that holds the end: the photos fall before it.
        let before = [t("2026-11-10T09:10:00Z"), t("2026-11-10T09:30:00Z")];
        assert_eq!(clock_start(&before, start, true, 3600.0), Some(t("2026-11-10T09:00:00Z")));
        // Most photos after the header time: it is the start (one photo taken before the lesson
        // does not change the reading for the others).
        let after = [t("2026-11-10T10:10:00Z"), t("2026-11-10T10:30:00Z"), t("2026-11-10T09:30:00Z")];
        assert_eq!(clock_start(&after, start, true, 3600.0), Some(start));
        // As many on each side: the clocks can't tell, the model decides.
        let mixed = [t("2026-11-10T10:10:00Z"), t("2026-11-10T09:30:00Z")];
        assert_eq!(clock_start(&mixed, start, true, 3600.0), None);
    }

    /// A JPEG with only an EXIF block: DateTimeOriginal and, if given, OffsetTimeOriginal.
    pub(crate) fn jpeg_with_exif(taken: &str, offset: Option<&str>) -> Vec<u8> {
        let ascii = |s: &str| [s.as_bytes(), &[0]].concat();
        let (dt, off) = (ascii(taken), offset.map(ascii));
        let n = 1 + off.is_some() as u16;
        // TIFF: header (8), IFD0 with the Exif pointer (2 + 12 + 4), Exif IFD, then the strings.
        let exif_ifd = 8 + 18;
        let data = exif_ifd + 2 + 12 * n as u32 + 4;
        let mut t = b"II*\0".to_vec();
        t.extend(8u32.to_le_bytes());
        t.extend(1u16.to_le_bytes());
        t.extend([0x69, 0x87, 4, 0]);
        t.extend(1u32.to_le_bytes());
        t.extend(exif_ifd.to_le_bytes());
        t.extend(0u32.to_le_bytes());
        t.extend(n.to_le_bytes());
        t.extend([0x03, 0x90, 2, 0]);
        t.extend((dt.len() as u32).to_le_bytes());
        t.extend(data.to_le_bytes());
        if let Some(o) = &off {
            t.extend([0x11, 0x90, 2, 0]);
            t.extend((o.len() as u32).to_le_bytes());
            t.extend((data + dt.len() as u32).to_le_bytes());
        }
        t.extend(0u32.to_le_bytes());
        t.extend(&dt);
        t.extend(off.unwrap_or_default());
        let app1 = [b"Exif\0\0".as_slice(), &t].concat();
        [&[0xFF, 0xD8, 0xFF, 0xE1][..], &((app1.len() + 2) as u16).to_be_bytes(), &app1, &[0xFF, 0xD9]].concat()
    }

    #[test]
    fn time_taken_from_exif() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("lavagna.jpg");
        fs::write(&p, jpeg_with_exif("2026:11:10 11:12:30", Some("+01:00"))).unwrap();
        assert_eq!(taken_at(&p), Some(t("2026-11-10T10:12:30Z")));
        // No time zone in the file: this computer's.
        fs::write(&p, jpeg_with_exif("2026:11:10 11:12:30", None)).unwrap();
        let local = chrono::Local.from_local_datetime(&chrono::NaiveDate::from_ymd_opt(2026, 11, 10).unwrap().and_hms_opt(11, 12, 30).unwrap()).unwrap();
        assert_eq!(taken_at(&p), Some(local.with_timezone(&Utc)));
        fs::write(&p, b"not a photo").unwrap();
        assert_eq!(taken_at(&p), None);
    }

    #[test]
    fn model_answer_checked() {
        let photos = [
            Photo { id: "a", name: "a.jpg", title: "", description: "", text: "Bazin" },
            Photo { id: "b", name: "b.jpg", title: "", description: "", text: "" },
            Photo { id: "c", name: "c.jpg", title: "", description: "", text: "" },
        ];
        let recs = [("r1".to_string(), 600)];
        let v = checked(r#"{"foto":[{"codice":"FOTO1","registrazione":"REG1","inizio":"00:05:00","motivo":""},{"codice":"FOTO2","registrazione":"REG1","inizio":"02:00:00","motivo":""},{"codice":"FOTO3","registrazione":"","inizio":"","motivo":"non c'entra"}]}"#, &photos, &recs).unwrap();
        assert_eq!((v[0].recording.as_str(), v[0].at), ("r1", Some(300)));
        assert_eq!(v[1].at, None, "after the end of the recording");
        assert_eq!((v[2].at, v[2].why.as_str()), (None, "non c'entra"));
    }

    #[test]
    fn links_saved_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let l = |image: &str, at| ImageLink { image: image.into(), recording: "r".into(), at, by: By::Mano, why: String::new(), inputs: String::new() };
        save(dir.path(), vec![l("a", Some(10)), l("b", None)]).unwrap();
        save(dir.path(), vec![l("a", Some(20))]).unwrap();
        assert_eq!(read(dir.path()).iter().map(|x| (x.image.as_str(), x.at)).collect::<Vec<_>>(), [("b", None), ("a", Some(20))]);
        // Automatic links never replace one chosen by hand meanwhile.
        let auto = |image: &str| ImageLink { by: By::Modello, ..l(image, Some(99)) };
        save_found(dir.path(), vec![auto("a"), auto("c")]).unwrap();
        let v = read(dir.path());
        assert_eq!(v.iter().find(|x| x.image == "a").unwrap().at, Some(20));
        assert_eq!(v.iter().find(|x| x.image == "c").unwrap().at, Some(99));
        remove_source(dir.path(), "r").unwrap();
        assert!(!dir.path().join(PHOTOS_FILE).exists(), "the recording went: no links left");
    }
}
