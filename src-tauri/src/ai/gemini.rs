//! Gemini API client (REST, `generateContent` + Files API), written against the official docs
//! (aistudio.google.com/docs, November 2026):
//! - files over ~20 MB of request go through the resumable Files API (2 GB per file, kept 48 h);
//! - audio costs 32 tokens per second, at most 9.5 hours per prompt;
//! - JSON output via `generationConfig.responseMimeType` + `responseJsonSchema`.
//!
//! The free tier is quota based and the quota is not published: 429 and 503 are retried with
//! the delay the server suggests (or exponential backoff), honouring cancellation.
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

const BASE: &str = "https://generativelanguage.googleapis.com";
/// Above this the file is uploaded instead of sent inline (limit is 20 MB per request, prompt included).
pub const INLINE_MAX: u64 = 14 * 1024 * 1024;
const RETRIES: u32 = 6;

#[derive(Clone)]
pub struct Gemini {
    http: reqwest::Client,
    key: String,
    base: String,
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
}

impl Usage {
    pub fn add(&mut self, o: &Usage) {
        self.prompt_token_count += o.prompt_token_count;
        self.candidates_token_count += o.candidates_token_count;
        self.thoughts_token_count += o.thoughts_token_count;
    }
}

#[derive(Debug, Clone)]
pub struct Answer {
    pub text: String,
    pub usage: Usage,
    /// `STOP` normally; `MAX_TOKENS` when the answer was cut.
    pub finish: String,
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
fn retry_delay(body: &str) -> Option<Duration> {
    let v: Value = serde_json::from_str(body).ok()?;
    v["error"]["details"].as_array()?.iter().find_map(|d| d["retryDelay"].as_str()?.trim_end_matches('s').parse::<f64>().ok()).map(|s| Duration::from_secs_f64(s.clamp(1.0, 120.0)))
}

/// A 429 that is a daily quota (not a per-minute one) will not go away by waiting a bit.
fn is_daily_quota(body: &str) -> bool {
    body.contains("PerDay") || body.contains("per day")
}

async fn sleep_or_cancel(d: Duration, cancel: &CancellationToken) -> Result<()> {
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
        Self { http, key: key.into(), base: base.into() }
    }

    /// POST JSON with retries on 429 / 5xx.
    async fn post_json(&self, url: &str, body: &Value, cancel: &CancellationToken) -> Result<Value> {
        let mut wait = Duration::from_secs(4);
        for attempt in 0..=RETRIES {
            if cancel.is_cancelled() {
                bail!("annullato");
            }
            let send = self.http.post(url).header("x-goog-api-key", &self.key).timeout(Duration::from_secs(900)).json(body).send();
            let resp = tokio::select! {
                r = send => r,
                _ = cancel.cancelled() => bail!("annullato"),
            };
            let resp = match resp {
                Ok(r) => r,
                Err(e) if attempt < RETRIES && (e.is_timeout() || e.is_connect()) => {
                    log::warn!("gemini: {e}, nuovo tentativo");
                    sleep_or_cancel(wait, cancel).await?;
                    wait *= 2;
                    continue;
                }
                Err(e) => return Err(anyhow!(e).context("connessione a Gemini")),
            };
            let status = resp.status();
            let text = resp.text().await.context("risposta di Gemini")?;
            if status.is_success() {
                return serde_json::from_str(&text).context("risposta di Gemini non valida");
            }
            let retriable = status.as_u16() == 429 && !is_daily_quota(&text) || status.is_server_error();
            if retriable && attempt < RETRIES {
                let d = retry_delay(&text).unwrap_or(wait);
                log::warn!("gemini HTTP {status}, attendo {d:?}");
                sleep_or_cancel(d, cancel).await?;
                wait = (wait * 2).min(Duration::from_secs(90));
                continue;
            }
            return Err(api_error(status, &text));
        }
        unreachable!()
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
        let url = format!("{}/v1beta/models/{model}:generateContent", self.base);
        let v = self.post_json(&url, &body, cancel).await?;
        parse_answer(&v)
    }

    /// Tokens of a prompt (used to measure audio: 32 tokens per second).
    pub async fn count_tokens(&self, model: &str, parts: &[Part], cancel: &CancellationToken) -> Result<u64> {
        let body = json!({ "contents": [{ "role": "user", "parts": parts.iter().map(Part::to_json).collect::<Vec<_>>() }] });
        let url = format!("{}/v1beta/models/{model}:countTokens", self.base);
        let v = self.post_json(&url, &body, cancel).await?;
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
        let body = reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::new(file));
        let send = self
            .http
            .post(&upload_url)
            .header("Content-Length", size.to_string())
            .header("X-Goog-Upload-Offset", "0")
            .header("X-Goog-Upload-Command", "upload, finalize")
            .body(body)
            .send();
        let resp = tokio::select! {
            r = send => r.context("caricamento su Gemini")?,
            _ = cancel.cancelled() => bail!("annullato"),
        };
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(api_error(status, &text));
        }
        let v: Value = serde_json::from_str(&text).context("risposta del caricamento non valida")?;
        let mut f: RemoteFile = serde_json::from_value(v["file"].clone()).context("risposta del caricamento non valida")?;
        // Audio and video are processed before they can be used.
        for _ in 0..120 {
            match f.state.as_deref() {
                Some("PROCESSING") => {
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
    Ok(Answer { text, usage, finish })
}

#[cfg(test)]
mod tests {
    use super::*;

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
