# Lezioni: regole per lo sviluppo

Piano del progetto: `docs/piano.md` (fasi, formati, AI). Le scelte fatte diversamente sono sotto.

- **Archivio = cartella.** La verità sta nei file (`corso.json`, `lezione.json`, `fonti/`, `estratti/`, `generati/`), vedi `src-tauri/src/library.rs`. Un eventuale indice SQLite deve essere ricostruibile dalla cartella. Le fonti si copiano, mai spostate o modificate.
- **Mac e Windows.** Nomi di file e cartelle passano da `safe_name`; percorsi solo con `PathBuf`, niente separatori scritti a mano in Rust.
- **Interfaccia ridimensionabile.** Colonne e pannelli con `usePaneSize` (`src/lib/colwidths.tsx`), misura passata come `style`.
- **Lingua.** Interfaccia, messaggi d'errore, manuale e CHANGELOG in italiano; commenti nel codice in inglese.
- **Manuale.** Ogni funzione nuova va nel manuale (`docs/manuale`, che è anche la Guida dell'app) e nel CHANGELOG. Se l'interfaccia cambia, aggiorna i passi di `scripts/smoke/run.mjs` e rigenera le immagini con `pnpm docs:screenshots`; `pnpm docs:check` controlla immagini e link. La documentazione tecnica è in `docs/tecnico`.
- **Formattazione.** Niente prettier su file interi: modifica solo le righe toccate.
- **Rust.** Compila sempre con il limite di memoria:
  `systemd-run --user --scope -p MemoryMax=3G -p MemorySwapMax=0 -p CPUQuota=150% -- nice -n 19 env CARGO_BUILD_JOBS=1 ~/.cargo/bin/cargo …`
- **Gemini.** Riferimento: le pagine ufficiali su aistudio.google.com/docs (verificate a novembre 2026). Si usa `generateContent` (ancora supportato), non la Interactions API. Le registrazioni si dividono in locale (`audio.rs`) e si trascrivono sul computer con Whisper (`asr.rs`); Gemini ripulisce il testo. Con la trascrizione «Con Gemini» le parti servono perché l'audio non ha offset di inizio/fine. Nell'UE il piano gratuito non va usato per un'app distribuita ad altri.
- **Scelte diverse dal piano.** Trascrizione con Whisper sul computer (il piano gratuito di Gemini rispondeva 503 sull'audio), poi ripulitura con Gemini parte per parte; il testo grezzo resta in `.parti/NN.whisper.md` solo finché la ripulitura non riesce; tutti i PDF vanno a Gemini (nessuna estrazione locale); la mappa è un albero JSON disegnato dall'app, con Mermaid solo come export (`mappa.mmd`); ricerca leggendo i file a ogni query, senza indice SQLite; PDF tramite la stampa del browser della pagina HTML esportata (niente typst); Anki come file di testo importabile, non `.apkg`.
