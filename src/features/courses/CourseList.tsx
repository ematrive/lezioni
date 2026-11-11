import { ask } from "@tauri-apps/plugin-dialog";
import { BookOpen, CircleHelp, Download, FolderOpen, Pencil, Plus, Search, Settings, Share2, Trash2, X } from "lucide-react";
import { useState } from "react";
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
}: {
  onHelp: () => void;
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
      <div className="relative px-2 pt-3">
        <Search className="pointer-events-none absolute top-1/2 left-4 mt-1.5 size-4 -translate-y-1/2 text-zinc-400" />
        <input
          className="field pr-7 pl-8"
          placeholder="Cerca in tutte le lezioni"
          value={query}
          onChange={(e) => onQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Escape" && onQuery("")}
        />
        {query && (
          <button className="absolute top-1/2 right-3 mt-1.5 -translate-y-1/2 rounded p-0.5 text-zinc-400 hover:text-zinc-700" title="Cancella la ricerca" onClick={() => onQuery("")}>
            <X className="size-3.5" />
          </button>
        )}
      </div>
      <div className="flex items-center justify-between px-3 pt-3 pb-2">
        <span className="text-xs font-semibold tracking-wide text-zinc-500 uppercase">Corsi</span>
        <button className="rounded p-1 text-zinc-500 hover:bg-zinc-200 dark:hover:bg-zinc-800" title="Nuovo corso" onClick={() => setEditing("new")}>
          <Plus className="size-4" />
        </button>
      </div>
      <ul className="min-h-0 flex-1 overflow-auto px-2">
        {courses.map((c) => (
          <li key={c.id} className="group relative">
            <button
              className={`flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left ${c.id === selected ? "bg-sky-600 text-white" : "hover:bg-zinc-200 dark:hover:bg-zinc-800"}`}
              onClick={() => onSelect(c.id)}
              onDoubleClick={() => setEditing(c)}
            >
              <BookOpen className="size-4 shrink-0 opacity-70" />
              <span className="min-w-0 flex-1 truncate">{c.name}</span>
              <span className={`text-xs ${c.id === selected ? "text-sky-100" : "text-zinc-400"} group-hover:hidden`}>{c.lessons}</span>
            </button>
            <span className="absolute top-1 right-1 hidden gap-0.5 group-hover:flex">
              <IconBtn title="Rinomina" onClick={() => setEditing(c)} active={c.id === selected}>
                <Pencil className="size-3.5" />
              </IconBtn>
              <IconBtn title="Esporta il corso" onClick={() => onExport(c)} active={c.id === selected}>
                <Share2 className="size-3.5" />
              </IconBtn>
              <IconBtn title="Mostra la cartella" onClick={() => api.reveal(c.path)} active={c.id === selected}>
                <FolderOpen className="size-3.5" />
              </IconBtn>
              <IconBtn title="Elimina" onClick={() => remove(c)} active={c.id === selected}>
                <Trash2 className="size-3.5" />
              </IconBtn>
            </span>
          </li>
        ))}
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

function IconBtn({ title, onClick, active, children }: { title: string; onClick: () => void; active: boolean; children: React.ReactNode }) {
  return (
    <button
      title={title}
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
      className={`rounded p-1 ${active ? "text-white hover:bg-sky-700" : "text-zinc-500 hover:bg-zinc-300 dark:hover:bg-zinc-700"}`}
    >
      {children}
    </button>
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
