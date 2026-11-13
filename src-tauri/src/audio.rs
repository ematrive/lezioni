//! Splits a recording into WAV parts (16 kHz, mono, 16 bit) of a fixed length, decoding with
//! Symphonia (pure Rust: m4a/AAC, mp3, wav, flac, ogg/vorbis; the audio track of mp4/mov).
//!
//! Why: Gemini has no start/end offset for audio parts, so asking for a 20-minute window of a
//! whole uploaded file is billed as the whole file every time. Sending parts bills each minute
//! once. 16 kHz mono is what Gemini downsamples to anyway (and plenty for speech): 32 KB/s,
//! about 38 MB per 20 minutes.
use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tokio_util::sync::CancellationToken;

pub const RATE: u32 = 16_000;

#[derive(Debug, Clone)]
pub struct Chunk {
    pub path: PathBuf,
    /// Start in the original recording, seconds.
    pub start: u64,
    pub secs: f64,
}

/// Linear-interpolation resampler to `RATE`, fed in blocks (keeps the tail between calls).
struct Resampler {
    step: f64,
    t: f64,
    buf: Vec<f32>,
}

impl Resampler {
    fn new(from: u32) -> Self {
        Self { step: from as f64 / RATE as f64, t: 0.0, buf: Vec::new() }
    }

    fn push(&mut self, input: &[f32], out: &mut Vec<i16>) {
        self.buf.extend_from_slice(input);
        while self.t + 1.0 < self.buf.len() as f64 {
            let i = self.t as usize;
            let f = (self.t - i as f64) as f32;
            let v = self.buf[i] * (1.0 - f) + self.buf[i + 1] * f;
            out.push((v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
            self.t += self.step;
        }
        let drop = (self.t as usize).min(self.buf.len());
        self.buf.drain(..drop);
        self.t -= drop as f64;
    }
}

struct Writer {
    dir: PathBuf,
    window: u64,
    chunks: Vec<Chunk>,
    current: Option<(hound::WavWriter<std::io::BufWriter<File>>, u64)>,
}

impl Writer {
    fn spec() -> hound::WavSpec {
        hound::WavSpec { channels: 1, sample_rate: RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int }
    }

    fn write(&mut self, samples: &[i16]) -> Result<()> {
        let per_chunk = self.window * RATE as u64;
        for &s in samples {
            if self.current.as_ref().is_none_or(|(_, n)| *n >= per_chunk) {
                self.close()?;
                let i = self.chunks.len() + 1;
                let path = self.dir.join(format!("parte-{i:02}.wav"));
                let w = hound::WavWriter::create(&path, Self::spec()).context("scrittura di una parte della registrazione")?;
                self.chunks.push(Chunk { path, start: (i as u64 - 1) * self.window, secs: 0.0 });
                self.current = Some((w, 0));
            }
            let (w, n) = self.current.as_mut().unwrap();
            w.write_sample(s)?;
            *n += 1;
        }
        Ok(())
    }

    fn close(&mut self) -> Result<()> {
        if let Some((w, n)) = self.current.take() {
            w.finalize()?;
            if let Some(c) = self.chunks.last_mut() {
                c.secs = n as f64 / RATE as f64;
            }
        }
        Ok(())
    }
}

/// Decodes `path` and writes parts of `window` seconds into `out_dir`. Errors mean the format
/// is not supported here (the caller can fall back to sending the whole file).
pub fn split(path: &Path, out_dir: &Path, window: u64, cancel: &CancellationToken) -> Result<Vec<Chunk>> {
    std::fs::create_dir_all(out_dir)?;
    let file = File::open(path).with_context(|| format!("apertura di {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe().format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default()).map_err(|e| anyhow!("formato non riconosciuto: {e}"))?;
    let mut format = probed.format;
    // The first track we can decode (a video file also has a video track).
    let (track_id, mut decoder, rate) = format
        .tracks()
        .iter()
        .filter(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .find_map(|t| {
            let d = symphonia::default::get_codecs().make(&t.codec_params, &DecoderOptions::default()).ok()?;
            Some((t.id, d, t.codec_params.sample_rate))
        })
        .ok_or_else(|| anyhow!("nessuna traccia audio decodificabile"))?;
    let mut resampler: Option<Resampler> = rate.map(Resampler::new);
    let mut w = Writer { dir: out_dir.to_path_buf(), window, chunks: Vec::new(), current: None };
    let mut mono = Vec::new();
    let mut out = Vec::new();
    let mut decoded_any = false;
    loop {
        if cancel.is_cancelled() {
            bail!("annullato");
        }
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => return Err(anyhow!("lettura della registrazione: {e}")),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let buf = match decoder.decode(&packet) {
            Ok(b) => b,
            // A damaged packet: skip it, as players do.
            Err(SymError::DecodeError(_)) => continue,
            Err(e) => return Err(anyhow!("decodifica della registrazione: {e}")),
        };
        let spec = *buf.spec();
        let ch = spec.channels.count().max(1);
        let mut sb = SampleBuffer::<f32>::new(buf.capacity() as u64, spec);
        sb.copy_interleaved_ref(buf);
        mono.clear();
        mono.extend(sb.samples().chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32));
        let rs = resampler.get_or_insert_with(|| Resampler::new(spec.rate));
        out.clear();
        rs.push(&mono, &mut out);
        w.write(&out)?;
        decoded_any = true;
    }
    w.close()?;
    if !decoded_any || w.chunks.is_empty() {
        bail!("la registrazione è vuota o non decodificabile");
    }
    Ok(w.chunks)
}

/// Length of a recording in seconds, from its header (nothing is decoded).
pub fn duration_secs(path: &Path) -> Option<f64> {
    if let Some((_, d)) = mp4_header(path) {
        return Some(d);
    }
    let file = File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe().format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default()).ok()?;
    let t = probed.format.tracks().iter().find(|t| t.codec_params.codec != CODEC_TYPE_NULL)?;
    let frames = t.codec_params.n_frames?;
    let rate = t.codec_params.sample_rate?;
    Some(frames as f64 / rate as f64)
}

/// Creation time and length from the `mvhd` box of an m4a/mp4/mov file. Phones write the
/// creation time there; some apps when the recording starts, others when it ends.
pub fn mp4_header(path: &Path) -> Option<(chrono::DateTime<chrono::Utc>, f64)> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    // Walks the boxes between `from` and `to`; returns the start and end of the content of `kind`.
    fn find(f: &mut File, from: u64, to: u64, kind: &[u8; 4]) -> Option<(u64, u64)> {
        let mut pos = from;
        while pos + 8 <= to {
            f.seek(SeekFrom::Start(pos)).ok()?;
            let mut h = [0u8; 8];
            f.read_exact(&mut h).ok()?;
            let mut size = u32::from_be_bytes(h[0..4].try_into().ok()?) as u64;
            let mut head = 8;
            if size == 1 {
                let mut b = [0u8; 8];
                f.read_exact(&mut b).ok()?;
                size = u64::from_be_bytes(b);
                head = 16;
            } else if size == 0 {
                size = to - pos;
            }
            if size < head {
                return None;
            }
            if &h[4..8] == kind {
                return Some((pos + head, (pos + size).min(to)));
            }
            pos += size;
        }
        None
    }
    let (ms, me) = find(&mut f, 0, len, b"moov")?;
    let (hs, he) = find(&mut f, ms, me, b"mvhd")?;
    let mut b = vec![0u8; (he - hs).min(64) as usize];
    f.seek(SeekFrom::Start(hs)).ok()?;
    f.read_exact(&mut b).ok()?;
    let u32_at = |i: usize| -> Option<u64> { Some(u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?) as u64) };
    let u64_at = |i: usize| -> Option<u64> { Some(u64::from_be_bytes(b.get(i..i + 8)?.try_into().ok()?)) };
    let (created, scale, dur) = match b.first()? {
        0 => (u32_at(4)?, u32_at(12)?, u32_at(16)?),
        1 => (u64_at(4)?, u32_at(20)?, u64_at(24)?),
        _ => return None,
    };
    // Seconds since 1904-01-01 UTC; 0 (or a date before the 2000s) means "not set".
    let unix = created.checked_sub(2_082_844_800)? as i64;
    if unix < 946_684_800 || scale == 0 {
        return None;
    }
    Some((chrono::DateTime::from_timestamp(unix, 0)?, dur as f64 / scale as f64))
}

/// When a recording started: the mp4 header (true: it may be the end instead), or else the date
/// the original file was last changed (`modified`, the end of the recording) minus its length.
pub fn started_at(copy: &Path, modified: Option<std::time::SystemTime>) -> Option<(chrono::DateTime<chrono::Utc>, bool)> {
    if let Some((at, _)) = mp4_header(copy) {
        return Some((at, true));
    }
    let end: chrono::DateTime<chrono::Utc> = modified?.into();
    let d = duration_secs(copy)?;
    Some((end - chrono::Duration::milliseconds((d * 1000.0) as i64), false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mp4_header_times() {
        // ftyp, a free box, then moov with a version 0 mvhd: created 2026-11-10 19:43:31 UTC, 494.5 s.
        let created: u32 = (1_794_339_811u64 + 2_082_844_800) as u32;
        let mut mvhd = vec![0u8; 4];
        for v in [created, created, 1000, 494_500] {
            mvhd.extend(v.to_be_bytes());
        }
        let boxed = |kind: &[u8], body: &[u8]| [&((body.len() + 8) as u32).to_be_bytes()[..], kind, body].concat();
        let file = [boxed(b"ftyp", b"M4A "), boxed(b"free", &[0; 10]), boxed(b"moov", &boxed(b"mvhd", &mvhd))].concat();
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.m4a");
        std::fs::write(&p, file).unwrap();
        let (at, d) = mp4_header(&p).unwrap();
        assert_eq!(at.to_rfc3339(), "2026-11-10T19:43:31+00:00");
        assert!((d - 494.5).abs() < 0.001);
        assert_eq!(duration_secs(&p), Some(494.5));
        assert_eq!(started_at(&p, None), Some((at, true)));
        // Not an mp4: the file's date minus the length.
        let w = dir.path().join("b.wav");
        tone(&w, 3);
        let end = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_794_340_000);
        assert_eq!(started_at(&w, Some(end)).unwrap().0.timestamp(), 1_794_339_997);
    }

    /// 44.1 kHz stereo WAV of `secs` seconds (a 440 Hz tone).
    pub fn tone(path: &Path, secs: u32) {
        let spec = hound::WavSpec { channels: 2, sample_rate: 44_100, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..44_100 * secs {
            let v = ((i as f32 * 440.0 * std::f32::consts::TAU / 44_100.0).sin() * 8000.0) as i16;
            w.write_sample(v).unwrap();
            w.write_sample(v).unwrap();
        }
        w.finalize().unwrap();
    }

    #[test]
    fn splits_in_parts() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("lezione.wav");
        tone(&src, 25);
        let parts = split(&src, &t.path().join("parti"), 10, &CancellationToken::new()).unwrap();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts.iter().map(|c| c.start).collect::<Vec<_>>(), [0, 10, 20]);
        assert!((parts[0].secs - 10.0).abs() < 0.01 && (parts[2].secs - 5.0).abs() < 0.05, "{parts:?}");
        let r = hound::WavReader::open(&parts[1].path).unwrap();
        assert_eq!(r.spec().sample_rate, RATE);
        assert_eq!(r.spec().channels, 1);
    }

    #[test]
    fn rejects_garbage() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("rotto.m4a");
        std::fs::write(&src, b"non sono audio").unwrap();
        assert!(split(&src, &t.path().join("p"), 10, &CancellationToken::new()).is_err());
    }
}
