import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState } from "react";
import { locale, t, tn } from "../../i18n";
import { fmtDate, share, toAppError, type Course, type OnExisting, type PackageInfo } from "../../lib/api";
import { Modal } from "../../lib/Modal";
import { toast } from "../../lib/toast";

export async function pickPackage(): Promise<string | null> {
  const r = await open({ title: t("Importa un pacchetto di lezioni"), filters: [{ name: t("Lezione"), extensions: ["lezione"] }] });
  return typeof r === "string" ? r : null;
}

/** Shows what a `.lezione` contains and imports it into a course. */
export function ImportDialog({ path, courses, onClose, onImported }: { path: string; courses: Course[]; onClose: () => void; onImported: (courseId: string, lessonId: string | null) => void }) {
  const [info, setInfo] = useState<PackageInfo | null>(null);
  /** "" = the package's course (created if missing). */
  const [target, setTarget] = useState("");
  const [onExisting, setOnExisting] = useState<OnExisting>("skip");
  const [busy, setBusy] = useState(false);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  useEffect(() => {
    share
      .inspect(path)
      .then((i) => {
        setInfo(i);
        setTarget(i.matchingCourse ?? "");
      })
      .catch((e) => {
        toast(toAppError(e).message, "error");
        closeRef.current();
      });
  }, [path]);
  if (!info) return null;
  const existing = info.lessons.filter((l) => l.exists).length;
  const run = async () => {
    setBusy(true);
    try {
      const r = await share.import(path, target || null, onExisting);
      const n = r.imported.length;
      toast([n ? tn(n, "1 lezione importata.", "{n} lezioni importate.") : "", r.skipped.length ? t("Già presenti, lasciate com'erano: {list}.", { list: r.skipped.join(", ") }) : ""].filter(Boolean).join("\n") || t("Nessuna lezione importata."));
      onImported(r.courseId, r.imported[0] ?? null);
    } catch (e) {
      toast(toAppError(e).message, "error");
      setBusy(false);
    }
  };
  return (
    <Modal
      wide
      title={t("Importa lezioni")}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            {t("Annulla")}
          </button>
          <button className="btn btn-primary" disabled={busy} onClick={run}>
            {t("Importa")}
          </button>
        </>
      }
    >
      <div className="space-y-4">
        <p className="text-sm text-zinc-500">
          {info.course.teacher
            ? t("Corso «{name}» ({teacher}), esportato il {date}.", { name: info.course.name, teacher: info.course.teacher, date: new Date(info.exported).toLocaleDateString(locale()) })
            : t("Corso «{name}», esportato il {date}.", { name: info.course.name, date: new Date(info.exported).toLocaleDateString(locale()) })}
        </p>
        <ul className="divide-y divide-zinc-200 rounded-lg border border-zinc-200 dark:divide-zinc-800 dark:border-zinc-800">
          {info.lessons.map((l) => (
            <li key={l.id} className="flex items-center gap-3 px-3 py-2">
              <div className="min-w-0 flex-1">
                <div className="truncate font-medium">{l.title}</div>
                <div className="text-xs text-zinc-500">
                  {fmtDate(l.date)} · {tn(l.sources, "1 fonte", "{n} fonti")}
                  {l.omitted ? tn(l.omitted, ", di cui 1 non inclusa (es. la registrazione)", ", di cui {n} non incluse (es. la registrazione)") : ""}
                </div>
              </div>
              {l.exists && <span className="rounded bg-amber-100 px-2 py-0.5 text-xs text-amber-800 dark:bg-amber-950 dark:text-amber-300">{t("già presente")}</span>}
            </li>
          ))}
        </ul>
        <label className="block">
          <span className="label">{t("Nel corso")}</span>
          <select className="field" value={target} onChange={(e) => setTarget(e.target.value)}>
            {!info.matchingCourse && <option value="">{t("Nuovo corso «{name}»", { name: info.course.name })}</option>}
            {courses.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
        </label>
        {existing > 0 && (
          <fieldset className="space-y-1">
            <legend className="label">{tn(existing, "Una lezione è già nell'archivio", "{n} lezioni sono già nell'archivio")}</legend>
            {(
              [
                ["skip", t("Lasciala com'è"), t("Importa solo le lezioni nuove.")],
                ["replace", t("Sostituiscila"), t("La versione nell'archivio va nel Cestino.")],
                ["copy", t("Tienile entrambe"), t("La lezione importata diventa una copia separata.")],
              ] as const
            ).map(([v, label, hint]) => (
              <label key={v} className="flex items-start gap-2">
                <input type="radio" className="mt-1" checked={onExisting === v} onChange={() => setOnExisting(v)} />
                <span>
                  {label} <span className="text-xs text-zinc-500">{hint}</span>
                </span>
              </label>
            ))}
          </fieldset>
        )}
      </div>
    </Modal>
  );
}
