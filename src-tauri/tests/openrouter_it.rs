//! OpenRouter for real: cleanup of a Whisper text, reading of a PDF and a summary with the real
//! prompts and schema, for each model. Prints time and cost. Spends a few cents.
//!   LEZIONI_IT_OPENROUTER=1 OPENROUTER_API_KEY=… LEZIONI_IT_PDF=slide.pdf \
//!   [LEZIONI_IT_MODELS=google/gemini-3.5-flash-lite,anthropic/claude-haiku-5.5] \
//!   [LEZIONI_IT_TEXT=whisper.md LEZIONI_IT_COURSE=Fisica LEZIONI_IT_TITLE="Moto vario"] \
//!   cargo test --test openrouter_it -- --nocapture
use std::time::Instant;

use lezioni_lib::ai::{OpenRouter, Part, Request, Usage, prompts};
use tokio_util::sync::CancellationToken;

/// Whisper-like text: fillers, a misheard term ("variabile entrante" → "variabile e trante").
const RAW: &str = "[00:20:00] Allora, ehm, dove eravamo rimasti? Sì, allora, la forma standard. Quindi abbiamo il problema max z uguale c trasposto x, con a x uguale b e x maggiore uguale di zero. Ok.

[00:20:41] Allora, il metodo del simplesso, cioè, l'idea è che partiamo da una soluzione di base ammissibile, che corrisponde a un vertice del poliedro, e ci spostiamo, ehm, su un vertice adiacente solo se la funzione obiettivo migliora. Ok? Quindi come scelgo la variabile e trante? Guardo i costi ridotti e prendo quello più negativo, cioè quello che fa crescere di più la z.

[00:21:30] E chi esce? Chi esce lo decido con il test del rapporto minimo, cioè faccio b i diviso a i j, ma solo sulle righe dove a i j è maggiore di zero, e prendo il minimo. Se nessun a i j è positivo, eh, vuol dire che il problema è illimitato. Domanda?

[00:22:05] Professore, ma se due rapporti sono uguali? Ottima domanda, allora in quel caso abbiamo una soluzione degenere, e possono esserci dei cicli. Per evitarli si usa la regola di Bland, che vediamo la prossima volta.";

fn cost(u: &Usage) -> f64 {
    u.cost.unwrap_or(f64::NAN)
}

#[tokio::test]
async fn compare_models() {
    if std::env::var("LEZIONI_IT_OPENROUTER").is_err() {
        eprintln!("LEZIONI_IT_OPENROUTER non impostata: test saltato");
        return;
    }
    let key = std::env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY");
    let pdf = std::env::var("LEZIONI_IT_PDF").ok().map(|p| std::fs::read(p).unwrap());
    let models = std::env::var("LEZIONI_IT_MODELS").unwrap_or_else(|_| "google/gemini-3.5-flash-lite,anthropic/claude-haiku-5.5,openai/gpt-5-nano,deepseek/deepseek-v4-flash".into());
    let raw = std::env::var("LEZIONI_IT_TEXT").map(|p| std::fs::read_to_string(p).unwrap()).unwrap_or_else(|_| RAW.into());
    let course = std::env::var("LEZIONI_IT_COURSE").unwrap_or_else(|_| "Ricerca operativa".into());
    let terms = std::env::var("LEZIONI_IT_TERMS").unwrap_or_default();
    let title = std::env::var("LEZIONI_IT_TITLE").unwrap_or_else(|_| "Metodo del simplesso".into());
    let o = OpenRouter::new(key);
    let cancel = CancellationToken::new();
    let mut table = Vec::new();
    for model in models.split(',') {
        eprintln!("\n===== {model}");
        // 1. Cleanup of Whisper's text.
        let t = Instant::now();
        let req = Request { system: Some(prompts::SYSTEM.into()), parts: vec![Part::Text(prompts::clean_transcript(1, 1, &course, &title, &terms, &raw))], max_output_tokens: Some(8000), thinking: Some("LOW"), ..Default::default() };
        let clean = match o.generate(model, &req, &cancel).await {
            Ok(a) => a,
            Err(e) => {
                eprintln!("ripulitura: {e:#}");
                continue;
            }
        };
        let tc = t.elapsed().as_secs_f64();
        eprintln!("--- ripulitura ({tc:.0} s, {:.5} $):\n{}", cost(&clean.usage), clean.text);
        // 2. PDF.
        let mut pdf_text = String::new();
        let (mut tp, mut cp) = (0.0, 0.0);
        if let Some(p) = &pdf {
            let t = Instant::now();
            let req = Request { system: Some(prompts::SYSTEM.into()), parts: vec![Part::Inline { mime: "application/pdf".into(), data: p.clone() }, Part::Text(prompts::PDF.into())], max_output_tokens: Some(8000), thinking: Some("LOW"), ..Default::default() };
            match o.generate(model, &req, &cancel).await {
                Ok(a) => {
                    tp = t.elapsed().as_secs_f64();
                    cp = cost(&a.usage);
                    eprintln!("--- PDF ({tp:.0} s, {cp:.5} $):\n{}", a.text);
                    pdf_text = a.text;
                }
                Err(e) => eprintln!("--- PDF: {e:#}"),
            }
        }
        // 3. Summary with the real schema.
        let sources = format!("[F1] lezione.m4a (registrazione)\n{}\n\n[F2] slide.pdf (PDF)\n{pdf_text}\n", clean.text);
        let req = Request {
            system: Some(prompts::SYSTEM.into()),
            parts: vec![Part::Text(prompts::generate(&course, &title, "2026-11-10", "", &terms, "", &sources))],
            schema: Some(prompts::generate_schema()),
            max_output_tokens: Some(16000),
            ..Default::default()
        };
        if let Ok(dir) = std::env::var("LEZIONI_IT_OUT") {
            let Part::Text(prompt) = &req.parts[0] else { unreachable!() };
            let dump = serde_json::json!({ "system": prompts::SYSTEM, "prompt": prompt, "schema": prompts::generate_schema() });
            std::fs::write(format!("{dir}/{}.richiesta.json", model.replace('/', "_")), dump.to_string()).unwrap();
        }
        let t = Instant::now();
        let (ts, cs, ok) = match o.generate(model, &req, &cancel).await {
            Ok(a) => {
                let v: Result<serde_json::Value, _> = serde_json::from_str(&a.text);
                let ok = v.as_ref().map(|v| ["titolo", "sintesi", "mappa", "glossario", "flashcard", "domande_esame"].iter().all(|k| v.get(k).is_some())).unwrap_or(false);
                eprintln!("uso: {:?}, fine: {}, modello: {}", a.usage, a.finish, a.model);
                eprintln!("--- sintesi ({:.0} s, {:.5} $, JSON valido: {ok}):\n{}", t.elapsed().as_secs_f64(), cost(&a.usage), v.as_ref().ok().and_then(|v| v["sintesi"].as_str()).unwrap_or(&a.text));
                if let Ok(dir) = std::env::var("LEZIONI_IT_OUT") {
                    let name = model.replace('/', "_");
                    std::fs::write(format!("{dir}/{name}.ripulitura.md"), &clean.text).unwrap();
                    std::fs::write(format!("{dir}/{name}.pdf.md"), &pdf_text).unwrap();
                    std::fs::write(format!("{dir}/{name}.generato.json"), &a.text).unwrap();
                }
                (t.elapsed().as_secs_f64(), cost(&a.usage), ok)
            }
            Err(e) => {
                eprintln!("--- sintesi: {e:#}");
                (0.0, f64::NAN, false)
            }
        };
        table.push(format!("{model:40} ripulitura {tc:4.0} s {:.5} $ | PDF {tp:4.0} s {cp:.5} $ | sintesi {ts:4.0} s {cs:.5} $ JSON {ok}", cost(&clean.usage)));
    }
    eprintln!("\n{}", table.join("\n"));
}
