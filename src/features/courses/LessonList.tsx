import { FileAudio, Image, Paperclip, Plus } from "lucide-react";
import { useEffect, useState } from "react";
import { api, fmtDate, review, today, toAppError, type Course, type Lesson } from "../../lib/api";
import { Modal } from "../../lib/Modal";
import { toast } from "../../lib/toast";

export function LessonList({ course, rev, selected, onSelect, onOverview, onChanged }: { course: Course; rev: number; selected: string | null; onSelect: (id: string) => void; onOverview: () => void; onChanged: () => void }) {
  const [lessons, setLessons] = useState<Lesson[]>([]);
  const [creating, setCreating] = useState(false);
  const [filter, setFilter] = useState("");
  const [toReview, setToReview] = useState(0);
  useEffect(() => {
    let live = true;
    review
      .due("course", course.id)
      .then((d) => live && setToReview(d.cards.length))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [course.id, rev, selected]);

  useEffect(() => {
    let live = true;
    api
      .lessons(course.id)
      .then((ls) => live && setLessons(ls))
      .catch((e) => toast(toAppError(e).message, "error"));
    return () => {
      live = false;
    };
  }, [course.id, rev]);

  const f = filter.trim().toLowerCase();
  const shown = f ? lessons.filter((l) => l.title.toLowerCase().includes(f) || l.date.includes(f)) : lessons;

  return (
    <div className="flex h-full flex-col">
      <div className="space-y-2 border-b border-zinc-200 p-3 dark:border-zinc-800">
        <div className="flex items-center justify-between gap-2">
          <button className={`min-w-0 rounded-md px-1 text-left ${selected ? "hover:bg-zinc-100 dark:hover:bg-zinc-900" : ""}`} onClick={onOverview} title="Pagina del corso: ripasso, sintesi del corso, costi">
            <h2 className="truncate text-base font-semibold">{course.name}</h2>
            {course.teacher && <p className="truncate text-xs text-zinc-500">{course.teacher}</p>}
            {toReview > 0 && <p className="text-xs text-sky-700 dark:text-sky-400">{toReview} flashcard da ripassare oggi</p>}
          </button>
          <button className="btn btn-primary shrink-0" onClick={() => setCreating(true)}>
            <Plus className="size-4" /> Lezione
          </button>
        </div>
        {lessons.length > 6 && <input className="field" placeholder="Filtra le lezioni" value={filter} onChange={(e) => setFilter(e.target.value)} />}
      </div>
      <ul className="min-h-0 flex-1 overflow-auto p-2">
        {shown.map((l) => {
          const audio = l.sources.filter((s) => s.kind === "audio" || s.kind === "video").length;
          const images = l.sources.filter((s) => s.kind === "image").length;
          const other = l.sources.length - audio - images;
          const sel = l.id === selected;
          return (
            <li key={l.id}>
              <button className={`w-full rounded-md px-3 py-2 text-left ${sel ? "bg-sky-600 text-white" : "hover:bg-zinc-100 dark:hover:bg-zinc-900"}`} onClick={() => onSelect(l.id)}>
                <div className="truncate font-medium">{l.title}</div>
                <div className={`mt-0.5 flex items-center gap-3 text-xs ${sel ? "text-sky-100" : "text-zinc-500"}`}>
                  <span>{fmtDate(l.date)}</span>
                  {audio > 0 && <Count icon={<FileAudio className="size-3" />} n={audio} />}
                  {images > 0 && <Count icon={<Image className="size-3" />} n={images} />}
                  {other > 0 && <Count icon={<Paperclip className="size-3" />} n={other} />}
                </div>
              </button>
            </li>
          );
        })}
        {lessons.length === 0 && <li className="p-3 text-zinc-500">Ancora nessuna lezione in questo corso.</li>}
      </ul>
      {creating && (
        <NewLesson
          course={course}
          onClose={() => setCreating(false)}
          onCreated={(l) => {
            setCreating(false);
            onChanged();
            onSelect(l.id);
          }}
        />
      )}
    </div>
  );
}

function Count({ icon, n }: { icon: React.ReactNode; n: number }) {
  return (
    <span className="inline-flex items-center gap-0.5">
      {icon}
      {n}
    </span>
  );
}

function NewLesson({ course, onClose, onCreated }: { course: Course; onClose: () => void; onCreated: (l: Lesson) => void }) {
  const [title, setTitle] = useState("");
  const [date, setDate] = useState(today());
  const [busy, setBusy] = useState(false);
  const create = async () => {
    setBusy(true);
    try {
      onCreated(await api.createLesson(course.id, title, date));
    } catch (e) {
      toast(toAppError(e).message, "error");
      setBusy(false);
    }
  };
  return (
    <Modal
      title={`Nuova lezione · ${course.name}`}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            Annulla
          </button>
          <button className="btn btn-primary" disabled={busy || !date} onClick={create}>
            Crea
          </button>
        </>
      }
    >
      <form
        className="space-y-3"
        onSubmit={(e) => {
          e.preventDefault();
          create();
        }}
      >
        <label className="block">
          <span className="label">Titolo</span>
          <input className="field" autoFocus value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Es. Metodo del simplesso (puoi cambiarlo dopo)" />
        </label>
        <label className="block">
          <span className="label">Data</span>
          <input className="field" type="date" value={date} onChange={(e) => setDate(e.target.value)} />
        </label>
        <button type="submit" hidden />
      </form>
    </Modal>
  );
}
