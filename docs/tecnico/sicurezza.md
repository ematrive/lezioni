# Sicurezza e privacy

## Segreti

- Le **chiavi API** di Gemini e OpenRouter sono nel portachiavi del sistema (`secrets.rs`, crate `keyring`: Portachiavi su macOS, Gestione credenziali su Windows, Secret Service su Linux), in **un solo elemento** (account `chiavi-api`, JSON con le due chiavi), così macOS chiede l'accesso una volta sola. I vecchi elementi `gemini-api-key` e `openrouter-api-key` vengono letti una volta per la migrazione e lasciati lì, per un eventuale ritorno a una versione precedente. Le chiavi non sono mai scritte nell'archivio, nelle impostazioni o nei log.
- La **chiave privata degli aggiornamenti** non è nel repository: sta sul computer di sviluppo e nel segreto `TAURI_SIGNING_PRIVATE_KEY` di GitHub. Nel repository c'è solo la chiave pubblica, usata dall'app per verificare gli aggiornamenti (vedi [../rilascio.md](../rilascio.md)).
- `.gitignore` esclude `*.key` e i file `.env`.

## Dati inviati a Google

Per l'elaborazione vengono inviati a Gemini audio (in parti WAV a 16 kHz), immagini, PDF e testi della lezione, le note e, per le domande, estratti o sintesi. I file caricati con la Files API vengono cancellati appena usati (Google li cancellerebbe comunque dopo 48 ore).

Termini di Google (novembre 2026):

- con il piano gratuito Google può usare i dati per migliorare i suoi prodotti, ma **per gli utenti dello Spazio economico europeo valgono sempre le regole del piano a pagamento**, anche nella quota gratuita: i dati non vengono usati così;
- «You may use only Paid Services when making API Clients available to users in the European Economic Area»: chi distribuisce l'app ad altri nell'UE dovrebbe far usare a ciascuno il piano a pagamento. L'app lo dice nelle Impostazioni;
- età minima 18 anni.

## Pacchetti ricevuti

Un `.lezione` può arrivare da chiunque, quindi l'importazione (`share::import`):

- accetta solo percorsi interni allo zip (`enclosed_name`: niente `..` né percorsi assoluti);
- rifiuta il pacchetto se un `file` delle fonti non è un nome semplice (`is_plain_file_name`), che altrimenti farebbe aprire o cestinare file fuori dall'archivio; lo stesso controllo è in `Library::source_path` e `remove_source`;
- estrae in una cartella temporanea dentro l'archivio e sposta la lezione solo alla fine; con *Sostituisci* la lezione esistente va nel Cestino solo dopo un'estrazione riuscita;
- rifiuta pacchetti di un formato più recente di quello supportato.

## Cosa non esce dal computer quando si condivide

- `chat.json` (conversazioni della scheda Domande): mai esportato;
- le note sulla lezione: solo se l'utente lo sceglie;
- le registrazioni: escluse di default (peso e permesso del docente);
- `generati/versioni` ed `estratti/.parti`: mai.

## Webview

L'interfaccia non fa richieste di rete proprie. I file dell'archivio sono mostrati con il protocollo `asset`, il cui ambito è limitato alla cartella dell'archivio (`allow_assets`). Le pagine esportate (HTML/PDF) sono file statici senza script esterni; quella per la stampa contiene solo lo script che apre la finestra di stampa.
