//! Whisper on the computer, with a real model. Skipped unless LEZIONI_IT_WHISPER points to a
//! ggml model file (e.g. ggml-tiny-q5_1.bin for a quick run). LEZIONI_IT_AUDIO: a recording
//! (default: a generated tone, which must give no text).
//! (LEZIONI_IT_PROMPT: the initial prompt, as the app builds it from course and title.)
//!   LEZIONI_IT_WHISPER=/path/ggml-tiny-q5_1.bin LEZIONI_IT_AUDIO=lezione.m4a cargo test --test whisper_it -- --nocapture
use lezioni_lib::asr::{self, Asr, Whisper};
use lezioni_lib::audio;
use tokio_util::sync::CancellationToken;

#[test]
fn whisper_transcribes() {
    let Ok(model) = std::env::var("LEZIONI_IT_WHISPER") else {
        eprintln!("LEZIONI_IT_WHISPER non impostata: test saltato");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let input = match std::env::var("LEZIONI_IT_AUDIO") {
        Ok(p) => std::path::PathBuf::from(p),
        Err(_) => {
            let p = dir.path().join("tono.wav");
            let spec = hound::WavSpec { channels: 1, sample_rate: 16_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
            let mut w = hound::WavWriter::create(&p, spec).unwrap();
            for i in 0..16_000 * 5 {
                w.write_sample(((i as f32 * 440.0 * std::f32::consts::TAU / 16_000.0).sin() * 8000.0) as i16).unwrap();
            }
            w.finalize().unwrap();
            p
        }
    };
    let cancel = CancellationToken::new();
    let parts = audio::split(&input, &dir.path().join("parti"), 20 * 60, &cancel).unwrap();
    let w = Whisper::load(std::path::Path::new(&model)).unwrap();
    let prompt = std::env::var("LEZIONI_IT_PROMPT").unwrap_or_else(|_| "Lezione universitaria di Ricerca operativa.".into());
    for c in &parts {
        let t = std::time::Instant::now();
        let segs = w.transcribe(&c.path, &prompt, &cancel, &|p| eprint!("{p}% ")).unwrap();
        if std::env::var("LEZIONI_IT_RAW").is_ok() {
            for s in &segs {
                eprintln!("{:7.1}-{:7.1} {}", s.start, s.end, s.text);
            }
        }
        eprintln!("\nparte da {} s: {:.1} s di audio in {:.1} s", c.start, c.secs, t.elapsed().as_secs_f64());
        println!("{}", asr::paragraphs(&segs, c.start));
    }
}

/// Cancelling stops whisper.cpp in the middle (abort callback), with "annullato".
#[test]
fn whisper_cancels() {
    let (Ok(model), Ok(input)) = (std::env::var("LEZIONI_IT_WHISPER"), std::env::var("LEZIONI_IT_AUDIO")) else {
        eprintln!("LEZIONI_IT_WHISPER o LEZIONI_IT_AUDIO non impostate: test saltato");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let cancel = CancellationToken::new();
    let parts = audio::split(std::path::Path::new(&input), dir.path(), 20 * 60, &cancel).unwrap();
    let w = Whisper::load(std::path::Path::new(&model)).unwrap();
    let c2 = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(800));
        c2.cancel();
    });
    let t = std::time::Instant::now();
    let r = w.transcribe(&parts[0].path, "", &cancel, &|_| {});
    assert_eq!(r.unwrap_err().to_string(), "annullato");
    eprintln!("fermato dopo {:.1} s", t.elapsed().as_secs_f64());
}
