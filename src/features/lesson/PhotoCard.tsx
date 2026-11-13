import { Pencil, Trash2 } from "lucide-react";
import { useState } from "react";
import { ai, api, fileUrl, type Lesson, type Outputs, type PhotoLink, sourceFile, type Source, toAppError } from "../../lib/api";
import { toSecs } from "../../lib/Markdown";
import { Modal } from "../../lib/Modal";
import { toast } from "../../lib/toast";
import { t } from "../../i18n";
import { clock } from "./moment";
import { photoTitle } from "./PhotosPanel";
import { photoState } from "./PhotoPicker";

// A function: the labels follow the interface language.
const byLabel = (by: PhotoLink["by"]): string =>
  ({
    orario: t("dall'orario in cui è stata scattata"),
    modello: t("trovato dal modello leggendo la trascrizione"),
    mano: t("scelto da te"),
  })[by];

/** A photo in the Sources: thumbnail, title, where it is linked; a dialog to change them. */
export function PhotoCard({ lesson, source: s, outputs, onRemove, onChanged }: { lesson: Lesson; source: Source; outputs: Outputs | null; onRemove: () => void; onChanged: (l?: Lesson) => void }) {
  const [editing, setEditing] = useState(false);
  const link = outputs?.photos.find((l) => l.image === s.id);
  const read = outputs?.extracts[s.id] !== undefined;
  const failed = link?.at === null && link.by === "modello";
  return (
    <figure className="group relative overflow-hidden rounded-lg border border-zinc-200 dark:border-zinc-800" data-photo-card={s.id}>
      <button className="block w-full" onClick={() => api.openSource(lesson.id, s.id)} title={t("Apri")}>
        <img src={fileUrl(sourceFile(lesson, s))} alt={photoTitle(s)} className="aspect-[4/3] w-full bg-zinc-100 object-cover dark:bg-zinc-900" loading="lazy" />
      </button>
      <figcaption className="px-2 py-1 text-xs">
        <span className="block truncate font-medium text-zinc-700 dark:text-zinc-300" title={s.description || photoTitle(s)}>
          {photoTitle(s)}
        </span>
        {link ? (
          <span className={`block truncate ${failed ? "text-amber-700 dark:text-amber-400" : "text-zinc-500"}`} title={failed ? link.why : link.at !== null ? byLabel(link.by) : t("Non verrà agganciata in automatico")}>
            {failed ? t("⚠ non agganciata") : photoState(link, lesson)}
          </span>
        ) : (
          <span className="block truncate text-zinc-400">{read ? t("non ancora agganciata") : t("si aggancerà quando elabori")}</span>
        )}
      </figcaption>
      <button className="absolute top-1 left-1 hidden rounded bg-black/50 p-1 text-white group-hover:block" title={t("Titolo, descrizione e minuto")} onClick={() => setEditing(true)}>
        <Pencil className="size-3.5" />
      </button>
      <button className="absolute top-1 right-1 hidden rounded bg-black/50 p-1 text-white group-hover:block" title={t("Rimuovi")} onClick={onRemove}>
        <Trash2 className="size-3.5" />
      </button>
      {editing && <PhotoDialog lesson={lesson} source={s} link={link} onClose={() => setEditing(false)} onChanged={onChanged} />}
    </figure>
  );
}

function PhotoDialog({ lesson, source: s, link, onClose, onChanged }: { lesson: Lesson; source: Source; link: PhotoLink | undefined; onClose: () => void; onChanged: (l?: Lesson) => void }) {
  const recordings = lesson.sources.filter((x) => x.kind === "audio" || x.kind === "video");
  const [title, setTitle] = useState(s.title ?? "");
  const [description, setDescription] = useState(s.description ?? "");
  const [recording, setRecording] = useState(link?.recording || recordings[0]?.id || "");
  const startTime = link?.at != null ? clock(link.at) : "";
  const [time, setTime] = useState(startTime);
  const [busy, setBusy] = useState(false);
  const validTime = /^\d{1,2}:\d{2}(:\d{2})?$/.test(time.trim());
  const timeChanged = time.trim() !== startTime || (link?.at != null && recording !== link.recording);
  const run = async (f: () => Promise<unknown>, done: string) => {
    setBusy(true);
    try {
      await f();
      toast(done);
      onChanged();
      onClose();
    } catch (e) {
      toast(toAppError(e).message, "error");
    } finally {
      setBusy(false);
    }
  };
  const save = () =>
    run(async () => {
      if (title !== (s.title ?? "") || description !== (s.description ?? "")) onChanged(await ai.setSourceInfo(lesson.id, s.id, title, description));
      if (timeChanged && validTime && recording) await ai.setPhotoLink(lesson.id, s.id, recording, toSecs(time.trim()));
    }, t("Foto salvata."));
  return (
    <Modal
      title={t("Foto")}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            {t("Annulla")}
          </button>
          <button className="btn btn-primary" disabled={busy || (timeChanged && time.trim() !== "" && !validTime)} onClick={save}>
            {t("Salva")}
          </button>
        </>
      }
    >
      <div className="space-y-3">
        <img src={fileUrl(sourceFile(lesson, s))} alt="" className="max-h-48 w-full rounded object-contain" />
        <label className="block">
          <span className="label">{t("Titolo")}</span>
          <input className="field" value={title} onChange={(e) => setTitle(e.target.value)} placeholder={s.originalName} autoFocus />
        </label>
        <label className="block">
          <span className="label">{t("Descrizione breve")}</span>
          <textarea className="field min-h-16" value={description} onChange={(e) => setDescription(e.target.value)} placeholder={t("Che cosa c'è, di che parte della lezione è (aiuta ad agganciarla)")} />
        </label>
        {recordings.length > 0 && (
          <div>
            <span className="label">{t("Minuto della registrazione")}</span>
            <div className="flex items-center gap-2">
              {recordings.length > 1 && (
                <select className="field min-w-0 flex-1" value={recording} onChange={(e) => setRecording(e.target.value)}>
                  {recordings.map((r) => (
                    <option key={r.id} value={r.id}>
                      {r.originalName}
                    </option>
                  ))}
                </select>
              )}
              <input className="field w-28 font-mono" value={time} onChange={(e) => setTime(e.target.value)} placeholder="00:12:30" aria-label={t("Minuto (hh:mm:ss)")} />
            </div>
            <p className="mt-1 text-xs text-zinc-500">
              {link?.at != null ? t("Ora: {state}, {by}.", { state: photoState(link, lesson), by: byLabel(link.by) }) : link?.by === "modello" ? t("Non agganciata: {why}", { why: link.why ?? "" }) : t("Lascia vuoto per l'aggancio automatico.")}
            </p>
            <div className="mt-2 flex flex-wrap gap-2">
              {link?.at != null && (
                <button className="btn btn-sm" disabled={busy} onClick={() => run(() => ai.setPhotoLink(lesson.id, s.id, null, null), t("Foto sganciata: non verrà più agganciata in automatico."))}>
                  {t("Sgancia")}
                </button>
              )}
              {link?.by === "mano" && (
                <button className="btn btn-sm" disabled={busy} onClick={() => run(() => ai.forgetPhotoLink(lesson.id, s.id), t("La foto si aggancerà in automatico alla prossima elaborazione."))}>
                  {t("Torna all'aggancio automatico")}
                </button>
              )}
            </div>
          </div>
        )}
      </div>
    </Modal>
  );
}
