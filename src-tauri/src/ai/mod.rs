//! AI providers: Gemini (Google) or OpenRouter (many models, one key). The pipeline talks to the
//! `Ai` trait, so the provider is chosen in Settings (and faked in tests).
pub mod gemini;
pub mod openrouter;
pub mod prompts;

use std::future::Future;
use std::path::Path;

use anyhow::{Result, bail};
use tokio_util::sync::CancellationToken;

pub use gemini::{Answer, Gemini, Part, RemoteFile, Request, Usage};
pub use openrouter::OpenRouter;

pub trait Ai: Send + Sync {
    fn generate(&self, model: &str, req: &Request, cancel: &CancellationToken) -> impl Future<Output = Result<Answer>> + Send;
    fn count_tokens(&self, model: &str, parts: &[Part], cancel: &CancellationToken) -> impl Future<Output = Result<u64>> + Send;
    fn upload(&self, path: &Path, mime: &str, cancel: &CancellationToken) -> impl Future<Output = Result<RemoteFile>> + Send;
    fn delete_file(&self, name: &str) -> impl Future<Output = ()> + Send;
    /// Files up to this size go inside the request; bigger ones are uploaded.
    fn inline_max(&self) -> u64 {
        gemini::INLINE_MAX
    }
}

impl Ai for Gemini {
    fn generate(&self, model: &str, req: &Request, cancel: &CancellationToken) -> impl Future<Output = Result<Answer>> + Send {
        Gemini::generate(self, model, req, cancel)
    }
    fn count_tokens(&self, model: &str, parts: &[Part], cancel: &CancellationToken) -> impl Future<Output = Result<u64>> + Send {
        Gemini::count_tokens(self, model, parts, cancel)
    }
    fn upload(&self, path: &Path, mime: &str, cancel: &CancellationToken) -> impl Future<Output = Result<RemoteFile>> + Send {
        Gemini::upload(self, path, mime, cancel)
    }
    fn delete_file(&self, name: &str) -> impl Future<Output = ()> + Send {
        Gemini::delete_file(self, name)
    }
}

/// Counts what every answer cost, model by model, also when the job fails afterwards.
pub struct Metered<A> {
    pub inner: A,
    pub spent: std::sync::Arc<std::sync::Mutex<std::collections::BTreeMap<String, Usage>>>,
}

impl<A> Metered<A> {
    pub fn new(inner: A) -> Self {
        Metered { inner, spent: Default::default() }
    }
}

impl<A: Ai> Ai for Metered<A> {
    async fn generate(&self, model: &str, req: &Request, cancel: &CancellationToken) -> Result<Answer> {
        let a = self.inner.generate(model, req, cancel).await?;
        let m = if a.model.is_empty() { model } else { &a.model };
        self.spent.lock().unwrap().entry(m.to_string()).or_default().add(&a.usage);
        Ok(a)
    }
    async fn count_tokens(&self, model: &str, parts: &[Part], cancel: &CancellationToken) -> Result<u64> {
        self.inner.count_tokens(model, parts, cancel).await
    }
    async fn upload(&self, path: &Path, mime: &str, cancel: &CancellationToken) -> Result<RemoteFile> {
        self.inner.upload(path, mime, cancel).await
    }
    async fn delete_file(&self, name: &str) {
        self.inner.delete_file(name).await
    }
    fn inline_max(&self) -> u64 {
        self.inner.inline_max()
    }
}

/// The provider chosen in Settings.
#[derive(Clone)]
pub enum Provider {
    Gemini(Gemini),
    OpenRouter(OpenRouter),
}

impl Provider {
    pub fn with_fallback(self, model: &str) -> Self {
        match self {
            Provider::Gemini(g) => Provider::Gemini(g.with_fallback(model)),
            Provider::OpenRouter(o) => Provider::OpenRouter(o.with_fallback(model)),
        }
    }

    pub fn with_notify(self, n: gemini::Notify) -> Self {
        match self {
            Provider::Gemini(g) => Provider::Gemini(g.with_notify(n)),
            Provider::OpenRouter(o) => Provider::OpenRouter(o.with_notify(n)),
        }
    }
}

impl Ai for Provider {
    async fn generate(&self, model: &str, req: &Request, cancel: &CancellationToken) -> Result<Answer> {
        match self {
            Provider::Gemini(g) => g.generate(model, req, cancel).await,
            Provider::OpenRouter(o) => o.generate(model, req, cancel).await,
        }
    }
    async fn count_tokens(&self, model: &str, parts: &[Part], cancel: &CancellationToken) -> Result<u64> {
        match self {
            Provider::Gemini(g) => g.count_tokens(model, parts, cancel).await,
            Provider::OpenRouter(_) => bail!(crate::tr!("con OpenRouter le registrazioni si trascrivono sul computer", "with OpenRouter the recordings are transcribed on the computer")),
        }
    }
    async fn upload(&self, path: &Path, mime: &str, cancel: &CancellationToken) -> Result<RemoteFile> {
        match self {
            Provider::Gemini(g) => g.upload(path, mime, cancel).await,
            Provider::OpenRouter(_) => {
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                bail!(crate::tr!("{name} è troppo grande per OpenRouter (oltre {} MB): dividilo o riducilo", "{name} is too large for OpenRouter (over {} MB): split it or make it smaller", openrouter::INLINE_MAX / 1024 / 1024))
            }
        }
    }
    async fn delete_file(&self, name: &str) {
        if let Provider::Gemini(g) = self {
            g.delete_file(name).await
        }
    }
    fn inline_max(&self) -> u64 {
        match self {
            Provider::Gemini(_) => gemini::INLINE_MAX,
            Provider::OpenRouter(_) => openrouter::INLINE_MAX,
        }
    }
}
