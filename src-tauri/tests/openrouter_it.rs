//! OpenRouter for real: cleanup of a Whisper text, reading of a PDF and a summary with the real
//! prompts and schema, for each model. Prints time and cost. Spends a few cents.
//!   LEZIONI_IT_OPENROUTER=1 OPENROUTER_API_KEY=… LEZIONI_IT_PDF=slide.pdf \
//!   [LEZIONI_IT_MODELS=google/gemini-3.5-flash-lite,anthropic/claude-haiku-5.5] \
//!   [LEZIONI_IT_TEXT=whisper.md LEZIONI_IT_COURSE=Fisica LEZIONI_IT_TITLE="Moto vario"] \
//!   cargo test --test openrouter_it -- --nocapture
use std::time::Instant;

use lezioni_lib::ai::{OpenRouter, Part, Request, Usage, prompts};
use lezioni_lib::lang::Lang::It;
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
        let req = Request { system: Some(prompts::system(It)), parts: vec![Part::Text(prompts::clean_transcript(It, 1, 1, &course, &title, &terms, &raw))], max_output_tokens: Some(8000), thinking: Some("LOW"), ..Default::default() };
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
            let req = Request { system: Some(prompts::system(It)), parts: vec![Part::Inline { mime: "application/pdf".into(), data: p.clone() }, Part::Text(prompts::pdf(It))], max_output_tokens: Some(32_000), thinking: Some("LOW"), ..Default::default() };
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
            system: Some(prompts::system(It)),
            parts: vec![Part::Text(prompts::generate(It, It, &course, &title, "2026-11-10", "", &terms, "", "", "", &sources))],
            schema: Some(prompts::generate_schema()),
            max_output_tokens: Some(16000),
            ..Default::default()
        };
        if let Ok(dir) = std::env::var("LEZIONI_IT_OUT") {
            let Part::Text(prompt) = &req.parts[0] else { unreachable!() };
            let dump = serde_json::json!({ "system": prompts::system(It), "prompt": prompt, "schema": prompts::generate_schema() });
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

/// Cleanup again of a piece (the "Ripulisci di nuovo la selezione" of the editor), for each
/// model: LEZIONI_IT_RECLEAN=from-to in seconds (to may be empty: up to the end).
///   LEZIONI_IT_OPENROUTER=1 OPENROUTER_API_KEY=… LEZIONI_IT_TEXT=whisper.md LEZIONI_IT_RECLEAN=0-100 \
///   cargo test --test openrouter_it reclean_piece -- --nocapture
#[tokio::test]
async fn reclean_piece() {
    let (Ok(_), Ok(range)) = (std::env::var("LEZIONI_IT_OPENROUTER"), std::env::var("LEZIONI_IT_RECLEAN")) else {
        eprintln!("LEZIONI_IT_OPENROUTER o LEZIONI_IT_RECLEAN non impostate: test saltato");
        return;
    };
    let (from, to) = range.split_once('-').expect("LEZIONI_IT_RECLEAN=da-a");
    let (from, to): (u64, Option<u64>) = (from.parse().unwrap(), if to.is_empty() { None } else { Some(to.parse().unwrap()) });
    let o = lezioni_lib::ai::Provider::OpenRouter(OpenRouter::new(std::env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY")));
    let raw = std::env::var("LEZIONI_IT_TEXT").map(|p| std::fs::read_to_string(p).unwrap()).unwrap_or_else(|_| RAW.into());
    let tmp = tempfile::tempdir().unwrap();
    let lib = lezioni_lib::library::Library::open(tmp.path()).unwrap();
    let c = lib.create_course(&std::env::var("LEZIONI_IT_COURSE").unwrap_or_else(|_| "Ricerca operativa".into()), "", &std::env::var("LEZIONI_IT_TERMS").unwrap_or_default()).unwrap();
    let l = lib.create_lesson(&c.meta.id, &std::env::var("LEZIONI_IT_TITLE").unwrap_or_else(|_| "Metodo del simplesso".into()), chrono::NaiveDate::from_ymd_opt(2026, 11, 10).unwrap()).unwrap();
    let l = lib.add_bytes(&l.meta.id, "lezione.m4a", b"x").unwrap();
    let s = &l.meta.sources[0];
    std::fs::create_dir_all(l.path.join("estratti")).unwrap();
    std::fs::write(l.path.join("estratti").join(format!("{}.whisper.md", s.file)), &raw).unwrap();
    let models = std::env::var("LEZIONI_IT_MODELS").unwrap_or_else(|_| "google/gemini-3.5-flash-lite,anthropic/claude-haiku-5.5".into());
    for m in models.split(',') {
        let t = Instant::now();
        match lezioni_lib::pipeline::reclean(&o, m, &lib, &l.meta.id, &s.id, from, to, &CancellationToken::new()).await {
            Ok((text, u, used)) => println!("\n=== {used}: {:.1} s, {} in / {} out, {:.4} $\n{text}", t.elapsed().as_secs_f64(), u.prompt_token_count, u.candidates_token_count, cost(&u)),
            Err(e) => println!("\n=== {m}: ERRORE {e:#}"),
        }
    }
}

/// Course summary for real, from lesson outputs saved by `compare_models` (LEZIONI_IT_OUT):
/// LEZIONI_IT_LESSONS=a.generato.json,b.generato.json (one lesson each, in date order).
///   LEZIONI_IT_OPENROUTER=1 OPENROUTER_API_KEY=… LEZIONI_IT_LESSONS=… \
///   cargo test --test openrouter_it course_summary -- --nocapture
#[tokio::test]
async fn course_summary() {
    let (Ok(_), Ok(files)) = (std::env::var("LEZIONI_IT_OPENROUTER"), std::env::var("LEZIONI_IT_LESSONS")) else {
        eprintln!("LEZIONI_IT_OPENROUTER o LEZIONI_IT_LESSONS non impostate: test saltato");
        return;
    };
    let o = lezioni_lib::ai::Provider::OpenRouter(OpenRouter::new(std::env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY")));
    let tmp = tempfile::tempdir().unwrap();
    let lib = lezioni_lib::library::Library::open(tmp.path()).unwrap();
    let c = lib.create_course(&std::env::var("LEZIONI_IT_COURSE").unwrap_or_else(|_| "Ricerca operativa".into()), "", &std::env::var("LEZIONI_IT_TERMS").unwrap_or_default()).unwrap();
    for (i, f) in files.split(',').enumerate() {
        let g: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(f).unwrap()).unwrap();
        let l = lib.create_lesson(&c.meta.id, g["titolo"].as_str().unwrap_or("Lezione"), chrono::NaiveDate::from_ymd_opt(2026, 10, 1 + i as u32 * 7).unwrap()).unwrap();
        let d = l.path.join("generati");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("sintesi.md"), g["sintesi"].as_str().unwrap()).unwrap();
        let study = serde_json::json!({ "glossario": g["glossario"], "flashcard": g["flashcard"], "domandeEsame": g["domande_esame"] });
        std::fs::write(d.join("studio.json"), study.to_string()).unwrap();
    }
    let models = std::env::var("LEZIONI_IT_MODELS").unwrap_or_else(|_| "anthropic/claude-haiku-5.5".into());
    for m in models.split(',') {
        let t = Instant::now();
        match lezioni_lib::course::summarize(&o, m, &lib, &c.meta.id, &std::sync::Mutex::new(()), &CancellationToken::new()).await {
            Ok((u, used)) => {
                let s = lezioni_lib::course::read(&lib, &c.meta.id).unwrap();
                println!("\n=== {used}: {:.1} s, {} in / {} out, {:.4} $\n{}\n\nDOMANDE:", t.elapsed().as_secs_f64(), u.prompt_token_count, u.candidates_token_count, cost(&u), s.summary.unwrap_or_default());
                for q in s.questions {
                    println!("- {} {:?}", q.domanda, q.lezioni);
                }
            }
            Err(e) => println!("\n=== {m}: ERRORE {e:#}"),
        }
    }
}

/// Slides linked to a recording for real: LEZIONI_IT_TEXT = the cleaned transcript (with times),
/// LEZIONI_IT_PDF_TEXT = the text read from the slides ("## Pagina N").
///   LEZIONI_IT_OPENROUTER=1 OPENROUTER_API_KEY=… LEZIONI_IT_TEXT=… LEZIONI_IT_PDF_TEXT=… \
///   cargo test --test openrouter_it slides_align -- --nocapture
#[tokio::test]
async fn slides_align() {
    let (Ok(_), Ok(text), Ok(pdf)) = (std::env::var("LEZIONI_IT_OPENROUTER"), std::env::var("LEZIONI_IT_TEXT"), std::env::var("LEZIONI_IT_PDF_TEXT")) else {
        eprintln!("LEZIONI_IT_OPENROUTER, LEZIONI_IT_TEXT o LEZIONI_IT_PDF_TEXT non impostate: test saltato");
        return;
    };
    let o = lezioni_lib::ai::Provider::OpenRouter(OpenRouter::new(std::env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY")));
    let tmp = tempfile::tempdir().unwrap();
    let lib = lezioni_lib::library::Library::open(tmp.path()).unwrap();
    let c = lib.create_course("Corso", "", "").unwrap();
    let l = lib.create_lesson(&c.meta.id, "Lezione", chrono::NaiveDate::from_ymd_opt(2026, 11, 10).unwrap()).unwrap();
    lib.add_bytes(&l.meta.id, "lezione.m4a", b"x").unwrap();
    let l = lib.add_bytes(&l.meta.id, "slide.pdf", b"x").unwrap();
    std::fs::create_dir_all(l.path.join("estratti")).unwrap();
    std::fs::write(l.path.join("estratti/lezione.m4a.md"), std::fs::read_to_string(text).unwrap()).unwrap();
    std::fs::write(l.path.join("estratti/slide.pdf.md"), std::fs::read_to_string(pdf).unwrap()).unwrap();
    let (rec, sl) = (l.meta.sources[0].id.clone(), l.meta.sources[1].id.clone());
    let models = std::env::var("LEZIONI_IT_MODELS").unwrap_or_else(|_| "google/gemini-3.5-flash-lite".into());
    for m in models.split(',') {
        let t = Instant::now();
        match lezioni_lib::slides::align(&o, m, &lib, &l.meta.id, &rec, &sl, &CancellationToken::new()).await {
            Ok((a, u)) => {
                println!("\n=== {m}: {:.1} s, {} in / {} out, {:.4} $, {} pagine", t.elapsed().as_secs_f64(), u.prompt_token_count, u.candidates_token_count, cost(&u), a.pages);
                for s in a.steps {
                    println!("  {} → pagina {}", lezioni_lib::pipeline::hms(s.at), s.page);
                }
            }
            Err(e) => println!("\n=== {m}: ERRORE {e:#}"),
        }
    }
}

/// Photos linked to the transcript by the model, then a summary that knows where they are:
/// one photo of the minimum ratio test (should land at 00:21:30), one unrelated (should not be
/// linked). Under 1 cent.
///   LEZIONI_IT_OPENROUTER=1 OPENROUTER_API_KEY=… cargo test --test openrouter_it photos_linked -- --nocapture
#[tokio::test]
async fn photos_linked() {
    if std::env::var("LEZIONI_IT_OPENROUTER").is_err() {
        eprintln!("LEZIONI_IT_OPENROUTER non impostata: test saltato");
        return;
    }
    use lezioni_lib::ai::Ai;
    use lezioni_lib::photos::{Photo, Recording, link};
    let o = lezioni_lib::ai::Provider::OpenRouter(OpenRouter::new(std::env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY")));
    let model = std::env::var("LEZIONI_IT_MODELS").unwrap_or_else(|_| "google/gemini-3.5-flash-lite".into());
    let model = model.split(',').next().unwrap();
    let photos = [
        Photo { id: "rapporto", name: "IMG_2041.jpg", title: "", description: "", text: "Test del rapporto minimo: $\\min_i b_i / a_{ij}$ con $a_{ij} > 0$; se nessun $a_{ij} > 0$ → illimitato" },
        Photo { id: "bode", name: "IMG_2042.jpg", title: "", description: "", text: "Diagramma di Bode: modulo in dB e fase di un filtro passa-basso del primo ordine" },
    ];
    let recordings = [Recording { id: "reg", name: "lezione.m4a", text: RAW }];
    let t = Instant::now();
    let (links, u) = link(&o, model, &photos, &recordings, &CancellationToken::new()).await.unwrap();
    eprintln!("aggancio ({model}, {:.1} s, {:.5} $): {links:#?}", t.elapsed().as_secs_f64(), cost(&u));
    assert_eq!((links[0].recording.as_str(), links[0].at), ("reg", Some(21 * 60 + 30)), "rapporto minimo");
    assert_eq!(links[1].at, None, "Bode non c'entra");

    // The summary, told where the photo is.
    let summary_model = std::env::var("LEZIONI_IT_SUMMARY_MODEL").unwrap_or_else(|_| "anthropic/claude-haiku-5.5".into());
    let photo_lines = "- F2 foto «IMG_2041.jpg» = F1 00:21:30\n";
    let sources = format!("[F1] lezione.m4a (registrazione)\n{RAW}\n\n[F2] IMG_2041.jpg (foto/appunti)\n{}\n", photos[0].text);
    let req = Request {
        system: Some(prompts::system(It)),
        parts: vec![Part::Text(prompts::generate(It, It, "Ricerca operativa", "Metodo del simplesso", "2026-11-10", "", "", "", "", photo_lines, &sources))],
        schema: Some(prompts::generate_schema()),
        max_output_tokens: Some(16000),
        ..Default::default()
    };
    let t = Instant::now();
    let a = o.generate(&summary_model, &req, &CancellationToken::new()).await.unwrap();
    let v: serde_json::Value = serde_json::from_str(&a.text).unwrap();
    let sintesi = v["sintesi"].as_str().unwrap();
    eprintln!("--- sintesi ({summary_model}, {:.1} s, {:.5} $):\n{sintesi}", t.elapsed().as_secs_f64(), cost(&a.usage));
    assert!(sintesi.contains("](F2)"), "la foto non è nella sintesi");
}

/// An English lecture: cleanup in English (no translation, times and pause kept), the PDF read
/// with the English prompt (`TIPO:` line and `## Page N`, text in its own language) and a summary
/// in Italian (materials in Italian).
///   LEZIONI_IT_OPENROUTER=1 OPENROUTER_API_KEY=… [LEZIONI_IT_PDF=slide.pdf] cargo test --test openrouter_it english_lecture -- --nocapture
#[tokio::test]
async fn english_lecture() {
    use lezioni_lib::lang::Lang::En;
    if std::env::var("LEZIONI_IT_OPENROUTER").is_err() {
        return;
    }
    let o = OpenRouter::new(std::env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY"));
    let model = std::env::var("LEZIONI_IT_MODELS").unwrap_or_else(|_| "google/gemini-3.5-flash-lite".into());
    let model = model.split(',').next().unwrap();
    let cancel = CancellationToken::new();
    let raw = "[00:20:00] So, um, where were we? Yeah, so, the standard form. So we have the problem max z equals c transpose x, with a x equals b and x greater or equal to zero. Okay.

[00:20:41] So, the simplex method, I mean, the idea is that we start from a basic feasible solution, which is a vertex of the polyhedron, and we move, uh, to an adjacent vertex only if the objective improves. So how do I choose the entering variable? I look at the reduced costs and take the most negative one.

[00:21:30] *(pause until 00:25:00)*

[00:25:00] And who leaves? The leaving variable comes from the minimum ratio test, b i over a i j, only on the rows where a i j is positive. If no a i j is positive, the problem is unbounded. Question?

[00:25:40] Professor, what if two ratios are equal? Good question, then the solution is degenerate and we can cycle. To avoid it we use Bland's rule, which we'll see next time.";
    let req = Request { system: Some(prompts::system(En)), parts: vec![Part::Text(prompts::clean_transcript(En, 1, 1, "Operations research", "The simplex method", "Bland", raw))], max_output_tokens: Some(8000), thinking: Some("LOW"), ..Default::default() };
    let clean = o.generate(model, &req, &cancel).await.unwrap();
    eprintln!("--- cleanup ({:.5} $):\n{}", cost(&clean.usage), clean.text);
    for t in ["[00:20:00]", "[00:20:41]", "[00:25:00]", "[00:25:40]"] {
        assert!(clean.text.contains(t), "time {t} lost");
    }
    assert!(clean.text.contains("pause until 00:25:00"), "pause line lost");
    assert!(clean.text.contains("simplex") && !clean.text.contains("simplesso"), "translated?");

    let mut pdf_text = String::new();
    if let Ok(p) = std::env::var("LEZIONI_IT_PDF") {
        let req = Request { system: Some(prompts::system(En)), parts: vec![Part::Inline { mime: "application/pdf".into(), data: std::fs::read(p).unwrap() }, Part::Text(prompts::pdf(En))], max_output_tokens: Some(32_000), thinking: Some("LOW"), ..Default::default() };
        let a = o.generate(model, &req, &cancel).await.unwrap();
        eprintln!("--- PDF ({:.5} $):\n{}", cost(&a.usage), a.text.chars().take(1500).collect::<String>());
        let first = a.text.trim_start().lines().next().unwrap_or("").to_lowercase();
        assert!(first.trim_matches(['*', '_']).starts_with("tipo:"), "first line: {first}");
        assert!(a.text.contains("## Page 1"), "no ## Page N headings");
        pdf_text = a.text;
    }

    let sources = format!("[F1] lecture.m4a (recording)\n{}\n\n[F2] slides.pdf (PDF)\n{pdf_text}\n", clean.text);
    let req = Request {
        system: Some(prompts::system(It)),
        parts: vec![Part::Text(prompts::generate(It, En, "Operations research", "The simplex method", "2026-11-13", "", "", "", "", "", &sources))],
        schema: Some(prompts::generate_schema()),
        max_output_tokens: Some(16000),
        ..Default::default()
    };
    let a = o.generate(model, &req, &cancel).await.unwrap();
    let v: serde_json::Value = serde_json::from_str(&a.text).unwrap();
    let summary = v["sintesi"].as_str().unwrap_or("");
    eprintln!("--- summary in Italian ({:.5} $):\n{summary}\nflashcard: {}", cost(&a.usage), v["flashcard"][0]);
    let it_words = [" il ", " la ", " di ", " che "].iter().filter(|w| summary.contains(*w)).count();
    assert!(it_words >= 3, "the summary is not in Italian");

    // Materials in English too (the main path for an English user), with the slides linked.
    let req = Request {
        system: Some(prompts::system(En)),
        parts: vec![Part::Text(prompts::generate(En, En, "Operations research", "The simplex method", "2026-11-13", "", "", "", "- F2 page 1 = F1 00:20:41\n", "", &sources))],
        schema: Some(prompts::generate_schema()),
        max_output_tokens: Some(16000),
        ..Default::default()
    };
    let a = o.generate(model, &req, &cancel).await.unwrap();
    let v: serde_json::Value = serde_json::from_str(&a.text).unwrap();
    let summary = v["sintesi"].as_str().unwrap_or("");
    eprintln!("--- summary in English ({:.5} $):\n{summary}\ntitle: {}", cost(&a.usage), v["titolo"]);
    let en_words = [" the ", " of ", " is ", " and "].iter().filter(|w| summary.contains(*w)).count();
    assert!(en_words >= 3, "the summary is not in English");
}
