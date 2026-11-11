import { ask } from "@tauri-apps/plugin-dialog";
import { History } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { ai, toAppError, versionDate, type Lesson, type Version } from "../../lib/api";
import { diffLines } from "../../lib/diff";
import { Modal } from "../../lib/Modal";
import { toast } from "../../lib/toast";

const when = (v: Version) => versionDate(v.stamp).toLocaleString("it-IT", { dateStyle: "medium", timeStyle: "short" });

/** "Versioni precedenti (N)": hidden when there are none, disabled while the lesson is processed. */
export function VersionsButton({ lesson, versions, area, file, current, disabled, onRestored }: { lesson: Lesson; versions: Version[]; area: Version["area"]; file: string; current: string; disabled?: boolean; onRestored: (l: Lesson) => void }) {
  const [open, setOpen] = useState(false);
  const mine = versions.filter((v) => v.area === area && v.files.includes(file));
  if (!mine.length) return null;
  return (
    <>
      <button className="btn" disabled={disabled} onClick={() => setOpen(true)} title={disabled ? "Aspetta la fine dell'elaborazione" : "Guarda, confronta e ripristina le versioni precedenti"}>
        <History className="size-4" /> Versioni precedenti ({mine.length})
      </button>
      {open && <VersionsDialog lesson={lesson} versions={mine} area={area} file={file} current={current} onClose={() => setOpen(false)} onRestored={onRestored} />}
    </>
  );
}

function VersionsDialog({ lesson, versions, area, file, current, onClose, onRestored }: { lesson: Lesson; versions: Version[]; area: Version["area"]; file: string; current: string; onClose: () => void; onRestored: (l: Lesson) => void }) {
  const [sel, setSel] = useState(versions[0]);
  const [text, setText] = useState<string | null>(null);
  const [mode, setMode] = useState<"diff" | "text">("diff");
  useEffect(() => {
    let live = true;
    setText(null);
    ai.versionRead(lesson.id, area, sel.stamp, file)
      .then((t) => live && setText(t))
      .catch((e) => toast(toAppError(e).message, "error"));
    return () => {
      live = false;
    };
  }, [lesson.id, area, sel, file]);
  const diff = useMemo(() => (text === null ? [] : diffLines(text, current)), [text, current]);
  const changed = diff.filter((d) => d.kind !== "same").length;
  const restore = async () => {
    const what = area === "generati" ? "sintesi, mappa e materiale di studio" : "la trascrizione";
    const ok = await ask(`Ripristinare ${what} del ${when(sel)}?\n\nLa versione attuale non si perde: diventa a sua volta una versione precedente.`, { title: "Ripristina", okLabel: "Ripristina", cancelLabel: "Annulla" });
    if (!ok) return;
    try {
      const l = await ai.versionRestore(lesson.id, area, sel.stamp, area === "estratti" ? file : null);
      toast("Versione ripristinata.");
      onRestored(l);
      onClose();
    } catch (e) {
      toast(toAppError(e).message, "error");
    }
  };
  return (
    <Modal
      title={area === "generati" ? "Versioni precedenti della sintesi" : "Versioni precedenti della trascrizione"}
      onClose={onClose}
      wide
      footer={
        <>
          <button className="btn" onClick={onClose}>
            Chiudi
          </button>
          <button className="btn btn-primary" onClick={restore}>
            Ripristina questa versione
          </button>
        </>
      }
    >
      <div className="flex gap-4">
        <ul className="w-44 shrink-0 space-y-1 text-sm">
          {versions.map((v) => (
            <li key={v.stamp}>
              <button className={`w-full rounded px-2 py-1 text-left ${v === sel ? "bg-sky-600 text-white" : "hover:bg-zinc-100 dark:hover:bg-zinc-800"}`} onClick={() => setSel(v)}>
                {when(v)}
              </button>
            </li>
          ))}
        </ul>
        <div className="min-w-0 flex-1">
          <div className="mb-2 flex items-center gap-2 text-xs text-zinc-500">
            <div className="flex rounded-md border border-zinc-300 p-0.5 dark:border-zinc-700" role="group" aria-label="Vista">
              {(["diff", "text"] as const).map((m) => (
                <button key={m} className={`rounded px-2 py-0.5 ${mode === m ? "bg-sky-600 text-white" : "hover:bg-zinc-100 dark:hover:bg-zinc-800"}`} onClick={() => setMode(m)}>
                  {m === "diff" ? "Differenze" : "Testo"}
                </button>
              ))}
            </div>
            {mode === "diff" && text !== null && <span>{changed ? `${changed} righe diverse dalla versione attuale: in rosso c'era allora, in verde c'è adesso` : "Uguale alla versione attuale"}</span>}
          </div>
          {area === "generati" && sel.files.length > 1 && <p className="mb-2 text-xs text-zinc-500">Il ripristino riporta anche {sel.files.filter((f) => f !== file && f !== "info.json" && f !== "mappa.mmd").map((f) => ({ "mappa.json": "la mappa", "studio.json": "il materiale di studio" })[f] ?? f).join(" e ")} di quella versione.</p>}
          <div className="max-h-[55vh] overflow-auto rounded border border-zinc-200 font-mono text-xs dark:border-zinc-800" data-version>
            {text === null ? null : mode === "text" ? (
              <pre className="whitespace-pre-wrap p-2">{text}</pre>
            ) : (
              diff.map((d, i) => (
                <div key={i} className={`whitespace-pre-wrap px-2 ${d.kind === "removed" ? "bg-rose-100 text-rose-900 dark:bg-rose-950/50 dark:text-rose-200" : d.kind === "added" ? "bg-emerald-100 text-emerald-900 dark:bg-emerald-950/50 dark:text-emerald-200" : ""}`}>
                  {d.kind === "removed" ? "− " : d.kind === "added" ? "+ " : "  "}
                  {d.text || " "}
                </div>
              ))
            )}
          </div>
        </div>
      </div>
    </Modal>
  );
}
