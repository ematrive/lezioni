import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, GraduationCap } from "lucide-react";
import { useState } from "react";
import { api, toAppError } from "../../lib/api";

/** First run: where the archive lives. */
export function Welcome({ suggested, onDone }: { suggested: string | null; onDone: () => void }) {
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
    const dir = await open({ directory: true, title: "Cartella dell'archivio delle lezioni" });
    if (typeof dir === "string") use(dir);
  };
  return (
    <div className="flex h-full items-center justify-center p-8">
      <div className="max-w-lg space-y-5">
        <GraduationCap className="size-12 text-sky-600" />
        <h1 className="text-2xl font-semibold">Benvenuto in Lezioni</h1>
        <p className="text-zinc-600 dark:text-zinc-400">
          Qui raccogli, per ogni lezione, registrazioni, appunti dal tablet, foto e PDF. Tutto viene salvato in una cartella del computer: una sottocartella per corso e una per
          lezione, con file normali che puoi aprire anche senza l'app e sincronizzare con Google Drive, iCloud o Dropbox.
        </p>
        <div className="flex flex-wrap gap-2">
          {suggested && (
            <button className="btn btn-primary" onClick={() => use(suggested)}>
              Usa {suggested}
            </button>
          )}
          <button className="btn" onClick={pick}>
            <FolderOpen className="size-4" /> Scegli un'altra cartella…
          </button>
        </div>
        {error && <p className="text-red-600">{error}</p>}
      </div>
    </div>
  );
}
