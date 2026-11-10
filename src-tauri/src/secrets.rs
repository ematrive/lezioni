//! The Gemini API key, in the system keychain (Portachiavi on the Mac, Credential Manager on
//! Windows, Secret Service on Linux). Cached for the life of the app.
use std::sync::Mutex;

use anyhow::{Context, Result};
use keyring::Entry;

const SERVICE: &str = "app.lezioni.desktop";
const GEMINI: &str = "gemini-api-key";

static CACHE: Mutex<Option<Option<String>>> = Mutex::new(None);

fn entry() -> Result<Entry> {
    Entry::new(SERVICE, GEMINI).context("portachiavi non disponibile")
}

pub fn gemini_key() -> Result<Option<String>> {
    if let Some(v) = CACHE.lock().unwrap().clone() {
        return Ok(v);
    }
    let v = match entry()?.get_password() {
        Ok(p) => Some(p),
        Err(keyring::Error::NoEntry) => None,
        Err(e) => return Err(anyhow::Error::new(e).context("lettura della chiave dal portachiavi")),
    };
    *CACHE.lock().unwrap() = Some(v.clone());
    Ok(v)
}

pub fn set_gemini_key(key: Option<&str>) -> Result<()> {
    let e = entry()?;
    match key {
        Some(k) => e.set_password(k).context("salvataggio della chiave nel portachiavi")?,
        None => match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(err) => return Err(anyhow::Error::new(err).context("eliminazione della chiave dal portachiavi")),
        },
    }
    *CACHE.lock().unwrap() = Some(key.map(str::to_string));
    Ok(())
}
