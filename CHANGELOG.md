# Novità

## Prossima versione

- Se il modello della sintesi risponde due volte vuoto o scrive una risposta così lunga da essere troncata, l'app riprova con il **modello di riserva** invece di fermarsi con un errore, e lo segnala; nel log resta la fine della risposta troncata.
- Il campo «Cerca nella lezione» non è più tagliato.
- Trascrizione: si passa da un documento all'altro (registrazione, slide, foto) con la riga **In questa lezione**, con il menu sul nome del documento o con le frecce ↑ ↓, senza scorrere tutta la pagina.
- Gli appunti a mano stanno ognuno su una riga, con la matita e senza la scritta «nota a mano:» ripetuta.
- **Mappa → trascrizione**: la freccia accanto a un concetto porta al punto della registrazione in cui se ne parla.
- **Mappa in PDF** (scheda Mappa → Esporta in PDF), disegnata come nell'app su una pagina orizzontale; anche nel PDF della lezione la mappa è ora disegnata invece che un elenco.
- Gli **appunti a mano** letti dalle slide si vedono in blu, con una penna.
- Con OpenRouter i PDF possono pesare fino a **50 MB** (prima 30): un PDF di 59 pagine di slide annotate (33 MB) veniva rifiutato.
- **Pause**: le parole che Whisper inventa durante un silenzio o un intervallo («grazie grazie grazie…») diventano una riga grigia «Pausa 00:12:23 – 00:16:19» e non arrivano più al modello. Per le lezioni già fatte c'è il pulsante «pause da sistemare»; a mano, *Segna come pausa* nella correzione.
- **Cerca nella lezione** (⌘F / Ctrl+F): occorrenze evidenziate nella scheda aperta, con successiva e precedente.
- Gli **appunti a mano** sopra slide e foto (note, evidenziature, frecce) vengono trascritti come *(nota a mano: …)*.
- Corretto: da un risultato della ricerca o da un riferimento della sintesi la trascrizione ora scorre fino al paragrafo di quel minuto, non solo l'audio.
- **Parole per tutti i corsi** (Impostazioni): nomi e sigle che tornano in ogni corso, suggeriti a Whisper, alla ripulitura e alla sintesi insieme alle parole del corso.
- **Glossario del corso** nella pagina del corso: le parole del corso, finalmente in vista e modificabili, e i glossari di tutte le lezioni uniti, con un clic per aggiungere un termine alle parole del corso.
- Menu **⋯** anche sulle lezioni nell'elenco (Condividi, Mostra la cartella, Elimina).
- Pulsanti e campi tutti della stessa altezza; il pulsante per nascondere i corsi è accanto alla ricerca.
- Le azioni di un corso sono nel menu **⋯** (prima le icone coprivano il nome); la colonna dei corsi si può **nascondere**; la lezione aperta può andare **a tutto schermo** (Esc per tornare).
- Trascrizione: intestazione più ordinata e che non lascia più intravedere il testo che scorre sopra; i pulsanti dei segnaposti sono accanto al lettore e l'elenco dei segnaposti si apre sopra il testo.
- Le **formule** nelle flashcard, nelle domande d'esame e nel glossario si vedono come formule, non più come `$\alpha$`.
- Corretto: con OpenRouter, rielaborare una lezione dopo una ripulitura non riuscita provava a inviare l'audio al modello (errore «parte-01.wav è troppo grande per OpenRouter»). Ora riprende dal testo di Whisper già salvato, e l'audio non viene mai inviato.
- **Sintesi del corso** nella pagina del corso: tutto il corso organizzato per argomenti, fatto dalle sintesi delle lezioni, con le **domande d'esame** più ricorrenti e i link alle lezioni da ripassare. Avvisa quando ci sono lezioni nuove.
- **Ripasso a intervalli** delle flashcard: dopo aver girato la carta dici se la sapevi; quelle che sai tornano dopo 1, 3, 7, 14 e 30 giorni, quelle che non sai tornano subito. Si fa per una lezione (Studio → Ripasso) o per tutto il corso (pagina del corso), al massimo 20 carte nuove per volta.
- **Versioni precedenti** nell'app: le sintesi sostituite da Rigenera e le trascrizioni corrette poi rifatte si possono confrontare con quella attuale (differenze riga per riga) e ripristinare; quella sostituita diventa a sua volta una versione.
- **Ripulisci di nuovo un pezzo**: nella correzione di una trascrizione selezioni un passaggio venuto male e il modello (anche un altro, scelto dalla tendina) lo ripulisce di nuovo dal testo di Whisper; vedi la proposta accanto al testo attuale e scegli se sostituirlo.
- **Segnaposti mentre ascolti**: accanto al lettore della registrazione, *Importante* (tasto I), *Non ho capito* (D) e *Nota* (N) segnano il minuto a cui sei arrivato. Nella trascrizione il paragrafo è evidenziato; con Rigenera la sintesi dà più spazio ai punti importanti e spiega meglio quelli non capiti, e le Domande li conoscono.
- **Pagina del corso** (clic sul nome del corso sopra le lezioni) con i **costi**: quanto hai speso con l'AI, per mese e per lezione, comprese domande ed elaborazioni interrotte. Nella Sintesi la riga in alto dice anche quanto è costata l'ultima elaborazione.
- **Testo di Whisper**: nella Trascrizione l'interruttore *Ripulita | Whisper* mostra il testo come l'ha riconosciuto Whisper, prima della ripulitura.
- **Correzioni a mano**: trascrizioni e testi letti, sintesi, flashcard, domande d'esame e glossario si possono correggere con **Modifica**. Nei testi c'è **Trova e sostituisci** per correggere in un colpo una parola sbagliata. Con **Rigenera** la sintesi usa le correzioni; con *Rielabora anche le fonti* l'app chiede conferma e conserva la versione corretta.
- **Parole del corso**: nel corso si scrivono i termini e i nomi ricorrenti (per esempio ARERA, colpo d'ariete); Whisper, la ripulitura e la sintesi li scrivono giusti. Si possono aggiungere anche da una sostituzione.
- Se il modello risponde con una sintesi vuota (succede ogni tanto con Claude Haiku), l'app la richiede fino a tre volte e non sostituisce mai la sintesi salvata con una vuota.
- I messaggi durante l'elaborazione non nominano più Gemini quando si usa OpenRouter.

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
