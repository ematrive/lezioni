//! AI providers. Only Gemini for now; the pipeline talks to the `Ai` trait so a provider can be
//! swapped (and faked in tests).
pub mod gemini;
pub mod prompts;

use std::future::Future;
use std::path::Path;

use anyhow::Result;
use tokio_util::sync::CancellationToken;

pub use gemini::{Answer, Gemini, Part, RemoteFile, Request, Usage};

pub trait Ai: Send + Sync {
    fn generate(&self, model: &str, req: &Request, cancel: &CancellationToken) -> impl Future<Output = Result<Answer>> + Send;
    fn count_tokens(&self, model: &str, parts: &[Part], cancel: &CancellationToken) -> impl Future<Output = Result<u64>> + Send;
    fn upload(&self, path: &Path, mime: &str, cancel: &CancellationToken) -> impl Future<Output = Result<RemoteFile>> + Send;
    fn delete_file(&self, name: &str) -> impl Future<Output = ()> + Send;
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
