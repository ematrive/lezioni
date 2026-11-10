//! Splitting of the formats phones and laptops record in. Files made with ffmpeg if installed
//! (skipped otherwise): m4a/AAC (iPhone Memo vocali, most Android recorders), mp3, ogg/vorbis,
//! the audio of an mp4 video. Opus is expected to fail (no decoder): the app sends it whole.
use std::process::Command;

use lezioni_lib::audio;
use tokio_util::sync::CancellationToken;

#[test]
fn common_formats() {
    if Command::new("ffmpeg").arg("-version").output().is_err() {
        eprintln!("saltato: ffmpeg non installato");
        return;
    }
    let t = tempfile::tempdir().unwrap();
    let cases: &[(&str, &[&str], bool)] = &[
        ("memo.m4a", &["-f", "lavfi", "-i", "sine=f=300:d=25", "-ac", "2", "-ar", "48000", "-c:a", "aac"], true),
        ("reg.mp3", &["-f", "lavfi", "-i", "sine=f=300:d=25", "-ar", "44100", "-c:a", "libmp3lame"], true),
        ("reg.ogg", &["-f", "lavfi", "-i", "sine=f=300:d=25", "-c:a", "libvorbis"], true),
        ("video.mp4", &["-f", "lavfi", "-i", "testsrc=d=25:s=160x120", "-f", "lavfi", "-i", "sine=f=300:d=25", "-c:v", "libx264", "-c:a", "aac", "-shortest"], true),
        ("reg.opus", &["-f", "lavfi", "-i", "sine=f=300:d=25", "-c:a", "libopus"], false),
    ];
    for (name, args, ok) in cases {
        let p = t.path().join(name);
        let st = Command::new("ffmpeg").args(["-y", "-loglevel", "error"]).args(*args).arg(&p).status().unwrap();
        if !st.success() {
            eprintln!("{name}: ffmpeg non sa crearlo, saltato");
            continue;
        }
        let r = audio::split(&p, &t.path().join(format!("parti-{name}")), 10, &CancellationToken::new());
        match (ok, r) {
            (true, Ok(parts)) => {
                let total: f64 = parts.iter().map(|c| c.secs).sum();
                assert_eq!(parts.len(), 3, "{name}");
                assert!((total - 25.0).abs() < 0.3, "{name}: {total} s");
                println!("{name}: {} parti, {total:.2} s", parts.len());
            }
            (false, Err(e)) => println!("{name}: non decodificabile come previsto ({e})"),
            (true, Err(e)) => panic!("{name}: {e:#}"),
            (false, Ok(_)) => println!("{name}: decodificato (meglio del previsto)"),
        }
    }
}
