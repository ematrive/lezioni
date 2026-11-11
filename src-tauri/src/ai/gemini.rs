//! Gemini API client (REST, `generateContent` + Files API), written against the official docs
//! (aistudio.google.com/docs, November 2026):
//! - files over ~20 MB of request go through the resumable Files API (2 GB per file, kept 48 h);
//! - audio costs 32 tokens per second, at most 9.5 hours per prompt;
//! - JSON output via `generationConfig.responseMimeType` + `responseJsonSchema`.
//!
//! The free tier is quota based and the quota is not published: 429 and 503 are retried with
//! the delay the server suggests (or exponential backoff), honouring cancellation.
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use futures::TryStreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

const BASE: &str = "https://generativelanguage.googleapis.com";
/// Above this the file is uploaded instead of sent inline (limit is 20 MB per request, prompt included).
pub const INLINE_MAX: u64 = 14 * 1024 * 1024;
const RETRIES: u32 = 6;
/// A model call that gets no answer in this time is abandoned (and retried once).
const CALL_TIMEOUT: Duration = Duration::from_secs(600);
/// An upload that sends nothing (or gets no reply) for this long has stalled.
const UPLOAD_STALL: Duration = Duration::from_secs(90);

/// Status lines for the user while a call waits (upload progress, retries).
pub type Notify = Arc<dyn Fn(String) + Send + Sync>;

#[derive(Clone)]
pub struct Gemini {
    http: reqwest::Client,
    key: String,
    base: String,
    notify: Option<Notify>,
    /// Model to switch to when the asked one answers 503 twice in a row.
    fallback: Option<String>,
}

/// A part of the prompt.
#[derive(Debug, Clone)]
pub enum Part {
    Text(String),
    Inline { mime: String, data: Vec<u8> },
    File { mime: String, uri: String },
}

impl Part {
    fn to_json(&self) -> Value {
        match self {
            Part::Text(t) => json!({ "text": t }),
            Part::Inline { mime, data } => json!({ "inlineData": { "mimeType": mime, "data": base64::engine::general_purpose::STANDARD.encode(data) } }),
            Part::File { mime, uri } => json!({ "fileData": { "mimeType": mime, "fileUri": uri } }),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Request {
    pub system: Option<String>,
    pub parts: Vec<Part>,
    /// JSON Schema of the answer; None = plain text (Markdown).
    pub schema: Option<Value>,
    pub max_output_tokens: Option<u32>,
    /// `MINIMAL`, `LOW`, `MEDIUM`, `HIGH`; None = model default.
    pub thinking: Option<&'static str>,
    /// Tries after the first on 429/5xx; None = the default (6). Low for best-effort calls.
    pub retries: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    #[serde(default)]
    pub prompt_token_count: u64,
    #[serde(default)]
    pub candidates_token_count: u64,
    #[serde(default)]
    pub thoughts_token_count: u64,
    /// What it really cost, in USD, when the provider says it (OpenRouter).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
}

impl Usage {
    pub fn add(&mut self, o: &Usage) {
        self.prompt_token_count += o.prompt_token_count;
        self.candidates_token_count += o.candidates_token_count;
        self.thoughts_token_count += o.thoughts_token_count;
        if let Some(c) = o.cost {
            self.cost = Some(self.cost.unwrap_or(0.0) + c);
        }
    }

    /// USD: the real cost, or else the paid-tier list price of gemini-3.8-flash (Nov 2026,
    /// 0.75 in / 3.75 out per 1M tokens; the same figure as `costEstimate` in `src/lib/api.ts`).
    pub fn cost_or_estimate(&self) -> f64 {
        self.cost.unwrap_or_else(|| (self.prompt_token_count as f64 * 0.75 + (self.candidates_token_count + self.thoughts_token_count) as f64 * 3.75) / 1e6)
    }
}

#[derive(Debug, Clone)]
pub struct Answer {
    pub text: String,
    pub usage: Usage,
    /// `STOP` normally; `MAX_TOKENS` when the answer was cut.
    pub finish: String,
    /// The model that answered (the fallback one when the first was overloaded); empty = as asked.
    pub model: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteFile {
    pub name: String,
    pub uri: String,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(rename = "mimeType", default)]
    pub mime: Option<String>,
}

/// MIME type Gemini accepts for a file, by extension (None = not supported).
pub fn mime_for(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "mp3" => "audio/mp3",
        "m4a" => "audio/m4a",
        "wav" => "audio/wav",
        "aac" => "audio/aac",
        "ogg" | "oga" => "audio/ogg",
        "opus" => "audio/opus",
        "flac" => "audio/flac",
        "aiff" | "aif" => "audio/aiff",
        "webm" => "audio/webm",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/mov",
        "avi" => "video/avi",
        "mkv" => "video/x-matroska",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "heic" => "image/heic",
        "heif" => "image/heif",
        "txt" => "text/plain",
        "md" | "markdown" => "text/markdown",
        _ => return None,
    })
}

/// Error from the API, with the message Google gives (in English) and a hint in Italian.
fn api_error(status: reqwest::StatusCode, body: &str) -> anyhow::Error {
    let msg = serde_json::from_str::<Value>(body).ok().and_then(|v| v["error"]["message"].as_str().map(str::to_string)).unwrap_or_else(|| body.chars().take(300).collect());
    let hint = match status.as_u16() {
        400 if msg.contains("API key") => "la chiave API non è valida",
        401 | 403 => "chiave API non valida o senza accesso a questo modello",
        404 => "modello o file non trovato (controlla il nome del modello nelle Impostazioni)",
        429 => "quota esaurita: il piano gratuito ha un limite giornaliero, riprova domani o attiva la fatturazione",
        _ => "errore di Gemini",
    };
    anyhow!("{hint} (HTTP {status}): {msg}")
}

/// Seconds the server asks to wait (`RetryInfo.retryDelay`, e.g. "31s").
pub(crate) fn retry_delay(body: &str) -> Option<Duration> {
    let v: Value = serde_json::from_str(body).ok()?;
    v["error"]["details"].as_array()?.iter().find_map(|d| d["retryDelay"].as_str()?.trim_end_matches('s').parse::<f64>().ok()).map(|s| Duration::from_secs_f64(s.clamp(1.0, 120.0)))
}

/// A 429 that is a daily quota (not a per-minute one) will not go away by waiting a bit.
fn is_daily_quota(body: &str) -> bool {
    body.contains("PerDay") || body.contains("per day")
}

pub(crate) async fn sleep_or_cancel(d: Duration, cancel: &CancellationToken) -> Result<()> {
    tokio::select! {
        _ = tokio::time::sleep(d) => Ok(()),
        _ = cancel.cancelled() => bail!("annullato"),
    }
}

impl Gemini {
    pub fn new(key: impl Into<String>) -> Self {
        Self::with_base(key, BASE)
    }

    pub fn with_base(key: impl Into<String>, base: impl Into<String>) -> Self {
        // No global timeout: an upload of a long recording on a slow connection may take a while.
        // Each model call has its own timeout (see post_json).
        let http = reqwest::Client::builder().connect_timeout(Duration::from_secs(20)).build().expect("http client");
        Self { http, key: key.into(), base: base.into(), notify: None, fallback: None }
    }

    pub fn with_fallback(mut self, model: &str) -> Self {
        let m = model.trim();
        self.fallback = (!m.is_empty()).then(|| m.to_string());
        self
    }

    pub fn with_notify(mut self, n: Notify) -> Self {
        self.notify = Some(n);
        self
    }

    fn notify_only(&self, msg: String) {
        if let Some(n) = &self.notify {
            n(msg);
        }
    }

    fn say(&self, msg: String) {
        log::info!("gemini: {msg}");
        if let Some(n) = &self.notify {
            n(msg);
        }
    }

    /// POST JSON to `models/{model}:{method}` with retries on 429 / 5xx. With `fallback`, two 503
    /// in a row switch to the fallback model. Returns the answer and the model that gave it.
    async fn post_json(&self, model: &str, method: &str, body: &Value, fallback: bool, retries: Option<u32>, cancel: &CancellationToken) -> Result<(Value, String)> {
        let max = retries.unwrap_or(RETRIES);
        let mut wait = Duration::from_secs(4);
        let mut timeouts = 0;
        let mut model = model.to_string();
        let mut overloaded = 0;
        for attempt in 0..=max {
            if cancel.is_cancelled() {
                bail!("annullato");
            }
            let url = format!("{}/v1beta/models/{model}:{method}", self.base);
            let send = self.http.post(&url).header("x-goog-api-key", &self.key).timeout(CALL_TIMEOUT).json(body).send();
            let resp = tokio::select! {
                r = send => r,
                _ = cancel.cancelled() => bail!("annullato"),
            };
            let resp = match resp {
                Ok(r) => r,
                // A timed-out call may be billed anyway: only one more try.
                Err(e) if e.is_timeout() && timeouts == 0 => {
                    timeouts += 1;
                    log::warn!("gemini: {e}");
                    self.say(format!("Gemini non ha risposto in {} minuti, riprovo un'ultima volta", CALL_TIMEOUT.as_secs() / 60));
                    continue;
                }
                Err(e) if e.is_timeout() => bail!("Gemini non risponde (nessuna risposta in {} minuti): riprova più tardi", CALL_TIMEOUT.as_secs() / 60),
                Err(e) if attempt < max && e.is_connect() => {
                    log::warn!("gemini: {e}");
                    self.say(format!("Connessione a Gemini non riuscita, riprovo tra {} s (tentativo {} di {max})", wait.as_secs(), attempt + 1));
                    sleep_or_cancel(wait, cancel).await?;
                    wait *= 2;
                    continue;
                }
                Err(e) => return Err(anyhow!(e).context("connessione a Gemini")),
            };
            let status = resp.status();
            let text = resp.text().await.context("risposta di Gemini")?;
            if status.is_success() {
                return Ok((serde_json::from_str(&text).context("risposta di Gemini non valida")?, model));
            }
            overloaded = if status.as_u16() == 503 { overloaded + 1 } else { 0 };
            if let Some(f) = self.fallback.as_ref().filter(|f| fallback && attempt < max && overloaded >= 2 && **f != model) {
                log::warn!("gemini: {model} sovraccarico, passo a {f}");
                self.say(format!("{model} è sovraccarico, uso {f} per questa richiesta"));
                model = f.clone();
                overloaded = 0;
                continue;
            }
            let retriable = status.as_u16() == 429 && !is_daily_quota(&text) || status.is_server_error();
            log::warn!("gemini HTTP {status}: {}", text.chars().take(500).collect::<String>());
            if retriable && attempt < max {
                let d = retry_delay(&text).unwrap_or(wait);
                let why = if status.as_u16() == 429 { "limite di richieste raggiunto" } else { "Gemini è sovraccarico" };
                self.say(format!("{why} (HTTP {}), riprovo tra {} s (tentativo {} di {max})", status.as_u16(), d.as_secs(), attempt + 1));
                sleep_or_cancel(d, cancel).await?;
                wait = (wait * 2).min(Duration::from_secs(90));
                continue;
            }
            return Err(api_error(status, &text));
        }
        bail!("Gemini non risponde: riprova più tardi")
    }

    pub async fn generate(&self, model: &str, req: &Request, cancel: &CancellationToken) -> Result<Answer> {
        let mut gcfg = serde_json::Map::new();
        if let Some(schema) = &req.schema {
            gcfg.insert("responseMimeType".into(), json!("application/json"));
            gcfg.insert("responseJsonSchema".into(), schema.clone());
        }
        if let Some(m) = req.max_output_tokens {
            gcfg.insert("maxOutputTokens".into(), json!(m));
        }
        if let Some(t) = req.thinking {
            gcfg.insert("thinkingConfig".into(), json!({ "thinkingLevel": t }));
        }
        let mut body = json!({
            "contents": [{ "role": "user", "parts": req.parts.iter().map(Part::to_json).collect::<Vec<_>>() }],
            "generationConfig": gcfg,
        });
        if let Some(s) = &req.system {
            body["systemInstruction"] = json!({ "parts": [{ "text": s }] });
        }
        let t = Instant::now();
        let (v, used) = self.post_json(model, "generateContent", &body, true, req.retries, cancel).await?;
        let mut a = parse_answer(&v);
        if let Ok(a) = &mut a {
            if used != model {
                a.model = used.clone();
            }
            log::info!("gemini {used}: risposta in {:.0} s, {} token in ingresso, {} in uscita, {} di ragionamento, fine {}", t.elapsed().as_secs_f64(), a.usage.prompt_token_count, a.usage.candidates_token_count, a.usage.thoughts_token_count, a.finish);
        }
        a
    }

    /// Tokens of a prompt (used to measure audio: 32 tokens per second).
    pub async fn count_tokens(&self, model: &str, parts: &[Part], cancel: &CancellationToken) -> Result<u64> {
        let body = json!({ "contents": [{ "role": "user", "parts": parts.iter().map(Part::to_json).collect::<Vec<_>>() }] });
        let (v, _) = self.post_json(model, "countTokens", &body, false, None, cancel).await?;
        v["totalTokens"].as_u64().ok_or_else(|| anyhow!("conteggio dei token non disponibile"))
    }

    /// Resumable upload, then waits until the file can be used.
    pub async fn upload(&self, path: &Path, mime: &str, cancel: &CancellationToken) -> Result<RemoteFile> {
        let size = tokio::fs::metadata(path).await?.len();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("file");
        let start = self
            .http
            .post(format!("{}/upload/v1beta/files", self.base))
            .header("x-goog-api-key", &self.key)
            .header("X-Goog-Upload-Protocol", "resumable")
            .header("X-Goog-Upload-Command", "start")
            .header("X-Goog-Upload-Header-Content-Length", size.to_string())
            .header("X-Goog-Upload-Header-Content-Type", mime)
            .json(&json!({ "file": { "display_name": name } }))
            .send()
            .await
            .context("caricamento su Gemini")?;
        let status = start.status();
        if !status.is_success() {
            let t = start.text().await.unwrap_or_default();
            return Err(api_error(status, &t));
        }
        let upload_url = start.headers().get("x-goog-upload-url").and_then(|v| v.to_str().ok()).ok_or_else(|| anyhow!("Gemini non ha dato l'indirizzo di caricamento"))?.to_string();
        let file = tokio::fs::File::open(path).await?;
        let mb = |b: u64| format!("{:.0}", b as f64 / 1e6);
        let sent = Arc::new(AtomicU64::new(0));
        let started = Instant::now();
        // Milliseconds since `started` of the last chunk handed to the connection.
        let last = Arc::new(AtomicU64::new(0));
        let (s2, l2) = (sent.clone(), last.clone());
        let stream = tokio_util::io::ReaderStream::new(file).inspect_ok(move |c| {
            s2.fetch_add(c.len() as u64, Ordering::Relaxed);
            l2.store(started.elapsed().as_millis() as u64, Ordering::Relaxed);
        });
        let body = reqwest::Body::wrap_stream(stream);
        let send = self
            .http
            .post(&upload_url)
            .header("Content-Length", size.to_string())
            .header("X-Goog-Upload-Offset", "0")
            .header("X-Goog-Upload-Command", "upload, finalize")
            .body(body)
            .send();
        tokio::pin!(send);
        self.say(format!("Caricamento su Gemini: 0 di {} MB", mb(size)));
        let mut tick = tokio::time::interval(Duration::from_secs(2));
        let resp = loop {
            let done = tokio::select! {
                r = &mut send => Some(r),
                _ = cancel.cancelled() => bail!("annullato"),
                _ = tick.tick() => None,
            };
            if let Some(r) = done {
                break r.context("caricamento su Gemini")?;
            }
            let n = sent.load(Ordering::Relaxed);
            self.notify_only(if n < size { format!("Caricamento su Gemini: {} di {} MB", mb(n), mb(size)) } else { "Caricamento completato, attendo la conferma di Gemini…".into() });
            let idle = started.elapsed().saturating_sub(Duration::from_millis(last.load(Ordering::Relaxed)));
            if idle > UPLOAD_STALL {
                bail!("il caricamento su Gemini si è fermato a {} di {} MB (nessun progresso per {} s): controlla la connessione e riprova", mb(n), mb(size), UPLOAD_STALL.as_secs());
            }
        };
        log::info!("gemini: caricati {} MB in {:.0} s", mb(size), started.elapsed().as_secs_f64());
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(api_error(status, &text));
        }
        let v: Value = serde_json::from_str(&text).context("risposta del caricamento non valida")?;
        let mut f: RemoteFile = serde_json::from_value(v["file"].clone()).context("risposta del caricamento non valida")?;
        // Audio and video are processed before they can be used.
        for i in 0..120 {
            match f.state.as_deref() {
                Some("PROCESSING") => {
                    self.notify_only(format!("Gemini sta preparando il file ({} s)…", i * 3));
                    sleep_or_cancel(Duration::from_secs(3), cancel).await?;
                    f = self.file(&f.name).await?;
                }
                Some("FAILED") => bail!("Gemini non è riuscito a elaborare {name}"),
                _ => return Ok(f),
            }
        }
        bail!("Gemini sta ancora elaborando {name}: riprova tra qualche minuto")
    }

    pub async fn file(&self, name: &str) -> Result<RemoteFile> {
        let resp = self.http.get(format!("{}/v1beta/{name}", self.base)).header("x-goog-api-key", &self.key).timeout(Duration::from_secs(60)).send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(api_error(status, &text));
        }
        Ok(serde_json::from_str(&text)?)
    }

    /// Best effort: files expire by themselves after 48 hours anyway.
    pub async fn delete_file(&self, name: &str) {
        let r = self.http.delete(format!("{}/v1beta/{name}", self.base)).header("x-goog-api-key", &self.key).send().await;
        if let Err(e) = r {
            log::warn!("eliminazione di {name} da Gemini: {e}");
        }
    }

    /// A cheap call to check the key: lists the models.
    pub async fn check(&self) -> Result<()> {
        let resp = self.http.get(format!("{}/v1beta/models?pageSize=1", self.base)).header("x-goog-api-key", &self.key).timeout(Duration::from_secs(30)).send().await.context("connessione a Gemini")?;
        let status = resp.status();
        if status.is_success() {
            return Ok(());
        }
        let t = resp.text().await.unwrap_or_default();
        Err(api_error(status, &t))
    }
}

fn parse_answer(v: &Value) -> Result<Answer> {
    if let Some(r) = v["promptFeedback"]["blockReason"].as_str() {
        bail!("Gemini ha rifiutato la richiesta ({r})");
    }
    let c = &v["candidates"][0];
    let finish = c["finishReason"].as_str().unwrap_or("").to_string();
    let text: String = c["content"]["parts"]
        .as_array()
        .map(|ps| ps.iter().filter(|p| !p["thought"].as_bool().unwrap_or(false)).filter_map(|p| p["text"].as_str()).collect())
        .unwrap_or_default();
    if text.is_empty() {
        bail!("Gemini non ha risposto (motivo: {})", if finish.is_empty() { "sconosciuto" } else { &finish });
    }
    let usage = serde_json::from_value(v["usageMetadata"].clone()).unwrap_or_default();
    Ok(Answer { text, usage, finish, model: String::new() })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-thread HTTP server answering the given (status, body) in order.
    fn fake_server(answers: Vec<(u16, &'static str)>) -> String {
        fake_server_h(answers.into_iter().map(|(c, b)| (c, String::new(), b)).collect())
    }

    fn fake_server_h(answers: Vec<(u16, String, &'static str)>) -> String {
        fake_server_log(answers).0
    }

    /// Also returns the request lines received ("POST /v1beta/… HTTP/1.1").
    fn fake_server_log(answers: Vec<(u16, String, &'static str)>) -> (String, Arc<std::sync::Mutex<Vec<String>>>) {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen2 = seen.clone();
        use std::io::{Read, Write};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            for (code, extra, body) in answers {
                let (mut c, _) = l.accept().unwrap();
                let mut buf = Vec::new();
                let mut b = [0u8; 4096];
                // Headers, then Content-Length bytes of body.
                while !String::from_utf8_lossy(&buf).contains("\r\n\r\n") {
                    let n = c.read(&mut b).unwrap();
                    buf.extend_from_slice(&b[..n]);
                }
                seen2.lock().unwrap().push(String::from_utf8_lossy(&buf).lines().next().unwrap_or_default().to_string());
                let head = String::from_utf8_lossy(&buf).to_lowercase();
                let len: usize = head.lines().find_map(|l| l.strip_prefix("content-length:")).map(|v| v.trim().parse().unwrap()).unwrap_or(0);
                let start = head.find("\r\n\r\n").unwrap() + 4;
                while buf.len() - start < len {
                    let n = c.read(&mut b).unwrap();
                    buf.extend_from_slice(&b[..n]);
                }
                let _ = write!(c, "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        (format!("http://{addr}"), seen)
    }

    #[tokio::test]
    async fn upload_reports_progress() {
        // The upload goes to the second server, named by the first answer.
        let up = format!("x-goog-upload-url: {}/up\r\n", fake_server_h(vec![(200, String::new(), r#"{"file":{"name":"files/a","uri":"u","mimeType":"audio/wav","state":"ACTIVE"}}"#)]));
        let base = fake_server_h(vec![
            (200, up, "{}"),
        ]);
        let dir = std::env::temp_dir().join(format!("lezioni-up-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.wav");
        std::fs::write(&f, vec![0u8; 300_000]).unwrap();
        let said = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let s2 = said.clone();
        let g = Gemini::with_base("k", base).with_notify(Arc::new(move |m| s2.lock().unwrap().push(m)));
        let r = g.upload(&f, "audio/wav", &CancellationToken::new()).await.unwrap();
        assert_eq!(r.name, "files/a");
        assert!(said.lock().unwrap().iter().any(|m| m.starts_with("Caricamento su Gemini")));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn overloaded_model_falls_back() {
        let busy = r#"{"error":{"code":503,"message":"high demand","details":[{"retryDelay":"1s"}]}}"#;
        let ok = r#"{"candidates":[{"content":{"parts":[{"text":"ok"}]},"finishReason":"STOP"}]}"#;
        let (base, seen) = fake_server_log(vec![(503, String::new(), busy), (503, String::new(), busy), (200, String::new(), ok)]);
        let g = Gemini::with_base("k", base).with_fallback("lite");
        let req = Request { parts: vec![Part::Text("ciao".into())], ..Default::default() };
        let a = g.generate("flash", &req, &CancellationToken::new()).await.unwrap();
        assert_eq!((a.text.as_str(), a.model.as_str()), ("ok", "lite"));
        let seen = seen.lock().unwrap();
        assert!(seen[1].contains("/models/flash:generateContent") && seen[2].contains("/models/lite:generateContent"), "{seen:?}");
    }

    #[tokio::test]
    async fn retries_are_reported() {
        let base = fake_server(vec![
            (503, r#"{"error":{"code":503,"message":"overloaded","details":[{"retryDelay":"1s"}]}}"#),
            (200, r#"{"candidates":[{"content":{"parts":[{"text":"ok"}]},"finishReason":"STOP"}]}"#),
        ]);
        let said = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let s2 = said.clone();
        let g = Gemini::with_base("k", base).with_notify(Arc::new(move |m| s2.lock().unwrap().push(m)));
        let req = Request { parts: vec![Part::Text("ciao".into())], ..Default::default() };
        let a = g.generate("m", &req, &CancellationToken::new()).await.unwrap();
        assert_eq!(a.text, "ok");
        let said = said.lock().unwrap();
        assert!(said.iter().any(|m| m.contains("HTTP 503") && m.contains("tentativo 1 di 6")), "{said:?}");
    }

    #[test]
    fn answer_parsing() {
        let v = json!({
            "candidates": [{ "content": { "parts": [{ "text": "pensiero", "thought": true }, { "text": "Ciao " }, { "text": "mondo" }] }, "finishReason": "STOP" }],
            "usageMetadata": { "promptTokenCount": 10, "candidatesTokenCount": 3, "totalTokenCount": 13 }
        });
        let a = parse_answer(&v).unwrap();
        assert_eq!(a.text, "Ciao mondo");
        assert_eq!(a.usage.prompt_token_count, 10);
        assert!(parse_answer(&json!({ "promptFeedback": { "blockReason": "SAFETY" } })).is_err());
        assert!(parse_answer(&json!({ "candidates": [{ "finishReason": "MAX_TOKENS" }] })).unwrap_err().to_string().contains("MAX_TOKENS"));
    }

    #[test]
    fn retry_info() {
        let body = r#"{"error":{"code":429,"message":"Resource exhausted","details":[{"@type":"type.googleapis.com/google.rpc.RetryInfo","retryDelay":"31s"}]}}"#;
        assert_eq!(retry_delay(body), Some(Duration::from_secs(31)));
        assert!(!is_daily_quota(body));
        assert!(is_daily_quota(r#"{"error":{"message":"GenerateRequestsPerDayPerProjectPerModel-FreeTier"}}"#));
        assert!(api_error(reqwest::StatusCode::TOO_MANY_REQUESTS, body).to_string().starts_with("quota esaurita"));
    }

    #[test]
    fn mimes() {
        assert_eq!(mime_for(Path::new("a.M4A")), Some("audio/m4a"));
        assert_eq!(mime_for(Path::new("a.docx")), None);
    }
}
