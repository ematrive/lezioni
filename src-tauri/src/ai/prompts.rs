//! Prompts, in the language the model must write in (see `crate::lang`). The ones on a source
//! (transcription, cleanup, photos, PDFs) are in the lesson's language and never translate; the
//! ones on the materials (summary, course, chat) are in the materials' language.
//! Some words are read back by the app and must not change: `TIPO: slide|documento` and
//! `[SILENZIO]` are fixed in both languages; `## Pagina N`/`## Page N` and the pause lines are
//! read in both forms.
use serde_json::{Value, json};

use crate::lang::Lang;

pub fn system(l: Lang) -> String {
    l.pick(
        "Sei un assistente per studenti universitari. Lavori su materiale di lezione (registrazioni, appunti, slide). \
Scrivi in italiano corretto, preciso e chiaro. Non inventare contenuti che non sono nelle fonti: se qualcosa non si capisce, scrivi [incomprensibile]. \
Scrivi le formule in LaTeX tra $...$ (in linea) o $$...$$ (a blocco).",
        "You are an assistant for university students. You work on lecture material (recordings, notes, slides). \
Write in correct, precise and clear English. Do not invent content that is not in the sources: if something cannot be understood, write [unintelligible]. \
Write formulas in LaTeX between $...$ (inline) or $$...$$ (display).",
    )
    .to_string()
}

/// The rules shared by the two transcriptions of audio by the model; `time` says the format.
fn transcribe_rules(l: Lang, time: &str) -> String {
    match l {
        Lang::It => format!(
            "Produci una trascrizione pulita ma fedele:\n\
- togli intercalari, ripetizioni e false partenze (ehm, cioè, allora…), senza riassumere né tagliare contenuti;\n\
- dividi in paragrafi; ogni paragrafo inizia con il tempo in cui comincia, nel formato {time};\n\
- quando cambia argomento metti un titolo Markdown (### Titolo);\n\
- se interviene uno studente scrivi **Studente:** all'inizio della battuta;\n\
- formule e simboli dettati a voce scrivili in LaTeX;\n\
- se il docente fa riferimento alla lavagna o a una slide, annotalo in corsivo, ad esempio *(alla lavagna: grafico della funzione di costo)*.\n"
        ),
        Lang::En => format!(
            "Produce a clean but faithful transcript, in English as it is spoken (do not translate):\n\
- remove fillers, repetitions and false starts (um, uh, you know, so…), without summarising or cutting content;\n\
- split it into paragraphs; each paragraph starts with the time it begins, in the format {time};\n\
- when the topic changes, add a Markdown heading (### Heading);\n\
- when a student speaks, write **Student:** at the start of their turn;\n\
- write formulas and symbols dictated aloud in LaTeX;\n\
- when the lecturer refers to the board or to a slide, note it in italics, for example *(on the board: graph of the cost function)*.\n"
        ),
    }
}

/// Transcription of one time window of a recording; times are absolute in the file.
pub fn transcribe_window(l: Lang, from: &str, to: &str, first: bool, last: bool) -> String {
    let rules = transcribe_rules(l, l.pick("[hh:mm:ss] riferito all'inizio del file", "[hh:mm:ss] from the start of the file"));
    match l {
        Lang::It => {
            let range = match (first, last) {
                (true, true) => "Trascrivi tutta la registrazione.".to_string(),
                (_, true) => format!("Trascrivi la registrazione da {from} fino alla fine."),
                _ => format!("Trascrivi SOLO la parte della registrazione da {from} a {to}: non andare oltre {to}."),
            };
            format!(
                "{range}\nÈ la registrazione di una lezione universitaria. {rules}\
Rispondi solo con la trascrizione in Markdown, senza introduzioni. Se in questo intervallo non si parla, rispondi solo [SILENZIO]."
            )
        }
        Lang::En => {
            let range = match (first, last) {
                (true, true) => "Transcribe the whole recording.".to_string(),
                (_, true) => format!("Transcribe the recording from {from} to the end."),
                _ => format!("Transcribe ONLY the part of the recording from {from} to {to}: do not go past {to}."),
            };
            format!(
                "{range}\nIt is the recording of a university lecture. {rules}\
Answer only with the transcript in Markdown, with no introduction. If nobody speaks in this interval, answer only [SILENZIO]."
            )
        }
    }
}

/// One part of a recording split by the app; times relative to the part (shifted afterwards).
pub fn transcribe_clip(l: Lang, n: usize, of: usize, starts_at: &str) -> String {
    match l {
        Lang::It => {
            let which = if of > 1 { format!("Questa è la parte {n} di {of} della registrazione di una lezione universitaria (comincia a {starts_at} della lezione).") } else { "È la registrazione di una lezione universitaria.".into() };
            let rules = transcribe_rules(l, "[mm:ss] dall'inizio di QUESTO file audio");
            format!(
                "{which} Trascrivila tutta. {rules}\
Se la parte comincia o finisce a metà frase, trascrivi comunque quello che si sente. Rispondi solo con la trascrizione in Markdown, senza introduzioni. Se non si parla, rispondi solo [SILENZIO]."
            )
        }
        Lang::En => {
            let which = if of > 1 { format!("This is part {n} of {of} of the recording of a university lecture (it starts at {starts_at} of the lecture).") } else { "It is the recording of a university lecture.".into() };
            let rules = transcribe_rules(l, "[mm:ss] from the start of THIS audio file");
            format!(
                "{which} Transcribe all of it. {rules}\
If the part starts or ends in the middle of a sentence, transcribe what can be heard anyway. Answer only with the transcript in Markdown, with no introduction. If nobody speaks, answer only [SILENZIO]."
            )
        }
    }
}

/// The course's terms as a line of the prompt (empty when there are none).
fn terms_line(l: Lang, terms: &str) -> String {
    let t = terms.split([',', ';', '\n']).map(str::trim).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(", ");
    if t.is_empty() {
        String::new()
    } else {
        l.pick(format!("Termini e nomi ricorrenti del corso, scritti correttamente: {t}.\n"), format!("Recurring terms and names of the course, spelled correctly: {t}.\n"))
    }
}

/// The cleanup rules shared by a whole part and a piece; `headings` tells what to do with them.
fn clean_rules(l: Lang, headings: &str, unclear: &str) -> String {
    match l {
        Lang::It => format!(
            "- togli intercalari, ripetizioni e false partenze (ehm, cioè, allora…);\n\
- correggi le parole riconosciute male quando il contesto rende chiaro cosa è stato detto (termini tecnici, nomi, formule); non inventare quello che non si capisce{unclear};\n\
- ogni paragrafo inizia con un tempo [hh:mm:ss]: mantienili esattamente come sono. Puoi unire due paragrafi brevi tenendo il tempo del primo, ma non scrivere tempi nuovi;\n\
- lascia com'è ogni paragrafo *(pausa fino a …)*: è una pausa della lezione;\n\
- {headings};\n\
- se dal contesto è chiaro che interviene uno studente, scrivi **Studente:** all'inizio della sua battuta;\n\
- formule e simboli dettati a voce scrivili in LaTeX ($...$).\n"
        ),
        Lang::En => format!(
            "- keep it in English as it was spoken: do not translate;\n\
- remove fillers, repetitions and false starts (um, uh, you know, so…);\n\
- fix words that were misrecognised when the context makes clear what was said (technical terms, names, formulas); do not invent what cannot be understood{unclear};\n\
- each paragraph starts with a time [hh:mm:ss]: keep them exactly as they are. You may join two short paragraphs keeping the time of the first, but do not write new times;\n\
- leave every *(pause until …)* paragraph as it is: it is a break in the lecture;\n\
- {headings};\n\
- when the context makes clear that a student is speaking, write **Student:** at the start of their turn;\n\
- write formulas and symbols dictated aloud in LaTeX ($...$).\n"
        ),
    }
}

/// Cleanup of Whisper's text (one part of the recording). Text only: small, cheap requests.
pub fn clean_transcript(l: Lang, n: usize, of: usize, course: &str, title: &str, terms: &str, raw: &str) -> String {
    let terms = terms_line(l, terms);
    match l {
        Lang::It => {
            let which = if of > 1 { format!("la parte {n} di {of} della trascrizione") } else { "la trascrizione".into() };
            let rules = clean_rules(l, "quando cambia argomento metti un titolo Markdown (### Titolo) prima del paragrafo", "");
            format!(
                "Questa è {which} automatica (fatta da un riconoscimento vocale) di una lezione universitaria del corso «{course}», argomento «{title}».\n{terms}\
Ripuliscila senza riassumere e senza togliere contenuti:\n{rules}\
Rispondi solo con la trascrizione ripulita in Markdown, senza introduzioni.\n\nTrascrizione:\n{raw}"
            )
        }
        Lang::En => {
            let which = if of > 1 { format!("part {n} of {of} of the automatic transcript") } else { "the automatic transcript".into() };
            let rules = clean_rules(l, "when the topic changes, add a Markdown heading (### Heading) before the paragraph", "");
            format!(
                "This is {which} (made by speech recognition) of a university lecture of the course “{course}”, topic “{title}”.\n{terms}\
Clean it up without summarising and without removing content:\n{rules}\
Answer only with the cleaned transcript in Markdown, with no introduction.\n\nTranscript:\n{raw}"
            )
        }
    }
}

/// Cleanup again of a piece of Whisper's text, chosen by the user in the middle of a transcript:
/// no headings (the piece sits inside a section), same rules on times and terms.
pub fn clean_fragment(l: Lang, course: &str, title: &str, terms: &str, raw: &str) -> String {
    let terms = terms_line(l, terms);
    match l {
        Lang::It => {
            let rules = clean_rules(l, "non mettere titoli: il pezzo sta in mezzo a una sezione", ": se un passaggio resta incomprensibile scrivi [incomprensibile]");
            format!(
                "Questo è un pezzo della trascrizione automatica (fatta da un riconoscimento vocale) di una lezione universitaria del corso «{course}», argomento «{title}». \
Lo studente lo vuole ripulito di nuovo perché la versione precedente aveva errori.\n{terms}\
Ripuliscilo senza riassumere e senza togliere contenuti:\n{rules}\
Rispondi solo con il pezzo ripulito in Markdown, senza introduzioni.\n\nPezzo:\n{raw}"
            )
        }
        Lang::En => {
            let rules = clean_rules(l, "do not add headings: the piece sits in the middle of a section", ": if a passage stays unintelligible write [unintelligible]");
            format!(
                "This is a piece of the automatic transcript (made by speech recognition) of a university lecture of the course “{course}”, topic “{title}”. \
The student wants it cleaned up again because the previous version had errors.\n{terms}\
Clean it up without summarising and without removing content:\n{rules}\
Answer only with the cleaned piece in Markdown, with no introduction.\n\nPiece:\n{raw}"
            )
        }
    }
}

/// A photo or a page of notes. `l`: the lesson's language, for the descriptions (the text in
/// the photo stays as it is).
pub fn image(l: Lang) -> String {
    l.pick(
        "Questa è una foto o una pagina di appunti di una lezione (scritti a mano, dal tablet, o una lavagna). \
Trascrivi tutto il contenuto in Markdown, nell'ordine di lettura e nella lingua in cui è scritto: titoli, elenchi, tabelle, formule in LaTeX. \
Descrivi grafici, schemi e disegni in corsivo tra parentesi, con assi, etichette e relazioni, così che si capiscano senza vederli. \
Se è una slide o una pagina stampata con sopra appunti a mano, evidenziature o frecce, trascrivi anche quelli, in corsivo tra parentesi: *(nota a mano: …)*. Se una parte è illeggibile scrivi [illeggibile]. Rispondi solo con il contenuto, senza introduzioni.",
        "This is a photo or a page of notes from a lecture (handwritten, from a tablet, or a blackboard). \
Transcribe all of its content in Markdown, in reading order and in the language it is written in: headings, lists, tables, formulas in LaTeX. \
Describe graphs, diagrams and drawings in italics between brackets, with axes, labels and relations, so that they can be understood without seeing them. \
If it is a slide or a printed page with handwritten notes, highlights or arrows on it, transcribe those too, in italics between brackets: *(handwritten: …)*. If a part is illegible write [illegible]. Answer only with the content, with no introduction.",
    )
    .to_string()
}

/// A PDF. The first line `TIPO: slide|documento` is fixed in both languages (`pipeline::pdf_kind`).
pub fn pdf(l: Lang) -> String {
    l.pick(
        "Questo è un PDF di una lezione (slide, dispense o appunti). Nella prima riga scrivi solo TIPO: slide se sono le pagine \
di una presentazione proiettata in aula (una slide per pagina), altrimenti TIPO: documento. Poi trascrivi il contenuto in Markdown, pagina per pagina, nella lingua in cui è scritto, \
con un titolo ## Pagina N per ogni pagina. Mantieni titoli, elenchi e tabelle; formule in LaTeX; descrivi grafici e schemi in corsivo tra parentesi. \
Se sopra la pagina ci sono appunti scritti a mano, evidenziature, frecce o sottolineature, trascrivili nel punto in cui sono, in corsivo tra parentesi: *(nota a mano: …)*; per un'evidenziatura scrivi cosa è evidenziato. \
Se è un testo lungo e già digitale, riportalo fedelmente. Rispondi solo con il contenuto.",
        "This is a PDF from a lecture (slides, handouts or notes). On the first line write only TIPO: slide if these are the pages \
of a presentation projected in class (one slide per page), otherwise TIPO: documento (these two lines exactly, in Italian). Then transcribe the content in Markdown, page by page, in the language it is written in, \
with a heading ## Page N for each page. Keep headings, lists and tables; formulas in LaTeX; describe graphs and diagrams in italics between brackets. \
If there are handwritten notes, highlights, arrows or underlines on the page, transcribe them where they are, in italics between brackets: *(handwritten: …)*; for a highlight, write what is highlighted. \
If it is a long text that is already digital, reproduce it faithfully. Answer only with the content.",
    )
    .to_string()
}

/// `l`: the materials' language; `spoken`: the lesson's. `marks`: the student's bookmarks, one
/// per line ("- [F1 00:12:30] importante: nota"). `slides`: when the recording talks about each
/// page of the slides ("- F2 pagina 3 = F1 00:05:10").
#[allow(clippy::too_many_arguments)]
pub fn generate(l: Lang, spoken: Lang, course: &str, title: &str, date: &str, notes: &str, terms: &str, marks: &str, slides: &str, photos: &str, sources: &str) -> String {
    let terms = terms_line(l, terms);
    let some = |s: &str| !s.trim().is_empty();
    match l {
        Lang::It => {
            let notes = if some(notes) { format!("\nNote dello studente sulla lezione:\n{notes}\n") } else { String::new() };
            let marks = if some(marks) {
                format!(
                    "\nPunti segnati dallo studente mentre riascoltava la registrazione. Nella sintesi dai più spazio ai punti «importante» \
e spiega con più calma e con un esempio i punti «non capito»; le note dicono cosa lo studente voleva ricordare:\n{marks}"
                )
            } else {
                String::new()
            };
            let slides = if some(slides) { format!("\nSlide e registrazione: quando il docente parla di ciascuna pagina delle slide. Segui l'ordine delle slide e, dove serve, cita insieme pagina e minuto:\n{slides}") } else { String::new() };
            let photos = if some(photos) {
                format!(
                    "\nFoto e registrazione: a che minuto si riferisce ogni foto. Dove una foto aiuta a capire (uno schema, un disegno, un esercizio alla lavagna), \
inseriscila nella sintesi su una riga a sé, nel punto giusto, così: ![titolo breve](F3) con il codice della foto. Ogni foto al massimo una volta:\n{photos}"
                )
            } else {
                String::new()
            };
            let language = if spoken != l {
                format!("\nLa lezione è in {}: scrivi comunque tutto in italiano. La prima volta che compare un termine tecnico, riporta tra parentesi anche il termine originale.\n", spoken.name_in(l))
            } else {
                String::new()
            };
            format!(
                "Corso: {course}\nLezione: {title} ({date})\n{terms}{notes}{marks}{slides}{photos}{language}\n\
Qui sotto ci sono tutte le fonti della lezione, ognuna con un codice [F1], [F2]…: la trascrizione della registrazione, appunti, foto della lavagna, slide. \
Collegale tra loro: gli appunti e le slide spesso spiegano o correggono ciò che si sente nella registrazione, e viceversa.\n\n\
Produci, in italiano:\n\
1. **sintesi**: una sintesi completa e ordinata della lezione in Markdown (titoli ##/###, elenchi, formule LaTeX, esempi svolti se ci sono). \
Deve permettere di studiare senza riascoltare la lezione. Alla fine di ogni paragrafo indica le fonti tra parentesi quadre, con il minuto per la registrazione, ad esempio [F1 00:12:30] o [F3].\n\
2. **mappa**: una mappa concettuale ad albero: radice = argomento della lezione, poi i concetti principali, poi i sotto-concetti. Etichette brevi (massimo 6 parole). Per ogni nodo, in rif, il punto delle fonti dove se ne parla, come nella sintesi ma senza parentesi: F1 00:12:30 oppure F3.\n\
3. **glossario**: i termini tecnici con una definizione breve.\n\
4. **flashcard**: 10-20 domande e risposte brevi per il ripasso.\n\
5. **domande_esame**: 4-8 domande come all'esame (teoria ed esercizi), ciascuna con una traccia di risposta.\n\
6. **titolo**: un titolo breve e specifico per la lezione.\n\n\
FONTI:\n\n{sources}"
            )
        }
        Lang::En => {
            let notes = if some(notes) { format!("\nThe student's notes on the lecture:\n{notes}\n") } else { String::new() };
            let marks = if some(marks) {
                format!(
                    "\nPoints marked by the student while listening to the recording again. In the summary give more room to the “important” points \
and explain the “not understood” points more slowly and with an example; the notes say what the student wanted to remember:\n{marks}"
                )
            } else {
                String::new()
            };
            let slides = if some(slides) { format!("\nSlides and recording: when the lecturer talks about each page of the slides. Follow the order of the slides and, where useful, cite page and minute together:\n{slides}") } else { String::new() };
            let photos = if some(photos) {
                format!(
                    "\nPhotos and recording: which minute each photo refers to. Where a photo helps to understand (a diagram, a drawing, an exercise on the board), \
put it in the summary on a line of its own, in the right place, like this: ![short title](F3) with the photo's code. Each photo at most once:\n{photos}"
                )
            } else {
                String::new()
            };
            let language = if spoken != l {
                format!("\nThe lecture is in {}: write everything in English anyway. The first time a technical term appears, also give the original term in brackets.\n", spoken.name_in(l))
            } else {
                String::new()
            };
            format!(
                "Course: {course}\nLecture: {title} ({date})\n{terms}{notes}{marks}{slides}{photos}{language}\n\
Below are all the sources of the lecture, each with a code [F1], [F2]…: the transcript of the recording, notes, photos of the board, slides. \
Connect them: notes and slides often explain or correct what is heard in the recording, and vice versa.\n\n\
Produce, in English:\n\
1. **sintesi**: a complete, well-ordered summary of the lecture in Markdown (## / ### headings, lists, LaTeX formulas, worked examples if there are any). \
It must allow studying without listening to the lecture again. At the end of each paragraph give the sources in square brackets, with the minute for the recording, for example [F1 00:12:30] or [F3].\n\
2. **mappa**: a tree-shaped concept map: root = topic of the lecture, then the main concepts, then the sub-concepts. Short labels (at most 6 words). For each node, in rif, the place in the sources where it is discussed, as in the summary but without brackets: F1 00:12:30 or F3.\n\
3. **glossario**: the technical terms with a short definition.\n\
4. **flashcard**: 10-20 short questions and answers for revision.\n\
5. **domande_esame**: 4-8 exam-style questions (theory and exercises), each with an outline of the answer.\n\
6. **titolo**: a short, specific title for the lecture.\n\n\
SOURCES:\n\n{sources}"
            )
        }
    }
}

/// Fixed depth instead of `$ref` recursion: root + 3 levels is plenty for a lesson.
/// `figli` is required where it exists: with it optional, gemini-3.5-flash-lite answered with
/// the root alone 3 times out of 3 (OpenRouter, Nov 2026); required, 4 full trees out of 4.
fn node(depth: u32) -> Value {
    let mut props = json!({ "etichetta": { "type": "string" }, "rif": { "type": "string" } });
    if depth == 0 {
        return json!({ "type": "object", "properties": props, "required": ["etichetta"] });
    }
    props["figli"] = json!({ "type": "array", "items": node(depth - 1) });
    json!({ "type": "object", "properties": props, "required": ["etichetta", "figli"] })
}

/// Summary of a whole course from the summaries of its lessons ([L1], [L2]…), in `l`.
pub fn course_summary(l: Lang, course: &str, terms: &str, lessons: usize, text: &str) -> String {
    let terms = terms_line(l, terms);
    match l {
        Lang::It => format!(
            "Corso: {course}\n{terms}\
Qui sotto ci sono le sintesi delle {lessons} lezioni del corso, in ordine di data, ognuna con un codice [L1], [L2]…, e le domande d'esame proposte per ciascuna.\n\n\
Produci, in italiano:\n\
1. **sintesi**: la sintesi del corso intero in Markdown, per studiare per l'esame. Organizzala per argomenti (titoli ##/###), non lezione per lezione: \
riunisci quello che è stato spiegato in più lezioni, mostra come gli argomenti si collegano e quali concetti servono per capirne altri. \
Tieni formule (LaTeX), definizioni e risultati importanti; tralascia aneddoti e dettagli organizzativi. \
Alla fine di ogni paragrafo indica tra parentesi quadre le lezioni da cui viene, ad esempio [L3] o [L2, L5]; usa solo questi codici. \
Non riportare note sulle fonti delle singole lezioni (slide mancanti, passaggi incomprensibili della registrazione).\n\
2. **domande**: 8-15 domande d'esame sul corso intero. Parti dalle domande proposte per le lezioni: unisci quelle simili, preferisci quelle che tornano in più lezioni \
e quelle che collegano argomenti diversi. Ognuna con una traccia di risposta e i codici delle lezioni da ripassare (\"lezioni\": [\"L2\", \"L5\"]).\n\n\
LEZIONI:\n\n{text}"
        ),
        Lang::En => format!(
            "Course: {course}\n{terms}\
Below are the summaries of the {lessons} lectures of the course, in date order, each with a code [L1], [L2]…, and the exam questions proposed for each.\n\n\
Produce, in English:\n\
1. **sintesi**: the summary of the whole course in Markdown, to study for the exam. Organise it by topic (## / ### headings), not lecture by lecture: \
bring together what was explained across several lectures, show how the topics connect and which concepts are needed to understand others. \
Keep formulas (LaTeX), definitions and important results; leave out anecdotes and organisational details. \
At the end of each paragraph give in square brackets the lectures it comes from, for example [L3] or [L2, L5]; use only these codes. \
Do not report notes about the sources of single lectures (missing slides, unintelligible passages of the recording).\n\
2. **domande**: 8-15 exam questions on the whole course. Start from the questions proposed for the lectures: merge similar ones, prefer those that come back in several lectures \
and those that connect different topics. Each with an outline of the answer and the codes of the lectures to revise (\"lezioni\": [\"L2\", \"L5\"]).\n\n\
LECTURES:\n\n{text}"
        ),
    }
}

/// When the teacher moves from one slide to the next, from the transcript's times. Only data
/// comes back: the prompt stays in Italian whatever the lesson's language.
pub fn align_slides(pages: &str, transcript: &str) -> String {
    format!(
        "Qui sotto ci sono le slide che il docente proiettava durante la lezione (una riga per pagina: numero e inizio del testo) \
e la trascrizione della lezione, con i tempi [hh:mm:ss] all'inizio dei paragrafi. Slide e trascrizione possono essere in un'altra lingua.\n\
Indica i momenti in cui il docente passa a una slide: per ogni passaggio il tempo (quello del paragrafo in cui comincia a parlarne) e il numero della pagina. \
In ordine di tempo; il docente può saltare pagine o tornare indietro. Se per un tratto non usa le slide, non mettere passaggi in quel tratto. \
Non inventare: metti una slide solo se il parlato ne tratta davvero il contenuto.\n\nSLIDE:\n{pages}\nTRASCRIZIONE:\n{transcript}"
    )
}

/// Where each photo belongs. The prompt stays in Italian (data comes back); `l` is the language
/// of the reasons, which the student reads.
pub fn link_photos(l: Lang, photos: &str, transcripts: &str) -> String {
    format!(
        "Qui sotto ci sono le foto scattate durante una lezione (lavagna, appunti, slide fotografate), ognuna con un codice FOTO1, FOTO2… \
e il testo che se ne è letto, e le trascrizioni delle registrazioni della lezione (REG1, REG2…), con i tempi [hh:mm:ss] all'inizio dei paragrafi.\n\
Per ogni foto indica la registrazione e il tempo del paragrafo in cui il docente comincia a parlare di quello che c'è nella foto (o lo scrive alla lavagna). \
Il titolo e la descrizione dati dallo studente, se ci sono, sono le indicazioni più affidabili. \
Non inventare: se una foto non corrisponde a nessun punto delle registrazioni, lascia vuoti registrazione e inizio e scrivi in motivo, in poche parole e in {}, perché. \
Una voce per ogni foto.\n\nFOTO:\n{photos}TRASCRIZIONI:\n{transcripts}",
        l.name_in(Lang::It)
    )
}

pub fn link_photos_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "foto": { "type": "array", "items": { "type": "object", "properties": {
            "codice": { "type": "string" }, "registrazione": { "type": "string" }, "inizio": { "type": "string" }, "motivo": { "type": "string" }
        }, "required": ["codice", "registrazione", "inizio", "motivo"] } } },
        "required": ["foto"]
    })
}

pub fn align_slides_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "passaggi": { "type": "array", "items": { "type": "object", "properties": {
            "inizio": { "type": "string" }, "pagina": { "type": "integer" }
        }, "required": ["inizio", "pagina"] } } },
        "required": ["passaggi"]
    })
}

pub fn course_summary_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "sintesi": { "type": "string" },
            "domande": { "type": "array", "items": { "type": "object", "properties": {
                "domanda": { "type": "string" }, "traccia": { "type": "string" }, "lezioni": { "type": "array", "items": { "type": "string" } }
            }, "required": ["domanda", "traccia", "lezioni"] } }
        },
        "required": ["sintesi", "domande"]
    })
}

pub fn generate_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "titolo": { "type": "string" },
            "sintesi": { "type": "string" },
            "mappa": node(3),
            "glossario": { "type": "array", "items": { "type": "object", "properties": { "termine": { "type": "string" }, "definizione": { "type": "string" } }, "required": ["termine", "definizione"] } },
            "flashcard": { "type": "array", "items": { "type": "object", "properties": { "domanda": { "type": "string" }, "risposta": { "type": "string" } }, "required": ["domanda", "risposta"] } },
            "domande_esame": { "type": "array", "items": { "type": "object", "properties": { "domanda": { "type": "string" }, "traccia": { "type": "string" } }, "required": ["domanda", "traccia"] } }
        },
        "required": ["titolo", "sintesi", "mappa", "glossario", "flashcard", "domande_esame"]
    })
}
