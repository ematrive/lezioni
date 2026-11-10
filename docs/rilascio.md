# Rilascio di una nuova versione

1. Aggiorna la versione in `package.json`, `src-tauri/Cargo.toml` e `src-tauri/tauri.conf.json` (stesso numero) e scrivi le novità in `CHANGELOG.md`.
2. Commit, poi tag e push:
   ```bash
   git tag v0.4.0
   git push origin main v0.4.0
   ```
3. Il workflow **Rilascio** (GitHub Actions) crea gli installer e li pubblica nella release del tag:
   - macOS: `.dmg` universale (Apple Silicon e Intel);
   - Windows: installer `.exe` e `.msi`;
   - Linux: `.AppImage`, `.deb`, `.rpm`.
4. Le app già installate trovano l'aggiornamento all'avvio (`latest.json` della release) e chiedono se installarlo.

## Firma degli aggiornamenti

Gli aggiornamenti sono firmati con una chiave del progetto (non è la firma Apple): l'app accetta solo aggiornamenti firmati con questa chiave.

- Chiave pubblica: in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`).
- Chiave privata: **mai nel repository**. Sta in `~/.tauri/lezioni.key` sul computer di sviluppo e nel segreto `TAURI_SIGNING_PRIVATE_KEY` del repository GitHub (Settings → Secrets and variables → Actions).
- Se la chiave privata va persa, le versioni già installate non potranno più aggiornarsi da sole: conservane una copia sicura (ad esempio in un gestore di password).

## macOS senza firma Apple

L'app non è firmata né notarizzata (servirebbe l'Apple Developer Program, 99 $/anno). La prima volta:

- clic destro sull'app → **Apri** → **Apri**;
- se macOS dice che l'app «è danneggiata» o non offre «Apri»: Impostazioni di Sistema → Privacy e sicurezza → in fondo **Apri comunque**; in alternativa nel Terminale `xattr -cr /Applications/Lezioni.app`.

## Windows

L'installer non è firmato: SmartScreen mostra «Windows ha protetto il PC» → **Ulteriori informazioni** → **Esegui comunque**.
