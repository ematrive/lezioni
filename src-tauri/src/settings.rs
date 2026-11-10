//! App settings in `app_config_dir()/settings.json` (not in the archive: they are per computer).
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::pipeline::Models;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Archive folder; None until the user picks one.
    pub library_dir: Option<PathBuf>,
    #[serde(default)]
    pub models: Models,
}

impl Settings {
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
