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

/// Downloads `MODEL` into `dir` (through a `.part` file, verified before the rename).
async fn download(dir: &Path, model: &ModelFile, notify: &(dyn Fn(String) + Send + Sync), cancel: &CancellationToken) -> Result<PathBuf> {
    let dest = dir.join(model.file);
    if dest.is_file() {
        return Ok(dest);
    }
    fs::create_dir_all(dir).with_context(|| format!("creazione di {}", dir.display()))?;
    let part = dir.join(format!("{}.part", model.file));
    let http = reqwest::Client::builder().connect_timeout(Duration::from_secs(20)).read_timeout(Duration::from_secs(60)).build()?;
    let resp = tokio::select! {
        r = http.get(model.url).send() => r.context("download del modello di trascrizione")?,
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
            notify(format!("Download del modello di trascrizione (una volta sola): {} di {} MB", mb(got), mb(model.size)));
        }
    }
    out.flush()?;
    drop(out);
    let sum = format!("{:x}", hash.finalize());
    if got != model.size || sum != model.sha256 {
        let _ = fs::remove_file(&part);
        bail!("il modello di trascrizione scaricato è incompleto o danneggiato ({} di {} MB): riprova", mb(got), mb(model.size));
    }
    fs::rename(&part, &dest)?;
    log::info!("modello di trascrizione scaricato in {}", dest.display());
    Ok(dest)
}

/// What Settings shows about the model, whoever started the download.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub ready: bool,
    /// Progress line while a download runs.
    pub downloading: Option<String>,
    /// Why the last download failed.
    pub error: Option<String>,
}

/// The model on disk and, once loaded, in memory (kept for the next lessons). One download at a
/// time, shared by Settings and by processing, and it goes on when Settings is closed.
pub struct Loader {
    dir: PathBuf,
    model: &'static ModelFile,
    loaded: tokio::sync::Mutex<Option<Arc<Whisper>>>,
    downloading: tokio::sync::Mutex<()>,
    progress: std::sync::Mutex<Option<String>>,
    error: std::sync::Mutex<Option<String>>,
    cancel: std::sync::Mutex<Option<CancellationToken>>,
}

impl Loader {
    pub fn new(dir: PathBuf) -> Self {
        Self::with_model(dir, &MODEL)
    }

    fn with_model(dir: PathBuf, model: &'static ModelFile) -> Self {
        Self { dir, model, loaded: Default::default(), downloading: Default::default(), progress: Default::default(), error: Default::default(), cancel: Default::default() }
    }

    fn path(&self) -> PathBuf {
        self.dir.join(self.model.file)
    }

    pub fn downloaded(&self) -> bool {
        self.path().is_file()
    }

    pub fn status(&self) -> ModelStatus {
        ModelStatus { ready: self.downloaded(), downloading: self.progress.lock().unwrap().clone(), error: self.error.lock().unwrap().clone() }
    }

    /// From Settings: starts the download in the background (nothing if one is running).
    pub fn start_download(self: &Arc<Self>) {
        if self.downloaded() || self.progress.lock().unwrap().is_some() {
            return;
        }
        // Shown at once, before the task gets the lock.
        *self.progress.lock().unwrap() = Some("Avvio del download…".into());
        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            let _ = me.download(&|_| {}, &CancellationToken::new()).await;
        });
    }

    pub fn cancel_download(&self) {
        if let Some(t) = &*self.cancel.lock().unwrap() {
            t.cancel();
        }
    }

    /// Downloads the model if missing. If another download is running, waits for it and passes
    /// its progress on.
    pub async fn download(&self, notify: &(dyn Fn(String) + Send + Sync), cancel: &CancellationToken) -> Result<PathBuf> {
        let dest = self.path();
        let _one = loop {
            if let Ok(g) = self.downloading.try_lock() {
                break g;
            }
            if let Some(p) = self.progress.lock().unwrap().clone() {
                notify(p);
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                _ = cancel.cancelled() => bail!("annullato"),
            }
        };
        if dest.is_file() {
            *self.progress.lock().unwrap() = None;
            return Ok(dest);
        }
        let token = cancel.child_token();
        *self.cancel.lock().unwrap() = Some(token.clone());
        *self.error.lock().unwrap() = None;
        *self.progress.lock().unwrap() = Some("Avvio del download…".into());
        let both = |m: String| {
            *self.progress.lock().unwrap() = Some(m.clone());
            notify(m);
        };
        let r = download(&self.dir, self.model, &both, &token).await;
        *self.progress.lock().unwrap() = None;
        *self.cancel.lock().unwrap() = None;
        if let Err(e) = &r {
            log::warn!("download del modello: {e:#}");
            *self.error.lock().unwrap() = Some(if e.to_string() == "annullato" { "Download annullato.".into() } else { format!("{e:#}") });
        }
        r
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
        let p = self.path();
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
    model: PathBuf,
    /// On the GPU where there is one (Metal on macOS).
    ctx: whisper_rs::WhisperContext,
    /// Loaded only if the GPU fails: the same model on the processor.
    cpu: std::sync::OnceLock<whisper_rs::WhisperContext>,
}

fn load_ctx(model: &Path, gpu: bool) -> Result<whisper_rs::WhisperContext> {
    let t = Instant::now();
    let mut params = whisper_rs::WhisperContextParameters::default();
    params.use_gpu(gpu && params.use_gpu);
    let ctx = whisper_rs::WhisperContext::new_with_params(model, params).map_err(|e| anyhow!("caricamento del modello di trascrizione: {e}"))?;
    log::info!("modello di trascrizione caricato in {:.1} s ({})", t.elapsed().as_secs_f64(), if gpu && cfg!(target_os = "macos") { "GPU" } else { "processore" });
    Ok(ctx)
}

impl Whisper {
    pub fn load(model: &Path) -> Result<Self> {
        static HOOKS: std::sync::Once = std::sync::Once::new();
        // whisper.cpp and ggml messages go to `log` (warnings and errors end up in lezioni.log).
        HOOKS.call_once(whisper_rs::install_logging_hooks);
        Ok(Self { model: model.to_path_buf(), ctx: load_ctx(model, true)?, cpu: Default::default() })
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

/// ggml's abort callback: `data` is an `AtomicBool` that outlives the call to `full`.
/// (Not `set_abort_callback_safe`: in whisper-rs 0.16 it registers a `Box<dyn FnMut>` but
/// reads it back as the closure type, so it returns garbage and aborts the encoder: error -6.)
unsafe extern "C" fn abort_flag(data: *mut std::ffi::c_void) -> bool {
    unsafe { (*(data as *const AtomicBool)).load(Ordering::Relaxed) }
}

fn run(ctx: &whisper_rs::WhisperContext, samples: &[f32], prompt: &str, cancel: &CancellationToken, progress: &(dyn Fn(i32) + Send + Sync)) -> Result<Vec<Segment>> {
    let mut state = ctx.create_state().map_err(|e| anyhow!("trascrizione: {e}"))?;
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
    // The progress callback must be 'static: share the value through an atomic.
    let pct = Arc::new(AtomicI32::new(-1));
    let pc = pct.clone();
    p.set_progress_callback_safe(move |v: i32| pc.store(v, Ordering::Relaxed));
    let stop = Arc::new(AtomicBool::new(false));
    unsafe {
        p.set_abort_callback(Some(abort_flag));
        p.set_abort_callback_user_data(Arc::as_ptr(&stop) as *mut std::ffi::c_void);
    }
    // A watcher thread forwards progress and cancellation while `full` runs.
    let done = AtomicBool::new(false);
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
        let r = state.full(p, samples);
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

impl Asr for Whisper {
    fn transcribe(&self, wav: &Path, prompt: &str, cancel: &CancellationToken, progress: &(dyn Fn(i32) + Send + Sync)) -> Result<Vec<Segment>> {
        let samples = read_wav(wav)?;
        if let Some(cpu) = self.cpu.get() {
            return run(cpu, &samples, prompt, cancel, progress);
        }
        match run(&self.ctx, &samples, prompt, cancel, progress) {
            Err(e) if cfg!(target_os = "macos") && e.to_string() != "annullato" => {
                // The GPU failed (Metal): the processor is slower but works.
                log::warn!("whisper sulla GPU: {e:#}; riprovo sul processore");
                let cpu = match self.cpu.get() {
                    Some(c) => c,
                    None => {
                        let c = load_ctx(&self.model, false)?;
                        self.cpu.get_or_init(|| c)
                    }
                };
                run(cpu, &samples, prompt, cancel, progress)
            }
            r => r,
        }
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

    /// Serves `body` once, slowly (so that a download is seen running), then stops listening.
    fn slow_server(body: Vec<u8>) -> String {
        use std::io::{Read, Write};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut c, _) = l.accept().unwrap();
            let mut b = [0u8; 4096];
            let _ = c.read(&mut b).unwrap();
            write!(c, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
            for piece in body.chunks(body.len() / 10) {
                c.write_all(piece).unwrap();
                std::thread::sleep(Duration::from_millis(150));
            }
        });
        format!("http://{addr}/model.bin")
    }

    #[tokio::test]
    async fn one_shared_download() {
        let body = vec![7u8; 100_000];
        let sha = format!("{:x}", Sha256::digest(&body));
        let url = slow_server(body);
        let model: &'static ModelFile = Box::leak(Box::new(ModelFile { file: "m.bin", url: Box::leak(url.into_boxed_str()), size: 100_000, sha256: Box::leak(sha.into_boxed_str()) }));
        let dir = tempfile::tempdir().unwrap();
        let l = Arc::new(Loader::with_model(dir.path().to_path_buf(), model));
        let (a, b) = (l.clone(), l.clone());
        let first = tokio::spawn(async move { a.download(&|_| {}, &CancellationToken::new()).await });
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert!(l.status().downloading.is_some(), "the status is visible while downloading");
        // A second caller (e.g. processing, or Settings opened again) waits for the same download:
        // the server answers only once.
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let s2 = seen.clone();
        let second = b.download(&move |m| s2.lock().unwrap().push(m), &CancellationToken::new()).await.unwrap();
        first.await.unwrap().unwrap();
        assert!(second.is_file());
        assert!(!seen.lock().unwrap().is_empty(), "the waiting caller got the progress");
        let st = l.status();
        assert!(st.ready && st.downloading.is_none() && st.error.is_none(), "{st:?}");
    }

    #[tokio::test]
    async fn cancelled_download_leaves_nothing() {
        let url = slow_server(vec![1u8; 100_000]);
        let model: &'static ModelFile = Box::leak(Box::new(ModelFile { file: "m.bin", url: Box::leak(url.into_boxed_str()), size: 100_000, sha256: "x" }));
        let dir = tempfile::tempdir().unwrap();
        let l = Arc::new(Loader::with_model(dir.path().to_path_buf(), model));
        let a = l.clone();
        let job = tokio::spawn(async move { a.download(&|_| {}, &CancellationToken::new()).await });
        tokio::time::sleep(Duration::from_millis(400)).await;
        l.cancel_download();
        assert!(job.await.unwrap().is_err());
        let st = l.status();
        assert!(!st.ready && st.downloading.is_none() && st.error.as_deref() == Some("Download annullato."), "{st:?}");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0, "no .part left");
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
