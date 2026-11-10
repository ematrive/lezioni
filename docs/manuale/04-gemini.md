# Elaborare con Gemini

Le **registrazioni** vengono trascritte sul tuo computer con **Whisper**, gratis e senza limiti: l'audio non lascia il computer. **Gemini**, l'intelligenza artificiale di Google, ripulisce poi la trascrizione, legge foto e PDF e scrive sintesi, mappa e materiale di studio.

## La chiave API (una volta sola)

1. Vai su [aistudio.google.com/apikey](https://aistudio.google.com/apikey) con il tuo account Google e crea una chiave (è gratuita).
2. In Lezioni apri **Impostazioni**, incolla la chiave e premi **Verifica e salva**.

La chiave resta nel **portachiavi** del computer (Portachiavi su Mac, Gestione credenziali su Windows), non nei file dell'archivio.

![Le impostazioni](img/impostazioni.png)

Nelle impostazioni puoi anche scegliere i **modelli**: quello predefinito è `gemini-3.8-flash`; `gemini-3.5-flash-lite` costa circa la metà ed è un po' meno accurato. Il **modello di riserva** (predefinito `gemini-3.5-flash-lite`) viene usato per una richiesta quando Google risponde due volte di fila che il primo è sovraccarico; l'app lo dice al termine. Lascialo vuoto per riprovare sempre con lo stesso modello.

## Elaborare una lezione

Premi **Elabora con Gemini** in alto nella lezione. Una barra mostra a che punto è:

![L'elaborazione in corso](img/elaborazione.png)

1. ogni **registrazione** viene divisa in parti di 20 minuti e trascritta sul computer da Whisper, una parte alla volta. Mentre Whisper fa la parte successiva, Gemini ripulisce quella già pronta: toglie intercalari e ripetizioni, divide in paragrafi con titoli, scrive le formule in LaTeX e segna gli interventi degli studenti, mantenendo i minuti;
2. **foto e PDF** vengono letti, con le formule e la descrizione di grafici e schemi;
3. da **tutte le fonti insieme**, più le tue note, vengono scritte sintesi, mappa, flashcard, domande d'esame e glossario.

Sotto la barra, una seconda riga dice cosa sta aspettando (il caricamento con i MB inviati, la risposta di Gemini, un nuovo tentativo se Gemini è occupato) e da quanto tempo. Puoi continuare a usare l'app mentre lavora, anche su altre lezioni; **Annulla** la ferma.

### La trascrizione sul computer

- La prima volta l'app scarica il **modello di trascrizione** (574 MB, una volta sola). Puoi scaricarlo prima da **Impostazioni → Trascrizione delle registrazioni → Scarica ora**.
- Su un Mac con chip Apple (M1 e successivi) è veloce; su computer più vecchi o senza scheda grafica può richiedere quanto la registrazione stessa.
- Se Gemini non riesce a ripulire una parte (per esempio è sovraccarico), resta il testo di Whisper, già con i minuti, e l'app lo segnala: la ripulitura viene ritentata alla prossima elaborazione.
- Nelle Impostazioni puoi scegliere invece **Con Gemini**: l'audio viene inviato a Google, che riconosce anche chi parla, ma usa molta quota e col piano gratuito a volte è sovraccarico.

## Rielaborare

- **Rigenera** rifà sintesi, mappa e studio **riusando le trascrizioni già fatte**: costa poco. Utile dopo aver aggiunto appunti o slide.
- Dalla freccia accanto: **Rielabora anche le fonti** rifà anche trascrizioni e letture.
- Le versioni precedenti di sintesi e mappa restano in `generati/versioni` nella cartella della lezione.

Se l'elaborazione si interrompe (connessione, quota esaurita), le parti di registrazione già trascritte sono salvate: la volta dopo riparte da lì. In quel caso la sintesi non viene scritta, per non farne una senza la registrazione.

## Costi e limiti

- Con il **piano gratuito** di Google non paghi nulla, ma c'è un limite di richieste al giorno (lo vedi su [AI Studio](https://aistudio.google.com/rate-limit)). Se lo superi, l'app lo dice: riprova il giorno dopo.
- Con la **fatturazione** attiva su Google, una lezione di 2 ore costa pochi centesimi con la trascrizione sul computer (Gemini lavora solo sul testo); con la trascrizione **Con Gemini** circa 0,30–0,40 $ con `gemini-3.8-flash`. Al termine di ogni elaborazione l'app mostra i token usati e quanto sarebbero costati.

## Privacy

Con la trascrizione sul computer l'audio resta sul computer; a Google vanno il testo trascritto, le foto, i PDF e le note. Con la trascrizione **Con Gemini** viene inviato anche l'audio. Per chi è nell'Unione Europea, Google non usa questi dati per migliorare i propri prodotti, nemmeno con il piano gratuito. Evita comunque di inserire dati personali sensibili.
