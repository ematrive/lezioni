# Novità

## 0.7.0

- **OpenRouter** come alternativa a Gemini (Impostazioni → Servizio AI): una chiave per centinaia di modelli, pagati a consumo. Accanto a ogni modello l'app mostra il prezzo, se legge immagini e PDF e quanto costerebbe una lezione; a fine elaborazione dice il **costo reale**. Predefiniti: Gemini 3.5 Flash-Lite per ripulire e leggere, Claude Haiku 5.5 per la sintesi. Solo fornitori che non usano i dati per addestrare i modelli.
- Con Gemini, ripulitura e lettura delle fonti usano ora `gemini-3.5-flash-lite` (più veloce e meno spesso sovraccarico).
- Il pulsante si chiama ora **Elabora**.
- I titoli che alcuni modelli scrivono dopo il minuto vengono messi al posto giusto.

## 0.6.0

- **Trascrizione sul computer** con Whisper: gratuita, senza limiti di quota e senza inviare l'audio a Google. Il modello (574 MB) si scarica una volta sola, anche dalle Impostazioni; il download continua a Impostazioni chiuse e si può annullare. Gemini poi ripulisce il testo parte per parte (paragrafi, titoli, formule, interventi degli studenti); se non risponde resta il testo di Whisper con i minuti, e la ripulitura viene ritentata la volta dopo. La trascrizione con Gemini resta disponibile nelle Impostazioni.
- Se la scheda grafica del Mac non riesce a trascrivere, Whisper ripete la parte sul processore (più lento).
- **Modello di riserva**: quando Google risponde due volte che il modello è sovraccarico, la richiesta passa a `gemini-3.5-flash-lite` (modificabile nelle Impostazioni) e l'app lo segnala.

## 0.5.1

- **Elaborazione più chiara**: la barra avanza parte per parte e una seconda riga dice cosa sta succedendo (caricamento su Gemini con i MB inviati, attesa della risposta, nuovi tentativi quando Gemini è occupato) e da quanto tempo.
- Niente più attese senza fine: se Gemini non risponde in 10 minuti l'app riprova una sola volta, e un caricamento fermo per un minuto e mezzo si interrompe con un messaggio. Prima un'attesa poteva durare più di un'ora senza segnali.
- Un errore interno ora chiude l'elaborazione con un messaggio, invece di lasciare la barra ferma.
- Rimuovere una fonte spiega cosa succede: va nel Cestino la copia nell'archivio, l'originale resta dov'era.
- Corretto l'angolo destro di **Elabora con Gemini** accanto alla freccia del menu.
- **File di log** (Impostazioni → Diagnostica → Mostra il file di log) da allegare alle segnalazioni.

## 0.5.0

- **Guida** nell'app (pulsante *Guida* in basso a sinistra o <kbd>F1</kbd>): tutti i capitoli del manuale con indice, ricerca e screenshot, disponibile anche senza internet.

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
