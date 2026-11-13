import { getCurrentWebview } from "@tauri-apps/api/webview";
import { ask, open } from "@tauri-apps/plugin-dialog";
import { Check, ClipboardPaste, ChevronDown, Search, Maximize2, Minimize2, MoreHorizontal, ExternalLink, Share2, Sparkles, FileAudio, FileText, FileVideo, FolderOpen, Image as ImageIcon, Paperclip, Plus, Trash2, Upload } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { ai, api, fileUrl, fmtSize, sourceFile, toAppError, type Course, type Lang, type Lesson, type Outputs, type Source, type SourceKind } from "../../lib/api";
import { ShareDialog } from "../share/ShareDialog";
import { ChatTab } from "./ChatTab";
import { PhotoCard } from "./PhotoCard";
import { JobBar, MapTab, startProcessing, StudyTab, SummaryTab, TranscriptTab, useJob, type Editing, type Seek } from "./Outputs";
import { Modal } from "../../lib/Modal";
import { FindBar } from "../../lib/findInPage";
import { toast } from "../../lib/toast";
import { t, tn } from "../../i18n";

const TABS = [
  { id: "sources", label: () => t("Fonti") },
  { id: "transcript", label: () => t("Trascrizione") },
  { id: "summary", label: () => t("Sintesi") },
  { id: "map", label: () => t("Mappa") },
  { id: "study", label: () => t("Studio") },
  { id: "chat", label: () => t("Domande") },
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
  full,
  onFull,
}: {
  /** The lesson takes the whole window. */
  full?: boolean;
  onFull?: (full: boolean) => void;
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
  const [finding, setFinding] = useState(false);
  const content = useRef<HTMLDivElement>(null);
  const contentEl = useCallback(() => content.current, []);
  // ⌘F / Ctrl+F: find in the lesson.
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && !e.altKey && e.key.toLowerCase() === "f" && !document.querySelector(".anim-fade")) {
        e.preventDefault();
        setFinding(false);
        requestAnimationFrame(() => setFinding(true));
      }
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, []);

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
        if (n) toast(tn(n, "1 file aggiunto.", "{n} file aggiunti."));
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
          const name = `${t("incollato|nome del file")}-${stamp()}${files.length > 1 ? `-${i + 1}` : ""}.${ext}`;
          l = await api.addBytes(lessonId, name, new Uint8Array(await f.arrayBuffer()));
        }
        if (l) changed(l);
        toast(tn(files.length, "Incollato.", "{n} file incollati."));
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
  // Corrections by hand: not while the lesson is being processed.
  const editing: Editing | undefined = job
    ? undefined
    : {
        course,
        onSaved: (l) => {
          if (l) changed(l);
          loadOutputs();
        },
      };
  // Rielabora anche le fonti redoes the transcripts corrected by hand: say so, a copy is kept.
  const processAgain = async () => {
    const edited = lesson.sources.filter((s) => s.edited);
    if (edited.length) {
      const names = edited.map((s) => `• ${s.originalName}`).join("\n");
      const ok = await ask(t("Hai corretto a mano:\n{names}\n\nQueste trascrizioni verranno rifatte da capo. La versione corretta resta in estratti/versioni nella cartella della lezione.", { names }), {
        title: t("Rielabora anche le fonti"),
        kind: "warning",
        okLabel: t("Rielabora"),
        cancelLabel: t("Annulla"),
      });
      if (!ok) return;
    }
    process(true);
  };
  const processButton = (
    <button className="btn btn-primary" disabled={!!job || !lesson.sources.length} onClick={() => process(false)} title={!lesson.sources.length ? t("Aggiungi prima le fonti") : processed ? t("Rifà sintesi, mappa e materiale di studio dalle trascrizioni attuali (comprese le tue correzioni). Le trascrizioni non vengono rifatte: per quello c'è «Rielabora anche le fonti».") : t("Trascrivi, leggi le fonti e genera sintesi, mappa e studio")}>
      <Sparkles className="size-4" /> {processed ? t("Rigenera") : t("Elabora")}
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
    const r = await open({ multiple: true, title: t("Aggiungi file alla lezione") });
    if (Array.isArray(r)) addFiles(r);
    else if (typeof r === "string") addFiles([r]);
  };

  const remove = async () => {
    if (await deleteLesson(lesson)) onDeleted();
  };

  return (
    <div className="relative flex h-full flex-col">
      <Header lesson={lesson} onChanged={changed} onDelete={remove} actions={
          <>
            <ProcessMenu main={processButton} disabled={!!job || !lesson.sources.length} onForce={processAgain} />
            <button className="btn" disabled={!course} onClick={() => setSharing(true)} title={t("Condividi: pacchetto, PDF, pagina web, Markdown, Anki")}>
              <Share2 className="size-4" /> {t("Condividi")}
            </button>
            {onFull && (
              <button className="btn" onClick={() => onFull(!full)} title={full ? t("Esci dal tutto schermo (Esc)") : t("Tutto schermo: nasconde corsi e lezioni")} aria-label={full ? t("Esci dal tutto schermo") : t("Tutto schermo")}>
                {full ? <Minimize2 className="size-4" /> : <Maximize2 className="size-4" />}
              </button>
            )}
          </>
        }
      />
      {sharing && course && <ShareDialog course={course} lesson={lesson} onClose={() => setSharing(false)} />}
      {job && <JobBar job={job} />}
      <nav className="flex flex-wrap gap-1 border-b border-zinc-200 px-4 dark:border-zinc-800">
        {TABS.map((tb) => (
          <button
            key={tb.id}
            className={`-mb-px border-b-2 px-3 py-2 ${tab === tb.id ? "border-sky-600 font-medium text-sky-700 dark:text-sky-400" : "border-transparent text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-200"}`}
            onClick={() => setTab(tb.id)}
          >
            {tb.label()}
          </button>
        ))}
        <div className="ml-auto self-center py-1">
          {finding ? (
            <FindBar root={contentEl} resetKey={`${tab}|${outputs ? 1 : 0}`} onClose={() => setFinding(false)} />
          ) : (
            <button className="rounded p-1.5 text-zinc-500 hover:bg-zinc-200 dark:hover:bg-zinc-800" onClick={() => setFinding(true)} title={t("Cerca nella lezione (⌘F / Ctrl+F)")} aria-label={t("Cerca nella lezione")}>
              <Search className="size-4" />
            </button>
          )}
        </div>
      </nav>
      <div ref={content} className="min-h-0 flex-1 overflow-auto p-4" data-lesson-content>
        {tab === "sources" ? (
          <Sources lesson={lesson} busy={busy || !!job} onPick={pick} onPasteText={() => setTextDraft("")} onChanged={changed} outputs={outputs} onLinked={loadOutputs} />
        ) : !outputs ? null : tab === "transcript" ? (
          <TranscriptTab lesson={lesson} outputs={outputs} seek={seek} process={processButton} editing={editing} onBookmarks={loadOutputs} />
        ) : tab === "summary" ? (
          <SummaryTab lesson={lesson} outputs={outputs} onRef={onSummaryRef} process={processButton} editing={editing} />
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
          <MapTab lesson={lesson} course={course} outputs={outputs} process={processButton} onRef={onSummaryRef} />
        ) : (
          <StudyTab lesson={lesson} outputs={outputs} process={processButton} editing={editing} />
        )}
      </div>
      {dragging && (
        <div className="pointer-events-none absolute inset-2 z-30 flex items-center justify-center rounded-xl border-2 border-dashed border-sky-500 bg-sky-500/10 text-lg font-medium text-sky-700 dark:text-sky-300">
          <Upload className="mr-2 size-6" /> {t("Rilascia per aggiungere a «{title}»", { title: lesson.title })}
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
      <button className="btn btn-primary -ml-px rounded-l-none border-l-sky-700 px-1.5" disabled={disabled} onClick={() => setOpen(!open)} title={t("Altre opzioni")}>
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
              <div className="font-medium">{t("Rielabora anche le fonti")}</div>
              <div className="text-xs text-zinc-500">{t("Trascrive e rilegge di nuovo tutto, anche ciò che era già stato estratto (usa più quota).")}</div>
            </button>
          </div>
        </>
      )}
    </div>
  );
}

function Header({ lesson, onChanged, onDelete, actions }: { lesson: Lesson; onChanged: (l: Lesson) => void; onDelete: () => void; actions: React.ReactNode }) {
  const [title, setTitle] = useState(lesson.title);
  const [menu, setMenu] = useState(false);
  useEffect(() => setTitle(lesson.title), [lesson.title]);
  const save = async (patch: { title?: string; date?: string; language?: Lang | null }) => {
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
        className="min-w-40 flex-1 rounded-md border border-transparent bg-transparent px-2 py-1 text-xl font-semibold outline-none hover:border-zinc-300 focus:border-sky-500 dark:hover:border-zinc-700"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        onBlur={() => title.trim() !== lesson.title && (title.trim() ? save({ title }) : setTitle(lesson.title))}
        onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
        title={t("Titolo della lezione")}
      />
      {actions}
      <input className="field w-auto" type="date" value={lesson.date} onChange={(e) => e.target.value && save({ date: e.target.value })} title={t("Data della lezione")} />
      <div className="relative">
        <button className="btn px-2" title={t("Altre azioni sulla lezione")} aria-label={t("Altre azioni sulla lezione")} onClick={() => setMenu(!menu)}>
          <MoreHorizontal className="size-4" />
        </button>
        {menu && (
          <>
            <div className="fixed inset-0 z-30" onClick={() => setMenu(false)} />
            <div className="anim-pop absolute top-full right-0 z-40 mt-1 w-56 rounded-md border border-zinc-200 bg-white p-1 text-sm shadow-lg dark:border-zinc-700 dark:bg-zinc-900" role="menu">
              <div className="px-2 pt-1 pb-0.5 text-xs text-zinc-500">{t("Lingua parlata nella lezione")}</div>
              {(
                [
                  ["", t("Lingua del corso")],
                  ["it", t("Italiano")],
                  ["en", t("Inglese")],
                ] as const
              ).map(([v, label]) => (
                <button
                  key={v}
                  role="menuitemradio"
                  aria-checked={(lesson.language ?? "") === v}
                  className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left hover:bg-zinc-100 dark:hover:bg-zinc-800"
                  onClick={() => {
                    setMenu(false);
                    if ((lesson.language ?? "") === v) return;
                    save({ language: (v || null) as Lang | null });
                    toast(t("Lingua cambiata: rielabora anche le fonti per trascrivere di nuovo la registrazione."));
                  }}
                >
                  <Check className={`size-4 ${(lesson.language ?? "") === v ? "" : "invisible"}`} /> {label}
                </button>
              ))}
              <div className="my-1 border-t border-zinc-200 dark:border-zinc-700" />
              <button
                className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left hover:bg-zinc-100 dark:hover:bg-zinc-800"
                onClick={() => {
                  setMenu(false);
                  api.reveal(lesson.path);
                }}
              >
                <FolderOpen className="size-4" /> {t("Mostra la cartella")}
              </button>
              <button
                className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-red-600 hover:bg-zinc-100 dark:hover:bg-zinc-800"
                onClick={() => {
                  setMenu(false);
                  onDelete();
                }}
              >
                <Trash2 className="size-4" /> {t("Elimina la lezione")}
              </button>
            </div>
          </>
        )}
      </div>
    </header>
  );
}

// A function: the labels follow the interface language.
const kindLabel = (k: SourceKind): string =>
  ({
    audio: t("Registrazioni"),
    video: t("Video"),
    pdf: t("PDF e slide"),
    image: t("Foto e appunti a mano"),
    text: t("Testi"),
    document: t("Documenti"),
    other: t("Altri file"),
  })[k];
const KIND_ORDER: SourceKind[] = ["audio", "video", "image", "pdf", "document", "text", "other"];

function KindIcon({ kind, className }: { kind: SourceKind; className?: string }) {
  const I = { audio: FileAudio, video: FileVideo, image: ImageIcon, pdf: FileText, text: FileText, document: FileText, other: Paperclip }[kind];
  return <I className={className} />;
}

function Sources({ lesson, busy, onPick, onPasteText, onChanged, outputs, onLinked }: { lesson: Lesson; busy: boolean; onPick: () => void; onPasteText: () => void; onChanged: (l: Lesson) => void; outputs: Outputs | null; onLinked: () => void }) {
  const [linking, setLinking] = useState<string | null>(null);
  const hasRecording = lesson.sources.some((s) => (s.kind === "audio" || s.kind === "video") && outputs?.extracts[s.id] !== undefined);
  const setRole = async (s: Source, role: "slide" | "materiale") => {
    try {
      onChanged(await ai.setRole(lesson.id, s.id, role));
    } catch (e) {
      toast(toAppError(e).message, "error");
    }
  };
  const link = async (s: Source) => {
    setLinking(s.id);
    try {
      const n = await ai.alignSlides(lesson.id, s.id);
      toast(tn(n, "Slide collegate alla registrazione: 1 passaggio da una slide all'altra.", "Slide collegate alla registrazione: {n} passaggi da una slide all'altra."));
      onLinked();
    } catch (e) {
      toast(toAppError(e).message, "error");
    } finally {
      setLinking(null);
    }
  };
  const linkPhotos = async () => {
    setLinking("photos");
    try {
      const warnings = await ai.linkPhotos(lesson.id);
      toast(warnings.length ? warnings.join("\n") : t("Foto agganciate alla registrazione."), warnings.length ? "error" : undefined);
      onLinked();
    } catch (e) {
      toast(toAppError(e).message, "error");
    } finally {
      setLinking(null);
    }
  };
  const [notes, setNotes] = useState(lesson.notes);
  const remove = async (s: Source) => {
    const name = s.originalName;
    const question = s.kind === "audio" || s.kind === "video" ? t("Rimuovere «{name}» e la sua trascrizione dalla lezione?", { name }) : s.kind === "text" ? t("Rimuovere «{name}» dalla lezione?", { name }) : t("Rimuovere «{name}» e il testo estratto dalla lezione?", { name });
    const ok = await ask(`${question}\n\n${t("Va nel Cestino la copia che Lezioni tiene nella cartella dell'archivio: il file originale che avevi aggiunto resta dov'era.")}`, { title: t("Rimuovi fonte"), kind: "warning", okLabel: t("Rimuovi"), cancelLabel: t("Annulla") });
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
        <p className="font-medium">{t("Trascina qui registrazioni, PDF, foto e appunti")}</p>
        <p className="mt-1 text-sm text-zinc-500">
          {t("oppure incolla con")} <kbd className="rounded border border-zinc-300 px-1 text-xs dark:border-zinc-700">{navigator.platform.startsWith("Mac") ? "⌘V" : "Ctrl+V"}</kbd> {t("uno screenshot, un'immagine o del testo")}
        </p>
        <div className="mt-4 flex justify-center gap-2">
          <button className="btn" onClick={onPick} disabled={busy}>
            <Plus className="size-4" /> {t("Aggiungi file…")}
          </button>
          <button className="btn" onClick={onPasteText} disabled={busy}>
            <ClipboardPaste className="size-4" /> {t("Scrivi o incolla testo…")}
          </button>
        </div>
        {busy && <p className="mt-3 text-sm text-sky-600">{t("Copia in corso…")}</p>}
      </div>

      {groups.map(([kind, list]) => (
        <section key={kind}>
          <h3 className="mb-2 flex items-center gap-2 text-sm font-semibold text-zinc-600 dark:text-zinc-400">
            <KindIcon kind={kind} className="size-4" /> {kindLabel(kind)} <span className="font-normal text-zinc-400">{list.length}</span>
            {kind === "image" && hasRecording && list.some((s) => outputs?.extracts[s.id] !== undefined) && (
              <button className="btn btn-sm ml-auto font-normal" disabled={linking === "photos" || busy} onClick={linkPhotos} title={t("Cerca di nuovo il minuto di ogni foto (non tocca quelle che hai agganciato tu)")}>
                {linking === "photos" ? t("Aggancio…") : t("Aggancia le foto alla registrazione")}
              </button>
            )}
          </h3>
          {kind === "image" && list.every((s) => !s.omitted) ? (
            <div className="grid grid-cols-[repeat(auto-fill,minmax(150px,1fr))] gap-3">
              {list.map((s) => (
                <PhotoCard key={s.id} lesson={lesson} source={s} outputs={outputs} onRemove={() => remove(s)} onChanged={(l) => (l ? onChanged(l) : onLinked())} />
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
                      <span className="rounded bg-zinc-100 px-2 py-0.5 text-xs text-zinc-500 dark:bg-zinc-800" title={t("Il file non era nel pacchetto ricevuto: c'è solo il testo estratto")}>
                        {t("file non incluso")}
                      </span>
                    ) : (
                      <>
                        <span className="text-xs text-zinc-400">{fmtSize(s.size)}</span>
                        <button className="rounded p-1 text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800" title={t("Apri con l'app di sistema")} onClick={() => api.openSource(lesson.id, s.id)}>
                          <ExternalLink className="size-4" />
                        </button>
                      </>
                    )}
                    <button className="rounded p-1 text-zinc-500 hover:bg-red-50 hover:text-red-600 dark:hover:bg-red-950" title={t("Rimuovi")} onClick={() => remove(s)}>
                      <Trash2 className="size-4" />
                    </button>
                  </div>
                  {s.kind === "audio" && !s.omitted && <audio className="mt-2 w-full" controls preload="none" src={fileUrl(sourceFile(lesson, s))} />}
                  {s.kind === "pdf" && (
                    <div className="mt-2 flex flex-wrap items-center gap-2 pl-7 text-xs text-zinc-500">
                      <div className="flex rounded-md border border-zinc-300 p-0.5 dark:border-zinc-700" role="group" aria-label={t("Che cos'è {name}", { name: s.originalName })}>
                        {(["slide", "materiale"] as const).map((r) => (
                          <button
                            key={r}
                            className={`rounded px-2 py-0.5 ${(s.role ?? "materiale") === r ? "bg-sky-600 text-white" : "hover:bg-zinc-100 dark:hover:bg-zinc-800"}`}
                            onClick={() => setRole(s, r)}
                            title={r === "slide" ? t("Le slide che il docente proiettava: si collegano alla registrazione, slide per slide") : t("Appunti, dispense, altri documenti: restano a parte")}
                          >
                            {r === "slide" ? t("Slide della lezione") : t("Altro materiale")}
                          </button>
                        ))}
                      </div>
                      {s.roleGuessed && <span title={t("Proposto dall'app leggendo il PDF: cambialo se non è giusto")}>{t("proposto")}</span>}
                      {s.role === "slide" &&
                        (() => {
                          const al = outputs?.slides.find((a) => a.pdf === s.id);
                          const read = outputs?.extracts[s.id] !== undefined;
                          if (!read) return <span>{t("si collegherà alla registrazione quando elabori la lezione")}</span>;
                          if (!hasRecording) return <span>{t("nessuna registrazione trascritta da collegare")}</span>;
                          return (
                            <>
                              {al && <span>{tn(al.steps.length, "collegata: 1 passaggio", "collegata: {n} passaggi")}</span>}
                              <button className="btn btn-sm" disabled={linking === s.id || busy} onClick={() => link(s)}>
                                {linking === s.id ? t("Collego…") : al ? t("Ricollega") : t("Collega alla registrazione")}
                              </button>
                            </>
                          );
                        })()}
                    </div>
                  )}
                </li>
              ))}
            </ul>
          )}
        </section>
      ))}

      <section>
        <h3 className="mb-2 text-sm font-semibold text-zinc-600 dark:text-zinc-400">{t("Note sulla lezione")}</h3>
        <textarea
          className="field min-h-24"
          placeholder={t("Argomenti, cose da rivedere, compiti per la prossima volta…")}
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
      title={t("Aggiungi testo")}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            {t("Annulla")}
          </button>
          <button className="btn btn-primary" disabled={!text.trim()} onClick={() => onSave(title, text)}>
            {t("Aggiungi")}
          </button>
        </>
      }
    >
      <div className="space-y-3">
        <label className="block">
          <span className="label">{t("Titolo")}</span>
          <input className="field" value={title} onChange={(e) => setTitle(e.target.value)} placeholder={t("Testo incollato")} autoFocus={!!initial} />
        </label>
        <label className="block">
          <span className="label">{t("Testo (anche Markdown)")}</span>
          <textarea className="field min-h-64 font-mono text-sm" value={text} onChange={(e) => setText(e.target.value)} autoFocus={!initial} />
        </label>
      </div>
    </Modal>
  );
}

/** Asks, then moves a lesson to the Trash; true when it went. */
export async function deleteLesson(lesson: Lesson): Promise<boolean> {
  const ok = await ask(t("Spostare nel Cestino la lezione «{title}» e tutti i suoi file?", { title: lesson.title }), { title: t("Elimina lezione"), kind: "warning", okLabel: t("Sposta nel Cestino"), cancelLabel: t("Annulla") });
  if (!ok) return false;
  try {
    await api.deleteLesson(lesson.id);
    return true;
  } catch (e) {
    toast(toAppError(e).message, "error");
    return false;
  }
}
