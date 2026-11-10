# Lezioni

App desktop (Tauri 2 + React) per raccogliere le lezioni dei corsi: registrazioni, appunti dal tablet, foto di appunti a mano, PDF e testo, organizzati per corso e lezione in una cartella del computer. Trascrizione delle registrazioni sul computer (Whisper), poi con Gemini o con un modello a scelta su OpenRouter: ripulitura del testo, lettura di foto, appunti e PDF, sintesi, mappa concettuale, glossario, flashcard e domande d'esame. Ricerca in tutto l'archivio; condivisione come pacchetto `.lezione`, PDF, pagina web, Markdown (Obsidian, NotebookLM) e flashcard per Anki. Domande sulla lezione o su tutto il corso, con le fonti citate.

## Documentazione

- **Manuale utente**: [docs/manuale](docs/manuale/01-introduzione.md), lo stesso mostrato nell'app (**Guida**, <kbd>F1</kbd>).
- **Novità**: [CHANGELOG.md](CHANGELOG.md).
- **Documentazione tecnica**: [architettura](docs/tecnico/architettura.md), [sviluppo e test](docs/tecnico/sviluppo.md), [sicurezza e privacy](docs/tecnico/sicurezza.md), [rilascio](docs/rilascio.md), [piano del progetto](docs/piano.md).

## Sviluppo

```bash
pnpm install
pnpm tauri dev
```

Test del backend (da `src-tauri`, con il limite di memoria di `CLAUDE.md`): `cargo test`.

Test sui formati audio (servono ffmpeg per creare i file di prova): `cargo test --test decode`.

Test con l'API vera di Gemini (consuma un po' di quota): `LEZIONI_IT=1 GEMINI_API_KEY=… cargo test --test gemini_it -- --nocapture`; con `LEZIONI_IT_AUDIO=/percorso/registrazione.m4a` prova una registrazione vera.

Test dell'interfaccia con un backend finto (Playwright, senza finestra): `pnpm test:ui`; gli screenshot finiscono in `scripts/smoke/out`.

## Archivio

```
<cartella scelta>/
  Ricerca operativa/
    corso.json
    2026-11-10 Metodo del simplesso/
      lezione.json
      fonti/      file aggiunti (copiati)
      estratti/   testo estratto dalle fonti
      generati/   sintesi, mappa, flashcard
```

## Installazione e rilasci

Gli installer per macOS, Windows e Linux sono nelle [release](https://github.com/ematrive/lezioni/releases). Come pubblicare una nuova versione, la firma degli aggiornamenti e la prima apertura su Mac senza firma Apple: [docs/rilascio.md](docs/rilascio.md). Il piano del progetto è in [docs/piano.md](docs/piano.md).
