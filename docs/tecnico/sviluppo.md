# Sviluppo e test

## Requisiti

- Node 24 e pnpm (versione in `package.json`, `packageManager`);
- Rust stabile;
- `cmake` e un compilatore C++ per whisper.cpp (su Mac: `xcode-select --install` e `brew install cmake`). I binding di `whisper-rs-sys` sono quelli inclusi nel crate (`WHISPER_DONT_GENERATE_BINDINGS` in `src-tauri/.cargo/config.toml`), quindi non serve libclang;
- su Linux le librerie di Tauri: `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`.

```bash
pnpm install
pnpm tauri dev
```

### Su Mac, da zero

```bash
xcode-select --install                     # compilatore e git di Apple
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # Rust (opzioni predefinite)
brew install node@24 pnpm cmake            # oppure Node 24 da nodejs.org e `npm i -g pnpm`
git clone https://github.com/ematrive/lezioni.git ~/lezioni
cd ~/lezioni && pnpm install && pnpm tauri dev
```

La prima compilazione di Rust richiede qualche minuto, le successive pochi secondi. Le modifiche all'interfaccia si vedono subito; quelle in `src-tauri` ricompilano e riavviano l'app da sole. I log escono nel Terminale. Per aggiornare: `git pull`, poi di nuovo `pnpm tauri dev` (dopo `pnpm install` se è cambiato `package.json`).

L'app in sviluppo usa le stesse impostazioni e la stessa chiave nel Portachiavi dell'app installata (stesso identificatore): macOS può chiedere il permesso di leggere la chiave, scegli **Consenti sempre**.

Sul computer di sviluppo principale compila Rust con il limite di memoria (vedi `CLAUDE.md`):

```bash
systemd-run --user --scope -p MemoryMax=3G -p MemorySwapMax=0 -p CPUQuota=150% -- nice -n 19 env CARGO_BUILD_JOBS=1 ~/.cargo/bin/cargo test
```

## Test

| Comando | Cosa verifica |
|---|---|
| `cd src-tauri && cargo test` | Archivio, divisione dell'audio, pipeline con un modello finto (parti, ripresa dopo errori, testi troncati, fonti escluse), ricerca, pacchetti (andata e ritorno, duplicati, percorsi malevoli, sostituzione fallita), Markdown, Anki, chat |
| `cargo test --test decode` | Divisione di m4a/AAC, mp3, ogg e dell'audio di mp4 creati con ffmpeg (saltato se ffmpeg manca); opus deve fallire |
| `LEZIONI_IT_WHISPER=/percorso/ggml-tiny-q5_1.bin cargo test --test whisper_it -- --nocapture` | Whisper vero su un tono o su `LEZIONI_IT_AUDIO`: stampa i paragrafi e il tempo impiegato. `LEZIONI_IT_PROMPT` è il prompt iniziale (come lo costruisce l'app da corso e titolo), `LEZIONI_IT_RAW=1` stampa anche i segmenti grezzi. Il modello `tiny` (32 MB) basta per provare il codice; la qualità vera è quella di large-v3-turbo |
| `LEZIONI_IT_OPENROUTER=1 OPENROUTER_API_KEY=… LEZIONI_IT_PDF=slide.pdf cargo test --test openrouter_it -- --nocapture` | OpenRouter vero: ripulitura, lettura di un PDF e sintesi con prompt e schema veri per ogni modello di `LEZIONI_IT_MODELS` (separati da virgola), con tempi e costi. `LEZIONI_IT_TEXT` (un testo di Whisper), `LEZIONI_IT_COURSE` e `LEZIONI_IT_TITLE` per una lezione vera; `LEZIONI_IT_OUT=cartella` salva ripulitura, PDF, sintesi e richiesta. Spende qualche centesimo |
| `… LEZIONI_IT_TEXT=whisper.md LEZIONI_IT_RECLEAN=0-100 cargo test --test openrouter_it reclean_piece -- --nocapture` | Ripulitura di un pezzo (secondi da-a, `a` vuoto = fino alla fine) con `pipeline::reclean`, per ogni modello di `LEZIONI_IT_MODELS`; `LEZIONI_IT_TERMS` per le parole del corso. Meno di un millesimo di dollaro a modello |
| `LEZIONI_IT=1 GEMINI_API_KEY=… cargo test --test gemini_it -- --nocapture` | L'API vera di Gemini: upload, countTokens, trascrizione di una parte, output JSON con lo schema vero. `LEZIONI_IT_AUDIO=/percorso/reg.m4a` per una registrazione vera, `LEZIONI_IT_MODEL` per un altro modello. Consuma un po' di quota |
| `pnpm build` | TypeScript e build dell'interfaccia |
| `pnpm test:ui` | Tutta l'interfaccia nel browser (Playwright) con il backend finto `scripts/smoke/mock.js`: prima apertura, corsi e lezioni, trascina e incolla, elaborazione, schede, domande, esportazione HTML e PDF, ricerca, importazione. Screenshot in `scripts/smoke/out` |
| `pnpm docs:check` | Ogni immagine citata dal manuale esiste e ogni immagine è usata |

Per le prove con una lezione vera, audio e PDF vanno in `training/`, esclusa da git: sono registrazioni del docente e non si versionano.

GitHub Actions (`.github/workflows/ci.yml`) esegue build, `cargo test` e `pnpm test:ui` a ogni push su `main`.

La prima volta che usi Playwright: `pnpm exec playwright install chromium`.

## Manuale e screenshot

Il manuale utente è in `docs/manuale/*.md`, in italiano, un file per capitolo (l'ordine segue il nome). Lo stesso testo è la **Guida** dell'app (F1): `src/features/help/Guide.tsx` lo include alla build insieme alle immagini di `docs/manuale/img`.

Gli screenshot si rigenerano dall'interfaccia vera con dati finti:

```bash
pnpm docs:screenshots
```

Lo script è lo stesso del test dell'interfaccia (`scripts/smoke/run.mjs`) in modalità manuale: aspetta la fine delle animazioni e nasconde i messaggi temporanei. Se l'interfaccia cambia, aggiorna i passi lì e rigenera.

Ogni funzione nuova va nel manuale e nel `CHANGELOG.md`.

## Convenzioni

- Interfaccia, messaggi d'errore, manuale e CHANGELOG in italiano; commenti nel codice in inglese.
- Pannelli e colonne ridimensionabili con `usePaneSize`.
- Percorsi solo con `PathBuf`; nomi di file e cartelle con `safe_name` / `safe_file_name`.
- Niente prettier su file interi: modifica solo le righe toccate.
