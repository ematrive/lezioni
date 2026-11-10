# Piano: app "Lezioni" (Tauri 2 + React, stile Connector)

## Contesto
Oggi il flusso è manuale: registrazione → upload su NotebookLM → trascrizione/sintesi/mappa; appunti da tablet, foto di appunti a mano e PDF caricati a parte. Obiettivo: app desktop che gestisca **corsi → lezioni → documenti** con drag&drop/incolla, storico, e generi da tutte le fonti insieme trascrizione pulita, sintesi, mappa concettuale, materiale di studio e chat sul corso. Costo AI: zero (Gemini free tier).

Scelte dell'utente: solo desktop; Gemini piano gratuito; tutti gli output (trascrizione, sintesi+mappa, studio attivo, chat); archivio in cartella locale.

NotebookLM non ha API pubblica (solo Enterprise su Google Cloud): l'app lo sostituisce chiamando Gemini direttamente; resta un export "pacchetto per NotebookLM" opzionale.

## Progetto
Nuovo progetto in questo repository, stessa struttura di Connector (un'altra app Tauri dello stesso autore): Tauri 2, React 19 + TypeScript + Tailwind v4, pnpm, UI e manuale in italiano, build Rust con il limite di memoria indicato nel CLAUDE.md di Connector (copiare le regole pertinenti in un CLAUDE.md nuovo).

## Archivio (cartella locale, sincronizzabile)
```
<cartella scelta>/
  <Corso>/
    2026-11-10 <titolo lezione>/
      fonti/        audio, pdf, immagini, testo incollato (copiati, mai spostati)
      estratti/     <fonte>.md (trascrizione con timestamp, OCR)
      generati/     sintesi.md, mappa.mmd, flashcard.json, glossario.md
      lezione.json  metadati (data, docente, stato elaborazione, versioni)
```
Indice SQLite in `app_data_dir()` (come `store.rs` di Connector) con FTS5 sugli estratti; ricostruibile dalla cartella. I file restano leggibili anche senza l'app.

## Backend Rust (`src-tauri/src`)
| Modulo | Ruolo |
|---|---|
| `commands.rs`, `manager.rs` | comandi Tauri e `AppState` (riprendere lo schema di Connector) |
| `store.rs` | SQLite: corsi, lezioni, fonti, FTS5, cronologia chat |
| `library.rs` | lettura/scrittura cartelle, import file (drag&drop, incolla immagini/testo dagli appunti con `tauri-plugin-clipboard-manager`) |
| `jobs.rs` | riuso di `JobEvent`/avanzamento/annullamento da Connector per elaborazioni lunghe |
| `secrets.rs`/`vault.rs` | riuso da Connector per la chiave API Gemini |
| `ai/mod.rs` | trait `AiProvider { transcribe, extract, generate, chat }` |
| `ai/gemini.rs` | client `reqwest`: Files API per audio/PDF grandi, `generateContent` con output JSON strutturato, coda con limite di richieste e retry su 429 (free tier) |
| `pipeline.rs` | orchestrazione per lezione (sotto) |
| `extract/pdf.rs` | testo dai PDF digitali in locale; PDF scansionati e immagini → Gemini |

## Pipeline per lezione
1. **Estrazione** per fonte, in parallelo limitato: audio → trascrizione con timestamp e speaker (chunk se oltre i limiti); immagini/PDF a mano → OCR con formule in LaTeX e descrizione di grafici; PDF digitali → testo locale.
2. **Pulizia** della trascrizione (intercalari, paragrafi, titoli), mantenendo i timestamp.
3. **Fusione**: un prompt con tutti gli estratti (il contesto lungo di Gemini contiene una lezione intera) → sintesi strutturata con riferimenti alle fonti, mappa concettuale in Mermaid `mindmap`, glossario, flashcard e domande d'esame in JSON.
4. Rigenerazione per singolo output, con versioni precedenti conservate.

## Chat sul corso
Prima versione senza database vettoriale: si mettono nel contesto gli estratti delle lezioni selezionate (o dell'intero corso finché ci stanno) con ID di fonte e si chiede di citare `[lezione/fonte/minuto]`; le citazioni sono cliccabili e aprono la fonte al punto giusto. Se un corso supera il contesto: ricerca FTS5 per ridurre gli estratti, ed eventualmente embedding Gemini in seguito.

## Frontend (`src/features`)
- `courses`: elenco corsi e lezioni (storico per data), pannelli ridimensionabili con `usePaneSize` (`src/lib/colwidths.tsx`, ripreso da Connector).
- `lesson`: area di drop/incolla, stato di elaborazione per fonte, schede Trascrizione (con player audio sincronizzato ai timestamp), Sintesi, Mappa (render `mermaid`), Studio (flashcard con ripasso), Fonti (anteprima PDF/immagini).
- `chat`: chat per lezione o corso con citazioni.
- `search`: ricerca full-text su tutto.
- `settings`: cartella archivio, chiave Gemini, modelli, lingua.
- `share`: export e import per condividere (sotto).

## Condivisione tra studenti (export/import)
Finestra "Esporta" con scelta della lezione o di un intero corso e di cosa includere (sintesi, mappa, trascrizione, flashcard, appunti originali; audio escluso di default: va chiesto il consenso del docente e pesa molto).
| Formato | Per chi | Come |
|---|---|---|
| **PDF** | chiunque | dispensa impaginata: copertina, sintesi, mappa come immagine vettoriale, glossario, domande, trascrizione in appendice. Generato in Rust con `typst` (crate `typst` + font inclusi), uguale su Mac e Windows |
| **Pacchetto `.lezione`** (zip con la cartella della lezione + `lezione.json`) | chi usa l'app | import con doppio clic o drag&drop: la lezione compare nel corso con tutte le fonti, senza rielaborare con l'AI. Rilevamento dei duplicati per ID |
| **HTML singolo file** | chi non ha l'app | pagina autonoma con mappa interattiva e flashcard cliccabili, apribile in qualsiasi browser |
| **Mazzo Anki `.apkg`** | chi studia con Anki | flashcard e domande d'esame (crate `genanki-rs`) |
| **Markdown/Obsidian** | chi prende appunti in Markdown | cartella con `.md` e mappa Mermaid |
| **Pacchetto NotebookLM** | chi usa NotebookLM | estratti in Markdown pronti da caricare |
Condivisione: si salva il file e lo si manda dove si vuole (WhatsApp, Drive, mail); niente server, nessun costo. Con l'archivio su una cartella Drive condivisa, un gruppo può anche lavorare sullo stesso corso (in un secondo momento: gestione dei conflitti).

## Piattaforme: macOS e Windows
Tauri 2 genera app native per entrambi (e Linux) dallo stesso codice:
- **macOS**: `.dmg`, firma e notarizzazione (serve l'Apple Developer Program, 99 $/anno, per distribuirla ad altri senza avvisi; senza firma funziona ma va aperta con clic destro → Apri).
- **Windows**: installer `.msi`/`.exe` (NSIS) con WebView2, già presente su Windows 10/11. Senza un certificato di firma compare l'avviso SmartScreen alla prima apertura.
- Build per entrambe le piattaforme con GitHub Actions (`tauri-action`), e aggiornamenti automatici con `tauri-plugin-updater`.
- Accorgimenti: percorsi con `dirs`/`PathBuf` (niente separatori fissi), nomi di cartella ripuliti dai caratteri vietati su Windows (`: ? * " < > |`), font inclusi nell'app per avere PDF identici, nessuna dipendenza nativa da installare (whisper locale resterebbe opzionale).

## Fasi
1. Scheletro, archivio su cartella, corsi/lezioni, import drag&drop e incolla.
2. Gemini: trascrizione, OCR, pulizia, sintesi e mappa.
3. Studio attivo (flashcard, domande, glossario) e ricerca.
4. Condivisione: PDF e pacchetto `.lezione` (export e import), poi HTML, Anki, Markdown.
5. Chat con citazioni.
6. Build e installer per macOS e Windows su GitHub Actions, aggiornamenti automatici.

## Verifica
- Test Rust per `library.rs` e `pipeline.rs` con un provider finto e risposte registrate.
- Prova reale: lezione con 10 minuti di audio, una foto di appunti a mano e un PDF; controllo trascrizione, OCR, sintesi, mappa, flashcard, una domanda in chat con citazione corretta.
- `pnpm tauri dev` e prova del drag&drop e dell'incolla.
- Export → import di un `.lezione` su un secondo profilo: contenuto identico. PDF aperto su Mac e Windows. Installer provato su una macchina Windows o VM.
