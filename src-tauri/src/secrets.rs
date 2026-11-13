//! API keys (Gemini, OpenRouter) in the system keychain (Portachiavi on the Mac, Credential
//! Manager on Windows, Secret Service on Linux). Cached for the life of the app.
//!
//! Both keys live in ONE keychain item (JSON), so macOS asks at most once per app build: with an
//! ad-hoc signature "Always allow" is tied to the build, and every item would ask after an update.
//! Older versions used one item per key: they are read once to migrate and then left alone (for a
//! downgrade).
use std::sync::Mutex;

use anyhow::{Context, Result};
use keyring::Entry;
use serde::{Deserialize, Serialize};

const SERVICE: &str = "app.lezioni.desktop";
const ACCOUNT: &str = "chiavi-api";
const OLD_GEMINI: &str = "gemini-api-key";
const OLD_OPENROUTER: &str = "openrouter-api-key";

#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
struct Keys {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    gemini: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    openrouter: Option<String>,
}

impl Keys {
    fn is_empty(&self) -> bool {
        self.gemini.is_none() && self.openrouter.is_none()
    }

    /// Parses the stored item; empty strings count as missing.
    fn parse(s: &str) -> Result<Keys> {
        let mut k: Keys = serde_json::from_str(s).with_context(|| crate::tr!("chiavi nel portachiavi non leggibili", "keys in the keychain not readable"))?;
        k.gemini = k.gemini.filter(|v| !v.trim().is_empty());
        k.openrouter = k.openrouter.filter(|v| !v.trim().is_empty());
        Ok(k)
    }
}

static CACHE: Mutex<Option<Keys>> = Mutex::new(None);

fn read_item(account: &str) -> Result<Option<String>> {
    match Entry::new(SERVICE, account).with_context(|| crate::tr!("portachiavi non disponibile", "keychain not available"))?.get_password() {
        Ok(p) => Ok(Some(p)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(anyhow::Error::new(e).context(crate::tr!("lettura della chiave dal portachiavi", "reading the key from the keychain"))),
    }
}

fn write(keys: &Keys) -> Result<()> {
    let e = Entry::new(SERVICE, ACCOUNT).with_context(|| crate::tr!("portachiavi non disponibile", "keychain not available"))?;
    if keys.is_empty() {
        match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(anyhow::Error::new(err).context(crate::tr!("eliminazione della chiave dal portachiavi", "deleting the key from the keychain"))),
        }
    } else {
        e.set_password(&serde_json::to_string(keys)?).with_context(|| crate::tr!("salvataggio della chiave nel portachiavi", "saving the key in the keychain"))
    }
}

/// The keys from the single item, or migrated from the old per-key items.
fn load() -> Result<Keys> {
    if let Some(s) = read_item(ACCOUNT)? {
        return Keys::parse(&s);
    }
    let keys = Keys { gemini: read_item(OLD_GEMINI)?, openrouter: read_item(OLD_OPENROUTER)? };
    if !keys.is_empty() {
        write(&keys)?;
    }
    Ok(keys)
}

fn keys() -> Result<Keys> {
    let mut cache = CACHE.lock().unwrap();
    if let Some(k) = cache.as_ref() {
        return Ok(k.clone());
    }
    let k = load()?;
    *cache = Some(k.clone());
    Ok(k)
}

fn update(change: impl FnOnce(&mut Keys)) -> Result<()> {
    let mut k = keys()?;
    change(&mut k);
    write(&k)?;
    *CACHE.lock().unwrap() = Some(k);
    Ok(())
}

pub fn gemini_key() -> Result<Option<String>> {
    Ok(keys()?.gemini)
}

pub fn set_gemini_key(key: Option<&str>) -> Result<()> {
    update(|k| k.gemini = key.map(str::to_string))
}

pub fn openrouter_key() -> Result<Option<String>> {
    Ok(keys()?.openrouter)
}

pub fn set_openrouter_key(key: Option<&str>) -> Result<()> {
    update(|k| k.openrouter = key.map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip_in_one_item() {
        let k = Keys { gemini: Some("g".into()), openrouter: None };
        let s = serde_json::to_string(&k).unwrap();
        assert_eq!(s, r#"{"gemini":"g"}"#);
        assert_eq!(Keys::parse(&s).unwrap(), k);
        assert_eq!(Keys::parse(r#"{"gemini":" ","openrouter":"o"}"#).unwrap(), Keys { gemini: None, openrouter: Some("o".into()) });
        assert!(Keys::parse("{}").unwrap().is_empty());
        assert!(Keys::parse("non json").is_err());
    }
}
