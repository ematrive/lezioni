//! Long jobs (lesson processing): cancellation tokens by id, events to the UI.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::ai::Usage;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum JobEvent {
    #[serde(rename_all = "camelCase")]
    Step { done: f64, total: u32, message: String },
    /// What the current step is waiting for (upload, Gemini's answer, a retry).
    #[serde(rename_all = "camelCase")]
    Detail { message: String },
    #[serde(rename_all = "camelCase")]
    Done { usage: Usage, warnings: Vec<String> },
    #[serde(rename_all = "camelCase")]
    Failed { message: String },
}

pub type Emit = Arc<dyn Fn(JobEvent) + Send + Sync>;

#[derive(Default)]
pub struct Jobs {
    map: Mutex<HashMap<String, CancellationToken>>,
}

impl Jobs {
    pub fn start(&self, id: &str) -> CancellationToken {
        let t = CancellationToken::new();
        self.map.lock().unwrap().insert(id.to_string(), t.clone());
        t
    }
    pub fn cancel(&self, id: &str) -> bool {
        self.map.lock().unwrap().get(id).map(|t| t.cancel()).is_some()
    }
    pub fn finish(&self, id: &str) {
        self.map.lock().unwrap().remove(id);
    }
}
