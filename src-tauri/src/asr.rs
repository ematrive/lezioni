//! Speech recognition on the computer with whisper.cpp (`whisper-rs`; Metal on macOS).
//!
//! The model (large-v3-turbo, 5-bit, ~570 MB) is downloaded once into the app data folder and
//! checked against its SHA-256. Transcription works on the 16 kHz mono WAV parts made by
//! `audio::split`, so a long recording is done (and saved) part by part.
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use futures::StreamExt;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

pub struct ModelFile {
    pub file: &'static str,
    pub url: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

/// Values from huggingface.co/api/models/ggerganov/whisper.cpp (November 2026).
pub const MODEL: ModelFile = ModelFile {
    file: "ggml-large-v3-turbo-q5_0.bin",
    url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
    size: 574_041_195,
    sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
};

pub fn model_path(dir: &Path) -> PathBuf {
    dir.join(MODEL.file)
}

/// Downloads the model into `dir` (through a `.part` file, verified before the rename).
pub async fn download(dir: &Path, notify: &(dyn Fn(String) + Send + Sync), cancel: &CancellationToken) -> Result<PathBuf> {
    let dest = model_path(dir);
    if dest.is_file() {
        return Ok(dest);
    }
    fs::create_dir_all(dir).with_context(|| format!("creazione di {}", dir.display()))?;
    let part = dir.join(format!("{}.part", MODEL.file));
    let http = reqwest::Client::builder().connect_timeout(Duration::from_secs(20)).read_timeout(Duration::from_secs(60)).build()?;
    let resp = tokio::select! {
        r = http.get(MODEL.url).send() => r.context("download del modello di trascrizione")?,
        _ = cancel.cancelled() => bail!("annullato"),
    };
    if !resp.status().is_success() {
        bail!("download del modello di trascrizione non riuscito (HTTP {})", resp.status().as_u16());
    }
    let mut out = fs::File::create(&part).with_context(|| format!("creazione di {}", part.display()))?;
    let mut hash = Sha256::new();
    let mut got = 0u64;
    let mut stream = resp.bytes_stream();
    let mut last = Instant::now() - Duration::from_secs(5);
    let mb = |b: u64| b / 1_000_000;
    loop {
        let chunk = tokio::select! {
            c = stream.next() => c,
            _ = cancel.cancelled() => {
                drop(out);
                let _ = fs::remove_file(&part);
                bail!("annullato");
            }
        };
        let Some(chunk) = chunk else { break };
        let chunk = chunk.context("download del modello di trascrizione interrotto")?;
        out.write_all(&chunk)?;
        hash.update(&chunk);
        got += chunk.len() as u64;
        if last.elapsed() > Duration::from_secs(1) {
            last = Instant::now();
            notify(format!("Download del modello di trascrizione (una volta sola): {} di {} MB", mb(got), mb(MODEL.size)));
        }
    }
    out.flush()?;
    drop(out);
    let sum = format!("{:x}", hash.finalize());
    if got != MODEL.size || sum != MODEL.sha256 {
        let _ = fs::remove_file(&part);
        bail!("il modello di trascrizione scaricato è incompleto o danneggiato ({} di {} MB): riprova", mb(got), mb(MODEL.size));
    }
    fs::rename(&part, &dest)?;
    log::info!("modello di trascrizione scaricato in {}", dest.display());
    Ok(dest)
}

/// The model on disk and, once loaded, in memory (kept for the next lessons).
pub struct Loader {
    dir: PathBuf,
    loaded: tokio::sync::Mutex<Option<Arc<Whisper>>>,
    downloading: tokio::sync::Mutex<()>,
}

impl Loader {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, loaded: Default::default(), downloading: Default::default() }
    }

    pub fn downloaded(&self) -> bool {
        model_path(&self.dir).is_file()
    }

    pub async fn download(&self, notify: &(dyn Fn(String) + Send + Sync), cancel: &CancellationToken) -> Result<PathBuf> {
        let _one = self.downloading.lock().await;
        download(&self.dir, notify, cancel).await
    }

    pub async fn get(&self, notify: &(dyn Fn(String) + Send + Sync), cancel: &CancellationToken) -> Result<Arc<dyn Asr>> {
        let mut g = self.loaded.lock().await;
        if let Some(w) = &*g {
            return Ok(w.clone());
        }
        let path = self.download(notify, cancel).await?;
        notify("Caricamento del modello di trascrizione…".into());
        let w = Arc::new(tokio::task::spawn_blocking(move || Whisper::load(&path)).await??);
        *g = Some(w.clone());
        Ok(w)
    }

    pub fn delete(&self) -> Result<()> {
        let mut g = self.loaded.try_lock().map_err(|_| anyhow!("il modello di trascrizione è in uso: riprova al termine dell'elaborazione"))?;
        *g = None;
        let p = model_path(&self.dir);
        if p.exists() {
            fs::remove_file(&p).with_context(|| format!("eliminazione di {}", p.display()))?;
        }
        Ok(())
    }
}

/// A piece of recognised speech, times in seconds from the start of the part.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// Speech to text on one 16 kHz mono WAV part. Blocking: call it from `spawn_blocking`.
pub trait Asr: Send + Sync {
    /// `progress` gets 0–100; `cancel` stops it between steps.
    fn transcribe(&self, wav: &Path, prompt: &str, cancel: &CancellationToken, progress: &(dyn Fn(i32) + Send + Sync)) -> Result<Vec<Segment>>;
}

pub struct Whisper {
    ctx: whisper_rs::WhisperContext,
}

impl Whisper {
    pub fn load(model: &Path) -> Result<Self> {
        static HOOKS: std::sync::Once = std::sync::Once::new();
        // whisper.cpp prints a lot on stderr: route it to `log` (off below warnings).
        HOOKS.call_once(whisper_rs::install_logging_hooks);
        let t = Instant::now();
        let ctx = whisper_rs::WhisperContext::new_with_params(model, whisper_rs::WhisperContextParameters::default()).map_err(|e| anyhow!("caricamento del modello di trascrizione: {e}"))?;
        log::info!("modello di trascrizione caricato in {:.1} s", t.elapsed().as_secs_f64());
        Ok(Self { ctx })
    }
}

fn read_wav(path: &Path) -> Result<Vec<f32>> {
    let mut r = hound::WavReader::open(path).with_context(|| format!("lettura di {}", path.display()))?;
    let spec = r.spec();
    if spec.channels != 1 || spec.sample_rate != crate::audio::RATE || spec.bits_per_sample != 16 {
        bail!("formato audio inatteso in {}", path.display());
    }
    r.samples::<i16>().map(|s| Ok(s? as f32 / 32768.0)).collect()
}

impl Asr for Whisper {
    fn transcribe(&self, wav: &Path, prompt: &str, cancel: &CancellationToken, progress: &(dyn Fn(i32) + Send + Sync)) -> Result<Vec<Segment>> {
        let samples = read_wav(wav)?;
        let mut state = self.ctx.create_state().map_err(|e| anyhow!("trascrizione: {e}"))?;
        let mut p = whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::BeamSearch { beam_size: 5, patience: -1.0 });
        p.set_language(Some("it"));
        p.set_n_threads(std::thread::available_parallelism().map(|n| n.get().min(8) as i32).unwrap_or(4));
        p.set_initial_prompt(prompt);
        p.set_print_progress(false);
        p.set_print_realtime(false);
        p.set_print_special(false);
        p.set_print_timestamps(false);
        p.set_suppress_blank(true);
        p.set_suppress_nst(true);
        // The callbacks must be 'static: share the progress through atomics.
        let pct = Arc::new(AtomicI32::new(-1));
        let pc = pct.clone();
        p.set_progress_callback_safe(move |v: i32| pc.store(v, Ordering::Relaxed));
        let stop = Arc::new(AtomicBool::new(false));
        let sc = stop.clone();
        p.set_abort_callback_safe(move || sc.load(Ordering::Relaxed));
        // A watcher thread forwards progress and cancellation while `full` runs.
        let done = Arc::new(AtomicBool::new(false));
        let t = Instant::now();
        let res = std::thread::scope(|s| {
            s.spawn(|| {
                let mut shown = -1;
                while !done.load(Ordering::Relaxed) {
                    if cancel.is_cancelled() {
                        stop.store(true, Ordering::Relaxed);
                    }
                    let v = pct.load(Ordering::Relaxed);
                    if v != shown && v >= 0 {
                        shown = v;
                        progress(v);
                    }
                    std::thread::sleep(Duration::from_millis(250));
                }
            });
            let r = state.full(p, &samples);
            done.store(true, Ordering::Relaxed);
            r
        });
        if cancel.is_cancelled() {
            bail!("annullato");
        }
        res.map_err(|e| anyhow!("trascrizione: {e}"))?;
        let secs = samples.len() as f64 / crate::audio::RATE as f64;
        log::info!("whisper: {:.0} s di audio in {:.0} s", secs, t.elapsed().as_secs_f64());
        let mut out = Vec::new();
        for seg in state.as_iter() {
            let text = seg.to_str_lossy().map_err(|e| anyhow!("trascrizione: {e}"))?.trim().to_string();
            if !text.is_empty() {
                out.push(Segment { start: seg.start_timestamp() as f64 / 100.0, end: seg.end_timestamp() as f64 / 100.0, text });
            }
        }
        Ok(out)
    }
}

/// Words Whisper writes on silence or noise (it was trained on subtitles).
fn is_hallucination(t: &str) -> bool {
    let l = t.to_lowercase();
    let l = l.trim_matches(|c: char| !c.is_alphanumeric());
    l.is_empty() || ["sottotitoli", "amara.org", "grazie per la visione", "iscriviti al canale"].iter().any(|h| l.contains(h))
}

/// Segments → Markdown paragraphs that start with `[hh:mm:ss]` (absolute: `offset` is the start of
/// the part in the recording). A paragraph ends at a pause longer than 1.5 s, or at the end of a
/// sentence once it is 45 s long, or in any case after 90 s.
pub fn paragraphs(segs: &[Segment], offset: u64) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut start = 0.0;
    let mut prev_end = 0.0;
    for s in segs.iter().filter(|s| !is_hallucination(&s.text)) {
        let long = s.start - start;
        let sentence_end = cur.trim_end().ends_with(['.', '?', '!']);
        if !cur.is_empty() && (s.start - prev_end > 1.5 || (long >= 45.0 && sentence_end) || long >= 90.0) {
            out.push(std::mem::take(&mut cur));
        }
        if cur.is_empty() {
            start = s.start;
            cur = format!("[{}] ", crate::pipeline::hms(offset + start as u64));
        } else {
            cur.push(' ');
        }
        cur.push_str(s.text.trim());
        prev_end = s.end;
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f64, end: f64, text: &str) -> Segment {
        Segment { start, end, text: text.into() }
    }

    #[test]
    fn paragraphs_from_segments() {
        let s = [
            seg(0.0, 4.0, "Bene, oggi vediamo il simplesso."),
            seg(4.2, 30.0, "Partiamo dalla forma standard"),
            seg(30.1, 50.0, "e dalle variabili di slack."),
            seg(50.2, 55.0, "Domande?"),
            seg(58.0, 60.0, "Sottotitoli creati dalla comunità Amara.org"),
            seg(60.0, 64.0, "Allora proseguiamo."),
        ];
        let p = paragraphs(&s, 1200);
        assert_eq!(
            p,
            "[00:20:00] Bene, oggi vediamo il simplesso. Partiamo dalla forma standard e dalle variabili di slack.\n\n[00:20:50] Domande?\n\n[00:21:00] Allora proseguiamo."
        );
    }
}
