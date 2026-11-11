//! Prompts (in Italian: the lessons are Italian, and so must be the answers).
use serde_json::{Value, json};

pub const SYSTEM: &str = "Sei un assistente per studenti universitari di Ingegneria gestionale. Lavori su materiale di lezione (registrazioni, appunti, slide). \
Scrivi in italiano corretto, preciso e chiaro. Non inventare contenuti che non sono nelle fonti: se qualcosa non si capisce, scrivi [incomprensibile]. \
Scrivi le formule in LaTeX tra $...$ (in linea) o $$...$$ (a blocco).";

/// Transcription of one time window of a recording; times are absolute in the file.
pub fn transcribe_window(from: &str, to: &str, first: bool, last: bool) -> String {
    let range = match (first, last) {
        (true, true) => "Trascrivi tutta la registrazione.".to_string(),
        (_, true) => format!("Trascrivi la registrazione da {from} fino alla fine."),
        _ => format!("Trascrivi SOLO la parte della registrazione da {from} a {to}: non andare oltre {to}."),
    };
    format!(
        "{range}\n\
È la registrazione di una lezione universitaria. Produci una trascrizione pulita ma fedele:\n\
- togli intercalari, ripetizioni e false partenze (ehm, cioè, allora…), senza riassumere né tagliare contenuti;\n\
- dividi in paragrafi; ogni paragrafo inizia con il tempo in cui comincia, nel formato [hh:mm:ss] riferito all'inizio del file;\n\
- quando cambia argomento metti un titolo Markdown (### Titolo);\n\
- se interviene uno studente scrivi **Studente:** all'inizio della battuta;\n\
- formule e simboli dettati a voce scrivili in LaTeX;\n\
- se il docente fa riferimento alla lavagna o a una slide, annotalo in corsivo, ad esempio *(alla lavagna: grafico della funzione di costo)*.\n\
Rispondi solo con la trascrizione in Markdown, senza introduzioni. Se in questo intervallo non si parla, rispondi solo [SILENZIO]."
    )
}

/// One part of a recording split by the app; times relative to the part (shifted afterwards).
pub fn transcribe_clip(n: usize, of: usize, starts_at: &str) -> String {
    let which = if of > 1 { format!("Questa è la parte {n} di {of} della registrazione di una lezione universitaria (comincia a {starts_at} della lezione).") } else { "È la registrazione di una lezione universitaria.".into() };
    format!(
        "{which} Trascrivila tutta. Produci una trascrizione pulita ma fedele:\n\
- togli intercalari, ripetizioni e false partenze (ehm, cioè, allora…), senza riassumere né tagliare contenuti;\n\
- dividi in paragrafi; ogni paragrafo inizia con il tempo in cui comincia, nel formato [mm:ss] dall'inizio di QUESTO file audio;\n\
- quando cambia argomento metti un titolo Markdown (### Titolo);\n\
- se interviene uno studente scrivi **Studente:** all'inizio della battuta;\n\
- formule e simboli dettati a voce scrivili in LaTeX;\n\
- se il docente fa riferimento alla lavagna o a una slide, annotalo in corsivo, ad esempio *(alla lavagna: grafico della funzione di costo)*.\n\
Se la parte comincia o finisce a metà frase, trascrivi comunque quello che si sente. Rispondi solo con la trascrizione in Markdown, senza introduzioni. Se non si parla, rispondi solo [SILENZIO]."
    )
}

/// The course's terms as a line of the prompt (empty when there are none).
fn terms_line(terms: &str) -> String {
    let t = terms.split([',', ';', '\n']).map(str::trim).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(", ");
    if t.is_empty() { String::new() } else { format!("Termini e nomi ricorrenti del corso, scritti correttamente: {t}.\n") }
}

/// Cleanup of Whisper's text (one part of the recording). Text only: small, cheap requests.
pub fn clean_transcript(n: usize, of: usize, course: &str, title: &str, terms: &str, raw: &str) -> String {
    let which = if of > 1 { format!("la parte {n} di {of} della trascrizione") } else { "la trascrizione".into() };
    let terms = terms_line(terms);
    format!(
        "Questa è {which} automatica (fatta da un riconoscimento vocale) di una lezione universitaria del corso «{course}», argomento «{title}».\n{terms}\
Ripuliscila senza riassumere e senza togliere contenuti:\n\
- togli intercalari, ripetizioni e false partenze (ehm, cioè, allora…);\n\
- correggi le parole riconosciute male quando il contesto rende chiaro cosa è stato detto (termini tecnici, nomi, formule); non inventare quello che non si capisce;\n\
- ogni paragrafo inizia con un tempo [hh:mm:ss]: mantienili esattamente come sono. Puoi unire due paragrafi brevi tenendo il tempo del primo, ma non scrivere tempi nuovi;\n\
- quando cambia argomento metti un titolo Markdown (### Titolo) prima del paragrafo;\n\
- se dal contesto è chiaro che interviene uno studente, scrivi **Studente:** all'inizio della sua battuta;\n\
- formule e simboli dettati a voce scrivili in LaTeX ($...$).\n\
Rispondi solo con la trascrizione ripulita in Markdown, senza introduzioni.\n\nTrascrizione:\n{raw}"
    )
}

pub const IMAGE: &str = "Questa è una foto o una pagina di appunti di una lezione (scritti a mano, dal tablet, o una lavagna). \
Trascrivi tutto il contenuto in Markdown, nell'ordine di lettura: titoli, elenchi, tabelle, formule in LaTeX. \
Descrivi grafici, schemi e disegni in corsivo tra parentesi, con assi, etichette e relazioni, così che si capiscano senza vederli. \
Se una parte è illeggibile scrivi [illeggibile]. Rispondi solo con il contenuto, senza introduzioni.";

pub const PDF: &str = "Questo è un PDF di una lezione (slide, dispense o appunti). Trascrivi il contenuto in Markdown, pagina per pagina, \
con un titolo ## Pagina N per ogni pagina. Mantieni titoli, elenchi e tabelle; formule in LaTeX; descrivi grafici e schemi in corsivo tra parentesi. \
Se è un testo lungo e già digitale, riportalo fedelmente. Rispondi solo con il contenuto.";

/// `marks`: the student's bookmarks, one per line ("- [F1 00:12:30] importante: nota").
pub fn generate(course: &str, title: &str, date: &str, notes: &str, terms: &str, marks: &str, sources: &str) -> String {
    let terms = terms_line(terms);
    let notes = if notes.trim().is_empty() { String::new() } else { format!("\nNote dello studente sulla lezione:\n{notes}\n") };
    let marks = if marks.trim().is_empty() {
        String::new()
    } else {
        format!(
            "\nPunti segnati dallo studente mentre riascoltava la registrazione. Nella sintesi dai più spazio ai punti «importante» \
e spiega con più calma e con un esempio i punti «non capito»; le note dicono cosa lo studente voleva ricordare:\n{marks}"
        )
    };
    format!(
        "Corso: {course}\nLezione: {title} ({date})\n{terms}{notes}{marks}\n\
Qui sotto ci sono tutte le fonti della lezione, ognuna con un codice [F1], [F2]…: la trascrizione della registrazione, appunti, foto della lavagna, slide. \
Collegale tra loro: gli appunti e le slide spesso spiegano o correggono ciò che si sente nella registrazione, e viceversa.\n\n\
Produci:\n\
1. **sintesi**: una sintesi completa e ordinata della lezione in Markdown (titoli ##/###, elenchi, formule LaTeX, esempi svolti se ci sono). \
Deve permettere di studiare senza riascoltare la lezione. Alla fine di ogni paragrafo indica le fonti tra parentesi quadre, con il minuto per la registrazione, ad esempio [F1 00:12:30] o [F3].\n\
2. **mappa**: una mappa concettuale ad albero: radice = argomento della lezione, poi i concetti principali, poi i sotto-concetti. Etichette brevi (massimo 6 parole).\n\
3. **glossario**: i termini tecnici con una definizione breve.\n\
4. **flashcard**: 10-20 domande e risposte brevi per il ripasso.\n\
5. **domande_esame**: 4-8 domande come all'esame (teoria ed esercizi), ciascuna con una traccia di risposta.\n\
6. **titolo**: un titolo breve e specifico per la lezione.\n\n\
FONTI:\n\n{sources}"
    )
}

fn node(depth: u32) -> Value {
    let mut props = json!({ "etichetta": { "type": "string" } });
    if depth > 0 {
        props["figli"] = json!({ "type": "array", "items": node(depth - 1) });
    }
    json!({ "type": "object", "properties": props, "required": ["etichetta"] })
}

/// Fixed depth instead of `$ref` recursion: root + 3 levels is plenty for a lesson.
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
