import { FileAudio, FolderOpen, Image, MoreHorizontal, PanelLeftOpen, Paperclip, Plus, Share2, Trash2 } from "lucide-react";
import { ShareDialog } from "../share/ShareDialog";
import { deleteLesson } from "../lesson/LessonView";
import { useEffect, useState } from "react";
import { api, fmtDate, review, today, toAppError, type Course, type Lesson } from "../../lib/api";
import { Modal } from "../../lib/Modal";
import { toast } from "../../lib/toast";

export function LessonList({
  course,
  rev,
  selected,
  onSelect,
  onOverview,
  onShowCourses,
  onChanged,
  onDeleted,
}: {
  /** A lesson was moved to the Trash from its menu. */
  onDeleted: (lessonId: string) => void;
  course: Course;
  rev: number;
  selected: string | null;
  onSelect: (id: string) => void;
  onOverview: () => void;
  /** Set when the column of the courses is hidden: shows it again. */
  onShowCourses?: () => void;
  onChanged: () => void;
}) {
  const [lessons, setLessons] = useState<Lesson[]>([]);
  const [creating, setCreating] = useState(false);
  const [filter, setFilter] = useState("");
  const [toReview, setToReview] = useState(0);
  /** Lesson whose "⋯" menu is open, and the one being shared. */
  const [menu, setMenu] = useState<string | null>(null);
  const [sharing, setSharing] = useState<Lesson | null>(null);
  useEffect(() => {
    if (!menu) return;
    const k = (e: KeyboardEvent) => e.key === "Escape" && setMenu(null);
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [menu]);
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
          {onShowCourses && (
            <button className="shrink-0 rounded p-1 text-zinc-500 hover:bg-zinc-200 dark:hover:bg-zinc-800" title="Mostra la colonna dei corsi" aria-label="Mostra i corsi" onClick={onShowCourses}>
              <PanelLeftOpen className="size-4" />
            </button>
          )}
          <button className={`min-w-0 flex-1 rounded-md px-1 text-left ${selected ? "hover:bg-zinc-100 dark:hover:bg-zinc-900" : ""}`} onClick={onOverview} title="Pagina del corso: ripasso, sintesi del corso, costi">
            <h2 className="truncate text-base font-semibold">{course.name}</h2>
            {course.teacher && <p className="truncate text-xs text-zinc-500">{course.teacher}</p>}
          </button>
          <button className="btn btn-primary shrink-0" onClick={() => setCreating(true)}>
            <Plus className="size-4" /> Lezione
          </button>
        </div>
        {toReview > 0 && (
          <button className="block w-full rounded px-1 text-left text-xs text-sky-700 hover:underline dark:text-sky-400" onClick={onOverview} title="Apri il ripasso del corso">
            {toReview} flashcard da ripassare oggi →
          </button>
        )}
        {lessons.length > 6 && <input className="field" placeholder="Filtra le lezioni" value={filter} onChange={(e) => setFilter(e.target.value)} />}
      </div>
      <ul className="min-h-0 flex-1 overflow-auto p-2">
        {shown.map((l) => {
          const audio = l.sources.filter((s) => s.kind === "audio" || s.kind === "video").length;
          const images = l.sources.filter((s) => s.kind === "image").length;
          const other = l.sources.length - audio - images;
          const sel = l.id === selected;
          const open = menu === l.id;
          const item = (label: string, icon: React.ReactNode, f: () => void, danger = false) => (
            <button
              className={`flex w-full items-center gap-2 rounded px-2 py-1.5 text-left hover:bg-zinc-100 dark:hover:bg-zinc-800 ${danger ? "text-red-600" : ""}`}
              onClick={() => {
                setMenu(null);
                f();
              }}
            >
              {icon} {label}
            </button>
          );
          return (
            <li key={l.id} className="group relative">
              <button
                className={`absolute top-1.5 right-1.5 z-10 rounded p-1 ${open || sel ? "block" : "hidden group-hover:block"} ${sel ? "text-white hover:bg-sky-700" : "text-zinc-500 hover:bg-zinc-200 dark:hover:bg-zinc-800"}`}
                title="Altre azioni sulla lezione"
                aria-label={`Azioni sulla lezione ${l.title}`}
                onClick={() => setMenu(open ? null : l.id)}
              >
                <MoreHorizontal className="size-4" />
              </button>
              {open && (
                <>
                  <div className="fixed inset-0 z-30" onClick={() => setMenu(null)} />
                  <div className="anim-pop absolute top-9 right-1 z-40 w-52 rounded-md border border-zinc-200 bg-white p-1 text-sm text-zinc-800 shadow-lg dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200" role="menu">
                    {item("Condividi…", <Share2 className="size-4" />, () => setSharing(l))}
                    {item("Mostra la cartella", <FolderOpen className="size-4" />, () => api.reveal(l.path))}
                    {item(
                      "Elimina",
                      <Trash2 className="size-4" />,
                      async () => {
                        if (await deleteLesson(l)) {
                          setLessons((cur) => cur.filter((x) => x.id !== l.id));
                          onDeleted(l.id);
                        }
                      },
                      true,
                    )}
                  </div>
                </>
              )}
              <button className={`w-full rounded-md py-2 pr-9 pl-3 text-left ${sel ? "bg-sky-600 text-white" : "hover:bg-zinc-100 dark:hover:bg-zinc-900"}`} onClick={() => onSelect(l.id)}>
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
      {sharing && <ShareDialog course={course} lesson={sharing} onClose={() => setSharing(null)} />}
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
