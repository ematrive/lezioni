# Novità

## 0.4.0

- **Domande**: una chat sulla lezione (risponde da trascrizioni, appunti e slide) o su tutto il corso (dalle sintesi delle lezioni). Le risposte citano le fonti: [F1 00:12:30] riapre la registrazione a quel minuto, [L3] apre la lezione. La conversazione resta salvata nella cartella della lezione e non viene mai inclusa nei pacchetti condivisi; le note personali vanno nel pacchetto solo se lo scegli.

## 0.3.0

- **Ricerca** in tutto l'archivio (titoli, note, trascrizioni, testi delle fonti, sintesi, flashcard), senza badare ad accenti e maiuscole. Un risultato in una trascrizione apre la lezione al minuto giusto.
- **Condividi** una lezione, o un intero corso dal menu del corso:
  - **pacchetto .lezione** per chi usa Lezioni: si apre con doppio clic o trascinandolo nella finestra, con fonti, trascrizioni e sintesi già pronte. Le registrazioni sono escluse di default (pesano e serve il permesso del docente): la trascrizione viaggia comunque;
  - **PDF**: dispensa con sintesi, mappa, flashcard, domande, glossario e, a scelta, le trascrizioni in appendice; si apre la stampa del browser, dove scegliere «Salva come PDF»;
  - **pagina web** in un solo file, leggibile anche dal telefono e senza internet;
  - **Markdown** per Obsidian e NotebookLM;
  - **flashcard per Anki**.
- **Importa lezioni…**: se una lezione c'è già si può lasciarla, sostituirla o tenerle entrambe.

## 0.2.0

- **Elabora con Gemini**: le registrazioni vengono divise dall'app in parti di 20 minuti e trascritte una parte alla volta (anche lezioni di più ore; ogni minuto si paga una volta sola). Se un'elaborazione si interrompe, ad esempio per la quota giornaliera, la successiva riprende dalle parti già fatte. Foto, appunti e PDF vengono letti con formule e descrizione dei grafici, e da tutte le fonti insieme si ottengono sintesi, mappa concettuale, glossario, flashcard e domande d'esame.
- Trascrizione con i minuti cliccabili: l'audio riparte da quel punto. Nella sintesi i riferimenti come [F1 00:12:30] portano alla trascrizione e alla registrazione.
- Una seconda elaborazione riusa le trascrizioni già fatte (si paga solo la sintesi); «Rielabora anche le fonti» rifà tutto. Le versioni precedenti restano in `generati/versioni`.
- Impostazioni: chiave API di Gemini (nel portachiavi), verifica della chiave, scelta dei modelli.
- Formule LaTeX e tabelle nelle trascrizioni e nelle sintesi.

## 0.1.0

- Archivio in una cartella a scelta, con una sottocartella per corso e una per lezione.
- Corsi: creazione, rinomina, eliminazione nel Cestino.
- Lezioni con titolo, data e note; elenco per data, filtro.
- Fonti: trascinamento di file dal Finder / Esplora file, scelta da finestra, incolla di immagini e screenshot, testo scritto o incollato. Anteprima delle foto e ascolto delle registrazioni nell'app.
