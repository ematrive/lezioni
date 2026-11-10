import { getCurrentWebview } from "@tauri-apps/api/webview";
import { ask, open } from "@tauri-apps/plugin-dialog";
import { ClipboardPaste, ChevronDown, ExternalLink, Share2, Sparkles, FileAudio, FileText, FileVideo, FolderOpen, Image as ImageIcon, Paperclip, Plus, Trash2, Upload } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { ai, api, fileUrl, fmtSize, sourceFile, toAppError, type Course, type Lesson, type Outputs, type Source, type SourceKind } from "../../lib/api";
import { ShareDialog } from "../share/ShareDialog";
import { ChatTab } from "./ChatTab";
import { JobBar, MapTab, startProcessing, StudyTab, SummaryTab, TranscriptTab, useJob, type Seek } from "./Outputs";
import { Modal } from "../../lib/Modal";
import { toast } from "../../lib/toast";

const TABS = [
  { id: "sources", label: "Fonti" },
  { id: "transcript", label: "Trascrizione" },
  { id: "summary", label: "Sintesi" },
  { id: "map", label: "Mappa" },
  { id: "study", label: "Studio" },
  { id: "chat", label: "Domande" },
] as const;
type Tab = (typeof TABS)[number]["id"];

const MIME_EXT: Record<string, string> = { "image/png": "png", "image/jpeg": "jpg", "image/gif": "gif", "image/webp": "webp", "image/heic": "heic", "application/pdf": "pdf" };

function stamp() {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
}

/** Text fields keep their own paste. */
const isEditable = (t: EventTarget | null) => t instanceof HTMLElement && (t.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(t.tagName));

/** Open the lesson at a given tab / point (from a search result). */
export interface Focus {
  lessonId: string;
  tab: Tab;
  sourceId: string | null;
  secs: number | null;
  n: number;
}

export function LessonView({
  lessonId,
  course,
  focus,
  onChanged,
  onDeleted,
  onMissing,
  onOpenLesson,
}: {
  onOpenLesson: (lessonId: string) => void;
  lessonId: string;
  course: Course | null;
  focus: Focus | null;
  onChanged: () => void;
  onDeleted: () => void;
  onMissing: () => void;
}) {
  const [lesson, setLesson] = useState<Lesson | null>(null);
  const [tab, setTab] = useState<Tab>("sources");
  const [dragging, setDragging] = useState(false);
  const [busy, setBusy] = useState(false);
  const [textDraft, setTextDraft] = useState<string | null>(null);
  const [outputs, setOutputs] = useState<Outputs | null>(null);
  const [seek, setSeek] = useState<Seek | null>(null);
  const job = useJob(lessonId);
  const [sharing, setSharing] = useState(false);

  useEffect(() => {
    if (!focus) return;
    setTab(focus.tab);
    if (focus.sourceId) setSeek({ sourceId: focus.sourceId, secs: focus.secs ?? -1, n: focus.n });
  }, [focus]);

  useEffect(() => {
    api
      .lesson(lessonId)
      .then(setLesson)
      // Folder moved or deleted outside the app (or a lesson remembered from another archive).
      .catch(onMissing);
  }, [lessonId, onMissing]);

  const loadOutputs = useCallback(() => {
    ai.outputs(lessonId)
      .then(setOutputs)
      .catch(() => setOutputs(null));
  }, [lessonId]);
  useEffect(loadOutputs, [loadOutputs]);

  const changed = useCallback(
    (l: Lesson) => {
      setLesson(l);
      onChanged();
    },
    [onChanged],
  );

  const addFiles = useCallback(
    async (paths: string[]) => {
      if (!paths.length) return;
      setBusy(true);
      try {
        const r = await api.addFiles(lessonId, paths);
        changed(r.lesson);
        const n = paths.length - r.warnings.length;
        if (n) toast(n === 1 ? "1 file aggiunto." : `${n} file aggiunti.`);
        if (r.warnings.length) toast(r.warnings.join("\n"), "error");
      } catch (e) {
        toast(toAppError(e).message, "error");
      } finally {
        setBusy(false);
      }
    },
    [lessonId, changed],
  );

  const addFilesRef = useRef(addFiles);
  addFilesRef.current = addFiles;

  // Drag & drop from Finder / Esplora file: Tauri gives the real paths. Subscribed once per
  // lesson (the ref always calls the current addFiles).
  useEffect(() => {
    const un = getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "enter" || e.payload.type === "over") setDragging(true);
      else if (e.payload.type === "leave") setDragging(false);
      else {
        setDragging(false);
        // Packages are imported by the app (App.tsx), not added as sources.
        addFilesRef.current(e.payload.paths.filter((p) => !p.toLowerCase().endsWith(".lezione")));
      }
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  // Paste: images and files from the clipboard become sources; text opens the text dialog.
  useEffect(() => {
    const onPaste = async (e: ClipboardEvent) => {
      if (isEditable(e.target) || !e.clipboardData) return;
      const files = Array.from(e.clipboardData.files);
      const text = e.clipboardData.getData("text/plain");
      if (!files.length && !text) return;
      e.preventDefault();
      if (!files.length) {
        setTextDraft(text);
        return;
      }
      setBusy(true);
      try {
        let l: Lesson | null = null;
        for (const [i, f] of files.entries()) {
          const ext = MIME_EXT[f.type] ?? f.name.split(".").pop() ?? "bin";
          // Header values must be ASCII: the name is ours, the original one (if any) is not kept.
          const name = `incollato-${stamp()}${files.length > 1 ? `-${i + 1}` : ""}.${ext}`;
          l = await api.addBytes(lessonId, name, new Uint8Array(await f.arrayBuffer()));
        }
        if (l) changed(l);
        toast(files.length === 1 ? "Incollato." : `${files.length} file incollati.`);
      } catch (err) {
        toast(toAppError(err).message, "error");
      } finally {
        setBusy(false);
      }
    };
    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
  }, [lessonId, changed]);

  if (!lesson) return null;

  const process = (force: boolean) =>
    startProcessing(lesson, force, () => {
      loadOutputs();
      api.lesson(lesson.id).then(setLesson, () => {});
    });
  const processed = !!outputs?.summary;
  const processButton = (
    <button className="btn btn-primary" disabled={!!job || !lesson.sources.length} onClick={() => process(false)} title={lesson.sources.length ? "Trascrivi, leggi le fonti e genera sintesi, mappa e studio con Gemini" : "Aggiungi prima le fonti"}>
      <Sparkles className="size-4" /> {processed ? "Rigenera" : "Elabora con Gemini"}
    </button>
  );

  // Summary references: [F1 00:12:30] → the recording at that minute, [F3] → that source.
  const onSummaryRef = (href: string) => {
    const m = href.match(/^#(t|f)=(F\d+)(?:@(\d+))?$/);
    const sourceId = m && outputs?.info?.sources[m[2]];
    if (!m || !sourceId) return;
    setTab("transcript");
    setSeek({ sourceId, secs: m[3] ? Number(m[3]) : -1, n: Date.now() });
  };

  const pick = async () => {
    const r = await open({ multiple: true, title: "Aggiungi file alla lezione" });
    if (Array.isArray(r)) addFiles(r);
    else if (typeof r === "string") addFiles([r]);
  };

  const remove = async () => {
    const ok = await ask(`Spostare nel Cestino la lezione «${lesson.title}» e tutti i suoi file?`, { title: "Elimina lezione", kind: "warning", okLabel: "Sposta nel Cestino", cancelLabel: "Annulla" });
    if (!ok) return;
    try {
      await api.deleteLesson(lesson.id);
      onDeleted();
    } catch (e) {
      toast(toAppError(e).message, "error");
    }
  };

  return (
    <div className="relative flex h-full flex-col">
      <Header lesson={lesson} onChanged={changed} onDelete={remove} actions={
          <>
            <ProcessMenu main={processButton} disabled={!!job || !lesson.sources.length} onForce={() => process(true)} />
            <button className="btn" disabled={!course} onClick={() => setSharing(true)} title="Condividi: pacchetto, PDF, pagina web, Markdown, Anki">
              <Share2 className="size-4" /> Condividi
            </button>
          </>
        }
      />
      {sharing && course && <ShareDialog course={course} lesson={lesson} onClose={() => setSharing(false)} />}
      {job && <JobBar job={job} />}
      <nav className="flex gap-1 border-b border-zinc-200 px-4 dark:border-zinc-800">
        {TABS.map((t) => (
          <button
            key={t.id}
            className={`-mb-px border-b-2 px-3 py-2 ${tab === t.id ? "border-sky-600 font-medium text-sky-700 dark:text-sky-400" : "border-transparent text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-200"}`}
            onClick={() => setTab(t.id)}
          >
            {t.label}
          </button>
        ))}
      </nav>
      <div className="min-h-0 flex-1 overflow-auto p-4">
        {tab === "sources" ? (
          <Sources lesson={lesson} busy={busy} onPick={pick} onPasteText={() => setTextDraft("")} onChanged={changed} />
        ) : !outputs ? null : tab === "transcript" ? (
          <TranscriptTab lesson={lesson} outputs={outputs} seek={seek} process={processButton} />
        ) : tab === "summary" ? (
          <SummaryTab outputs={outputs} onRef={onSummaryRef} process={processButton} />
        ) : tab === "chat" ? (
          <ChatTab
            lesson={lesson}
            onLesson={onOpenLesson}
            onSource={(sourceId, secs) => {
              setTab("transcript");
              setSeek({ sourceId, secs, n: Date.now() });
            }}
          />
        ) : tab === "map" ? (
          <MapTab outputs={outputs} process={processButton} />
        ) : (
          <StudyTab outputs={outputs} process={processButton} />
        )}
      </div>
      {dragging && (
        <div className="pointer-events-none absolute inset-2 z-30 flex items-center justify-center rounded-xl border-2 border-dashed border-sky-500 bg-sky-500/10 text-lg font-medium text-sky-700 dark:text-sky-300">
          <Upload className="mr-2 size-6" /> Rilascia per aggiungere a «{lesson.title}»
        </div>
      )}
      {textDraft !== null && (
        <TextDialog
          initial={textDraft}
          onClose={() => setTextDraft(null)}
          onSave={async (title, text) => {
            try {
              changed(await api.addText(lesson.id, title, text));
              setTextDraft(null);
            } catch (e) {
              toast(toAppError(e).message, "error");
            }
          }}
        />
      )}
    </div>
  );
}

function ProcessMenu({ main, disabled, onForce }: { main: React.ReactNode; disabled: boolean; onForce: () => void }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="relative flex [&>button:first-child]:rounded-r-none">
      {main}
      <button className="btn btn-primary -ml-px rounded-l-none border-l-sky-700 px-1.5" disabled={disabled} onClick={() => setOpen(!open)} title="Altre opzioni">
        <ChevronDown className="size-4" />
      </button>
      {open && (
        <>
          <div className="fixed inset-0 z-30" onClick={() => setOpen(false)} />
          <div className="anim-pop absolute top-full right-0 z-40 mt-1 w-80 rounded-md border border-zinc-200 bg-white p-1 shadow-lg dark:border-zinc-700 dark:bg-zinc-900">
            <button
              className="w-full rounded px-3 py-2 text-left hover:bg-zinc-100 dark:hover:bg-zinc-800"
              onClick={() => {
                setOpen(false);
                onForce();
              }}
            >
              <div className="font-medium">Rielabora anche le fonti</div>
              <div className="text-xs text-zinc-500">Trascrive e rilegge di nuovo tutto, anche ciò che era già stato estratto (usa più quota).</div>
            </button>
          </div>
        </>
      )}
    </div>
  );
}

function Header({ lesson, onChanged, onDelete, actions }: { lesson: Lesson; onChanged: (l: Lesson) => void; onDelete: () => void; actions: React.ReactNode }) {
  const [title, setTitle] = useState(lesson.title);
  useEffect(() => setTitle(lesson.title), [lesson.title]);
  const save = async (patch: { title?: string; date?: string }) => {
    try {
      onChanged(await api.updateLesson(lesson.id, patch));
    } catch (e) {
      toast(toAppError(e).message, "error");
      setTitle(lesson.title);
    }
  };
  return (
    <header className="flex flex-wrap items-center gap-3 px-4 pt-4 pb-3">
      <input
        className="min-w-64 flex-1 rounded-md border border-transparent bg-transparent px-2 py-1 text-xl font-semibold outline-none hover:border-zinc-300 focus:border-sky-500 dark:hover:border-zinc-700"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        onBlur={() => title.trim() !== lesson.title && (title.trim() ? save({ title }) : setTitle(lesson.title))}
        onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
        title="Titolo della lezione"
      />
      {actions}
      <input className="field w-auto" type="date" value={lesson.date} onChange={(e) => e.target.value && save({ date: e.target.value })} title="Data della lezione" />
      <button className="btn" title="Mostra la cartella della lezione" onClick={() => api.reveal(lesson.path)}>
        <FolderOpen className="size-4" />
      </button>
      <button className="btn btn-danger" title="Elimina la lezione" onClick={onDelete}>
        <Trash2 className="size-4" />
      </button>
    </header>
  );
}

const KIND_LABEL: Record<SourceKind, string> = {
  audio: "Registrazioni",
  video: "Video",
  pdf: "PDF e slide",
  image: "Foto e appunti a mano",
  text: "Testi",
  document: "Documenti",
  other: "Altri file",
};
const KIND_ORDER: SourceKind[] = ["audio", "video", "image", "pdf", "document", "text", "other"];

function KindIcon({ kind, className }: { kind: SourceKind; className?: string }) {
  const I = { audio: FileAudio, video: FileVideo, image: ImageIcon, pdf: FileText, text: FileText, document: FileText, other: Paperclip }[kind];
  return <I className={className} />;
}

function Sources({ lesson, busy, onPick, onPasteText, onChanged }: { lesson: Lesson; busy: boolean; onPick: () => void; onPasteText: () => void; onChanged: (l: Lesson) => void }) {
  const [notes, setNotes] = useState(lesson.notes);
  const remove = async (s: Source) => {
    const extracted = s.kind === "audio" || s.kind === "video" ? " e la sua trascrizione" : s.kind === "text" ? "" : " e il testo estratto";
    const ok = await ask(`Rimuovere «${s.originalName}»${extracted} dalla lezione?\n\nVa nel Cestino la copia che Lezioni tiene nella cartella dell'archivio: il file originale che avevi aggiunto resta dov'era.`, { title: "Rimuovi fonte", kind: "warning", okLabel: "Rimuovi", cancelLabel: "Annulla" });
    if (!ok) return;
    try {
      onChanged(await api.removeSource(lesson.id, s.id));
    } catch (e) {
      toast(toAppError(e).message, "error");
    }
  };
  const saveNotes = async () => {
    if (notes === lesson.notes) return;
    try {
      onChanged(await api.updateLesson(lesson.id, { notes }));
    } catch (e) {
      toast(toAppError(e).message, "error");
    }
  };

  const groups = KIND_ORDER.map((k) => [k, lesson.sources.filter((s) => s.kind === k)] as const).filter(([, list]) => list.length);

  return (
    <div className="mx-auto max-w-4xl space-y-6">
      <div className="rounded-xl border-2 border-dashed border-zinc-300 p-6 text-center dark:border-zinc-700">
        <Upload className="mx-auto mb-2 size-7 text-zinc-400" />
        <p className="font-medium">Trascina qui registrazioni, PDF, foto e appunti</p>
        <p className="mt-1 text-sm text-zinc-500">
          oppure incolla con <kbd className="rounded border border-zinc-300 px-1 text-xs dark:border-zinc-700">{navigator.platform.startsWith("Mac") ? "⌘V" : "Ctrl+V"}</kbd> uno screenshot, un'immagine o del testo
        </p>
        <div className="mt-4 flex justify-center gap-2">
          <button className="btn" onClick={onPick} disabled={busy}>
            <Plus className="size-4" /> Aggiungi file…
          </button>
          <button className="btn" onClick={onPasteText} disabled={busy}>
            <ClipboardPaste className="size-4" /> Scrivi o incolla testo…
          </button>
        </div>
        {busy && <p className="mt-3 text-sm text-sky-600">Copia in corso…</p>}
      </div>

      {groups.map(([kind, list]) => (
        <section key={kind}>
          <h3 className="mb-2 flex items-center gap-2 text-sm font-semibold text-zinc-600 dark:text-zinc-400">
            <KindIcon kind={kind} className="size-4" /> {KIND_LABEL[kind]} <span className="font-normal text-zinc-400">{list.length}</span>
          </h3>
          {kind === "image" && list.every((s) => !s.omitted) ? (
            <div className="grid grid-cols-[repeat(auto-fill,minmax(150px,1fr))] gap-3">
              {list.map((s) => (
                <figure key={s.id} className="group relative overflow-hidden rounded-lg border border-zinc-200 dark:border-zinc-800">
                  <button className="block w-full" onClick={() => api.openSource(lesson.id, s.id)} title="Apri">
                    <img src={fileUrl(sourceFile(lesson, s))} alt={s.originalName} className="aspect-[4/3] w-full bg-zinc-100 object-cover dark:bg-zinc-900" loading="lazy" />
                  </button>
                  <figcaption className="truncate px-2 py-1 text-xs text-zinc-500">{s.originalName}</figcaption>
                  <button className="absolute top-1 right-1 hidden rounded bg-black/50 p-1 text-white group-hover:block" title="Rimuovi" onClick={() => remove(s)}>
                    <Trash2 className="size-3.5" />
                  </button>
                </figure>
              ))}
            </div>
          ) : (
            <ul className="divide-y divide-zinc-200 rounded-lg border border-zinc-200 dark:divide-zinc-800 dark:border-zinc-800">
              {list.map((s) => (
                <li key={s.id} className="group px-3 py-2">
                  <div className="flex items-center gap-3">
                    <KindIcon kind={s.kind} className="size-4 shrink-0 text-zinc-400" />
                    <span className="min-w-0 flex-1 truncate" title={s.file}>
                      {s.originalName}
                    </span>
                    {s.omitted ? (
                      <span className="rounded bg-zinc-100 px-2 py-0.5 text-xs text-zinc-500 dark:bg-zinc-800" title="Il file non era nel pacchetto ricevuto: c'è solo il testo estratto">
                        file non incluso
                      </span>
                    ) : (
                      <>
                        <span className="text-xs text-zinc-400">{fmtSize(s.size)}</span>
                        <button className="rounded p-1 text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800" title="Apri con l'app di sistema" onClick={() => api.openSource(lesson.id, s.id)}>
                          <ExternalLink className="size-4" />
                        </button>
                      </>
                    )}
                    <button className="rounded p-1 text-zinc-500 hover:bg-red-50 hover:text-red-600 dark:hover:bg-red-950" title="Rimuovi" onClick={() => remove(s)}>
                      <Trash2 className="size-4" />
                    </button>
                  </div>
                  {s.kind === "audio" && !s.omitted && <audio className="mt-2 w-full" controls preload="none" src={fileUrl(sourceFile(lesson, s))} />}
                </li>
              ))}
            </ul>
          )}
        </section>
      ))}

      <section>
        <h3 className="mb-2 text-sm font-semibold text-zinc-600 dark:text-zinc-400">Note sulla lezione</h3>
        <textarea
          className="field min-h-24"
          placeholder="Argomenti, cose da rivedere, compiti per la prossima volta…"
          value={notes}
          onChange={(e) => setNotes(e.target.value)}
          onBlur={saveNotes}
        />
      </section>
    </div>
  );
}

function TextDialog({ initial, onClose, onSave }: { initial: string; onClose: () => void; onSave: (title: string, text: string) => void }) {
  const [title, setTitle] = useState("");
  const [text, setText] = useState(initial);
  return (
    <Modal
      wide
      title="Aggiungi testo"
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            Annulla
          </button>
          <button className="btn btn-primary" disabled={!text.trim()} onClick={() => onSave(title, text)}>
            Aggiungi
          </button>
        </>
      }
    >
      <div className="space-y-3">
        <label className="block">
          <span className="label">Titolo</span>
          <input className="field" value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Testo incollato" autoFocus={!!initial} />
        </label>
        <label className="block">
          <span className="label">Testo (anche Markdown)</span>
          <textarea className="field min-h-64 font-mono text-sm" value={text} onChange={(e) => setText(e.target.value)} autoFocus={!initial} />
        </label>
      </div>
    </Modal>
  );
}
