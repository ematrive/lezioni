import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Check, Download, FolderOpen, Mic, Sparkles } from "lucide-react";
import { useEffect, useState } from "react";
import { ai, api, lessonCost, toAppError, type AiSettings, type ModelInfo, type Models, type ModelSet } from "../../lib/api";
import { Modal } from "../../lib/Modal";
import { setTheme, useTheme, type Theme } from "../../lib/theme";
import { toast } from "../../lib/toast";
import { setUiLang, systemLang, t, uiChoice, uiLang, type UiLang } from "../../i18n";

export function SettingsDialog({ libraryDir, onClose, onChanged }: { libraryDir: string; onClose: () => void; onChanged: () => void }) {
  const { theme } = useTheme();
  const [langChoice, setLangChoice] = useState<UiLang>(uiChoice);
  const change = async () => {
    const dir = await open({ directory: true, title: t("Cartella dell'archivio delle lezioni"), defaultPath: libraryDir });
    if (typeof dir !== "string" || dir === libraryDir) return;
    try {
      await api.setLibrary(dir);
      toast(t("Archivio cambiato. I file della cartella precedente restano dove sono."));
      onChanged();
    } catch (e) {
      toast(toAppError(e).message, "error");
    }
  };
  return (
    <Modal wide title={t("Impostazioni")} onClose={onClose} footer={<button className="btn" onClick={onClose}>{t("Chiudi")}</button>}>
      <div className="space-y-5">
        <div>
          <span className="label">{t("Cartella dell'archivio")}</span>
          <div className="flex items-center gap-2">
            <code className="min-w-0 flex-1 truncate rounded bg-zinc-100 px-2 py-1.5 text-xs dark:bg-zinc-800" title={libraryDir}>
              {libraryDir}
            </code>
            <button className="btn" onClick={() => api.reveal(libraryDir)} title={t("Mostra nel Finder / in Esplora file")}>
              <FolderOpen className="size-4" />
            </button>
            <button className="btn" onClick={change}>
              {t("Cambia…")}
            </button>
          </div>
          <p className="mt-1 text-xs text-zinc-500">{t("Per spostare l'archivio, sposta la cartella e poi sceglila qui.")}</p>
        </div>
        <GeminiSettings />
        <GlobalTerms />
        <div>
          <span className="label">{t("Aspetto")}</span>
          <select className="field" value={theme} onChange={(e) => setTheme(e.target.value as Theme)}>
            <option value="system">{t("Come il sistema")}</option>
            <option value="light">{t("Chiaro")}</option>
            <option value="dark">{t("Scuro")}</option>
          </select>
        </div>
        <div>
          <span className="label">{t("Lingua")}</span>
          <select
            className="field"
            value={langChoice}
            onChange={async (e) => {
              const v = e.target.value as UiLang;
              // Kept here too: choosing the language already in use mounts nothing again.
              setLangChoice(v);
              try {
                // Saved first: the app mounts again with the new language and reads the settings.
                await api.setLanguage(v, systemLang());
                setUiLang(v);
              } catch (err) {
                toast(toAppError(err).message, "error");
              }
            }}
          >
            <option value="system">{t("Come il sistema")}</option>
            <option value="it">Italiano</option>
            <option value="en">English</option>
          </select>
          <p className="mt-1 text-xs text-zinc-500">{t("La lingua dell'interfaccia. Quella delle lezioni e dei materiali si sceglie per ogni corso.")}</p>
        </div>
        <div>
          <span className="label">{t("Diagnostica")}</span>
          <button className="btn" onClick={() => api.revealLog().catch((e) => toast(toAppError(e).message, "error"))}>
            <FolderOpen className="size-4" /> {t("Mostra il file di log")}
          </button>
          <p className="mt-1 text-xs text-zinc-500">{t("Se qualcosa si blocca o va storto, il file")} <code>lezioni.log</code> {t("racconta cosa stava facendo l'app: allegalo alla segnalazione.")}</p>
        </div>
      </div>
    </Modal>
  );
}

const LINK = "text-sky-600 hover:underline";
const BOX = "space-y-3 rounded-lg border border-zinc-200 p-3 dark:border-zinc-800";

function GeminiSettings() {
  const [cfg, setCfg] = useState<AiSettings | null>(null);
  const [whisperReady, setWhisperReady] = useState(false);
  // The download runs in the backend (it goes on with Settings closed): ask how it is going.
  const [download, setDownload] = useState<string | null>(null);
  const [downloadError, setDownloadError] = useState<string | null>(null);
  const refresh = () =>
    ai
      .whisperStatus()
      .then((s) => {
        setWhisperReady(s.ready);
        setDownload(s.downloading);
        setDownloadError(s.error);
      })
      .catch(() => {});
  useEffect(() => {
    ai.settings().then(setCfg);
    refresh();
  }, []);
  useEffect(() => {
    if (!download) return;
    const t = setInterval(refresh, 1000);
    return () => clearInterval(t);
  }, [download]);
  if (!cfg) return null;
  /** Saves the settings (and a key, if given). */
  const save = async (next: AiSettings, keys: { key?: string; openrouterKey?: string } = {}) => {
    setCfg(next);
    try {
      await ai.setSettings({ ...keys, provider: next.provider, models: next.models, openrouter: next.openrouter });
      return true;
    } catch (e) {
      toast(toAppError(e).message, "error");
      return false;
    }
  };
  const getModel = async () => {
    try {
      await ai.whisperDownload();
    } catch (e) {
      toast(toAppError(e).message, "error");
    }
    refresh();
  };
  const dropModel = async () => {
    try {
      await ai.whisperDelete();
      setWhisperReady(false);
    } catch (e) {
      toast(toAppError(e).message, "error");
    }
  };
  const openrouter = cfg.provider === "openrouter";
  const engine = openrouter ? "local" : cfg.models.engine;
  return (
    <>
      <div className={BOX}>
        <div className="flex items-center gap-2 font-medium">
          <Mic className="size-4" /> {t("Trascrizione delle registrazioni")}
        </div>
        <div className="space-y-2">
          <label className="flex items-start gap-2">
            <input type="radio" className="mt-1" checked={engine === "local"} onChange={() => save({ ...cfg, models: { ...cfg.models, engine: "local" } })} />
            <span>
              <span className="font-medium">{t("Sul computer (Whisper)")}</span>
              <span className="block text-xs text-zinc-500">{t("Gratuita e senza limiti: l'audio non lascia il computer. Poi il modello scelto sotto ripulisce il testo (paragrafi, titoli, formule); se non risponde resta il testo di Whisper.")}</span>
            </span>
          </label>
          <label className={`flex items-start gap-2 ${openrouter ? "opacity-50" : ""}`}>
            <input type="radio" className="mt-1" disabled={openrouter} checked={engine === "gemini"} onChange={() => save({ ...cfg, models: { ...cfg.models, engine: "gemini" } })} />
            <span>
              <span className="font-medium">{t("Con Gemini")}</span>
              <span className="block text-xs text-zinc-500">
                {openrouter ? t("Solo con Gemini come servizio: OpenRouter non trascrive l'audio.") : t("Invia l'audio a Google: capisce anche chi parla, ma usa molta quota e col piano gratuito può essere sovraccarico.")}
              </span>
            </span>
          </label>
        </div>
        {engine === "local" && (
          <div className="flex items-center gap-2 text-sm" data-whisper>
            <span className="min-w-0 flex-1 text-zinc-600 dark:text-zinc-400">
              {download ?? (whisperReady ? t("Modello di trascrizione scaricato (574 MB).") : t("Il modello di trascrizione (574 MB) si scarica una volta sola, alla prima elaborazione o adesso. Puoi chiudere le Impostazioni: il download continua."))}
              {!download && downloadError && <span className="block text-xs text-red-600">{downloadError}</span>}
            </span>
            {!whisperReady && !download && (
              <button className="btn" onClick={getModel}>
                <Download className="size-4" /> {t("Scarica ora")}
              </button>
            )}
            {download && (
              <button className="btn" onClick={() => ai.whisperCancel().then(refresh)}>
                {t("Annulla")}
              </button>
            )}
            {whisperReady && (
              <button className="btn btn-danger" onClick={dropModel} title={t("Libera 574 MB; si riscarica alla prossima elaborazione")}>
                {t("Elimina")}
              </button>
            )}
          </div>
        )}
      </div>
      <div className={BOX}>
        <div className="flex items-center gap-2 font-medium">
          <Sparkles className="size-4" /> {t("Servizio AI per testi, foto, PDF e sintesi")}
        </div>
        <div className="flex gap-4">
          <label className="flex items-center gap-2">
            <input type="radio" checked={!openrouter} onChange={() => save({ ...cfg, provider: "gemini" })} /> Gemini (Google)
          </label>
          <label className="flex items-center gap-2">
            <input type="radio" checked={openrouter} onChange={() => save({ ...cfg, provider: "openrouter" })} /> {t("OpenRouter (molti modelli)")}
          </label>
        </div>
        {openrouter ? <OpenRouterSettings cfg={cfg} save={save} /> : <GeminiKey cfg={cfg} save={save} />}
      </div>
    </>
  );
}

type Save = (next: AiSettings, keys?: { key?: string; openrouterKey?: string }) => Promise<boolean>;

/** Key field with Verifica / Elimina; `onCheck` returns the line to show. */
function KeyField({ hasKey, onCheck, onSave }: { hasKey: boolean; onCheck: (key: string | null) => Promise<string>; onSave: (key: string) => Promise<boolean> }) {
  const [key, setKey] = useState("");
  const [state, setState] = useState<"" | "checking" | "ok">("");
  const [info, setInfo] = useState("");
  const check = async () => {
    setState("checking");
    try {
      setInfo(await onCheck(key.trim() || null));
      if (key.trim() && (await onSave(key))) setKey("");
      setState("ok");
    } catch (e) {
      setState("");
      toast(toAppError(e).message, "error");
    }
  };
  return (
    <>
      <div className="flex gap-2">
        <input
          className="field flex-1 font-mono"
          type="password"
          placeholder={hasKey ? t("•••••••• chiave salvata (scrivine una nuova per cambiarla)") : t("Chiave API")}
          value={key}
          onChange={(e) => {
            setKey(e.target.value);
            setState("");
          }}
        />
        <button className="btn btn-primary" disabled={state === "checking" || (!key.trim() && !hasKey)} onClick={check}>
          {state === "ok" ? <Check className="size-4" /> : null}
          {state === "checking" ? t("Verifica…") : state === "ok" ? t("Funziona") : key.trim() ? t("Verifica e salva") : t("Verifica")}
        </button>
        {hasKey && (
          <button className="btn btn-danger" onClick={() => onSave("")} title={t("Elimina la chiave dal portachiavi")}>
            {t("Elimina")}
          </button>
        )}
      </div>
      {state === "ok" && info && info !== "Funziona" && info !== t("Funziona") && <p className="text-xs text-emerald-700 dark:text-emerald-400">{info}</p>}
    </>
  );
}

function GeminiKey({ cfg, save }: { cfg: AiSettings; save: Save }) {
  const m = cfg.models;
  const set = (k: keyof Models, v: string) => save({ ...cfg, models: { ...m, [k]: v } });
  const [draft, setDraft] = useState(m);
  return (
    <>
      <p className="text-xs text-zinc-500">
        {t("Crea una chiave gratuita su")}{" "}
        <button className={LINK} onClick={() => openUrl("https://aistudio.google.com/apikey")}>
          aistudio.google.com/apikey
        </button>{" "}
        {t("e incollala qui: resta nel portachiavi del computer. I limiti giornalieri del piano gratuito si vedono su")}{" "}
        <button className={LINK} onClick={() => openUrl("https://aistudio.google.com/rate-limit")}>
          AI Studio
        </button>
        .
      </p>
      <KeyField hasKey={cfg.hasKey} onCheck={(k) => ai.check("gemini", k)} onSave={async (k) => (await save({ ...cfg, hasKey: !!k.trim() }, { key: k })) } />
      <div className="grid grid-cols-3 gap-3">
        {(
          [
            ["transcribe", t("Per ripulire e leggere le fonti")],
            ["generate", t("Per sintesi, mappa, studio e domande")],
            ["fallback", t("Di riserva, se Gemini è sovraccarico")],
          ] as const
        ).map(([k, label]) => (
          <label className="flex flex-col justify-end" key={k}>
            <span className="label">{label}</span>
            <input className="field font-mono" placeholder={t("nessuno")} value={draft[k]} onChange={(e) => setDraft({ ...draft, [k]: e.target.value })} onBlur={() => set(k, draft[k])} />
          </label>
        ))}
      </div>
      <p className="text-xs text-zinc-500">
        {t("Predefiniti: gemini-3.5-flash-lite per ripulire e leggere, gemini-3.8-flash per la sintesi, con flash-lite di riserva. Nell'Unione Europea i dati inviati non vengono usati da Google per migliorare i suoi prodotti, nemmeno col piano gratuito. Se condividi l'app con altri studenti nell'UE, i termini di Google chiedono che ciascuno usi il piano a pagamento.")}
      </p>
    </>
  );
}

const usd = (x: number) => {
  const n = x < 0.01 ? x.toFixed(3) : x.toFixed(2);
  return uiLang() === "it" ? n.replace(".", ",") + " $" : "$" + n;
};

function OpenRouterSettings({ cfg, save }: { cfg: AiSettings; save: Save }) {
  const [list, setList] = useState<ModelInfo[]>([]);
  const [draft, setDraft] = useState(cfg.openrouter);
  useEffect(() => {
    ai.openrouterModels()
      .then(setList)
      .catch((e) => toast(t("Listino di OpenRouter non disponibile: {error}", { error: toAppError(e).message }), "error"));
  }, []);
  const set = (k: keyof ModelSet, v: string) => save({ ...cfg, openrouter: { ...cfg.openrouter, [k]: v.trim() } });
  const info = (id: string) => list.find((m) => m.id === id.trim());
  return (
    <>
      <p className="text-xs text-zinc-500">
        {t("Una chiave per centinaia di modelli, pagati a consumo (centesimi a lezione). Crea la chiave e carica un po' di credito su")}{" "}
        <button className={LINK} onClick={() => openUrl("https://openrouter.ai/keys")}>
          openrouter.ai/keys
        </button>
        {t(". L'app chiede solo fornitori che non usano i dati per addestrare i modelli.")}
      </p>
      <KeyField hasKey={cfg.hasOpenrouterKey} onCheck={(k) => ai.check("openrouter", k)} onSave={async (k) => await save({ ...cfg, hasOpenrouterKey: !!k.trim() }, { openrouterKey: k })} />
      <datalist id="openrouter-models">
        {list.map((m) => (
          <option key={m.id} value={m.id}>
            {m.name}
          </option>
        ))}
      </datalist>
      <div className="space-y-3">
        {(
          [
            ["transcribe", t("Per ripulire la trascrizione e leggere foto e PDF")],
            ["generate", t("Per sintesi, mappa, studio e domande")],
            ["fallback", t("Di riserva, se il modello non risponde")],
          ] as const
        ).map(([k, label]) => {
          const m = info(draft[k]);
          return (
            <label className="flex flex-col justify-end" key={k}>
              <span className="label">{label}</span>
              <input className="field font-mono" list="openrouter-models" placeholder={t("nessuno")} value={draft[k]} onChange={(e) => setDraft({ ...draft, [k]: e.target.value })} onBlur={() => set(k, draft[k])} />
              <span className="mt-1 block text-xs text-zinc-500" data-model-price>
                {!draft[k].trim()
                  ? ""
                  : !list.length
                    ? "…"
                    : !m
                      ? t("Modello non trovato nel listino di OpenRouter.")
                      : t("{input} / {output} per milione di token (ingresso / uscita) · {reads} · una lezione di 1 h 30 ≈ {lesson}", {
                          input: usd(m.input),
                          output: usd(m.output),
                          reads: m.images && m.files ? t("legge immagini e PDF") : m.images ? t("legge immagini") : m.files ? t("legge PDF") : t("solo testo"),
                          lesson: usd(lessonCost(m)),
                        })}
                {k === "transcribe" && m && !m.images && <span className="block text-amber-700 dark:text-amber-400">{t("Questo modello non legge le immagini: foto e appunti a mano non verranno letti.")}</span>}
              </span>
            </label>
          );
        })}
      </div>
      <p className="text-xs text-zinc-500">
        {t("Predefiniti: google/gemini-3.5-flash-lite per ripulire e leggere (il più veloce), anthropic/claude-haiku-5.5 per la sintesi (la più fedele). Per confrontare un altro modello, cambialo qui e premi Rigenera: a fine elaborazione l'app dice quanto è costata davvero.")}
      </p>
    </>
  );
}

/** Names and terms that come back in every course: they help Whisper, the cleanup and the summary. */
function GlobalTerms() {
  const [terms, setTerms] = useState<string | null>(null);
  const [saved, setSaved] = useState("");
  useEffect(() => {
    api.globalTerms().then((t) => {
      setTerms(t.trim());
      setSaved(t.trim());
    }, () => setTerms(""));
  }, []);
  if (terms === null) return null;
  return (
    <div>
      <label className="label" htmlFor="global-terms">
        {t("Parole per tutti i corsi")}
      </label>
      <textarea
        id="global-terms"
        className="field min-h-20 text-sm"
        placeholder={t("Per esempio: nomi di docenti, enti, sigle e luoghi che tornano in più corsi, separati da virgole")}
        value={terms}
        onChange={(e) => setTerms(e.target.value)}
        onBlur={() => {
          if (terms.trim() === saved) return;
          api.setGlobalTerms(terms).then(
            () => {
              setSaved(terms.trim());
              toast(t("Parole per tutti i corsi salvate: valgono dalle prossime elaborazioni."));
            },
            (e) => toast(toAppError(e).message, "error"),
          );
        }}
      />
      <p className="mt-1 text-xs text-zinc-500">
        {t("Scritte giuste, aiutano Whisper, la ripulitura e la sintesi in ogni corso. Quelle di un solo corso si scrivono nel corso (pagina del corso → Glossario) e vengono prima di queste. Stanno nel file")} <code>parole.txt</code> {t("dell'archivio.")}
      </p>
    </div>
  );
}
