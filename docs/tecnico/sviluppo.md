# Sviluppo e test

## Requisiti

- Node 24 e pnpm (versione in `package.json`, `packageManager`);
- Rust stabile;
- su Linux le librerie di Tauri: `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`.

```bash
pnpm install
pnpm tauri dev
```

Sul computer di sviluppo principale compila Rust con il limite di memoria (vedi `CLAUDE.md`):

```bash
systemd-run --user --scope -p MemoryMax=3G -p MemorySwapMax=0 -p CPUQuota=150% -- nice -n 19 env CARGO_BUILD_JOBS=1 ~/.cargo/bin/cargo test
```

## Test

| Comando | Cosa verifica |
|---|---|
| `cd src-tauri && cargo test` | Archivio, divisione dell'audio, pipeline con un modello finto (parti, ripresa dopo errori, testi troncati, fonti escluse), ricerca, pacchetti (andata e ritorno, duplicati, percorsi malevoli, sostituzione fallita), Markdown, Anki, chat |
| `cargo test --test decode` | Divisione di m4a/AAC, mp3, ogg e dell'audio di mp4 creati con ffmpeg (saltato se ffmpeg manca); opus deve fallire |
| `LEZIONI_IT=1 GEMINI_API_KEY=… cargo test --test gemini_it -- --nocapture` | L'API vera di Gemini: upload, countTokens, trascrizione di una parte, output JSON con lo schema vero. `LEZIONI_IT_AUDIO=/percorso/reg.m4a` per una registrazione vera, `LEZIONI_IT_MODEL` per un altro modello. Consuma un po' di quota |
| `pnpm build` | TypeScript e build dell'interfaccia |
| `pnpm test:ui` | Tutta l'interfaccia nel browser (Playwright) con il backend finto `scripts/smoke/mock.js`: prima apertura, corsi e lezioni, trascina e incolla, elaborazione, schede, domande, esportazione HTML e PDF, ricerca, importazione. Screenshot in `scripts/smoke/out` |
| `pnpm docs:check` | Ogni immagine citata dal manuale esiste e ogni immagine è usata |

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
