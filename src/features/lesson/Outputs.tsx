import { ChevronDown, ChevronRight, Pencil, Plus, RotateCcw, Shuffle, Sparkles, Trash2, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { ai, api, courseAi, costLine, fileUrl, sourceFile, toAppError, type Course, type JobEvent, type Bookmark, type BookmarkKind, type Lesson, type MapNode, type Outputs, type Study } from "../../lib/api";
import { Markdown, MARK_ICON, MARK_LABEL } from "../../lib/Markdown";
import { toast } from "../../lib/toast";
import { VersionsButton } from "./Versions";
import { FlashCard, ReviewSession } from "../study/Review";

// ---------- running jobs, by lesson (they go on while you look at other lessons) ----------

export interface JobState {
  jobId: string | null;
  done: number;
  total: number;
  message: string;
  /** What the step is waiting for (upload, Gemini's answer, a retry). */
  detail?: string;
  /** Date.now() of the last step: the bar shows how long it has been going. */
  since: number;
}

let jobs: Record<string, JobState> = {};
const listeners = new Set<() => void>();
const setJob = (lessonId: string, s: JobState | null) => {
  const next = { ...jobs };
  if (s) next[lessonId] = s;
  else delete next[lessonId];
  jobs = next;
  listeners.forEach((l) => l());
};

export function useJob(lessonId: string): JobState | null {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => jobs[lessonId] ?? null,
  );
}

export async function startProcessing(lesson: Lesson, force: boolean, onFinished: () => void) {
  if (jobs[lesson.id]) return;
  setJob(lesson.id, { jobId: null, done: 0, total: lesson.sources.length + 1, message: "Avvio…", since: Date.now() });
  const onEvent = (e: JobEvent) => {
    const cur = jobs[lesson.id] ?? { jobId: null, done: 0, total: 1, message: "", since: Date.now() };
    if (e.event === "step") setJob(lesson.id, { ...cur, done: e.done, total: e.total, message: e.message, detail: undefined, since: Date.now() });
    else if (e.event === "detail") setJob(lesson.id, { ...cur, detail: e.message });
    else {
      setJob(lesson.id, null);
      if (e.event === "done") {
        const tokens = e.usage.promptTokenCount + e.usage.candidatesTokenCount + e.usage.thoughtsTokenCount;
        toast(`«${lesson.title}» elaborata: ${tokens.toLocaleString("it-IT")} token (${costLine(e.usage)}).`);
        if (e.warnings.length) toast(e.warnings.join("\n"), "error");
      } else if (e.message !== "annullato") toast(e.message, "error");
      onFinished();
    }
  };
  try {
    const jobId = await ai.process(lesson.id, { force }, onEvent);
    if (jobs[lesson.id]) setJob(lesson.id, { ...jobs[lesson.id], jobId });
  } catch (e) {
    setJob(lesson.id, null);
    toast(toAppError(e).message, "error");
  }
}

/** The course summary, as a job of the course (same store and bar as the lessons). */
export async function startCourseSummary(course: Course, onFinished: () => void) {
  if (jobs[course.id]) return;
  setJob(course.id, { jobId: null, done: 0, total: 1, message: "Avvio…", since: Date.now() });
  const onEvent = (e: JobEvent) => {
    const cur = jobs[course.id] ?? { jobId: null, done: 0, total: 1, message: "", since: Date.now() };
    if (e.event === "step") setJob(course.id, { ...cur, done: e.done, total: e.total, message: e.message, detail: undefined, since: Date.now() });
    else if (e.event === "detail") setJob(course.id, { ...cur, detail: e.message });
    else {
      setJob(course.id, null);
      if (e.event === "done") toast(`Sintesi del corso «${course.name}» pronta (${costLine(e.usage)}).`);
      else if (e.message !== "annullato") toast(e.message, "error");
      onFinished();
    }
  };
  try {
    const jobId = await courseAi.summarize(course.id, onEvent);
    if (jobs[course.id]) setJob(course.id, { ...jobs[course.id], jobId });
  } catch (e) {
    setJob(course.id, null);
    toast(toAppError(e).message, "error");
  }
}

export function JobBar({ job }: { job: JobState }) {
  const pct = job.total ? Math.round((job.done / job.total) * 100) : 0;
  // Re-render every second for the elapsed time: a long Gemini call must not look frozen.
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);
  const secs = Math.max(0, Math.floor((now - job.since) / 1000));
  const elapsed = `${Math.floor(secs / 60)}:${String(secs % 60).padStart(2, "0")}`;
  return (
    <div className="flex items-center gap-3 border-b border-sky-200 bg-sky-50 px-4 py-2 dark:border-sky-900 dark:bg-sky-950/40">
      <Sparkles className="size-4 shrink-0 animate-pulse text-sky-600" />
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm">{job.message}</div>
        <div className="truncate text-xs text-zinc-500" data-job-detail>
          {job.detail ?? "In corso…"} · {elapsed}
        </div>
        <div className="mt-1 h-1 overflow-hidden rounded bg-sky-200 dark:bg-sky-900">
          <div className="h-full bg-sky-600 transition-all" style={{ width: `${Math.max(pct, 3)}%` }} />
        </div>
      </div>
      <button className="btn" disabled={!job.jobId} onClick={() => job.jobId && ai.cancel(job.jobId)} title="Annulla l'elaborazione">
        <X className="size-4" /> Annulla
      </button>
    </div>
  );
}

// ---------- tabs ----------

export interface Seek {
  sourceId: string;
  secs: number;
  /** Bumped so the same position can be asked twice. */
  n: number;
}

function Placeholder({ text, action }: { text: string; action?: React.ReactNode }) {
  return (
    <div className="mx-auto max-w-lg pt-12 text-center text-zinc-500">
      <p>{text}</p>
      {action && <div className="mt-4">{action}</div>}
    </div>
  );
}

// ---------- corrections by hand ----------

/** What a tab needs to correct its texts: off (undefined) while the lesson is being processed. */
export interface Editing {
  course: Course | null;
  /** After a save: the lesson (when it changed) and the outputs are loaded again. */
  onSaved: (lesson?: Lesson) => void;
}

const escapeRe = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const fmtDate = (iso: string) => new Date(iso).toLocaleString("it-IT", { dateStyle: "short", timeStyle: "short" });

/** Adds a term to the course's ones (read fresh, so that two adds in a row both stay). */
async function addCourseTerm(courseId: string, term: string) {
  const c = (await api.courses()).find((x) => x.id === courseId);
  if (!c) return;
  const have = c.terms.split(/[,;\n]/).map((t) => t.trim().toLowerCase());
  if (have.includes(term.toLowerCase())) {
    toast(`«${term}» è già tra le parole del corso.`);
    return;
  }
  await api.updateCourse(c.id, c.name, c.teacher, c.terms.trim() ? `${c.terms.trim()}, ${term}` : term);
  toast(`«${term}» aggiunta alle parole del corso: varrà dalle prossime elaborazioni.`);
}

/** The paragraphs of a timed transcript touched by a selection: where they are in the text and
 *  their times (`to` = time of the next paragraph, null at the end). Headings just before the
 *  next paragraph stay out. */
export function pieceAt(text: string, selStart: number, selEnd: number): { start: number; end: number; from: number; to: number | null } | null {
  const marks = [...text.matchAll(/^\[(\d{2}):(\d{2}):(\d{2})\]/gm)].map((m) => ({ pos: m.index ?? 0, secs: Number(m[1]) * 3600 + Number(m[2]) * 60 + Number(m[3]) }));
  if (!marks.length) return null;
  let a = 0;
  marks.forEach((m, i) => m.pos <= selStart && (a = i));
  let b = a;
  marks.forEach((m, i) => m.pos < Math.max(selEnd, selStart + 1) && i > b && (b = i));
  const next = marks[b + 1];
  let end = next ? next.pos : text.length;
  const tail = text.slice(marks[b].pos, end).replace(/(\n+#{1,6} [^\n]*)*\s*$/, "");
  end = marks[b].pos + tail.length;
  return { start: marks[a].pos, end, from: marks[a].secs, to: next ? next.secs : null };
}

/** Choice of the model for a cleanup again: the ones of Settings, and OpenRouter's list. */
function useModelChoices(): string[] {
  const [models, setModels] = useState<string[]>([]);
  useEffect(() => {
    let live = true;
    ai.settings()
      .then(async (s) => {
        const set = s.provider === "openrouter" ? s.openrouter : s.models;
        const own = [set.transcribe, set.generate, set.fallback];
        const more = s.provider === "openrouter" ? (await ai.openrouterModels().catch(() => [])).filter((m) => m.input > 0).map((m) => m.id) : [];
        if (live) setModels([...new Set([...own, ...more])]);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, []);
  return models;
}

/** Selection in a transcript → "clean this piece again", then replace or keep. */
function Reclean({ lessonId, sourceId, text, sel, onReplace }: { lessonId: string; sourceId: string; text: string; sel: [number, number]; onReplace: (start: number, end: number, piece: string) => void }) {
  const models = useModelChoices();
  const [model, setModel] = useState("");
  const [busy, setBusy] = useState(false);
  const [proposal, setProposal] = useState<{ start: number; end: number; text: string; note: string } | null>(null);
  const piece = sel[1] > sel[0] ? pieceAt(text, sel[0], sel[1]) : null;
  const go = async () => {
    if (!piece) return;
    setBusy(true);
    try {
      const r = await ai.reclean(lessonId, sourceId, piece.from, piece.to, model || null);
      setProposal({ start: piece.start, end: piece.end, text: r.text, note: `${r.model}, ${costLine(r.usage)}` });
    } catch (e) {
      toast(toAppError(e).message, "error");
    } finally {
      setBusy(false);
    }
  };
  if (proposal)
    return (
      <div className="space-y-2 rounded-md border border-sky-300 bg-sky-50 p-3 dark:border-sky-800 dark:bg-sky-950/40">
        <p className="text-sm">
          Nuova ripulitura dal testo di Whisper <span className="text-xs text-zinc-500">({proposal.note})</span>
        </p>
        <div className="grid gap-2 md:grid-cols-2">
          <div>
            <p className="mb-1 text-xs text-zinc-500">Ora</p>
            <pre className="max-h-64 overflow-auto whitespace-pre-wrap rounded bg-white p-2 text-xs dark:bg-zinc-900">{text.slice(proposal.start, proposal.end)}</pre>
          </div>
          <div>
            <p className="mb-1 text-xs text-zinc-500">Proposta</p>
            <pre className="max-h-64 overflow-auto whitespace-pre-wrap rounded bg-white p-2 text-xs dark:bg-zinc-900" data-proposal>
              {proposal.text}
            </pre>
          </div>
        </div>
        <div className="flex justify-end gap-2">
          <button className="btn" onClick={() => setProposal(null)}>
            Tieni com'era
          </button>
          <button
            className="btn btn-primary"
            onClick={() => {
              onReplace(proposal.start, proposal.end, proposal.text);
              setProposal(null);
            }}
          >
            Sostituisci
          </button>
        </div>
      </div>
    );
  if (!piece) return <p className="text-xs text-zinc-500">Seleziona un pezzo del testo per farlo ripulire di nuovo dal testo di Whisper, anche con un altro modello.</p>;
  return (
    <div className="flex flex-wrap items-center gap-2 text-sm">
      <button className="btn" disabled={busy} onClick={go}>
        <Sparkles className="size-4" /> {busy ? "Ripulitura…" : "Ripulisci di nuovo la selezione"}
      </button>
      <label className="flex items-center gap-1 text-xs text-zinc-500">
        con
        <select className="field max-w-72 py-0.5 text-sm" value={model} onChange={(e) => setModel(e.target.value)} aria-label="Modello">
          <option value="">il modello della ripulitura</option>
          {models.map((m) => (
            <option key={m} value={m}>
              {m}
            </option>
          ))}
        </select>
      </label>
    </div>
  );
}

/** Markdown text editor with find and replace (all occurrences, ignoring case). With `reclean`,
 *  a selected piece of a transcript can be cleaned again from Whisper's text. */
export function TextEditor({ initial, course, onSave, onCancel, reclean }: { initial: string; course?: Course | null; onSave: (text: string) => Promise<void>; onCancel: () => void; reclean?: { lessonId: string; sourceId: string } }) {
  const [text, setText] = useState(initial);
  const [sel, setSel] = useState<[number, number]>([0, 0]);
  const [find, setFind] = useState("");
  const [repl, setRepl] = useState("");
  const [replaced, setReplaced] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const re = find ? new RegExp(escapeRe(find), "gi") : null;
  const n = re ? (text.match(re)?.length ?? 0) : 0;
  const save = async () => {
    setSaving(true);
    try {
      await onSave(text);
    } catch (e) {
      toast(toAppError(e).message, "error");
      setSaving(false);
    }
  };
  return (
    <div className="space-y-2" data-editor>
      <div className="flex flex-wrap items-center gap-2">
        <input className="field max-w-52" placeholder="Trova" value={find} onChange={(e) => setFind(e.target.value)} spellCheck={false} />
        <input className="field max-w-52" placeholder="Sostituisci con" value={repl} onChange={(e) => setRepl(e.target.value)} spellCheck={false} />
        <button
          className="btn"
          disabled={!n}
          onClick={() => {
            if (!re) return;
            setText(text.replace(re, () => repl));
            setReplaced(repl.trim() || null);
            toast(`${n} ${n === 1 ? "sostituzione" : "sostituzioni"}.`);
          }}
        >
          Sostituisci tutte ({n})
        </button>
        {replaced && course && (
          <button
            className="text-sm text-sky-700 hover:underline dark:text-sky-400"
            onClick={() => {
              addCourseTerm(course.id, replaced).catch((e) => toast(toAppError(e).message, "error"));
              setReplaced(null);
            }}
          >
            Aggiungi «{replaced}» alle parole del corso
          </button>
        )}
      </div>
      {reclean && <Reclean lessonId={reclean.lessonId} sourceId={reclean.sourceId} text={text} sel={sel} onReplace={(a, b, piece) => setText(text.slice(0, a) + piece + text.slice(b))} />}
      <textarea
        className="field min-h-[60vh] font-mono text-sm"
        value={text}
        onChange={(e) => setText(e.target.value)}
        onSelect={(e) => setSel([e.currentTarget.selectionStart, e.currentTarget.selectionEnd])}
        onMouseUp={(e) => setSel([e.currentTarget.selectionStart, e.currentTarget.selectionEnd])}
        onKeyUp={(e) => setSel([e.currentTarget.selectionStart, e.currentTarget.selectionEnd])}
      />
      <div className="flex justify-end gap-2">
        <button className="btn" onClick={onCancel}>
          Annulla
        </button>
        <button className="btn btn-primary" disabled={saving || text === initial} onClick={save}>
          Salva
        </button>
      </div>
    </div>
  );
}

function EditButton({ editing, onClick, label = "Modifica" }: { editing?: Editing; onClick: () => void; label?: string }) {
  return (
    <button className="btn" disabled={!editing} onClick={onClick} title={editing ? "Correggi il testo a mano" : "Aspetta la fine dell'elaborazione"}>
      <Pencil className="size-4" /> {label}
    </button>
  );
}

/** Corrections not yet in the summary: they need a Rigenera. */
export function staleSources(lesson: Lesson, outputs: Outputs) {
  const generated = outputs.info?.generated;
  return lesson.sources.filter((s) => s.edited && (!generated || s.edited > generated));
}

/** Bookmarks added or changed after the summary was made. */
const newMarks = (outputs: Outputs) => outputs.bookmarks.some((b) => !outputs.info || b.created > outputs.info.generated);

function RegenerateHint({ lesson, outputs }: { lesson: Lesson; outputs: Outputs }) {
  if (!outputs.summary) return null;
  const edited = staleSources(lesson, outputs).length > 0;
  if (!edited && !newMarks(outputs)) return null;
  return (
    <p className="mb-4 rounded-md border border-amber-300 bg-amber-50 px-3 py-2 text-sm text-amber-900 dark:border-amber-800 dark:bg-amber-950/40 dark:text-amber-200">
      {edited ? "Hai corretto la trascrizione" : "Hai aggiunto dei segnaposti"}: premi <b>Rigenera</b> per aggiornare sintesi, mappa e materiale di studio.
    </p>
  );
}

const clock = (secs: number) => {
  const s = Math.floor(secs);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(Math.floor(s / 3600))}:${p(Math.floor(s / 60) % 60)}:${p(s % 60)}`;
};

/** Next to a player: mark the current minute. */
function MarkButtons({ onMark, onNote }: { onMark: (kind: BookmarkKind) => void; onNote: () => void }) {
  const b = (label: string, key: string, icon: string, f: () => void) => (
    <button className="btn px-2 py-1" onClick={f} title={`${label} (tasto ${key} mentre ascolti)`} aria-label={label}>
      {icon}
    </button>
  );
  return (
    <span className="flex shrink-0 gap-1" role="group" aria-label="Segna questo punto">
      {b("Importante", "I", MARK_ICON.importante, () => onMark("importante"))}
      {b("Non ho capito", "D", MARK_ICON.dubbio, () => onMark("dubbio"))}
      {b("Nota", "N", MARK_ICON.nota, onNote)}
    </span>
  );
}

/** Under the header of a recording: the note being written and the marks, folded. */
function MarkList({
  lessonId,
  sourceId,
  marks,
  draft,
  setDraft,
  seekTo,
  onChanged,
}: {
  lessonId: string;
  sourceId: string;
  marks: Bookmark[];
  draft: { at: number; note: string } | null;
  setDraft: (d: { at: number; note: string } | null) => void;
  seekTo: (at: number) => void;
  onChanged: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [editId, setEditId] = useState<string | null>(null);
  const [editNote, setEditNote] = useState("");
  const run = async (f: () => Promise<unknown>) => {
    try {
      await f();
      onChanged();
    } catch (e) {
      toast(toAppError(e).message, "error");
    }
  };
  if (!draft && !marks.length) return null;
  return (
    <div className="mb-4 text-sm">
      {draft && (
        <form
          className="mb-2 flex items-center gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            const d = draft;
            setDraft(null);
            setOpen(true);
            run(() => ai.addBookmark(lessonId, sourceId, d.at, "nota", d.note));
          }}
        >
          <span className="font-mono text-xs text-zinc-500">
            {MARK_ICON.nota} {clock(draft.at)}
          </span>
          <input autoFocus className="field flex-1" placeholder="Cosa vuoi ricordare di questo punto?" value={draft.note} onChange={(e) => setDraft({ ...draft, note: e.target.value })} onKeyDown={(e) => e.key === "Escape" && setDraft(null)} />
          <button className="btn btn-primary py-0.5">Aggiungi</button>
          <button type="button" className="btn py-0.5" onClick={() => setDraft(null)}>
            Annulla
          </button>
        </form>
      )}
      {marks.length > 0 && (
        <button className="flex items-center gap-1 text-xs text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-200" onClick={() => setOpen(!open)} aria-expanded={open}>
          {open ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}
          {marks.length} {marks.length === 1 ? "segnaposto" : "segnaposti"} {!open && <span className="ml-1 tracking-widest">{marks.map((m) => MARK_ICON[m.kind]).join("")}</span>}
        </button>
      )}
      {open && marks.length > 0 && (
        <ul className="mt-1 space-y-1 border-l-2 border-zinc-200 pl-3 dark:border-zinc-800" aria-label="Segnaposti">
          {marks.map((m) => (
            <li key={m.id} className="group flex items-center gap-2">
              <span title={MARK_LABEL[m.kind]}>{MARK_ICON[m.kind]}</span>
              <button className="rounded bg-sky-100 px-1 font-mono text-xs text-sky-800 hover:bg-sky-200 dark:bg-sky-950 dark:text-sky-300" onClick={() => seekTo(m.at)}>
                {clock(m.at)}
              </button>
              {editId === m.id ? (
                <input
                  autoFocus
                  className="field flex-1 py-0.5"
                  value={editNote}
                  onChange={(e) => setEditNote(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") e.currentTarget.blur();
                    if (e.key === "Escape") setEditId(null);
                  }}
                  onBlur={() => {
                    setEditId(null);
                    if (editNote.trim() !== m.note) run(() => ai.updateBookmark(lessonId, m.id, m.kind, editNote));
                  }}
                />
              ) : (
                <button
                  className={`min-w-0 flex-1 truncate text-left ${m.note ? "" : "text-zinc-400"}`}
                  onClick={() => {
                    setEditId(m.id);
                    setEditNote(m.note);
                  }}
                  title="Clicca per scrivere una nota"
                >
                  {m.note || MARK_LABEL[m.kind]}
                </button>
              )}
              <button className="text-zinc-400 opacity-0 group-hover:opacity-100 hover:text-red-600" onClick={() => run(() => ai.removeBookmark(lessonId, m.id))} title="Elimina il segnaposto" aria-label="Elimina il segnaposto">
                <Trash2 className="size-3.5" />
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

export function TranscriptTab({ lesson, outputs, seek, process, editing, onBookmarks }: { lesson: Lesson; outputs: Outputs; seek: Seek | null; process: React.ReactNode; editing?: Editing; onBookmarks: () => void }) {
  const [view, setView] = useState<Record<string, "clean" | "raw">>({});
  const [editingId, setEditingId] = useState<string | null>(null);
  const players = useRef<Record<string, HTMLAudioElement | HTMLVideoElement | null>>({});
  const sections = useRef<Record<string, HTMLElement | null>>({});
  /** The recording played last: the keys I, D and N mark it. */
  const playing = useRef<string | null>(null);
  /** The note being written, at a minute of a recording. */
  const [draft, setDraft] = useState<{ sourceId: string; at: number; note: string } | null>(null);
  const mark = useCallback(
    (sid: string, kind: BookmarkKind) => {
      const at = players.current[sid]?.currentTime ?? 0;
      ai.addBookmark(lesson.id, sid, at, kind).then(
        () => {
          toast(`${MARK_LABEL[kind]} a ${clock(at)}`);
          onBookmarks();
        },
        (err) => toast(toAppError(err).message, "error"),
      );
    },
    [lesson.id, onBookmarks],
  );
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      const t = e.target;
      if (e.metaKey || e.ctrlKey || e.altKey || (t instanceof HTMLElement && (t.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(t.tagName)))) return;
      const media = lesson.sources.filter((s) => (s.kind === "audio" || s.kind === "video") && !s.omitted && players.current[s.id]);
      const sid = playing.current ?? (media.length === 1 ? media[0].id : null);
      const p = sid ? players.current[sid] : null;
      if (!sid || !p) return;
      const kind = ({ i: "importante", d: "dubbio" } as const)[e.key.toLowerCase() as "i" | "d"];
      if (kind) {
        e.preventDefault();
        mark(sid, kind);
      } else if (e.key.toLowerCase() === "n") {
        e.preventDefault();
        setDraft({ sourceId: sid, at: p.currentTime, note: "" });
      }
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [lesson, mark]);
  useEffect(() => {
    if (!seek) return;
    sections.current[seek.sourceId]?.scrollIntoView({ behavior: "smooth", block: "start" });
    const p = players.current[seek.sourceId];
    if (p && seek.secs >= 0) {
      p.currentTime = seek.secs;
      p.play().catch(() => {});
    }
  }, [seek]);
  const withText = lesson.sources.filter((s) => outputs.extracts[s.id] !== undefined);
  if (!withText.length) return <Placeholder text="Qui comparirà la trascrizione delle registrazioni e il testo letto da foto, appunti e PDF." action={process} />;
  const onRef = (href: string) => {
    const m = href.match(/^#t=(.+)@(\d+)$/);
    const p = m && players.current[m[1]];
    if (p) {
      p.currentTime = Number(m[2]);
      p.play().catch(() => {});
    }
  };
  return (
    <div className="mx-auto max-w-3xl space-y-10">
      <RegenerateHint lesson={lesson} outputs={outputs} />
      {withText.map((s) => {
        const media = s.kind === "audio" || s.kind === "video";
        const raw = outputs.raw[s.id];
        const showRaw = !!raw && view[s.id] === "raw";
        return (
          <section key={s.id} ref={(el) => void (sections.current[s.id] = el)} className="scroll-mt-4">
            <div className="sticky -top-4 z-10 -mx-4 -mt-4 mb-3 border-b border-zinc-200 bg-zinc-50 px-4 pt-4 pb-2 shadow-sm dark:border-zinc-800 dark:bg-zinc-950">
              <div className="flex flex-wrap items-center gap-2">
                <h3 className="min-w-0 flex-1 truncate font-semibold" title={s.originalName}>
                  {s.originalName}
                </h3>
                {s.edited && editingId !== s.id && <span className="text-xs text-zinc-500">corretta a mano il {fmtDate(s.edited)}</span>}
                {raw && editingId !== s.id && (
                  <div className="flex rounded-md border border-zinc-300 p-0.5 text-sm dark:border-zinc-700" role="group" aria-label="Versione del testo">
                    {(["clean", "raw"] as const).map((v) => (
                      <button
                        key={v}
                        className={`rounded px-2 py-0.5 ${(view[s.id] ?? "clean") === v ? "bg-sky-600 text-white" : "text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"}`}
                        onClick={() => setView({ ...view, [s.id]: v })}
                        title={v === "raw" ? "Il testo come l'ha riconosciuto Whisper, prima della ripulitura" : "Il testo ripulito dal modello"}
                      >
                        {v === "clean" ? "Ripulita" : "Whisper"}
                      </button>
                    ))}
                  </div>
                )}
                {editingId !== s.id && !showRaw && <EditButton editing={editing} onClick={() => setEditingId(s.id)} />}
                {editingId !== s.id && <VersionsButton lesson={lesson} versions={outputs.versions} area="estratti" file={`${s.file}.md`} current={outputs.extracts[s.id]} disabled={!editing} onRestored={(l) => editing?.onSaved(l)} />}
              </div>
              {s.omitted && <p className="mt-1 text-xs text-zinc-500">La registrazione non era nel pacchetto ricevuto: c'è solo la trascrizione.</p>}
              {media && !s.omitted && (
                <div className="mt-2 flex items-center gap-2">
                  {s.kind === "audio" ? (
                    <audio ref={(el) => void (players.current[s.id] = el)} onPlay={() => (playing.current = s.id)} className="h-10 min-w-0 flex-1" controls preload="metadata" src={fileUrl(sourceFile(lesson, s))} title="Clicca un minuto nel testo per ascoltare da quel punto" />
                  ) : (
                    <video ref={(el) => void (players.current[s.id] = el)} onPlay={() => (playing.current = s.id)} className="max-h-40 min-w-0 flex-1 rounded" controls preload="metadata" src={fileUrl(sourceFile(lesson, s))} title="Clicca un minuto nel testo per guardare da quel punto" />
                  )}
                  {editingId !== s.id && <MarkButtons onMark={(k) => mark(s.id, k)} onNote={() => setDraft({ sourceId: s.id, at: players.current[s.id]?.currentTime ?? 0, note: "" })} />}
                </div>
              )}
            </div>
            {media && !s.omitted && editingId !== s.id && (
              <MarkList
                lessonId={lesson.id}
                sourceId={s.id}
                marks={outputs.bookmarks.filter((b) => b.sourceId === s.id)}
                draft={draft?.sourceId === s.id ? draft : null}
                setDraft={(d) => setDraft(d && { ...d, sourceId: s.id })}
                seekTo={(at) => {
                  const p = players.current[s.id];
                  if (p) {
                    p.currentTime = at;
                    p.play().catch(() => {});
                  }
                }}
                onChanged={onBookmarks}
              />
            )}
            {editingId === s.id && editing ? (
              <TextEditor
                initial={outputs.extracts[s.id]}
                course={editing.course}
                reclean={raw ? { lessonId: lesson.id, sourceId: s.id } : undefined}
                onCancel={() => setEditingId(null)}
                onSave={async (text) => {
                  const l = await ai.saveExtract(lesson.id, s.id, text);
                  setEditingId(null);
                  editing.onSaved(l);
                  toast(outputs.summary ? "Correzione salvata. Premi Rigenera per aggiornare la sintesi." : "Correzione salvata.");
                }}
              />
            ) : (
              <Markdown text={showRaw ? raw : outputs.extracts[s.id]} source={media ? s.id : undefined} onRef={onRef} marks={media ? outputs.bookmarks.filter((b) => b.sourceId === s.id) : undefined} />
            )}
          </section>
        );
      })}
    </div>
  );
}

export function SummaryTab({ lesson, outputs, onRef, process, editing }: { lesson: Lesson; outputs: Outputs; onRef: (href: string) => void; process: React.ReactNode; editing?: Editing }) {
  const [edit, setEdit] = useState(false);
  if (!outputs.summary) return <Placeholder text="La sintesi della lezione, costruita da tutte le fonti insieme, con i riferimenti al minuto della registrazione." action={process} />;
  return (
    <div className="mx-auto max-w-3xl">
      <RegenerateHint lesson={lesson} outputs={outputs} />
      <div className="mb-4 flex items-start gap-3">
        {outputs.info && (
          <p className="flex-1 text-xs text-zinc-500">
            Generata il {new Date(outputs.info.generated).toLocaleString("it-IT")} con {outputs.info.model} ({costLine(outputs.info.usage)})
            {outputs.info.edited && <>, corretta a mano il {fmtDate(outputs.info.edited)}</>}. I riferimenti [F1 00:12:30] portano alla trascrizione.
          </p>
        )}
        {!edit && <VersionsButton lesson={lesson} versions={outputs.versions} area="generati" file="sintesi.md" current={outputs.summary} disabled={!editing} onRestored={() => editing?.onSaved()} />}
        {!edit && <EditButton editing={editing} onClick={() => setEdit(true)} />}
      </div>
      {edit && editing ? (
        <TextEditor
          initial={outputs.summary}
          course={editing.course}
          onCancel={() => setEdit(false)}
          onSave={async (text) => {
            await ai.saveSummary(lesson.id, text);
            setEdit(false);
            editing.onSaved();
            toast("Sintesi salvata. Con Rigenera la versione corretta resta in generati/versioni.");
          }}
        />
      ) : (
        <Markdown text={outputs.summary} onRef={onRef} />
      )}
    </div>
  );
}

const DEPTH_STYLE = [
  "bg-sky-600 text-white text-base font-semibold",
  "bg-sky-100 text-sky-900 font-medium dark:bg-sky-900 dark:text-sky-100",
  "bg-white border border-zinc-300 dark:bg-zinc-900 dark:border-zinc-700",
  "text-zinc-600 dark:text-zinc-400",
];

function TreeNode({ node, depth }: { node: MapNode; depth: number }) {
  const [open, setOpen] = useState(true);
  const kids = node.figli ?? [];
  return (
    <div className="flex items-center">
      <button
        className={`flex shrink-0 items-center gap-1 rounded-lg px-3 py-1.5 text-left ${DEPTH_STYLE[Math.min(depth, 3)]} ${kids.length ? "cursor-pointer" : "cursor-default"}`}
        onClick={() => kids.length && setOpen(!open)}
        title={kids.length ? (open ? "Comprimi" : "Espandi") : undefined}
      >
        <span className="max-w-64">{node.etichetta}</span>
        {kids.length > 0 && !open && <span className="ml-1 text-xs opacity-70">+{kids.length}</span>}
      </button>
      {kids.length > 0 && open && (
        <div className="relative ml-6 flex flex-col gap-2 border-l-2 border-sky-300 py-1 pl-4 dark:border-sky-800">
          {kids.map((k, i) => (
            <div key={i} className="relative before:absolute before:top-1/2 before:-left-4 before:w-4 before:border-t-2 before:border-sky-300 dark:before:border-sky-800">
              <TreeNode node={k} depth={depth + 1} />
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

export function MapTab({ outputs, process }: { outputs: Outputs; process: React.ReactNode }) {
  if (!outputs.map) return <Placeholder text="La mappa concettuale degli argomenti della lezione." action={process} />;
  return (
    <div className="overflow-auto pb-6 select-text">
      <TreeNode node={outputs.map} depth={0} />
    </div>
  );
}

export function StudyTab({ lesson, outputs, process, editing }: { lesson: Lesson; outputs: Outputs; process: React.ReactNode; editing?: Editing }) {
  const [edit, setEdit] = useState(false);
  if (!outputs.study) return <Placeholder text="Flashcard, domande d'esame e glossario per ripassare." action={process} />;
  if (edit && editing)
    return (
      <StudyEditor
        initial={outputs.study}
        onCancel={() => setEdit(false)}
        onSave={async (study) => {
          await ai.saveStudy(lesson.id, study);
          setEdit(false);
          editing.onSaved();
          toast("Materiale di studio salvato.");
        }}
      />
    );
  return (
    <div className="mx-auto max-w-3xl space-y-10">
      <div className="flex justify-end">
        <EditButton editing={editing} onClick={() => setEdit(true)} label="Modifica flashcard e glossario" />
      </div>
      <Flashcards cards={outputs.study.flashcard} lessonId={lesson.id} />
      <Exam items={outputs.study.domandeEsame} />
      <Glossary items={outputs.study.glossario} />
    </div>
  );
}

function Flashcards({ cards, lessonId }: { cards: Study["flashcard"]; lessonId: string }) {
  const [mode, setMode] = useState<"browse" | "review">("browse");
  if (!cards.length) return null;
  const modes = (
    <div className="flex rounded-md border border-zinc-300 p-0.5 text-sm font-normal dark:border-zinc-700" role="group" aria-label="Modalità">
      {(["browse", "review"] as const).map((m) => (
        <button
          key={m}
          className={`rounded px-2 py-0.5 ${mode === m ? "bg-sky-600 text-white" : "text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"}`}
          onClick={() => setMode(m)}
          title={m === "review" ? "Solo le carte da ripassare oggi; quelle che sai tornano dopo 1, 3, 7, 14, 30 giorni" : "Tutte le carte, in ordine"}
        >
          {m === "browse" ? "Sfoglia" : "Ripasso"}
        </button>
      ))}
    </div>
  );
  if (mode === "review")
    return (
      <section>
        <h3 className="mb-3 flex items-center justify-between font-semibold">
          <span>Flashcard</span>
          {modes}
        </h3>
        <ReviewSession scope="lesson" id={lessonId} />
      </section>
    );
  return <BrowseCards cards={cards} modes={modes} />;
}

function BrowseCards({ cards, modes }: { cards: Study["flashcard"]; modes: React.ReactNode }) {
  const [order, setOrder] = useState(() => cards.map((_, i) => i));
  const [i, setI] = useState(0);
  const [flipped, setFlipped] = useState(false);
  useEffect(() => {
    setOrder(cards.map((_, k) => k));
    setI(0);
  }, [cards]);
  if (!cards.length) return null;
  const card = cards[order[i] ?? 0];
  const go = (d: number) => {
    setFlipped(false);
    setI((x) => (x + d + cards.length) % cards.length);
  };
  return (
    <section>
      <h3 className="mb-3 flex items-center gap-2 font-semibold">
        <span>
          Flashcard <span className="font-normal text-zinc-400">{i + 1} di {cards.length}</span>
        </span>
        <span className="flex-1" />
        {modes}
        <button
          className="btn"
          onClick={() => {
            setOrder([...order].sort(() => Math.random() - 0.5));
            setI(0);
            setFlipped(false);
          }}
        >
          <Shuffle className="size-4" /> Mescola
        </button>
      </h3>
      <FlashCard text={flipped ? card.risposta : card.domanda} flipped={flipped} onFlip={() => setFlipped(!flipped)} />
      <div className="mt-3 flex justify-center gap-2">
        <button className="btn" onClick={() => go(-1)}>
          ← Precedente
        </button>
        <button className="btn" onClick={() => setFlipped(!flipped)}>
          <RotateCcw className="size-4" /> Gira
        </button>
        <button className="btn btn-primary" onClick={() => go(1)}>
          Successiva →
        </button>
      </div>
    </section>
  );
}

function Exam({ items }: { items: Study["domandeEsame"] }) {
  const [open, setOpen] = useState<Set<number>>(new Set());
  if (!items.length) return null;
  return (
    <section>
      <h3 className="mb-3 font-semibold">Domande d'esame</h3>
      <ol className="space-y-2">
        {items.map((q, i) => (
          <li key={i} className="rounded-lg border border-zinc-200 dark:border-zinc-800">
            <button
              className="flex w-full items-start gap-2 px-3 py-2 text-left"
              onClick={() => {
                const n = new Set(open);
                if (n.has(i)) n.delete(i);
                else n.add(i);
                setOpen(n);
              }}
            >
              {open.has(i) ? <ChevronDown className="mt-0.5 size-4 shrink-0" /> : <ChevronRight className="mt-0.5 size-4 shrink-0" />}
              <span className="font-medium">
                {i + 1}. <Markdown text={q.domanda} inline />
              </span>
            </button>
            {open.has(i) && (
              <div className="border-t border-zinc-200 px-3 py-2 dark:border-zinc-800">
                <Markdown text={q.traccia} />
              </div>
            )}
          </li>
        ))}
      </ol>
    </section>
  );
}

function Glossary({ items }: { items: Study["glossario"] }) {
  const sorted = useMemo(() => [...items].sort((a, b) => a.termine.localeCompare(b.termine, "it")), [items]);
  if (!items.length) return null;
  return (
    <section>
      <h3 className="mb-3 font-semibold">Glossario</h3>
      <dl className="divide-y divide-zinc-200 rounded-lg border border-zinc-200 select-text dark:divide-zinc-800 dark:border-zinc-800">
        {sorted.map((t, i) => (
          <div key={i} className="grid grid-cols-[minmax(8rem,1fr)_3fr] gap-3 px-3 py-2">
            <dt className="font-medium">
              <Markdown text={t.termine} inline />
            </dt>
            <dd className="text-zinc-700 dark:text-zinc-300">
              <Markdown text={t.definizione} inline />
            </dd>
          </div>
        ))}
      </dl>
    </section>
  );
}

// ---------- study material editor ----------

type Rows = { a: string; b: string }[];
const toRows = <T,>(xs: T[], a: keyof T, b: keyof T): Rows => xs.map((x) => ({ a: String(x[a]), b: String(x[b]) }));

function RowsEditor({ title, rows, labels, onChange }: { title: string; rows: Rows; labels: [string, string]; onChange: (r: Rows) => void }) {
  const set = (i: number, k: "a" | "b", v: string) => onChange(rows.map((r, j) => (j === i ? { ...r, [k]: v } : r)));
  return (
    <section>
      <h3 className="mb-3 font-semibold">
        {title} <span className="font-normal text-zinc-400">{rows.length}</span>
      </h3>
      <div className="space-y-2">
        {rows.map((r, i) => (
          <div key={i} className="flex items-start gap-2 rounded-lg border border-zinc-200 p-2 dark:border-zinc-800" data-study-row>
            <div className="grid flex-1 gap-2 md:grid-cols-2">
              <textarea className="field min-h-16 text-sm" aria-label={labels[0]} placeholder={labels[0]} value={r.a} onChange={(e) => set(i, "a", e.target.value)} />
              <textarea className="field min-h-16 text-sm" aria-label={labels[1]} placeholder={labels[1]} value={r.b} onChange={(e) => set(i, "b", e.target.value)} />
            </div>
            <button className="btn px-2" title="Elimina" onClick={() => onChange(rows.filter((_, j) => j !== i))}>
              <Trash2 className="size-4" />
            </button>
          </div>
        ))}
      </div>
      <button className="btn mt-2" onClick={() => onChange([...rows, { a: "", b: "" }])}>
        <Plus className="size-4" /> Aggiungi
      </button>
    </section>
  );
}

function StudyEditor({ initial, onSave, onCancel }: { initial: Study; onSave: (s: Study) => Promise<void>; onCancel: () => void }) {
  const [cards, setCards] = useState(() => toRows(initial.flashcard, "domanda", "risposta"));
  const [exam, setExam] = useState(() => toRows(initial.domandeEsame, "domanda", "traccia"));
  const [terms, setTerms] = useState(() => toRows(initial.glossario, "termine", "definizione"));
  const [saving, setSaving] = useState(false);
  const filled = (r: Rows) => r.filter((x) => x.a.trim() || x.b.trim());
  const save = async () => {
    setSaving(true);
    try {
      await onSave({
        flashcard: filled(cards).map((x) => ({ domanda: x.a.trim(), risposta: x.b.trim() })),
        domandeEsame: filled(exam).map((x) => ({ domanda: x.a.trim(), traccia: x.b.trim() })),
        glossario: filled(terms).map((x) => ({ termine: x.a.trim(), definizione: x.b.trim() })),
      });
    } catch (e) {
      toast(toAppError(e).message, "error");
      setSaving(false);
    }
  };
  return (
    <div className="mx-auto max-w-3xl space-y-10">
      <RowsEditor title="Flashcard" rows={cards} labels={["Domanda", "Risposta"]} onChange={setCards} />
      <RowsEditor title="Domande d'esame" rows={exam} labels={["Domanda", "Traccia di risposta"]} onChange={setExam} />
      <RowsEditor title="Glossario" rows={terms} labels={["Termine", "Definizione"]} onChange={setTerms} />
      <div className="sticky bottom-0 flex justify-end gap-2 bg-zinc-50/95 py-3 dark:bg-zinc-950/95">
        <button className="btn" onClick={onCancel}>
          Annulla
        </button>
        <button className="btn btn-primary" disabled={saving} onClick={save}>
          Salva
        </button>
      </div>
    </div>
  );
}
