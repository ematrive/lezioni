# Architettura

Lezioni è un'app desktop **Tauri 2**: un backend **Rust** che legge e scrive l'archivio e parla con Gemini, e un'interfaccia **React 19** (TypeScript, Tailwind v4) dentro la webview di sistema. L'interfaccia non apre connessioni di rete: tutto passa dai comandi Tauri (`invoke`), tranne il controllo degli aggiornamenti (plugin updater).

```
┌──────────────── webview (React) ────────────────────────────┐
│ features/ courses · lesson · search · share · help · settings│
│ lib/api.ts (invoke, Channel)  lib/Markdown.tsx (GFM + KaTeX)  │
└───────────────┬──────────────────────────────────────────────┘
                │ comandi Tauri, Channel per l'avanzamento
┌───────────────▼──────────────── Rust (src-tauri/src) ────────┐
│ commands.rs → library.rs (archivio su cartella)               │
│               pipeline.rs ─ audio.rs (divisione) ─ asr.rs (Whisper)│
│                           └ ai/ (trait Ai, gemini.rs, prompts)│
│               search.rs · share.rs · chat.rs                  │
│ secrets.rs (portachiavi) · settings.rs · jobs.rs              │
└───────────────┬──────────────────────────────────────────────┘
                │ HTTPS (reqwest + rustls)
      generativelanguage.googleapis.com (Gemini)
```

## L'archivio

La fonte di verità è una **cartella scelta dall'utente**, sincronizzabile con Drive o simili. Non c'è un database: ogni comando rilegge i file, quindi modifiche fatte fuori dall'app (o da un altro computer tramite sincronizzazione) sono viste subito.

```
<archivio>/
  <Corso>/corso.json                         { id, name, teacher, created }
  <Corso>/<AAAA-MM-GG titolo>/lezione.json    { id, title, date, notes, created, sources[] }
                             /fonti/          file copiati (mai spostati o modificati)
                             /estratti/<file>.md         testo di ogni fonte
                             /estratti/.parti/<file>/NN.md  parti di una trascrizione in corso
                             /generati/sintesi.md, mappa.json, mappa.mmd, studio.json, info.json
                             /generati/versioni/<data-ora>/  output precedenti
                             /chat.json       conversazioni (mai esportate)
```

- Gli **id** (UUID) identificano corsi e lezioni; i nomi delle cartelle seguono nome, data e titolo e vengono rinominati quando cambiano (`safe_name`: niente caratteri vietati su Windows, nomi riservati, lunghezza massima).
- Ogni `Source` ha `file` (nome dentro `fonti/`, sempre un nome semplice: `is_plain_file_name`), `kind` (dall'estensione), `originalName`, `size`, `added` e `omitted` (fonte arrivata da un pacchetto senza il file).
- I JSON si scrivono con file temporaneo + rinomina, quindi non restano mai a metà.
- Eliminare un corso, una lezione o una fonte li sposta nel Cestino del sistema (crate `trash`).

### Concorrenza

`AppState.write` è un mutex preso da ogni comando che modifica l'archivio: `lezione.json` viene letto, modificato e riscritto, e una rinomina della cartella a metà di una copia perderebbe file. I job lunghi (`pipeline.rs`) non tengono il lock per minuti: a ogni scrittura **ritrovano la lezione per id** (`Archive::write`), così la cartella può essere rinominata mentre lavorano.

Tutti i comandi sono `async` o `#[tauri::command(async)]`: nessuno gira sul thread principale, che bloccherebbe la finestra su un archivio lento.

## Elaborazione (`pipeline.rs`)

1. **Estrazione per fonte** → `estratti/<file>.md`, riusata nelle elaborazioni successive (salvo *Rielabora anche le fonti*):
   - **registrazioni e video**: `audio.rs` decodifica con Symphonia (m4a/AAC, mp3, wav, flac, ogg/vorbis, audio di mp4/mov), ricampiona a 16 kHz mono e scrive parti WAV di `WINDOW_SECS` (20 minuti). Poi, secondo `Models.engine`:
     - **`local`** (predefinito): `asr.rs` trascrive ogni parte con whisper.cpp (`whisper-rs`, Metal su macOS; modello `ggml-large-v3-turbo-q5_0.bin` scaricato in `app_data_dir()/modelli` e verificato con SHA-256, tenuto in memoria da `asr::Loader`). I segmenti diventano paragrafi `[hh:mm:ss]` assoluti (`asr::paragraphs`, che scarta le frasi «allucinate» sul silenzio) salvati in `.parti/NN.whisper.md`; Gemini li ripulisce (`prompts::clean_transcript`, solo testo) e il risultato va in `.parti/NN.md`. La ripulitura della parte N corre mentre Whisper fa la N+1 (`futures::join!`). Una ripulitura fallita, troncata o non plausibile (`plausible_cleanup`: metà delle parole e dei tempi, tempi dentro la parte) lascia il testo di Whisper con un avviso e l'estratto non viene salvato, così il giro dopo rifà solo la ripulitura;
     - **`gemini`**: ogni parte va a Gemini (inline se piccola, altrimenti Files API) con `prompts::transcribe_clip`; i tempi `[mm:ss]` relativi alla parte vengono spostati con `shift_times`. Motivo delle parti: l'API non ha offset di inizio/fine per l'audio, quindi chiedere finestre di un file intero farebbe pagare il file intero a ogni finestra.

     Ogni parte è salvata appena pronta in `estratti/.parti/`, così dopo un errore si riprende da lì;
   - **formati non decodificabili** (opus, amr…): con Whisper errore chiaro (convertire in m4a/mp3); con Gemini file intero con finestre di tempo nel prompt (`transcribe_whole`, durata da `countTokens`: 32 token/s di audio), con avviso;
   - **foto e PDF**: inline o Files API con `prompts::IMAGE` / `prompts::PDF`;
   - **testo**: copiato così com'è;
   - **Office**: saltato con avviso (Gemini non lo legge).
2. Una risposta troncata (`finishReason = MAX_TOKENS`) viene usata per la sintesi di quel giro ma **non salvata** come estratto completo.
3. Se una **registrazione** non è stata trascritta, l'elaborazione si ferma prima della sintesi.
4. **Sintesi**: un'unica richiesta con tutti gli estratti codificati `[F1]`, `[F2]`… e le note, con output JSON vincolato da `prompts::generate_schema()` (`responseJsonSchema`): `titolo`, `sintesi` (Markdown con riferimenti `[F1 00:12:30]`), `mappa` (albero a profondità fissa), `glossario`, `flashcard`, `domande_esame`. `info.json` registra modello, token usati e la corrispondenza codice → fonte.

L'avanzamento arriva all'interfaccia su un `Channel<JobEvent>` (`Step` con avanzamento frazionario, `Detail` per la riga di stato, `Done`, `Failed`); `jobs.rs` tiene i token di annullamento.

### Fornitori (`ai/mod.rs`)

`ai::Provider` (`Gemini` o `OpenRouter`, da `Settings.provider`) implementa il trait `Ai` delegando; `Settings::effective_models()` dà i modelli del fornitore scelto (con OpenRouter la trascrizione è sempre locale). `Usage.cost` porta il costo reale quando il fornitore lo dà.

### Client OpenRouter (`ai/openrouter.rs`)

`POST /api/v1/chat/completions` in stile OpenAI: immagini come `image_url` e PDF come `file` in data URL (fino a 30 MB, niente upload), `models: [modello, riserva]` per il fallback fatto da OpenRouter, `provider.data_collection = "deny"`, `response_format` con lo schema JSON (non strict), `usage.include` per il costo. Stessi nuovi tentativi di Gemini su 408/429/5xx; 401 e 402 diventano messaggi chiari. `GET /api/v1/credits` per la verifica della chiave, `GET /api/v1/models` (pubblico, in cache un'ora) per il listino delle Impostazioni. I modelli predefiniti sono stati scelti con `tests/openrouter_it.rs`.

### Client Gemini (`ai/gemini.rs`)

REST `v1beta` con `generateContent` (ancora supportato; Google raccomanda la nuova Interactions API, che però al momento della scrittura non documentava il formato REST della risposta). Upload resumable della Files API (file fino a 2 GB, conservati 48 ore e cancellati dall'app appena usati). Su 429 e 5xx riprova con il ritardo indicato dal server (`RetryInfo`) o con backoff esponenziale, ma non insiste sulla quota giornaliera. Dopo due 503 di fila passa al modello di riserva (`Models.fallback`, `Answer.model` dice chi ha risposto). Timeout di 10 minuti per chiamata, ritentato una volta; un caricamento fermo per 90 s si interrompe. Ogni chiamata è registrata nel log con durata e token. La pipeline parla col trait `Ai`, così nei test c'è un modello finto.

## Ricerca (`search.rs`)

Legge i file a ogni ricerca (l'archivio è piccolo: decine o centinaia di lezioni). Normalizza accenti e maiuscole, richiede tutte le parole nello stesso documento, ordina per titolo, numero di occorrenze e data; per le trascrizioni restituisce il tempo `[hh:mm:ss]` più vicino prima del risultato.

## Condivisione (`share.rs`)

- **Pacchetto `.lezione`**: zip con `pacchetto.json` (formato, corso, elenco) e `lezioni/NN/` con la cartella della lezione. Esclusi: `chat.json`, `generati/versioni`, `estratti/.parti`, file temporanei; registrazioni e altri file secondo le opzioni (le fonti escluse restano in `lezione.json` con `omitted: true`); note solo se scelto. Importazione: `enclosed_name` contro i percorsi fuori dallo zip, rifiuto dei `file` non semplici, estrazione in una cartella temporanea dentro l'archivio e spostamento finale; con *Sostituisci* la lezione esistente va nel Cestino solo dopo l'estrazione riuscita.
- **Markdown** (Obsidian, NotebookLM) e **Anki** (testo con intestazioni `#separator:tab`, `#html:true`, `#tags column:3`) scritti dal backend.
- **HTML e PDF** costruiti dall'interfaccia (`features/share/html.tsx`) con lo stesso renderer Markdown dell'app e i font KaTeX incorporati. Il PDF è la stessa pagina aperta nel browser di sistema con la finestra di stampa (`export_print`).

## Domande (`chat.rs`)

Lezione: tutti gli estratti con gli stessi codici `[F1]` della sintesi. Corso: le sintesi di tutte le lezioni (`[L1]`, `[L2]`…), perché le trascrizioni di un corso intero sarebbero centinaia di migliaia di token per domanda. Le ultime 10 battute dello stesso ambito vanno nel prompt; la conversazione è in `chat.json` con la mappa codice → id per rendere cliccabili i riferimenti.

## Interfaccia (`src/`)

| Percorso | Contenuto |
|---|---|
| `App.tsx` | Tre colonne (corsi, lezioni o risultati, lezione), import dei pacchetti (doppio clic, trascinamento), guida (F1), controllo aggiornamenti |
| `features/lesson/` | `LessonView` (intestazione, fonti, trascina e incolla, schede), `Outputs.tsx` (trascrizione con player sincronizzato, sintesi, mappa, studio, stato dei job), `ChatTab.tsx` |
| `features/share/` | Finestre di esportazione e importazione, generatore HTML/PDF |
| `features/search/` | Risultati con evidenziazione |
| `features/help/Guide.tsx` | La guida: i file di `docs/manuale` inclusi alla build con le immagini |
| `lib/Markdown.tsx` | Markdown + GFM + KaTeX; trasforma `[hh:mm:ss]`, `[F1 00:12:30]`, `[L3]` in pulsanti |
| `lib/colwidths.tsx` | `usePaneSize`: pannelli ridimensionabili che ricordano la misura |

## Piattaforme e rilascio

Lo stesso codice produce l'app per macOS (universale), Windows e Linux con GitHub Actions (`.github/workflows/release.yml`); gli aggiornamenti sono firmati con la chiave del progetto. Dettagli in [../rilascio.md](../rilascio.md). I file `.lezione` sono associati all'app (`bundle.fileAssociations`): su Windows e Linux il percorso arriva come argomento, su macOS con `RunEvent::Opened`.
