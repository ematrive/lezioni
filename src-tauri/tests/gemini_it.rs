//! Test against the real Gemini API (uses your quota, a few thousand tokens). Off by default:
//!
//!   LEZIONI_IT=1 GEMINI_API_KEY=... cargo test --test gemini_it -- --nocapture
//!
//! Optional: LEZIONI_IT_AUDIO=/path/registrazione.m4a (a real recording, ideally a few minutes
//! of speech) and LEZIONI_IT_MODEL (default gemini-3.8-flash). Prints the tokens of each call.
use std::path::PathBuf;

use lezioni_lib::ai::{Gemini, Part, Request, prompts};
use lezioni_lib::audio;
use lezioni_lib::lang::Lang::It;
use tokio_util::sync::CancellationToken;

fn enabled() -> Option<(Gemini, String)> {
    if std::env::var("LEZIONI_IT").ok().as_deref() != Some("1") {
        eprintln!("saltato: LEZIONI_IT=1 non impostato");
        return None;
    }
    let key = std::env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY");
    Some((Gemini::new(key), std::env::var("LEZIONI_IT_MODEL").unwrap_or_else(|_| "gemini-3.8-flash".into())))
}

fn tone(path: &std::path::Path, secs: u32) {
    let spec = hound::WavSpec { channels: 1, sample_rate: 16_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..16_000 * secs {
        w.write_sample(((i as f32 * 0.17).sin() * 5000.0) as i16).unwrap();
    }
    w.finalize().unwrap();
}

#[tokio::test]
async fn real_api() {
    let Some((g, model)) = enabled() else { return };
    let cancel = CancellationToken::new();
    g.check().await.expect("chiave");

    let dir = tempfile::tempdir().unwrap();
    let (src, real) = match std::env::var("LEZIONI_IT_AUDIO") {
        Ok(p) => (PathBuf::from(p), true),
        Err(_) => {
            let p = dir.path().join("tono.wav");
            tone(&p, 30);
            (p, false)
        }
    };
    // Local split (2-minute parts), then the first part through the Files API (as for long lectures).
    let parts = audio::split(&src, &dir.path().join("parti"), 120, &cancel).expect("divisione");
    println!("{} parti, la prima di {:.1} s", parts.len(), parts[0].secs);
    let f = g.upload(&parts[0].path, "audio/wav", &cancel).await.expect("upload");
    println!("file: {} stato {:?}", f.name, f.state);
    let part = Part::File { mime: f.mime.clone().unwrap_or("audio/wav".into()), uri: f.uri.clone() };
    let tokens = g.count_tokens(&model, std::slice::from_ref(&part), &cancel).await.expect("countTokens");
    println!("countTokens: {tokens} (atteso ≈ {:.0})", parts[0].secs * 32.0);

    let req = Request { system: Some(prompts::system(It)), parts: vec![part, Part::Text(prompts::transcribe_clip(It, 1, parts.len(), "00:00:00"))], thinking: Some("LOW"), max_output_tokens: Some(8000), ..Default::default() };
    let a = g.generate(&model, &req, &cancel).await.expect("trascrizione");
    println!("trascrizione ({} / {:?}):\n{}\n", a.finish, a.usage, a.text.chars().take(1500).collect::<String>());
    g.delete_file(&f.name).await;
    if real {
        assert!(a.text.contains('['), "mi aspettavo dei tempi [mm:ss]");
    }

    // Structured output with the real schema, on a small text.
    let req = Request {
        system: Some(prompts::system(It)),
        parts: vec![Part::Text(prompts::generate(It, It, "Ricerca operativa", "Prova", "2026-11-10", "", "", "", "", "", "[F1] appunti (testo)\nIl metodo del simplesso si muove tra i vertici del poliedro ammissibile. [00:01:00] La variabile entrante si sceglie col costo ridotto più negativo."))],
        schema: Some(prompts::generate_schema()),
        max_output_tokens: Some(8000),
        ..Default::default()
    };
    let a = g.generate(&model, &req, &cancel).await.expect("sintesi");
    println!("sintesi ({} / {:?})", a.finish, a.usage);
    let v: serde_json::Value = serde_json::from_str(&a.text).expect("JSON valido");
    for k in ["titolo", "sintesi", "mappa", "glossario", "flashcard", "domande_esame"] {
        assert!(v.get(k).is_some(), "manca {k}: {v}");
    }
    println!("{}", serde_json::to_string_pretty(&v["mappa"]).unwrap());
}
