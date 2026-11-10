import { ChevronDown, ChevronRight, RotateCcw, Shuffle, Sparkles, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { ai, costLine, fileUrl, sourceFile, toAppError, type JobEvent, type Lesson, type MapNode, type Outputs, type Study } from "../../lib/api";
import { Markdown } from "../../lib/Markdown";
import { toast } from "../../lib/toast";

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

export function TranscriptTab({ lesson, outputs, seek, process }: { lesson: Lesson; outputs: Outputs; seek: Seek | null; process: React.ReactNode }) {
  const players = useRef<Record<string, HTMLAudioElement | HTMLVideoElement | null>>({});
  const sections = useRef<Record<string, HTMLElement | null>>({});
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
      {withText.map((s) => {
        const media = s.kind === "audio" || s.kind === "video";
        return (
          <section key={s.id} ref={(el) => void (sections.current[s.id] = el)} className="scroll-mt-4">
            <div className="sticky top-0 z-10 -mx-4 mb-3 bg-zinc-50/95 px-4 py-2 backdrop-blur dark:bg-zinc-950/95">
              <h3 className="font-semibold">{s.originalName}</h3>
              {s.omitted && <p className="mt-1 text-xs text-zinc-500">La registrazione non era nel pacchetto ricevuto: c'è solo la trascrizione.</p>}
              {s.kind === "audio" && !s.omitted && <audio ref={(el) => void (players.current[s.id] = el)} className="mt-2 w-full" controls preload="metadata" src={fileUrl(sourceFile(lesson, s))} />}
              {s.kind === "video" && !s.omitted && <video ref={(el) => void (players.current[s.id] = el)} className="mt-2 max-h-64 w-full rounded" controls preload="metadata" src={fileUrl(sourceFile(lesson, s))} />}
              {media && !s.omitted && <p className="mt-1 text-xs text-zinc-500">Clicca un tempo per ascoltare da quel punto.</p>}
            </div>
            <Markdown text={outputs.extracts[s.id]} source={media ? s.id : undefined} onRef={onRef} />
          </section>
        );
      })}
    </div>
  );
}

export function SummaryTab({ outputs, onRef, process }: { outputs: Outputs; onRef: (href: string) => void; process: React.ReactNode }) {
  if (!outputs.summary) return <Placeholder text="La sintesi della lezione, costruita da tutte le fonti insieme, con i riferimenti al minuto della registrazione." action={process} />;
  return (
    <div className="mx-auto max-w-3xl">
      {outputs.info && (
        <p className="mb-4 text-xs text-zinc-500">
          Generata il {new Date(outputs.info.generated).toLocaleString("it-IT")} con {outputs.info.model}. I riferimenti [F1 00:12:30] portano alla trascrizione.
        </p>
      )}
      <Markdown text={outputs.summary} onRef={onRef} />
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

export function StudyTab({ outputs, process }: { outputs: Outputs; process: React.ReactNode }) {
  if (!outputs.study) return <Placeholder text="Flashcard, domande d'esame e glossario per ripassare." action={process} />;
  return (
    <div className="mx-auto max-w-3xl space-y-10">
      <Flashcards cards={outputs.study.flashcard} />
      <Exam items={outputs.study.domandeEsame} />
      <Glossary items={outputs.study.glossario} />
    </div>
  );
}

function Flashcards({ cards }: { cards: Study["flashcard"] }) {
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
      <h3 className="mb-3 flex items-center justify-between font-semibold">
        <span>
          Flashcard <span className="font-normal text-zinc-400">{i + 1} di {cards.length}</span>
        </span>
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
      <button
        className={`flex min-h-44 w-full items-center justify-center rounded-xl border-2 p-6 text-center text-lg transition-colors ${flipped ? "border-emerald-400 bg-emerald-50 dark:border-emerald-800 dark:bg-emerald-950/40" : "border-sky-300 bg-white dark:border-sky-800 dark:bg-zinc-900"}`}
        onClick={() => setFlipped(!flipped)}
        title="Clicca per girare"
      >
        <span className="select-text">{flipped ? card.risposta : card.domanda}</span>
      </button>
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
                {i + 1}. {q.domanda}
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
            <dt className="font-medium">{t.termine}</dt>
            <dd className="text-zinc-700 dark:text-zinc-300">{t.definizione}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}
