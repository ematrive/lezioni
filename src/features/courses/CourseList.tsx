import { ask } from "@tauri-apps/plugin-dialog";
import { BookOpen, CircleHelp, Download, FolderOpen, MoreHorizontal, PanelLeftClose, Pencil, Plus, Search, Settings, Share2, Trash2, X } from "lucide-react";
import { useEffect, useState } from "react";
import { api, toAppError, type Course } from "../../lib/api";
import { Modal } from "../../lib/Modal";
import { toast } from "../../lib/toast";

export function CourseList({
  courses,
  selected,
  onSelect,
  onChanged,
  onSettings,
  query,
  onQuery,
  onImport,
  onExport,
  onHelp,
  onHide,
}: {
  onHelp: () => void;
  /** Collapse the column of the courses. */
  onHide: () => void;
  courses: Course[];
  selected: string | null;
  onSelect: (id: string) => void;
  onChanged: () => void;
  onSettings: () => void;
  query: string;
  onQuery: (q: string) => void;
  onImport: () => void;
  onExport: (c: Course) => void;
}) {
  /** null = closed, "new" = create, otherwise the course being edited. */
  const [editing, setEditing] = useState<Course | "new" | null>(null);
  /** Course whose "⋯" menu is open. */
  const [menu, setMenu] = useState<string | null>(null);
  useEffect(() => {
    if (!menu) return;
    const k = (e: KeyboardEvent) => e.key === "Escape" && setMenu(null);
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [menu]);

  const remove = async (c: Course) => {
    const ok = await ask(`Spostare nel Cestino il corso «${c.name}» con le sue ${c.lessons} lezioni e tutti i file?`, { title: "Elimina corso", kind: "warning", okLabel: "Sposta nel Cestino", cancelLabel: "Annulla" });
    if (!ok) return;
    try {
      await api.deleteCourse(c.id);
      onChanged();
    } catch (e) {
      toast(toAppError(e).message, "error");
    }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-1 px-2 pt-3">
      <div className="relative min-w-0 flex-1">
        <Search className="pointer-events-none absolute top-1/2 left-2 size-4 -translate-y-1/2 text-zinc-400" />
        <input
          className="field pr-7 pl-8"
          placeholder="Cerca in tutte le lezioni"
          value={query}
          onChange={(e) => onQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Escape" && onQuery("")}
        />
        {query && (
          <button className="absolute top-1/2 right-1.5 -translate-y-1/2 rounded p-0.5 text-zinc-400 hover:text-zinc-700" title="Cancella la ricerca" onClick={() => onQuery("")}>
            <X className="size-3.5" />
          </button>
        )}
      </div>
        <button className="shrink-0 rounded p-1.5 text-zinc-500 hover:bg-zinc-200 dark:hover:bg-zinc-800" title="Nascondi la colonna dei corsi" aria-label="Nascondi i corsi" onClick={onHide}>
          <PanelLeftClose className="size-4" />
        </button>
      </div>
      <div className="flex items-center justify-between px-3 pt-3 pb-2">
        <span className="text-xs font-semibold tracking-wide text-zinc-500 uppercase">Corsi</span>
        <button className="rounded p-1 text-zinc-500 hover:bg-zinc-200 dark:hover:bg-zinc-800" title="Nuovo corso" onClick={() => setEditing("new")}>
          <Plus className="size-4" />
        </button>
      </div>
      <ul className="min-h-0 flex-1 overflow-auto px-2">
        {courses.map((c) => {
          const sel = c.id === selected;
          const open = menu === c.id;
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
            <li key={c.id} className="group relative">
              <button
                className={`flex w-full items-center gap-2 rounded-md py-1.5 pr-8 pl-2 text-left ${sel ? "bg-sky-600 text-white" : "hover:bg-zinc-200 dark:hover:bg-zinc-800"}`}
                onClick={() => onSelect(c.id)}
                onDoubleClick={() => setEditing(c)}
                title={c.name}
              >
                <BookOpen className="size-4 shrink-0 opacity-70" />
                <span className="min-w-0 flex-1 truncate">{c.name}</span>
              </button>
              <span className="absolute inset-y-0 right-1 flex items-center">
                <span className={`px-1.5 text-xs ${sel ? "text-sky-100" : "text-zinc-400"} ${open || sel ? "hidden" : "group-hover:hidden"}`}>{c.lessons}</span>
                <button
                  className={`rounded p-1 ${open || sel ? "flex" : "hidden group-hover:flex"} ${sel ? "text-white hover:bg-sky-700" : "text-zinc-500 hover:bg-zinc-300 dark:hover:bg-zinc-700"}`}
                  title="Altre azioni sul corso"
                  aria-label={`Azioni sul corso ${c.name}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    setMenu(open ? null : c.id);
                  }}
                >
                  <MoreHorizontal className="size-4" />
                </button>
              </span>
              {open && (
                <>
                  <div className="fixed inset-0 z-30" onClick={() => setMenu(null)} />
                  <div className="anim-pop absolute top-full right-0 z-40 mt-1 w-52 rounded-md border border-zinc-200 bg-white p-1 text-sm text-zinc-800 shadow-lg dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200" role="menu">
                    {item("Rinomina e modifica", <Pencil className="size-4" />, () => setEditing(c))}
                    {item("Esporta il corso", <Share2 className="size-4" />, () => onExport(c))}
                    {item("Mostra la cartella", <FolderOpen className="size-4" />, () => api.reveal(c.path))}
                    {item("Elimina", <Trash2 className="size-4" />, () => remove(c), true)}
                  </div>
                </>
              )}
            </li>
          );
        })}
        {courses.length === 0 && (
          <li className="px-2 py-4 text-zinc-500">
            Nessun corso.{" "}
            <button className="text-sky-600 hover:underline" onClick={() => setEditing("new")}>
              Creane uno
            </button>
          </li>
        )}
      </ul>
      <div className="border-t border-zinc-200 p-2 dark:border-zinc-800">
        <button className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-zinc-600 hover:bg-zinc-200 dark:text-zinc-400 dark:hover:bg-zinc-800" onClick={onImport}>
          <Download className="size-4" /> Importa lezioni…
        </button>
        <button className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-zinc-600 hover:bg-zinc-200 dark:text-zinc-400 dark:hover:bg-zinc-800" onClick={onSettings}>
          <Settings className="size-4" /> Impostazioni
        </button>
        <button className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-zinc-600 hover:bg-zinc-200 dark:text-zinc-400 dark:hover:bg-zinc-800" onClick={onHelp} title="Guida (F1)">
          <CircleHelp className="size-4" /> Guida
          <kbd className="ml-auto rounded border border-zinc-300 px-1 text-[10px] text-zinc-400 dark:border-zinc-700">F1</kbd>
        </button>
      </div>
      {editing && (
        <CourseDialog
          course={editing === "new" ? null : editing}
          onClose={() => setEditing(null)}
          onSaved={(c) => {
            setEditing(null);
            onChanged();
            onSelect(c.id);
          }}
        />
      )}
    </div>
  );
}


function CourseDialog({ course, onClose, onSaved }: { course: Course | null; onClose: () => void; onSaved: (c: Course) => void }) {
  const [name, setName] = useState(course?.name ?? "");
  const [teacher, setTeacher] = useState(course?.teacher ?? "");
  const [terms, setTerms] = useState(course?.terms ?? "");
  const [busy, setBusy] = useState(false);
  const save = async () => {
    setBusy(true);
    try {
      onSaved(course ? await api.updateCourse(course.id, name, teacher, terms) : await api.createCourse(name, teacher, terms));
    } catch (e) {
      toast(toAppError(e).message, "error");
      setBusy(false);
    }
  };
  return (
    <Modal
      title={course ? "Modifica corso" : "Nuovo corso"}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            Annulla
          </button>
          <button className="btn btn-primary" disabled={busy || !name.trim()} onClick={save}>
            {course ? "Salva" : "Crea"}
          </button>
        </>
      }
    >
      <form
        className="space-y-3"
        onSubmit={(e) => {
          e.preventDefault();
          if (name.trim()) save();
        }}
      >
        <label className="block">
          <span className="label">Nome</span>
          <input className="field" autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="Ricerca operativa" />
        </label>
        <label className="block">
          <span className="label">Docente</span>
          <input className="field" value={teacher} onChange={(e) => setTeacher(e.target.value)} placeholder="facoltativo" />
        </label>
        <label className="block">
          <span className="label">Parole del corso</span>
          <textarea className="field min-h-20" value={terms} onChange={(e) => setTerms(e.target.value)} placeholder="ARERA, colpo d'ariete, cassa d'aria, Gauckler-Strickler" />
          <span className="mt-1 block text-xs text-zinc-500">Termini tecnici e nomi che tornano spesso, separati da virgole, scritti giusti. La trascrizione e la ripulitura li riconoscono meglio dalle prossime elaborazioni.</span>
        </label>
        <button type="submit" hidden />
      </form>
    </Modal>
  );
}
