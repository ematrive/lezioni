//! OpenRouter client (openrouter.ai/docs, November 2026): one key, many models, OpenAI-style
//! `chat/completions`. Used for the text work (cleanup of Whisper's text, photos and PDFs,
//! summary, questions): it does not transcribe audio. Every answer says what it cost.
//!
//! - images and PDFs go inline as data URLs (`image_url`, `file`);
//! - `models: [model, fallback]` lets OpenRouter switch by itself when the first is down;
//! - `provider.data_collection = "deny"`: only providers that do not train on the data.
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use serde::Serialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::gemini::{Answer, Notify, Part, Request, Usage, retry_delay, sleep_or_cancel};

const BASE: &str = "https://openrouter.ai";
const RETRIES: u32 = 6;
const CALL_TIMEOUT: Duration = Duration::from_secs(600);
/// Images and PDFs travel inside the request: keep it reasonable.
/// Checked with a 33 MB PDF (44 MB in base64) of 59 pages, Nov 2026.
pub const INLINE_MAX: u64 = 50 * 1024 * 1024;

#[derive(Clone)]
pub struct OpenRouter {
    http: reqwest::Client,
    key: String,
    base: String,
    notify: Option<Notify>,
    fallback: Option<String>,
}

/// A model of the price list, for Settings.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    /// USD per million tokens.
    pub input: f64,
    pub output: f64,
    pub images: bool,
    pub files: bool,
}

fn b64(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn content_part(p: &Part) -> Result<Value> {
    Ok(match p {
        Part::Text(t) => json!({ "type": "text", "text": t }),
        Part::Inline { mime, data } if mime.starts_with("image/") => json!({ "type": "image_url", "image_url": { "url": format!("data:{mime};base64,{}", b64(data)) } }),
        Part::Inline { mime, data } if mime == "application/pdf" => {
            json!({ "type": "file", "file": { "filename": "documento.pdf", "file_data": format!("data:{mime};base64,{}", b64(data)) } })
        }
        Part::Inline { mime, .. } | Part::File { mime, .. } => bail!(crate::tr!("OpenRouter non accetta file {mime} da questa app: per l'audio usa la trascrizione sul computer", "OpenRouter does not accept {mime} files from this app: for audio use the transcription on the computer")),
    })
}

/// The request body (separate for the tests).
fn body(model: &str, fallback: Option<&str>, req: &Request) -> Result<Value> {
    let mut messages = Vec::new();
    if let Some(s) = &req.system {
        messages.push(json!({ "role": "system", "content": s }));
    }
    let parts = req.parts.iter().map(content_part).collect::<Result<Vec<_>>>()?;
    messages.push(json!({ "role": "user", "content": parts }));
    let mut b = json!({
        "model": model,
        "messages": messages,
        "provider": { "data_collection": "deny" },
        "usage": { "include": true },
    });
    if let Some(f) = fallback.filter(|f| *f != model) {
        b["models"] = json!([model, f]);
    }
    if let Some(m) = req.max_output_tokens {
        b["max_tokens"] = json!(m);
    }
    if let Some(t) = req.thinking {
        b["reasoning"] = json!({ "effort": t.to_lowercase() });
    }
    if let Some(schema) = &req.schema {
        b["response_format"] = json!({ "type": "json_schema", "json_schema": { "name": "risposta", "strict": false, "schema": schema } });
    }
    Ok(b)
}

/// Some models wrap JSON in a Markdown fence even when asked for JSON.
fn strip_fence(t: &str) -> &str {
    let t = t.trim();
    match t.strip_prefix("```") {
        Some(rest) => {
            let rest = rest.trim_start_matches(|c: char| c.is_ascii_alphabetic());
            rest.strip_suffix("```").unwrap_or(rest).trim()
        }
        None => t,
    }
}

fn parse_answer(v: &Value, asked: &str, json_wanted: bool) -> Result<Answer> {
    if let Some(e) = v.get("error") {
        bail!(crate::tr!("errore di OpenRouter: {}", "OpenRouter error: {}", e["message"].as_str().unwrap_or(crate::lang::ui().pick("sconosciuto", "unknown"))));
    }
    let c = &v["choices"][0];
    if let Some(e) = c.get("error") {
        bail!(crate::tr!("errore del modello: {}", "model error: {}", e["message"].as_str().unwrap_or(crate::lang::ui().pick("sconosciuto", "unknown"))));
    }
    let text = c["message"]["content"].as_str().unwrap_or_default();
    let text = if json_wanted { strip_fence(text) } else { text.trim() }.to_string();
    let finish = match c["finish_reason"].as_str().unwrap_or("stop") {
        "length" => "MAX_TOKENS",
        "content_filter" => bail!(crate::tr!("il modello ha rifiutato la richiesta (filtro dei contenuti)", "the model refused the request (content filter)")),
        _ => "STOP",
    }
    .to_string();
    let u = &v["usage"];
    let usage = Usage {
        prompt_token_count: u["prompt_tokens"].as_u64().unwrap_or(0),
        candidates_token_count: u["completion_tokens"].as_u64().unwrap_or(0).saturating_sub(u["completion_tokens_details"]["reasoning_tokens"].as_u64().unwrap_or(0)),
        thoughts_token_count: u["completion_tokens_details"]["reasoning_tokens"].as_u64().unwrap_or(0),
        cost: u["cost"].as_f64(),
    };
    // OpenRouter answers with the model that served the request: the fallback, or a dated id
    // of the same model ("google/gemini-x" → "google/gemini-x-20261101").
    let served = v["model"].as_str().unwrap_or(asked);
    let model = if served.starts_with(asked) { String::new() } else { served.to_string() };
    Ok(Answer { text, usage, finish, model })
}

fn api_error(status: reqwest::StatusCode, body: &str) -> anyhow::Error {
    let msg = serde_json::from_str::<Value>(body).ok().and_then(|v| v["error"]["message"].as_str().map(str::to_string)).unwrap_or_else(|| body.chars().take(300).collect());
    match status.as_u16() {
        401 => anyhow!(crate::tr!("chiave OpenRouter non valida: controllala nelle Impostazioni", "OpenRouter key not valid: check it in Settings")),
        402 => anyhow!(crate::tr!("credito OpenRouter esaurito: ricaricalo su openrouter.ai/settings/credits", "OpenRouter credit exhausted: top it up at openrouter.ai/settings/credits")),
        404 => anyhow!(crate::tr!("modello non trovato su OpenRouter ({msg}): controlla il nome nelle Impostazioni", "model not found on OpenRouter ({msg}): check the name in Settings")),
        _ => anyhow!(crate::tr!("errore di OpenRouter (HTTP {status}): {msg}", "OpenRouter error (HTTP {status}): {msg}")),
    }
}

static MODELS: Mutex<Option<(Instant, Arc<Vec<ModelInfo>>)>> = Mutex::new(None);

impl OpenRouter {
    pub fn new(key: impl Into<String>) -> Self {
        Self::with_base(key, BASE)
    }

    pub fn with_base(key: impl Into<String>, base: impl Into<String>) -> Self {
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

    fn say(&self, msg: String) {
        log::info!("openrouter: {msg}");
        if let Some(n) = &self.notify {
            n(msg);
        }
    }

    pub async fn generate(&self, model: &str, req: &Request, cancel: &CancellationToken) -> Result<Answer> {
        let body = body(model, self.fallback.as_deref(), req)?;
        let max = req.retries.unwrap_or(RETRIES);
        let mut wait = Duration::from_secs(4);
        let mut timeouts = 0;
        let t = Instant::now();
        for attempt in 0..=max {
            if cancel.is_cancelled() {
                bail!("annullato");
            }
            let send = self
                .http
                .post(format!("{}/api/v1/chat/completions", self.base))
                .bearer_auth(&self.key)
                .header("X-Title", "Lezioni")
                .timeout(CALL_TIMEOUT)
                .json(&body)
                .send();
            let resp = tokio::select! {
                r = send => r,
                _ = cancel.cancelled() => bail!("annullato"),
            };
            let resp = match resp {
                Ok(r) => r,
                Err(e) if e.is_timeout() && timeouts == 0 => {
                    timeouts += 1;
                    log::warn!("openrouter: {e}");
                    self.say(crate::tr!("{model} non ha risposto in {} minuti, riprovo un'ultima volta", "{model} did not answer in {} minutes, trying one last time", CALL_TIMEOUT.as_secs() / 60));
                    continue;
                }
                Err(e) if e.is_timeout() => bail!(crate::tr!("OpenRouter non risponde (nessuna risposta in {} minuti): riprova più tardi", "OpenRouter is not answering (no answer in {} minutes): try again later", CALL_TIMEOUT.as_secs() / 60)),
                Err(e) if attempt < max && e.is_connect() => {
                    log::warn!("openrouter: {e}");
                    self.say(crate::tr!("Connessione a OpenRouter non riuscita, riprovo tra {} s (tentativo {} di {max})", "Could not connect to OpenRouter, retrying in {} s (attempt {} of {max})", wait.as_secs(), attempt + 1));
                    sleep_or_cancel(wait, cancel).await?;
                    wait *= 2;
                    continue;
                }
                Err(e) => return Err(anyhow!(e).context(crate::tr!("connessione a OpenRouter", "connecting to OpenRouter"))),
            };
            let status = resp.status();
            let text = resp.text().await.with_context(|| crate::tr!("risposta di OpenRouter", "OpenRouter's answer"))?;
            if status.is_success() {
                let v: Value = serde_json::from_str(&text).with_context(|| crate::tr!("risposta di OpenRouter non valida", "invalid answer from OpenRouter"))?;
                let a = parse_answer(&v, model, req.schema.is_some())?;
                log::info!(
                    "openrouter {}: risposta in {:.0} s, {} token in ingresso, {} in uscita, {} di ragionamento, costo {}, fine {}",
                    if a.model.is_empty() { model } else { &a.model },
                    t.elapsed().as_secs_f64(),
                    a.usage.prompt_token_count,
                    a.usage.candidates_token_count,
                    a.usage.thoughts_token_count,
                    a.usage.cost.map(|c| format!("{c:.4} $")).unwrap_or_else(|| "?".into()),
                    a.finish
                );
                return Ok(a);
            }
            log::warn!("openrouter HTTP {status}: {}", text.chars().take(500).collect::<String>());
            let code = status.as_u16();
            if (code == 408 || code == 429 || status.is_server_error()) && attempt < max {
                let d = retry_delay(&text).unwrap_or(wait);
                let why = if code == 429 { crate::lang::ui().pick("troppe richieste", "too many requests") } else { crate::lang::ui().pick("servizio sovraccarico", "service overloaded") };
                self.say(crate::tr!("OpenRouter: {why} (HTTP {code}), riprovo tra {} s (tentativo {} di {max})", "OpenRouter: {why} (HTTP {code}), retrying in {} s (attempt {} of {max})", d.as_secs(), attempt + 1));
                sleep_or_cancel(d, cancel).await?;
                wait = (wait * 2).min(Duration::from_secs(90));
                continue;
            }
            return Err(api_error(status, &text));
        }
        bail!(crate::tr!("OpenRouter non risponde: riprova più tardi", "OpenRouter is not answering: try again later"))
    }

    /// Checks the key; returns what is left of the credit, when OpenRouter says it.
    pub async fn check(&self) -> Result<String> {
        let resp = self.http.get(format!("{}/api/v1/credits", self.base)).bearer_auth(&self.key).timeout(Duration::from_secs(30)).send().await.with_context(|| crate::tr!("connessione a OpenRouter", "connecting to OpenRouter"))?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(api_error(status, &text));
        }
        let v: Value = serde_json::from_str(&text).with_context(|| crate::tr!("risposta di OpenRouter non valida", "invalid answer from OpenRouter"))?;
        let (total, used) = (v["data"]["total_credits"].as_f64(), v["data"]["total_usage"].as_f64());
        Ok(match (total, used) {
            (Some(t), Some(u)) => crate::tr!("Credito residuo: {:.2} $ (usati {:.2} $)", "Credit left: ${:.2} (${:.2} used)", t - u, u),
            _ => crate::tr!("Chiave valida", "Valid key"),
        })
    }

    /// The price list (public; kept for an hour).
    pub async fn models(&self) -> Result<Arc<Vec<ModelInfo>>> {
        if let Some((at, m)) = &*MODELS.lock().unwrap()
            && at.elapsed() < Duration::from_secs(3600)
        {
            return Ok(m.clone());
        }
        let resp = self.http.get(format!("{}/api/v1/models", self.base)).timeout(Duration::from_secs(30)).send().await.with_context(|| crate::tr!("connessione a OpenRouter", "connecting to OpenRouter"))?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(api_error(status, &text));
        }
        let v: Value = serde_json::from_str(&text).with_context(|| crate::tr!("listino di OpenRouter non valido", "invalid OpenRouter price list"))?;
        let per_m = |x: &Value| x.as_str().and_then(|s| s.parse::<f64>().ok()).map(|p| p * 1e6).unwrap_or(0.0);
        let mut list: Vec<ModelInfo> = v["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| {
                let mods: Vec<&str> = m["architecture"]["input_modalities"].as_array()?.iter().filter_map(Value::as_str).collect();
                Some(ModelInfo {
                    id: m["id"].as_str()?.to_string(),
                    name: m["name"].as_str().unwrap_or_default().to_string(),
                    input: per_m(&m["pricing"]["prompt"]),
                    output: per_m(&m["pricing"]["completion"]),
                    images: mods.contains(&"image"),
                    files: mods.contains(&"file"),
                })
            })
            // Negative prices mean "variable" (routers): not useful here.
            .filter(|m| m.input >= 0.0 && m.output >= 0.0)
            .collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        let list = Arc::new(list);
        *MODELS.lock().unwrap() = Some((Instant::now(), list.clone()));
        Ok(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_body() {
        let req = Request {
            system: Some("sistema".into()),
            parts: vec![Part::Inline { mime: "image/jpeg".into(), data: b"img".to_vec() }, Part::Inline { mime: "application/pdf".into(), data: b"pdf".to_vec() }, Part::Text("leggi".into())],
            schema: Some(json!({ "type": "object" })),
            max_output_tokens: Some(1000),
            thinking: Some("LOW"),
            retries: None,
        };
        let b = body("google/flash", Some("anthropic/haiku"), &req).unwrap();
        assert_eq!(b["models"], json!(["google/flash", "anthropic/haiku"]));
        assert_eq!(b["provider"]["data_collection"], "deny");
        assert_eq!(b["reasoning"]["effort"], "low");
        assert_eq!(b["max_tokens"], 1000);
        assert_eq!(b["response_format"]["json_schema"]["schema"], json!({ "type": "object" }));
        assert_eq!(b["messages"][0], json!({ "role": "system", "content": "sistema" }));
        let parts = &b["messages"][1]["content"];
        assert_eq!(parts[0]["image_url"]["url"], "data:image/jpeg;base64,aW1n");
        assert_eq!(parts[1]["file"]["file_data"], "data:application/pdf;base64,cGRm");
        assert_eq!(parts[2], json!({ "type": "text", "text": "leggi" }));
        // No fallback when it is the same model; audio is refused.
        assert!(body("m", Some("m"), &Request::default()).unwrap().get("models").is_none());
        assert!(body("m", None, &Request { parts: vec![Part::Inline { mime: "audio/wav".into(), data: vec![] }], ..Default::default() }).is_err());
    }

    #[test]
    fn answer_parsing() {
        let v = json!({
            "model": "anthropic/haiku",
            "choices": [{ "finish_reason": "length", "message": { "content": "```json\n{\"a\":1}\n```" } }],
            "usage": { "prompt_tokens": 100, "completion_tokens": 30, "completion_tokens_details": { "reasoning_tokens": 10 }, "cost": 0.0012 }
        });
        let a = parse_answer(&v, "google/flash", true).unwrap();
        assert_eq!((a.text.as_str(), a.finish.as_str(), a.model.as_str()), ("{\"a\":1}", "MAX_TOKENS", "anthropic/haiku"));
        assert_eq!(a.usage, Usage { prompt_token_count: 100, candidates_token_count: 20, thoughts_token_count: 10, cost: Some(0.0012) });
        // A dated id of the asked model is not a fallback.
        let v = json!({ "model": "google/flash-20261101", "choices": [{ "finish_reason": "stop", "message": { "content": " ciao " } }] });
        let a = parse_answer(&v, "google/flash", false).unwrap();
        assert_eq!((a.text.as_str(), a.model.as_str()), ("ciao", ""));
        assert!(parse_answer(&json!({ "error": { "message": "x" } }), "m", false).is_err());
    }

    /// One-shot HTTP server answering (status, body) in order.
    fn fake_server(answers: Vec<(u16, &'static str)>) -> String {
        use std::io::{Read, Write};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            for (code, body) in answers {
                let (mut c, _) = l.accept().unwrap();
                let mut buf = Vec::new();
                let mut b = [0u8; 8192];
                while !String::from_utf8_lossy(&buf).contains("\r\n\r\n") {
                    let n = c.read(&mut b).unwrap();
                    buf.extend_from_slice(&b[..n]);
                }
                let head = String::from_utf8_lossy(&buf).to_lowercase();
                let len: usize = head.lines().find_map(|l| l.strip_prefix("content-length:")).map(|v| v.trim().parse().unwrap()).unwrap_or(0);
                let start = head.find("\r\n\r\n").unwrap() + 4;
                while buf.len() - start < len {
                    let n = c.read(&mut b).unwrap();
                    buf.extend_from_slice(&b[..n]);
                }
                let _ = write!(c, "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn retries_then_answers_with_cost() {
        let base = fake_server(vec![
            (503, r#"{"error":{"code":503,"message":"no provider"}}"#),
            (200, r#"{"model":"m","choices":[{"finish_reason":"stop","message":{"content":"ok"}}],"usage":{"prompt_tokens":5,"completion_tokens":2,"cost":0.5}}"#),
        ]);
        let said = Arc::new(Mutex::new(Vec::<String>::new()));
        let s2 = said.clone();
        let o = OpenRouter::with_base("k", base).with_notify(Arc::new(move |m| s2.lock().unwrap().push(m)));
        let req = Request { parts: vec![Part::Text("ciao".into())], retries: Some(2), ..Default::default() };
        let a = tokio::time::timeout(Duration::from_secs(20), o.generate("m", &req, &CancellationToken::new())).await.unwrap().unwrap();
        assert_eq!((a.text.as_str(), a.usage.cost), ("ok", Some(0.5)));
        assert!(said.lock().unwrap().iter().any(|m| m.contains("HTTP 503") && m.contains("tentativo 1 di 2")));
    }

    #[tokio::test]
    async fn no_credit_is_explained() {
        let base = fake_server(vec![(402, r#"{"error":{"code":402,"message":"Insufficient credits"}}"#)]);
        let o = OpenRouter::with_base("k", base);
        let e = o.generate("m", &Request { parts: vec![Part::Text("x".into())], ..Default::default() }, &CancellationToken::new()).await.unwrap_err();
        assert!(e.to_string().contains("credito OpenRouter esaurito"), "{e}");
    }
}
