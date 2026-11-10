//! API keys (Gemini, OpenRouter) in the system keychain (Portachiavi on the Mac, Credential
//! Manager on Windows, Secret Service on Linux). Cached for the life of the app.
use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::{Context, Result};
use keyring::Entry;

const SERVICE: &str = "app.lezioni.desktop";
const GEMINI: &str = "gemini-api-key";
const OPENROUTER: &str = "openrouter-api-key";

static CACHE: Mutex<Option<HashMap<&'static str, Option<String>>>> = Mutex::new(None);

fn get(account: &'static str) -> Result<Option<String>> {
    if let Some(v) = CACHE.lock().unwrap().as_ref().and_then(|m| m.get(account)) {
        return Ok(v.clone());
    }
    let v = match Entry::new(SERVICE, account).context("portachiavi non disponibile")?.get_password() {
        Ok(p) => Some(p),
        Err(keyring::Error::NoEntry) => None,
        Err(e) => return Err(anyhow::Error::new(e).context("lettura della chiave dal portachiavi")),
    };
    CACHE.lock().unwrap().get_or_insert_default().insert(account, v.clone());
    Ok(v)
}

fn set(account: &'static str, key: Option<&str>) -> Result<()> {
    let e = Entry::new(SERVICE, account).context("portachiavi non disponibile")?;
    match key {
        Some(k) => e.set_password(k).context("salvataggio della chiave nel portachiavi")?,
        None => match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(err) => return Err(anyhow::Error::new(err).context("eliminazione della chiave dal portachiavi")),
        },
    }
    CACHE.lock().unwrap().get_or_insert_default().insert(account, key.map(str::to_string));
    Ok(())
}

pub fn gemini_key() -> Result<Option<String>> {
    get(GEMINI)
}

pub fn set_gemini_key(key: Option<&str>) -> Result<()> {
    set(GEMINI, key)
}

pub fn openrouter_key() -> Result<Option<String>> {
    get(OPENROUTER)
}

pub fn set_openrouter_key(key: Option<&str>) -> Result<()> {
    set(OPENROUTER, key)
}
