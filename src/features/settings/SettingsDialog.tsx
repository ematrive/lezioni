import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Check, Download, FolderOpen, KeyRound, Mic } from "lucide-react";
import { useEffect, useState } from "react";
import { ai, api, toAppError, type Models } from "../../lib/api";
import { Modal } from "../../lib/Modal";
import { setTheme, useTheme, type Theme } from "../../lib/theme";
import { toast } from "../../lib/toast";

export function SettingsDialog({ libraryDir, onClose, onChanged }: { libraryDir: string; onClose: () => void; onChanged: () => void }) {
  const { theme } = useTheme();
  const change = async () => {
    const dir = await open({ directory: true, title: "Cartella dell'archivio delle lezioni", defaultPath: libraryDir });
    if (typeof dir !== "string" || dir === libraryDir) return;
    try {
      await api.setLibrary(dir);
      toast("Archivio cambiato. I file della cartella precedente restano dove sono.");
      onChanged();
    } catch (e) {
      toast(toAppError(e).message, "error");
    }
  };
  return (
    <Modal wide title="Impostazioni" onClose={onClose} footer={<button className="btn" onClick={onClose}>Chiudi</button>}>
      <div className="space-y-5">
        <div>
          <span className="label">Cartella dell'archivio</span>
          <div className="flex items-center gap-2">
            <code className="min-w-0 flex-1 truncate rounded bg-zinc-100 px-2 py-1.5 text-xs dark:bg-zinc-800" title={libraryDir}>
              {libraryDir}
            </code>
            <button className="btn" onClick={() => api.reveal(libraryDir)} title="Mostra nel Finder / in Esplora file">
              <FolderOpen className="size-4" />
            </button>
            <button className="btn" onClick={change}>
              Cambia…
            </button>
          </div>
          <p className="mt-1 text-xs text-zinc-500">Per spostare l'archivio, sposta la cartella e poi sceglila qui.</p>
        </div>
        <GeminiSettings />
        <div>
          <span className="label">Aspetto</span>
          <select className="field" value={theme} onChange={(e) => setTheme(e.target.value as Theme)}>
            <option value="system">Come il sistema</option>
            <option value="light">Chiaro</option>
            <option value="dark">Scuro</option>
          </select>
        </div>
        <div>
          <span className="label">Diagnostica</span>
          <button className="btn" onClick={() => api.revealLog().catch((e) => toast(toAppError(e).message, "error"))}>
            <FolderOpen className="size-4" /> Mostra il file di log
          </button>
          <p className="mt-1 text-xs text-zinc-500">Se qualcosa si blocca o va storto, il file <code>lezioni.log</code> racconta cosa stava facendo l'app: allegalo alla segnalazione.</p>
        </div>
      </div>
    </Modal>
  );
}

const LINK = "text-sky-600 hover:underline";

function GeminiSettings() {
  const [hasKey, setHasKey] = useState(false);
  const [key, setKey] = useState("");
  const [models, setModels] = useState<Models>({ engine: "local", transcribe: "", generate: "", fallback: "" });
  const [state, setState] = useState<"" | "checking" | "ok">("");
  const [whisperReady, setWhisperReady] = useState(false);
  const [download, setDownload] = useState<string | null>(null);
  useEffect(() => {
    ai.settings().then((s) => {
      setHasKey(s.hasKey);
      setModels(s.models);
      setWhisperReady(s.whisperReady);
    });
  }, []);
  const getModel = async () => {
    setDownload("Avvio del download…");
    try {
      await ai.whisperDownload(setDownload);
      setWhisperReady(true);
    } catch (e) {
      toast(toAppError(e).message, "error");
    }
    setDownload(null);
  };
  const dropModel = async () => {
    try {
      await ai.whisperDelete();
      setWhisperReady(false);
    } catch (e) {
      toast(toAppError(e).message, "error");
    }
  };
  const engine = (engine: Models["engine"]) => {
    const m = { ...models, engine };
    setModels(m);
    save(null, m);
  };
  const save = async (newKey: string | null, m = models) => {
    try {
      await ai.setSettings(newKey, m);
      if (newKey !== null) setHasKey(!!newKey.trim());
      return true;
    } catch (e) {
      toast(toAppError(e).message, "error");
      return false;
    }
  };
  const check = async () => {
    setState("checking");
    try {
      await ai.check(key.trim() || null);
      if (key.trim() && (await save(key))) setKey("");
      setState("ok");
    } catch (e) {
      setState("");
      toast(toAppError(e).message, "error");
    }
  };
  return (
    <>
    <div className="space-y-3 rounded-lg border border-zinc-200 p-3 dark:border-zinc-800">
      <div className="flex items-center gap-2 font-medium">
        <Mic className="size-4" /> Trascrizione delle registrazioni
      </div>
      <div className="space-y-2">
        <label className="flex items-start gap-2">
          <input type="radio" className="mt-1" checked={models.engine === "local"} onChange={() => engine("local")} />
          <span>
            <span className="font-medium">Sul computer (Whisper)</span>
            <span className="block text-xs text-zinc-500">Gratuita e senza limiti: l'audio non lascia il computer. Poi Gemini ripulisce il testo (paragrafi, titoli, formule); se non risponde resta il testo di Whisper.</span>
          </span>
        </label>
        <label className="flex items-start gap-2">
          <input type="radio" className="mt-1" checked={models.engine === "gemini"} onChange={() => engine("gemini")} />
          <span>
            <span className="font-medium">Con Gemini</span>
            <span className="block text-xs text-zinc-500">Invia l'audio a Google: capisce anche chi parla, ma usa molta quota e col piano gratuito può essere sovraccarico.</span>
          </span>
        </label>
      </div>
      {models.engine === "local" && (
        <div className="flex items-center gap-2 text-sm" data-whisper>
          <span className="min-w-0 flex-1 text-zinc-600 dark:text-zinc-400">
            {download ?? (whisperReady ? "Modello di trascrizione scaricato (574 MB)." : "Il modello di trascrizione (574 MB) si scarica una volta sola, alla prima elaborazione o adesso.")}
          </span>
          {!whisperReady && !download && (
            <button className="btn" onClick={getModel}>
              <Download className="size-4" /> Scarica ora
            </button>
          )}
          {whisperReady && (
            <button className="btn btn-danger" onClick={dropModel} title="Libera 574 MB; si riscarica alla prossima elaborazione">
              Elimina
            </button>
          )}
        </div>
      )}
    </div>
    <div className="space-y-3 rounded-lg border border-zinc-200 p-3 dark:border-zinc-800">
      <div className="flex items-center gap-2 font-medium">
        <KeyRound className="size-4" /> Gemini (Google)
      </div>
      <p className="text-xs text-zinc-500">
        Crea una chiave gratuita su{" "}
        <button className={LINK} onClick={() => openUrl("https://aistudio.google.com/apikey")}>
          aistudio.google.com/apikey
        </button>{" "}
        e incollala qui: resta nel portachiavi del computer. I limiti giornalieri del piano gratuito si vedono su{" "}
        <button className={LINK} onClick={() => openUrl("https://aistudio.google.com/rate-limit")}>
          AI Studio
        </button>
        .
      </p>
      <div className="flex gap-2">
        <input
          className="field flex-1 font-mono"
          type="password"
          placeholder={hasKey ? "•••••••• chiave salvata (scrivine una nuova per cambiarla)" : "Chiave API"}
          value={key}
          onChange={(e) => {
            setKey(e.target.value);
            setState("");
          }}
        />
        <button className="btn btn-primary" disabled={state === "checking" || (!key.trim() && !hasKey)} onClick={check}>
          {state === "ok" ? <Check className="size-4" /> : null}
          {state === "checking" ? "Verifica…" : state === "ok" ? "Funziona" : key.trim() ? "Verifica e salva" : "Verifica"}
        </button>
        {hasKey && (
          <button className="btn btn-danger" onClick={() => save("")} title="Elimina la chiave dal portachiavi">
            Elimina
          </button>
        )}
      </div>
      <div className="grid grid-cols-3 gap-3">
        <label className="block">
          <span className="label">Per trascrivere, ripulire e leggere le fonti</span>
          <input className="field font-mono" value={models.transcribe} onChange={(e) => setModels({ ...models, transcribe: e.target.value })} onBlur={() => save(null)} />
        </label>
        <label className="block">
          <span className="label">Modello per sintesi, mappa e studio</span>
          <input className="field font-mono" value={models.generate} onChange={(e) => setModels({ ...models, generate: e.target.value })} onBlur={() => save(null)} />
        </label>
        <label className="block">
          <span className="label">Di riserva, se Gemini è sovraccarico</span>
          <input className="field font-mono" placeholder="nessuno" value={models.fallback} onChange={(e) => setModels({ ...models, fallback: e.target.value })} onBlur={() => save(null)} />
        </label>
      </div>
      <p className="text-xs text-zinc-500">
        Predefiniti: gemini-3.8-flash, e gemini-3.5-flash-lite di riserva (costa la metà, un po' meno accurato). Nell'Unione Europea i dati inviati non vengono usati da Google per migliorare i suoi prodotti, nemmeno col piano
        gratuito. Se condividi l'app con altri studenti nell'UE, i termini di Google chiedono che ciascuno usi il piano a pagamento (pochi centesimi a lezione).
      </p>
    </div>
    </>
  );
}
