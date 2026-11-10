# Problemi e domande frequenti

**«Quota esaurita» durante l'elaborazione.** Il piano gratuito di Gemini ha un limite giornaliero. Le parti di registrazione già trascritte sono salvate: rilancia **Elabora con Gemini** il giorno dopo e riprende da lì. In alternativa attiva la fatturazione su Google (pochi centesimi a lezione).

**«Gemini è sovraccarico» (errore 503).** Sono i server di Google, pieni in quel momento (succede soprattutto col piano gratuito). L'app riprova da sola e dopo due rifiuti passa al modello di riserva; se non basta, riprova più tardi: le parti già fatte restano salvate. La trascrizione sul computer non ne risente.

**«Trascrizione non ripulita».** Whisper ha trascritto la parte ma Gemini non è riuscito a ripulirla: nella lezione c'è il testo di Whisper, con i minuti, e alla prossima elaborazione l'app riprova solo la ripulitura.

**Il download del modello di trascrizione non riesce.** Servono 574 MB liberi e una connessione stabile; il download riparte da capo a ogni tentativo. Si può avviare da **Impostazioni → Scarica ora**.

**L'elaborazione sembra ferma.** Guarda la riga sotto la barra: dice cosa sta aspettando e da quanto. Il caricamento di una parte dipende dalla connessione; la risposta di Gemini per una parte di 20 minuti arriva di solito in 1–3 minuti. Se Gemini non risponde in 10 minuti l'app riprova una volta e poi si ferma con un errore; se il caricamento non avanza per un minuto e mezzo, si ferma subito. Le parti già fatte restano salvate.

**Segnalare un problema.** In **Impostazioni → Diagnostica**, **Mostra il file di log** apre la cartella con `lezioni.log`, che registra cosa ha fatto l'app (contiene i nomi dei file, ma non la chiave API né il testo delle lezioni): allegalo alla segnalazione insieme a uno screenshot.

**«Chiave API non valida».** Controlla di aver copiato tutta la chiave da [aistudio.google.com/apikey](https://aistudio.google.com/apikey) e premi **Verifica** nelle Impostazioni.

**La registrazione viene inviata intera e costa di più.** Succede con i formati che l'app non sa dividere (per esempio opus). Converti la registrazione in m4a o mp3 e aggiungila di nuovo.

**Un file Word o PowerPoint viene saltato.** Gemini non legge i file di Office: esportalo in PDF e aggiungi il PDF.

**La trascrizione ha un pezzo incompleto.** L'app lo segnala e non salva il testo incompleto: rielabora la lezione con **Rielabora anche le fonti**.

**Su Mac l'app non si apre («sviluppatore non identificato» o «danneggiata»).** L'app non è firmata da Apple. La prima volta fai clic destro sull'app → **Apri** → **Apri**. Se non basta: Impostazioni di Sistema → Privacy e sicurezza → **Apri comunque**.

**Su Windows compare «Windows ha protetto il PC».** Clicca **Ulteriori informazioni** → **Esegui comunque**.

**Aggiornamenti.** All'avvio l'app controlla se c'è una versione nuova e chiede se installarla; poi si riavvia da sola.

**Copia di sicurezza.** Basta copiare la cartella dell'archivio, oppure tenerla dentro Google Drive, iCloud o Dropbox. Le impostazioni e la chiave API sono per computer: su un computer nuovo scegli la stessa cartella e inserisci di nuovo la chiave.

**Il PDF non si apre nel browser (Linux).** Alcuni browser installati come Snap o Flatpak non possono aprire i file temporanei: usa **Pagina web**, salva il file in una cartella tua, aprilo e stampalo in PDF dal browser.
