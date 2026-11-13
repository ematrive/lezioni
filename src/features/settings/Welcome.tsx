import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, GraduationCap } from "lucide-react";
import { useState } from "react";
import { api, toAppError } from "../../lib/api";
import { t } from "../../i18n";

/** First run: where the archive lives. */
export function Welcome({ suggested, onDone, onHelp }: { suggested: string | null; onDone: () => void; onHelp: () => void }) {
  const [error, setError] = useState("");
  const use = async (dir: string) => {
    try {
      await api.setLibrary(dir);
      onDone();
    } catch (e) {
      setError(toAppError(e).message);
    }
  };
  const pick = async () => {
    const dir = await open({ directory: true, title: t("Cartella dell'archivio delle lezioni") });
    if (typeof dir === "string") use(dir);
  };
  return (
    <div className="flex h-full items-center justify-center p-8">
      <div className="max-w-lg space-y-5">
        <GraduationCap className="size-12 text-sky-600" />
        <h1 className="text-2xl font-semibold">{t("Benvenuto in Lezioni")}</h1>
        <p className="text-zinc-600 dark:text-zinc-400">
          {t("Qui raccogli, per ogni lezione, registrazioni, appunti dal tablet, foto e PDF. Tutto viene salvato in una cartella del computer: una sottocartella per corso e una per lezione, con file normali che puoi aprire anche senza l'app e sincronizzare con Google Drive, iCloud o Dropbox.")}
        </p>
        <div className="flex flex-wrap gap-2">
          {suggested && (
            <button className="btn btn-primary" onClick={() => use(suggested)}>
              {t("Usa {dir}", { dir: suggested })}
            </button>
          )}
          <button className="btn" onClick={pick}>
            <FolderOpen className="size-4" /> {t("Scegli un'altra cartella…")}
          </button>
        </div>
        {error && <p className="text-red-600">{error}</p>}
        <p className="text-sm text-zinc-500">
          {t("Prima volta?")}{" "}
          <button className="text-sky-600 hover:underline" onClick={onHelp}>
            {t("Leggi la guida")}
          </button>{" "}
          {t("(anche più tardi, con F1).")}
        </p>
      </div>
    </div>
  );
}
