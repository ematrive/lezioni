# Corsi e lezioni

## Prima apertura: la cartella dell'archivio

Alla prima apertura l'app chiede dove tenere l'archivio. La proposta è la cartella **Documenti/Lezioni**; puoi sceglierne un'altra, anche dentro Google Drive, iCloud Drive o Dropbox, così l'archivio si sincronizza da solo e ne hai sempre una copia.

![La prima apertura](img/benvenuto.png)

L'archivio è fatto di cartelle normali:

```
Lezioni/
  Ricerca operativa/
    2026-11-10 Metodo del simplesso/
      fonti/       i file che hai aggiunto
      estratti/    il testo letto da ogni fonte (trascrizioni, appunti), il testo
                   di Whisper (.whisper.md) e le copie delle correzioni (versioni/)
      generati/    sintesi, mappa, flashcard
    generati/      sintesi del corso
```

Puoi aprire questi file anche senza l'app. La cartella si cambia da **Impostazioni**: per spostare l'archivio, sposta la cartella e poi sceglila lì.

## Corsi

- **Nuovo corso**: il **+** accanto a *Corsi*. Il docente è facoltativo.
- **Parole del corso**: nel corso (anche da **Rinomina**) puoi scrivere i termini tecnici e i nomi che tornano spesso, separati da virgole e scritti giusti: per esempio `ARERA, colpo d'ariete, cassa d'aria, Gauckler-Strickler`. Vengono suggeriti a Whisper, alla ripulitura e alla sintesi, così sbagliano meno; valgono dalle elaborazioni successive. Puoi aggiungerne una anche mentre correggi una trascrizione.
- Passando col mouse su un corso compaiono **Rinomina**, **Esporta il corso**, **Mostra la cartella** ed **Elimina**.
- Cliccando il **nome del corso** in cima all'elenco delle lezioni si apre la **pagina del corso** (vedi sotto). Si apre anche quando scegli un corso e nessuna lezione.
- Eliminare un corso lo sposta nel **Cestino** con tutte le sue lezioni: si può recuperare da lì.

## Lezioni

- **+ Lezione** in alto nell'elenco: titolo (puoi cambiarlo dopo) e data, che di default è oggi.
- Il **titolo** si modifica cliccandoci sopra nella pagina della lezione, la **data** dal campo accanto. Il nome della cartella segue titolo e data.
- Nell'elenco, accanto alla data, le icone contano registrazioni, foto e altri file.
- Con più di sei lezioni compare un campo per filtrarle.
- In fondo alla scheda **Fonti** ci sono le **note sulla lezione**: argomenti, cose da rivedere, compiti. Vengono usate anche per la sintesi.

## La pagina del corso

Raccoglie quello che riguarda il corso intero.

### Ripasso

Le flashcard di tutte le lezioni del corso da ripassare oggi, come il ripasso di una lezione (vedi [Studio](05-studiare.md#ripasso-a-intervalli)). Sopra ogni carta c'è la lezione da cui viene: cliccala per aprirla.

### Sintesi del corso

Una sintesi di **tutto il corso** per preparare l'esame, fatta dalle sintesi delle lezioni: è organizzata per argomenti, non lezione per lezione, e collega quello che è stato spiegato in giorni diversi. Sotto ci sono le **domande d'esame del corso**: quelle proposte per le singole lezioni, unite e scelte fra le più ricorrenti, ognuna con la traccia di risposta e le lezioni da ripassare.

- **Genera la sintesi del corso** la crea (serve almeno una lezione elaborata); poi **Rigenera** la rifà, e la precedente resta in `generati/versioni` nella cartella del corso.
- I riferimenti **L1**, **L2**… indicano le lezioni da cui viene ogni paragrafo, dalla più vecchia: cliccandoli apri la lezione.
- Quando aggiungi o rielabori lezioni l'app avvisa che la sintesi del corso è superata.
- Costa poco: dalle sintesi, non dalle trascrizioni, quindi qualche millesimo di dollaro per lezione con i modelli predefiniti.

![La sintesi del corso con le domande d'esame](img/sintesi-corso.png)

La sintesi del corso non entra nei pacchetti: chi li riceve la genera dalle proprie lezioni.

### Costi

Quanto hai speso con l'AI per il corso: il totale, mese per mese e lezione per lezione. Contano le elaborazioni (anche quelle interrotte o finite con un errore, per la parte già fatta), le domande e le ripuliture. Cliccando una lezione la apri.

![I costi del corso](img/corso-costi.png)

Con OpenRouter il costo è quello vero, comunicato dal servizio. Con Gemini il servizio non lo dice: l'app lo stima al prezzo del piano a pagamento, e lo segnala. Per le lezioni elaborate prima di questa versione conta solo l'ultima elaborazione. Il registro è nel file `costi.json` della lezione e non entra nei pacchetti che condividi.
