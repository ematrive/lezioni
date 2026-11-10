//! App settings in `app_config_dir()/settings.json` (not in the archive: they are per computer).
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::pipeline::{Engine, Models};

/// Who does the text work (cleanup, photos and PDFs, summary, questions).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    #[default]
    Gemini,
    OpenRouter,
}

/// Models on OpenRouter (ids like `google/gemini-3.5-flash-lite`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSet {
    /// Cleanup of the transcript, photos and PDFs: must read images and files.
    pub transcribe: String,
    pub generate: String,
    pub fallback: String,
}

impl Default for ModelSet {
    fn default() -> Self {
        // Chosen with tests/openrouter_it.rs (November 2026): Flash-Lite is the fastest at cleanup
        // and PDFs, Haiku writes the most faithful summary; both cost cents per lesson.
        Self { transcribe: "google/gemini-3.5-flash-lite".into(), generate: "anthropic/claude-haiku-5.5".into(), fallback: "google/gemini-3.5-flash-lite".into() }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Archive folder; None until the user picks one.
    pub library_dir: Option<PathBuf>,
    /// Gemini models, and how recordings are transcribed.
    #[serde(default)]
    pub models: Models,
    #[serde(default)]
    pub provider: ProviderKind,
    #[serde(default)]
    pub openrouter: ModelSet,
}

impl Settings {
    /// The models of the chosen provider. OpenRouter does not transcribe audio: recordings are
    /// always done on the computer then.
    pub fn effective_models(&self) -> Models {
        match self.provider {
            ProviderKind::Gemini => self.models.clone(),
            ProviderKind::OpenRouter => Models { engine: Engine::Local, transcribe: self.openrouter.transcribe.clone(), generate: self.openrouter.generate.clone(), fallback: self.openrouter.fallback.clone() },
        }
    }

    pub fn load(path: &Path) -> Settings {
        fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(d) = path.parent() {
            fs::create_dir_all(d)?;
        }
        fs::write(path, serde_json::to_vec_pretty(self)?).with_context(|| format!("scrittura di {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openrouter_always_transcribes_locally() {
        let mut s: Settings = serde_json::from_str(r#"{"libraryDir":null,"models":{"engine":"gemini","transcribe":"a","generate":"b"}}"#).unwrap();
        assert_eq!(s.provider, ProviderKind::Gemini, "old settings files still load");
        assert_eq!(s.effective_models().engine, Engine::Gemini);
        s.provider = ProviderKind::OpenRouter;
        let m = s.effective_models();
        assert_eq!((m.engine, m.transcribe.as_str()), (Engine::Local, "google/gemini-3.5-flash-lite"));
    }
}
